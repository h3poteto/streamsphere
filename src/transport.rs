use crate::{
    config::WebRTCTransportConfig,
    error::{Error, TransportErrorKind},
    media_engine,
    media_track::MediaTrack,
    router::RouterEvent,
};
use enclose::enc;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use webrtc::{
    api::{media_engine::MediaEngine, APIBuilder},
    ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
    peer_connection::{
        offer_answer_options::{RTCAnswerOptions, RTCOfferOptions},
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtcp,
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, rtp_sender::RTCRtpSender, RTCRtpTransceiver},
    track::{track_local::TrackLocal, track_remote::TrackRemote},
};

pub type RtcpSender = mpsc::UnboundedSender<Box<dyn rtcp::packet::Packet + Send + Sync>>;
pub type RtcpReceiver = mpsc::UnboundedReceiver<Box<dyn rtcp::packet::Packet + Send + Sync>>;

pub type OnIceCandidateFn = Box<dyn Fn(RTCIceCandidate) + Send + Sync>;
pub type OnNegotiationNeededFn = Box<dyn Fn(RTCSessionDescription) + Send + Sync>;
pub type OnTrackFn =
    Box<dyn Fn(Arc<TrackRemote>, Arc<RTCRtpReceiver>, Arc<RTCRtpTransceiver>) + Send + Sync>;

#[derive(Clone)]
pub struct Transport {
    pub id: String,
    pub router_event_sender: mpsc::UnboundedSender<RouterEvent>,
    config: WebRTCTransportConfig,
    peer_connection: Option<Arc<RTCPeerConnection>>,
    rtcp_sender_channel: Arc<RtcpSender>,
    rtcp_receiver_channel: Arc<Mutex<RtcpReceiver>>,
    stop_sender_channel: Arc<Mutex<mpsc::UnboundedSender<()>>>,
    stop_receiver_channel: Arc<Mutex<mpsc::UnboundedReceiver<()>>>,
    on_ice_candidate_fn: Arc<OnIceCandidateFn>,
    on_negotiation_needed_fn: Arc<OnNegotiationNeededFn>,
    on_track_fn: Arc<OnTrackFn>,
}

impl Transport {
    // TODO: Pass ICEservers information to config from client-side.
    pub async fn new(
        router_event_sender: mpsc::UnboundedSender<RouterEvent>,
        config: WebRTCTransportConfig,
    ) -> Transport {
        let id = Uuid::new_v4().to_string();
        let (s, r) = mpsc::unbounded_channel();
        let (stop_sender, stop_receiver) = mpsc::unbounded_channel();

        let mut transport = Transport {
            id,
            router_event_sender,
            config,
            peer_connection: None,
            rtcp_sender_channel: Arc::new(s),
            rtcp_receiver_channel: Arc::new(Mutex::new(r)),
            stop_sender_channel: Arc::new(Mutex::new(stop_sender)),
            stop_receiver_channel: Arc::new(Mutex::new(stop_receiver)),
            on_ice_candidate_fn: Arc::new(Box::new(|_| {})),
            on_negotiation_needed_fn: Arc::new(Box::new(|_| {})),
            on_track_fn: Arc::new(Box::new(|_, _, _| {})),
        };

        transport.ice_state_hooks().await;

        transport
    }

    pub async fn build_transport(mut self) -> Result<(), webrtc::error::Error> {
        let mut me = MediaEngine::default();
        media_engine::register_default_codecs(&mut me)?;
        media_engine::register_extensions(&mut me)?;
        let api = APIBuilder::new()
            .with_media_engine(me)
            .with_setting_engine(self.config.setting_engine.clone())
            .build();

        let peer_connection = api
            .new_peer_connection(self.config.configuration.clone())
            .await?;

        let pc = Some(Arc::new(peer_connection));
        self.peer_connection = pc.clone();

        if let Some(pc) = pc {
            tokio::spawn(async move {
                tracing::info!("RTCP writer loop");
                loop {
                    let mut rtcp_receiver = self.rtcp_receiver_channel.lock().await;
                    let mut stop_receiver = self.stop_receiver_channel.lock().await;
                    tokio::select! {
                        data = rtcp_receiver.recv() => {
                            if let Some(data) = data {
                                if let Err(err) = pc.write_rtcp(&[data]).await {
                                    tracing::error!("Error writing RTCP: {}", err);
                                }
                            }
                        }
                        _data = stop_receiver.recv() => {
                            tracing::info!("RTCP writer loop stopped");
                            return;
                        }
                    };
                }
            });
        }

        Ok(())
    }

