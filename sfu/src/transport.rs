use std::sync::Arc;
use tokio::sync::mpsc;
use webrtc::{
    ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtcp,
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::error::Error;

pub type RtcpSender = mpsc::UnboundedSender<Box<dyn rtcp::packet::Packet + Send + Sync>>;
pub type RtcpReceiver = mpsc::UnboundedReceiver<Box<dyn rtcp::packet::Packet + Send + Sync>>;

pub type OnIceCandidateFn = Box<dyn Fn(RTCIceCandidate) + Send + Sync>;
pub type OnNegotiationNeededFn = Box<dyn Fn(RTCSessionDescription) + Send + Sync>;
pub type OnTrackFn =
    Box<dyn Fn(Arc<TrackRemote>, Arc<RTCRtpReceiver>, Arc<RTCRtpTransceiver>) + Send + Sync>;

pub trait Transport {
    fn add_ice_candidate(
        &self,
        candidate: RTCIceCandidateInit,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
