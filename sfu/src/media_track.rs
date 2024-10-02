use std::sync::Arc;

use enclose::enc;
use tokio::sync::oneshot;
use webrtc::{
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::transport;

#[derive(Clone)]
pub struct MediaTrack {
    pub id: String,
    pub track: Arc<TrackRemote>,
    _rtp_receiver: Arc<RTCRtpReceiver>,
    _rtp_transceiver: Arc<RTCRtpTransceiver>,
    pub rtcp_sender: Arc<transport::RtcpSender>,
}

impl MediaTrack {
    pub fn new(
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        rtp_transceiver: Arc<RTCRtpTransceiver>,
        rtcp_sender: Arc<transport::RtcpSender>,
    ) -> (Self, oneshot::Receiver<bool>) {
        let id = track.id();

        let (closed_sender, closed_receiver) = oneshot::channel();
        tokio::spawn(enc!((track) async move {
            Self::rtp_event_loop(track).await;
            let _ = closed_sender.send(true);
        }));

        (
            Self {
                id,
                track,
                _rtp_receiver: rtp_receiver,
                _rtp_transceiver: rtp_transceiver,
                rtcp_sender,
            },
            closed_receiver,
        )
    }

    async fn rtp_event_loop(track: Arc<TrackRemote>) {
        let track_id = track.id().clone();
        tracing::debug!(
            "MediaTrack RTP event loop has started for {}, {}: {}",
            track_id,
            track.payload_type(),
            track.codec().capability.mime_type
        );

        while let Ok((rtp, _attr)) = track.read_rtp().await {
            tracing::trace!(
                "MediaTrack {} received RTP ssrc={} seq={} timestamp={}",
                track_id,
                rtp.header.ssrc,
                rtp.header.sequence_number,
                rtp.header.timestamp
            )
        }

        tracing::debug!(
            "MediaTrack RTP event loop has finished for {}, {}: {}",
            track_id,
            track.payload_type(),
            track.codec().capability.mime_type
        );
    }
}
