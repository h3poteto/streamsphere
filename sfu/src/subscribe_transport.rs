use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use derivative::Derivative;
use enclose::enc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::sleep;
use uuid::Uuid;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::{
    offer_answer_options::RTCOfferOptions, sdp::session_description::RTCSessionDescription,
};
use webrtc::rtp_transceiver::rtp_codec::RTCRtpHeaderExtensionParameters;
use webrtc_sdp::attribute_type::{SdpAttribute, SdpAttributeType};
use webrtc_sdp::parse_sdp;

use crate::config::{MediaConfig, WebRTCTransportConfig};
use crate::data_publisher::DataPublisher;
use crate::data_subscriber::DataSubscriber;
use crate::subscriber::Subscriber;
use crate::transport::{OnIceCandidateFn, OnNegotiationNeededFn, PeerConnection, Transport};
use crate::{
    error::{Error, SubscriberErrorKind},
    publisher::Publisher,
    router::RouterEvent,
};

/// This handle [`webrtc::peer_connection::RTCPeerConnection`] methods for subscriber.
#[derive(Derivative)]
#[derivative(Clone, Debug)]
pub struct SubscribeTransport {
    pub id: String,
    peer_connection: Arc<RTCPeerConnection>,
    pending_candidates: Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    router_event_sender: mpsc::UnboundedSender<RouterEvent>,
    offer_options: RTCOfferOptions,
    // For callback fn
    #[derivative(Debug = "ignore")]
    on_ice_candidate_fn: Arc<Mutex<OnIceCandidateFn>>,
    #[derivative(Debug = "ignore")]
    on_negotiation_needed_fn: Arc<Mutex<OnNegotiationNeededFn>>,
    // rtp event
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
    closed_receiver: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    signaling_pending: Arc<AtomicBool>,
}

impl SubscribeTransport {
    pub(crate) async fn new(
        router_event_sender: mpsc::UnboundedSender<RouterEvent>,
        media_config: MediaConfig,
        transport_config: WebRTCTransportConfig,
    ) -> Arc<Self> {
        let id = Uuid::new_v4().to_string();

        let peer_connection = Self::generate_peer_connection(media_config, transport_config)
            .await
            .unwrap();

        let (closed_sender, closed_receiver) = mpsc::unbounded_channel();

        let mut transport = Self {
            id,
            peer_connection: Arc::new(peer_connection),
            router_event_sender,
            offer_options: RTCOfferOptions {
                ice_restart: false,
                voice_activity_detection: false,
            },
            pending_candidates: Arc::new(Mutex::new(Vec::new())),
            on_ice_candidate_fn: Arc::new(Mutex::new(Box::new(|_| {}))),
            on_negotiation_needed_fn: Arc::new(Mutex::new(Box::new(|_| {}))),
            closed_sender: Arc::new(closed_sender),
            closed_receiver: Arc::new(Mutex::new(closed_receiver)),
            signaling_pending: Arc::new(AtomicBool::new(false)),
        };

        transport.ice_state_hooks().await;

        let subscriber = Arc::new(transport);

        tracing::debug!("SubscribeTransport {} is created", subscriber.id);

        subscriber
    }

    /// This starts subscribing the published media and returns an offer sdp. Please provide a [`crate::publisher::Publisher`] ID.
    pub async fn subscribe(
        &self,
        publisher_id: String,
    ) -> Result<(Subscriber, RTCSessionDescription), Error> {
        // We have to add a track before creating offer.
        // https://datatracker.ietf.org/doc/html/rfc3264
        // https://github.com/webrtc-rs/webrtc/issues/115#issuecomment-1958137875
        let (tx, rx) = oneshot::channel();

        let _ = self
            .router_event_sender
            .send(RouterEvent::GetPublisher(publisher_id.clone(), tx));

        let reply = rx.await.unwrap();
        match reply {
            None => {
                return Err(Error::new_subscriber(
                    format!("Publisher for {} is not found", publisher_id),
                    SubscriberErrorKind::TrackNotFoundError,
                ))
            }
            Some(publisher) => {
                while self.signaling_pending.load(Ordering::Relaxed) {
                    sleep(Duration::from_millis(10)).await;
                }
                self.signaling_pending.store(true, Ordering::Relaxed);
                let subscriber = self.subscribe_track(publisher).await?;

                let offer = self.create_offer().await?;
                Ok((subscriber, offer))
            }
        }
    }

