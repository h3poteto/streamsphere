use std::sync::Arc;

use chrono::Utc;
use enclose::enc;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use uuid::Uuid;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::header::FORMAT_REMB;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::{
    peer_connection::{
        offer_answer_options::RTCOfferOptions, sdp::session_description::RTCSessionDescription,
    },
    rtcp::{
        self,
        header::{PacketType, FORMAT_PLI},
    },
    rtp::packet::Packet,
    rtp_transceiver::rtp_sender::RTCRtpSender,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::config::WebRTCTransportConfig;
use crate::media_track::{detect_mime_type, MediaType};
use crate::transport;
use crate::transport::{OnIceCandidateFn, OnNegotiationNeededFn, Transport};
use crate::{
    error::{Error, SubscriberErrorKind},
    media_track::MediaTrack,
    router::RouterEvent,
};

#[derive(Clone)]
pub struct SubscribeTransport {
    pub id: String,
    peer_connection: Arc<RTCPeerConnection>,
    pending_candidates: Arc<Mutex<Vec<RTCIceCandidateInit>>>,
    router_event_sender: mpsc::UnboundedSender<RouterEvent>,
    offer_options: RTCOfferOptions,
    // For callback fn
    on_ice_candidate_fn: Arc<Mutex<OnIceCandidateFn>>,
    on_negotiation_needed_fn: Arc<Mutex<OnNegotiationNeededFn>>,
    // rtp event
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
    closed_receiver: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
}

impl SubscribeTransport {
    pub async fn new(
        router_event_sender: mpsc::UnboundedSender<RouterEvent>,
        config: WebRTCTransportConfig,
    ) -> Arc<Self> {
        let id = Uuid::new_v4().to_string();

        let peer_connection = Self::generate_peer_connection(config)
            .await
            .expect("failed to generate peer connection");

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
        };

        transport.ice_state_hooks().await;

        let subscriber = Arc::new(transport);
        let copied = Arc::clone(&subscriber);
        let sender = subscriber.router_event_sender.clone();
        let _ = sender.send(RouterEvent::SubscriberAdded(copied));

        tracing::trace!("SubscribeTransport {} is created", subscriber.id);

        subscriber
    }

    pub async fn subscribe(&self, track_id: String) -> Result<RTCSessionDescription, Error> {
        // We have to add a track before creating offer.
        // https://datatracker.ietf.org/doc/html/rfc3264
        // https://github.com/webrtc-rs/webrtc/issues/115#issuecomment-1958137875
        let (tx, rx) = oneshot::channel();

        let _ = self
            .router_event_sender
            .send(RouterEvent::GetMediaTrack(track_id.clone(), tx));

        let reply = rx.await.unwrap();
        match reply {
            None => {
                return Err(Error::new_subscriber(
                    format!("Media track for {} is not found", track_id),
                    SubscriberErrorKind::TrackNotFoundError,
                ))
            }
            Some(track) => {
                self.subscribe_track(track).await?;

                let offer = self.create_offer().await?;
                Ok(offer)
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
            Some(offer) => Ok(offer),
            None => Err(Error::new_transport(
                "Failed to set local description".to_string(),
                crate::error::TransportErrorKind::LocalDescriptionError,
            )),
        }
    }

    pub async fn set_answer(&self, answer: RTCSessionDescription) -> Result<(), Error> {
        tracing::debug!("subscriber set answer");
        self.peer_connection.set_remote_description(answer).await?;

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

    async fn subscribe_track(&self, media_track: Arc<MediaTrack>) -> Result<(), Error> {
        let publisher_rtcp_sender = media_track.rtcp_sender.clone();
        let track_id = media_track.track.id();
        let local_track = TrackLocalStaticRTP::new(
            media_track.track.codec().capability,
            media_track.track.id(),
            media_track.track.stream_id(),
        );
        let mime_type = media_track.track.codec().capability.mime_type;

        let local_track = Arc::new(local_track);
        let cloned_track = local_track.clone();

        let rtcp_sender = self.peer_connection.add_track(local_track).await?;
        let media_ssrc = media_track.track.ssrc();
        tokio::spawn(enc!((rtcp_sender, publisher_rtcp_sender) async move {
            Self::rtcp_event_loop(rtcp_sender, publisher_rtcp_sender, mime_type, media_ssrc).await;
        }));

        let rtp_buffer = media_track.rtp_sender.clone();
        let peer = self.peer_connection.clone();
        let closed_receiver = self.closed_receiver.clone();
        tokio::spawn(enc!((cloned_track, rtp_buffer) async move{
            let receiver = rtp_buffer.subscribe();
            // We have to drop sender to receive close event when the track has been closed.
            drop(rtp_buffer);

            Self::rtp_event_loop(track_id, cloned_track, receiver, closed_receiver).await;
            // Upstream track has been closed, so we should remove subscribed track from peer connection.
            let _ = peer.remove_track(&rtcp_sender).await;
        }));

        Ok(())
    }

    pub async fn rtcp_event_loop(
        rtcp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
        mime_type: String,
        media_ssrc: u32,
    ) {
        let media_type = detect_mime_type(mime_type);
        let start_timestamp = Utc::now();

        while let Ok((rtcp_packets, attr)) = rtcp_sender.read_rtcp().await {
            for rtcp in rtcp_packets.into_iter() {
                tracing::trace!("Receive RTCP rtcp={:#?}, attr={:#?}", rtcp, attr);

                let header = rtcp.header();
                match header.packet_type {
                    PacketType::ReceiverReport => {
                        if let Some(rr) = rtcp
                            .as_any()
                            .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
                        {
                            let rr = rr.clone();
                            match publisher_rtcp_sender.send(Box::new(rr)) {
                                Ok(_) => tracing::trace!("send rtcp: rr"),
                                Err(err) => tracing::error!("failed to send rtcp rr: {}", err),
                            }
                        }
                    }
                    PacketType::PayloadSpecificFeedback => match header.count {
                        FORMAT_PLI => {
                            if let Some(_pli) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication>() {
                                match publisher_rtcp_sender.send(Box::new(PictureLossIndication {
                                    sender_ssrc: 0,
                                    media_ssrc,
                                })) {
                                    Ok(_) => tracing::trace!("send rtcp: pli"),
                                    Err(err) => tracing::error!("failed to send rtcp pli: {}", err)
                                }
                            }
                        }
                        FORMAT_REMB => {
                            if let Some(remb) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate>() {

                                let mut remb = remb.clone();
                                let diff = Utc::now() - start_timestamp;
                                if diff.num_seconds() < 30 {
                                    // Min bitrate is 128kbps if it is video and first 30seconds.
                                    match media_type {
                                        MediaType::Video => {
                                            if remb.bitrate < 128000.0 {
                                                remb.bitrate = 128000.0;
                                            }
                                        }
                                        MediaType::Audio => {
                                            if remb.bitrate < 64000.0 {
                                                remb.bitrate = 640000.0
                                            }
                                        }
                                    }
                                }

                                match publisher_rtcp_sender.send(Box::new(remb)) {
                                    Ok(_) => tracing::trace!("send rtcp: remb"),
                                    Err(err) => tracing::error!("failed to send rtcp remb: {}", err)
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        tracing::debug!("Subscriber RTCP event loop finished");
    }

    pub async fn rtp_event_loop(
        track_id: String,
        local_track: Arc<TrackLocalStaticRTP>,
        mut rtp_receiver: broadcast::Receiver<Packet>,
        subscriber_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    ) {
        tracing::debug!("Subscriber RTP event loop has started for {}", track_id);

        let mut curr_timestamp = 0;

        loop {
            let mut closed = subscriber_closed.lock().await;
            tokio::select! {
                _closed = closed.recv() => {
                    break;
                }
                res = rtp_receiver.recv() => {
                    match res {
                        Ok(mut packet) => {
                            curr_timestamp += packet.header.timestamp;
                            packet.header.timestamp = curr_timestamp;

                            tracing::trace!(
                                "Subscriber write RTP ssrc={} seq={} timestamp={}",
                                packet.header.ssrc,
                                packet.header.sequence_number,
                                packet.header.timestamp
                            );

                            if let Err(err) = local_track.write_rtp(&packet).await {
                                tracing::error!("Subscriber failed to write rtp: {}", err);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                        Err(err) => {
                            tracing::error!("Subscriber failed to receive rtp: {}", err);
                            break;
                        }
                    }

                }
            }
        }

        tracing::debug!("Subscriber RTP event loop has finished for {}", track_id);
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
        peer.on_negotiation_needed(Box::new(enc!( (downgraded_peer, on_negotiation_needed) move || {
                Box::pin(enc!( (downgraded_peer, on_negotiation_needed) async move {
                    tracing::info!("on negotiation needed");
                    let locked = on_negotiation_needed.lock().await;
                    if let Some(pc) = downgraded_peer.upgrade() {
                        if pc.connection_state() == RTCPeerConnectionState::Closed {
                                return;
                        }

                        let offer = pc.create_offer(None).await.expect("could not create subscriber offer:");
                        pc.set_local_description(offer).await.expect("could not set local description");
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
    pub async fn on_ice_candidate(&self, f: OnIceCandidateFn) {
        let mut callback = self.on_ice_candidate_fn.lock().await;
        *callback = f;
    }

    pub async fn on_negotiation_needed(&self, f: OnNegotiationNeededFn) {
        let mut callback = self.on_negotiation_needed_fn.lock().await;
        *callback = f;
    }

    pub async fn close(&self) -> Result<(), Error> {
        let _ = self
            .router_event_sender
            .send(RouterEvent::SubscriberRemoved(self.id.clone()));

        self.closed_sender.send(true).unwrap();

        self.peer_connection.close().await?;
        Ok(())
    }
}

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
        tracing::debug!("Subscriber {} is dropped", self.id);
    }
}
