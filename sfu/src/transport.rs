use std::sync::Arc;
use tokio::sync::mpsc;
use webrtc::{
    api::{
        interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder,
    },
    ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
    interceptor::registry::Registry,
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
    rtcp,
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::{config::WebRTCTransportConfig, error::Error, media_engine};

pub type RtcpSender = mpsc::UnboundedSender<Box<dyn rtcp::packet::Packet + Send + Sync>>;
pub type RtcpReceiver = mpsc::UnboundedReceiver<Box<dyn rtcp::packet::Packet + Send + Sync>>;

pub type OnIceCandidateFn = Box<dyn Fn(RTCIceCandidate) + Send + Sync>;
pub type OnNegotiationNeededFn = Box<dyn Fn(RTCSessionDescription) + Send + Sync>;
pub type OnTrackFn =
    Box<dyn Fn(Arc<TrackRemote>, Arc<RTCRtpReceiver>, Arc<RTCRtpTransceiver>) + Send + Sync>;

pub trait Transport {
    fn generate_peer_connection(
        config: WebRTCTransportConfig,
    ) -> impl std::future::Future<Output = Result<RTCPeerConnection, Error>> + Send {
        async move {
            let mut me = MediaEngine::default();
            me.register_default_codecs()
                .expect("failed to register default codec");
            // media_engine::register_default_codecs(&mut me).expect("failed to register default codecs");
            media_engine::register_extensions(&mut me)?;
            let mut registry = Registry::new();
            registry = register_default_interceptors(registry, &mut me)
                .expect("failed to register interceptors");

            let api = APIBuilder::new()
                .with_media_engine(me)
                .with_interceptor_registry(registry)
                .with_setting_engine(config.setting_engine())
                .build();

            let peer_connection = api
                .new_peer_connection(config.configuration.clone())
                .await?;

            Ok(peer_connection)
        }
    }

    fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