    /// This starts subscribing the data channel and returns an offer sdp. Please provide a [`crate::data_publisher::DataPublisher`] ID.
    pub async fn data_subscribe(
        &self,
        data_publisher_id: String,
    ) -> Result<(DataSubscriber, RTCSessionDescription), Error> {
        let (tx, rx) = oneshot::channel();

        let _ = self
            .router_event_sender
            .send(RouterEvent::GetDataPublisher(data_publisher_id.clone(), tx));

        let reply = rx.await.unwrap();
        match reply {
            None => Err(Error::new_subscriber(
                format!("DataPublisher for {} is not found", data_publisher_id),
                SubscriberErrorKind::DataChannelNotFoundError,
            )),
            Some(data_publisher) => {
                let data_subscriber = self.subscribe_data(data_publisher).await?;

                let offer = self.create_offer().await?;
                Ok((data_subscriber, offer))
            }
        }
    }

    async fn create_offer(&self) -> Result<RTCSessionDescription, Error> {
        tracing::debug!("subscriber creates offer");

        let offer = self
            .peer_connection
            .create_offer(Some(self.offer_options.clone()))
            .await?;

        let mut gathering_complete = self.peer_connection.gathering_complete_promise().await;
        self.peer_connection.set_local_description(offer).await?;
        let _ = gathering_complete.recv().await;

        match self.peer_connection.local_description().await {
            Some(offer) => {
                let (tx, rx) = oneshot::channel();
                let _ = self
                    .router_event_sender
                    .send(RouterEvent::GetPublishersExtmap(tx));

                let reply = rx.await.unwrap();

                let offer = Self::adjust_extmap(offer, reply)?;
                Ok(offer)
            }
            None => Err(Error::new_transport(
                "Failed to set local description".to_string(),
                crate::error::TransportErrorKind::LocalDescriptionError,
            )),
        }
    }

    /// This sets the answer to the [`webrtc::peer_connection::RTCPeerConnection`].
    pub async fn set_answer(&self, answer: RTCSessionDescription) -> Result<(), Error> {
        tracing::debug!("subscriber set answer");
        self.peer_connection.set_remote_description(answer).await?;

        self.signaling_pending.store(false, Ordering::Relaxed);
        let pendings = self.pending_candidates.lock().await;
        for candidate in pendings.iter() {
            tracing::debug!("Adding pending ICE candidate: {:#?}", candidate);
            if let Err(err) = self
                .peer_connection
                .add_ice_candidate(candidate.clone())
                .await
            {
                tracing::error!("failed to add_ice_candidate: {}", err);
            }
        }

        Ok(())
    }

    async fn subscribe_track(&self, publisher: Arc<Publisher>) -> Result<Subscriber, Error> {
        let publisher_rtcp_sender = publisher.rtcp_sender.clone();
        let mime_type = publisher.track.codec().capability.mime_type;

        let local_track = publisher.local_track.clone();
        let rtcp_sender = self.peer_connection.add_track(local_track).await?;
        let media_ssrc = publisher.track.ssrc();

        let subscriber = Subscriber::new(rtcp_sender, publisher_rtcp_sender, mime_type, media_ssrc);

        Ok(subscriber)
    }

    async fn subscribe_data(
        &self,
        data_publisher: Arc<DataPublisher>,
    ) -> Result<DataSubscriber, Error> {
        let data_sender = data_publisher.data_sender.clone();

        let data_channel = self
            .peer_connection
            .create_data_channel(data_publisher.id.as_str(), None)
            .await?;

        let closed_receiver = self.closed_receiver.clone();
        let data_subscriber = DataSubscriber::new(
            data_publisher.id.clone(),
            data_channel,
            data_sender,
            closed_receiver,
        );

        Ok(data_subscriber)
    }