    // This function is for client side PeerConnection. This is called after on_ice_candidate in client-side.
    pub async fn trickle_ice(&self, candidate: RTCIceCandidateInit) -> Result<(), Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let res = peer.add_ice_candidate(candidate).await?;
        Ok(res)
    }

    pub async fn set_remote_description(&self, sdp: RTCSessionDescription) -> Result<(), Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let res = peer.set_remote_description(sdp).await?;
        Ok(res)
    }

    pub async fn set_local_description(&self, sdp: RTCSessionDescription) -> Result<(), Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let res = peer.set_local_description(sdp).await?;
        Ok(res)
    }

    pub async fn create_offer(
        &self,
        options: Option<RTCOfferOptions>,
    ) -> Result<RTCSessionDescription, Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let offer = peer.create_offer(options).await?;
        Ok(offer)
    }

    pub async fn create_answer(
        &self,
        options: Option<RTCAnswerOptions>,
    ) -> Result<RTCSessionDescription, Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let answer = peer.create_answer(options).await?;
        Ok(answer)
    }

    pub async fn local_description(&self) -> Result<Option<RTCSessionDescription>, Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let answer = peer.local_description().await;
        Ok(answer)
    }

    pub async fn gathering_complete_promise(&self) -> Result<mpsc::Receiver<()>, Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let promise = peer.gathering_complete_promise().await;
        Ok(promise)
    }

    pub async fn add_ice_candidate(&self, candidate: RTCIceCandidateInit) -> Result<(), Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let res = peer.add_ice_candidate(candidate).await?;
        Ok(res)
    }

    pub async fn add_track(
        &self,
        track: Arc<dyn TrackLocal + Send + Sync>,
    ) -> Result<Arc<RTCRtpSender>, Error> {
        let peer = self.peer_connection.clone().ok_or(Error::new_transport(
            "PeerConnection does not exist".to_string(),
            TransportErrorKind::PeerConnectionError,
        ))?;

        let res = peer.add_track(track).await?;
        Ok(res)
    }

    pub async fn close(self) -> Result<(), Error> {
        if let Err(err) = self.stop_sender_channel.lock().await.send(()) {
            tracing::error!("failed to stop rtcp writer loop: {}", err);
        }
        match self.peer_connection {
            Some(peer) => {
                peer.close().await?;
                Ok(())
            }
            None => Ok(()),
        }
    }

    async fn ice_state_hooks(&mut self) -> Option<()> {
        let peer = self.peer_connection.clone()?;
        let on_ice_candidate = Arc::clone(&self.on_ice_candidate_fn);

        // This callback is called after initializing PeerConnection with ICE servers.
        peer.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            Box::pin({
                let func = on_ice_candidate.clone();
                async move {
                    if let Some(candidate) = candidate {
                        tracing::info!("on ice candidate: {}", candidate);
                        // Call on_ice_candidate_fn as callback.
                        (func)(candidate);
                    }
                }
            })
        }));

        let downgraded_peer = Arc::downgrade(&peer);
        let on_negotiation_needed = Arc::clone(&self.on_negotiation_needed_fn);
        peer.on_negotiation_needed(Box::new(enc!( (downgraded_peer, on_negotiation_needed) move || {
                Box::pin(enc!( (downgraded_peer, on_negotiation_needed) async move {
                    tracing::info!("on negotiation needed");

                    if let Some(pc) = downgraded_peer.upgrade() {
                        if pc.connection_state() == RTCPeerConnectionState::Closed {
                                return;
                        }

                        let offer = pc.create_offer(None).await.expect("could not create subscriber offer:");
                        pc.set_local_description(offer).await.expect("could not set local description");
                        let offer = pc.local_description().await.unwrap();

                        tracing::info!("peer sending offer");
                        (on_negotiation_needed)(offer);
                    }
                }))
            })));

        let on_track = Arc::clone(&self.on_track_fn);
        let sender = self.router_event_sender.clone();
        let rtcp_sender = self.rtcp_sender_channel.clone();
        peer.on_track(Box::new(enc!( (on_track, sender, rtcp_sender)
            move |track: Arc<TrackRemote>,
                  receiver: Arc<RTCRtpReceiver>,
                  transceiver: Arc<RTCRtpTransceiver>| {
                Box::pin(enc!( (on_track, sender, rtcp_sender) async move {
                    let id = track.id();
                    tracing::info!("on track: {}", id);
                    let (media_track, closed) = MediaTrack::new(track.clone(), receiver.clone(), transceiver.clone(), rtcp_sender);
                    let _ = sender.send(RouterEvent::TrackPublished(media_track));

                    (on_track)(track, receiver, transceiver);

                    // Keep this thread until closed, and send TrackRemove event
                    let _ = closed.await;
                    let _ = sender.send(RouterEvent::TrackRemoved(id));
                }))
            }
        )));

        Some(())
    }

    // Hooks
    pub async fn on_ice_candidate(&mut self, f: OnIceCandidateFn) {
        self.on_ice_candidate_fn = Arc::new(f);
    }

    pub async fn on_negotiation_needed(&mut self, f: OnNegotiationNeededFn) {
        self.on_negotiation_needed_fn = Arc::new(f);
    }

    pub async fn on_track(&mut self, f: OnTrackFn) {
        self.on_track_fn = Arc::new(f);
    }
}
