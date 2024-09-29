use std::sync::Arc;

use webrtc::{
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

#[derive(Clone)]
pub struct MediaTrack {
    pub id: String,
    track: Arc<TrackRemote>,
    rtp_receiver: Arc<RTCRtpReceiver>,
    rtp_transceiver: Arc<RTCRtpTransceiver>,
}

impl MediaTrack {
    pub fn new(
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        rtp_transceiver: Arc<RTCRtpTransceiver>,
    ) -> Self {
        let id = track.id();
        Self {
            id,
            track,
            rtp_receiver,
            rtp_transceiver,
        }
    }
}