    async fn ice_state_hooks(&mut self) {
        let peer = self.peer_connection.clone();
        let on_ice_candidate = Arc::clone(&self.on_ice_candidate_fn);

        // This callback is called after initializing PeerConnection with ICE servers.
        peer.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            Box::pin({
                let func = on_ice_candidate.clone();
                async move {
                    let locked = func.lock().await;
                    if let Some(candidate) = candidate {
                        tracing::info!("on ice candidate: {}", candidate);
                        // Call on_ice_candidate_fn as callback.
                        (locked)(candidate);
                    }
                }
            })
        }));

        let downgraded_peer = Arc::downgrade(&peer);
        let on_negotiation_needed = Arc::clone(&self.on_negotiation_needed_fn);
        let signaling_pending = self.signaling_pending.clone();
        let offer_options = self.offer_options.clone();
        let router_sender = self.router_event_sender.clone();
        peer.on_negotiation_needed(Box::new(enc!( (downgraded_peer, on_negotiation_needed, signaling_pending, router_sender) move || {
                Box::pin(enc!( (downgraded_peer, on_negotiation_needed, signaling_pending, router_sender) async move {
                    tracing::info!("on negotiation needed");
                    while signaling_pending.load(Ordering::Relaxed) {
                        sleep(Duration::from_millis(10)).await;
                    }
                    let locked = on_negotiation_needed.lock().await;
                    if let Some(pc) = downgraded_peer.upgrade() {
                        if pc.connection_state() == RTCPeerConnectionState::Closed {
                                return;
                        }
                        signaling_pending.store(true, Ordering::Relaxed);

                        let (tx, rx) = oneshot::channel();
                        let _ = router_sender
                            .send(RouterEvent::GetPublishersExtmap(tx));

                        let reply = rx.await.unwrap();

                        let offer = pc.create_offer(Some(offer_options)).await.expect("could not create subscriber offer:");
                        let offer = Self::adjust_extmap(offer, reply).expect("could not adjust sdp");

                        let mut gathering_complete = pc.gathering_complete_promise().await;
                        pc.set_local_description(offer).await.expect("could not set local description");
                        let _ = gathering_complete.recv().await;

                        let offer = pc.local_description().await.unwrap();

                        tracing::info!("peer sending offer");
                        (locked)(offer);
                    }
                }))
            })));

        peer.on_ice_gathering_state_change(Box::new(move |state| {
            Box::pin(async move {
                tracing::debug!("ICE gathering state changed: {}", state);
            })
        }));
    }

    // Hooks
    /// Set callback function when the [`webrtc::peer_connection::RTCPeerConnection`] receives `on_ice_candidate` events.
    pub async fn on_ice_candidate(&self, f: OnIceCandidateFn) {
        let mut callback = self.on_ice_candidate_fn.lock().await;
        *callback = f;
    }

    /// Set callback function when the [`webrtc::peer_connection::RTCPeerConnection`] receives `on_negotiation_needed` events.
    pub async fn on_negotiation_needed(&self, f: OnNegotiationNeededFn) {
        let mut callback = self.on_negotiation_needed_fn.lock().await;
        *callback = f;
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.closed_sender.send(true).unwrap();

        self.peer_connection.close().await?;
        Ok(())
    }

    fn adjust_extmap(
        mut sdp: RTCSessionDescription,
        publishers_extmap: HashMap<String, Vec<RTCRtpHeaderExtensionParameters>>,
    ) -> Result<RTCSessionDescription, Error> {
        let mut session = parse_sdp(&sdp.sdp, false)?;

        for media in session.media.iter_mut() {
            let msid = match media.get_attribute(SdpAttributeType::Msid) {
                Some(&SdpAttribute::Msid(ref msid)) => msid.clone(),
                _ => continue,
            };
            let track_id = match msid.appdata {
                Some(ref appdata) => appdata.clone(),
                None => continue,
            };

            tracing::debug!("debug_msid: {:#?}", track_id);
            let extmaps = match publishers_extmap.get(&track_id) {
                Some(extmaps) => extmaps,
                None => continue,
            };

            let mut found_attr = vec![];
            for attr in media.get_attributes() {
                match attr {
                    SdpAttribute::Extmap(extmap) => {
                        found_attr.push(extmap.clone());
                    }
                    _ => continue,
                }
            }
            media.remove_attribute(SdpAttributeType::Extmap);
            for attr in found_attr {
                if let Some(order) = find_extmap_order(&attr.url, extmaps) {
                    let mut new_attr = attr.clone();
                    new_attr.id = order;
                    let _ = media.add_attribute(SdpAttribute::Extmap(new_attr))?;
                };
            }
        }
        tracing::trace!("updated session: {:#?}", session);
        sdp.sdp = session.to_string();
        Ok(sdp)
    }
}

impl PeerConnection for SubscribeTransport {}

impl Transport for SubscribeTransport {
    async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<(), Error> {
        if let Some(_rd) = self.peer_connection.remote_description().await {
            tracing::debug!("Adding ICE candidate for {:#?}", candidate);
            let _ = self
                .peer_connection
                .add_ice_candidate(candidate.clone())
                .await?;
        } else {
            tracing::debug!("Pending ICE candidate for {:#?}", candidate);
            self.pending_candidates.lock().await.push(candidate.clone());
        }

        Ok(())
    }
}

impl Drop for SubscribeTransport {
    fn drop(&mut self) {
        tracing::debug!("SubscribeTransport {} is dropped", self.id);
    }
}

fn find_extmap_order(url: &str, extmaps: &Vec<RTCRtpHeaderExtensionParameters>) -> Option<u16> {
    for extmap in extmaps {
        if extmap.uri == url {
            match extmap.id.try_into() {
                Ok(id) => return Some(id),
                Err(_) => return None,
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use std::fs;

    use webrtc::sdp::extmap;
    use webrtc_sdp::attribute_type::SdpAttributeExtmap;

    use crate::config;

    use super::*;

    fn check_extmap_index(
        publishers_extmap: HashMap<String, Vec<RTCRtpHeaderExtensionParameters>>,
        original_sdp_path: &str,
        correct_sdp_path: &str,
    ) {
        let original = fs::read_to_string(original_sdp_path)
            .expect(format!("failed to open {}", original_sdp_path).as_str());
        let correct = fs::read_to_string(correct_sdp_path)
            .expect(format!("failed to open {}", correct_sdp_path).as_str());
        let mut original_sdp = RTCSessionDescription::default();
        original_sdp.sdp = original;
        let res = SubscribeTransport::adjust_extmap(original_sdp, publishers_extmap)
            .expect("failed to adjust extmap");

        let correct_session = parse_sdp(&correct, false).expect("failed to parse correct sdp");
        let response_session = parse_sdp(&res.sdp, false).expect("failed to parse response sdp");
        for media in response_session.media {
            let SdpAttribute::Mid(mid) = media
                .get_attribute(SdpAttributeType::Mid)
                .expect("failed to find mid")
            else {
                todo!()
            };

            let correct_media = correct_session
                .media
                .clone()
                .into_iter()
                .find(|m| {
                    let SdpAttribute::Mid(correct_mid) = m
                        .get_attribute(SdpAttributeType::Mid)
                        .expect("failed to find mid")
                    else {
                        todo!()
                    };
                    correct_mid == mid
                })
                .expect("failed to find correct media");

            let correct_extmaps: Vec<SdpAttributeExtmap> = correct_media
                .get_attributes()
                .iter()
                .filter_map(|a| {
                    if let SdpAttribute::Extmap(extmap) = a {
                        Some(extmap.clone())
                    } else {
                        None
                    }
                })
                .collect();

            let extmaps: Vec<SdpAttributeExtmap> = media
                .get_attributes()
                .iter()
                .filter_map(|a| {
                    if let SdpAttribute::Extmap(extmap) = a {
                        Some(extmap.clone())
                    } else {
                        None
                    }
                })
                .collect();

            for extmap in extmaps.iter() {
                let correct_extmap = correct_extmaps
                    .iter()
                    .find(|e| e.url == extmap.url)
                    .expect("failed to find correct extmap");

                assert_eq!(extmap.id, correct_extmap.id);
                assert_eq!(extmap.url, correct_extmap.url);
            }
        }
    }

    #[test]
    fn test_adjust_extmap_video() {
        let publishers_extmap = HashMap::from([(
            "b0734cb9-da91-4957-8957-25de07ab05d0".to_owned(),
            vec![
                RTCRtpHeaderExtensionParameters {
                    uri: config::EXT_TOFFSET.to_string(),
                    id: 14,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::ABS_SEND_TIME_URI.to_owned(),
                    id: 2,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::VIDEO_ORIENTATION_URI.to_owned(),
                    id: 13,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::TRANSPORT_CC_URI.to_owned(),
                    id: 3,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::SDES_MID_URI.to_owned(),
                    id: 4,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::SDES_RTP_STREAM_ID_URI.to_owned(),
                    id: 10,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::SDES_REPAIR_RTP_STREAM_ID_URI.to_owned(),
                    id: 11,
                },
            ],
        )]);
        check_extmap_index(
            publishers_extmap,
            "./test_data/sdp_video_original",
            "./test_data/sdp_video_correct",
        );
    }

    #[test]
    fn test_adjust_extmap_audio() {
        let publishers_extmap = HashMap::from([(
            "17c3e6ab-4b11-47cb-bcab-8d8b88abe0d7".to_owned(),
            vec![
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::AUDIO_LEVEL_URI.to_owned(),
                    id: 1,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::ABS_SEND_TIME_URI.to_owned(),
                    id: 2,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::TRANSPORT_CC_URI.to_owned(),
                    id: 3,
                },
                RTCRtpHeaderExtensionParameters {
                    uri: extmap::SDES_MID_URI.to_owned(),
                    id: 4,
                },
            ],
        )]);
        check_extmap_index(
            publishers_extmap,
            "./test_data/sdp_audio_original",
            "./test_data/sdp_audio_correct",
        );
    }

    #[test]
    fn test_adjust_extmap_audio_video() {
        let publishers_extmap = HashMap::from([
            (
                "9b7db7c1-b108-4c3e-aac8-b81301062ef6".to_owned(),
                vec![
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::AUDIO_LEVEL_URI.to_owned(),
                        id: 1,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::ABS_SEND_TIME_URI.to_owned(),
                        id: 2,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::TRANSPORT_CC_URI.to_owned(),
                        id: 3,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::SDES_MID_URI.to_owned(),
                        id: 4,
                    },
                ],
            ),
            (
                "0af5300f-99df-490d-8b06-c9f49ef95eb5".to_owned(),
                vec![
                    RTCRtpHeaderExtensionParameters {
                        uri: config::EXT_TOFFSET.to_string(),
                        id: 14,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::ABS_SEND_TIME_URI.to_owned(),
                        id: 2,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::VIDEO_ORIENTATION_URI.to_owned(),
                        id: 13,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::TRANSPORT_CC_URI.to_owned(),
                        id: 3,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::SDES_MID_URI.to_owned(),
                        id: 4,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::SDES_RTP_STREAM_ID_URI.to_owned(),
                        id: 10,
                    },
                    RTCRtpHeaderExtensionParameters {
                        uri: extmap::SDES_REPAIR_RTP_STREAM_ID_URI.to_owned(),
                        id: 11,
                    },
                ],
            ),
        ]);
        check_extmap_index(
            publishers_extmap,
            "./test_data/sdp_audio_video_original",
            "./test_data/sdp_audio_video_correct",
        );
    }
}
