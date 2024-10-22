use std::sync::Arc;

use enclose::enc;
use tokio::sync::{broadcast, oneshot};
use webrtc::rtp::packet::Packet;
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
    pub rtp_sender: Arc<broadcast::Sender<Packet>>,
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
        let (rtp_sender, _) = broadcast::channel(1024);

        tokio::spawn(enc!((track, rtp_sender) async move {
            Self::rtp_event_loop(track, rtp_sender).await;
            let _ = closed_sender.send(true);
        }));

        let rtp_sender = Arc::new(rtp_sender);

        (
            Self {
                id,
                track,
                _rtp_receiver: rtp_receiver,
                _rtp_transceiver: rtp_transceiver,
                rtcp_sender,
                rtp_sender,
            },
            closed_receiver,
        )
    }

    async fn rtp_event_loop(track: Arc<TrackRemote>, rtp_sender: broadcast::Sender<Packet>) {
        let track_id = track.id().clone();
        tracing::debug!(
            "MediaTrack RTP event loop has started for {}, {}: {}",
            track_id,
            track.payload_type(),
            track.codec().capability.mime_type
        );

        let mut last_timestamp = 0;
        while let Ok((mut rtp, _attr)) = track.read_rtp().await {
            let old_timestamp = rtp.header.timestamp;
            if last_timestamp == 0 {
                rtp.header.timestamp = 0
            } else {
                rtp.header.timestamp -= last_timestamp;
            }
            last_timestamp = old_timestamp;

            tracing::trace!(
                "MediaTrack {} received RTP ssrc={} seq={} timestamp={}",
                track_id,
                rtp.header.ssrc,
                rtp.header.sequence_number,
                rtp.header.timestamp
            );

            if rtp_sender.receiver_count() > 0 {
                if let Err(e) = rtp_sender.send(rtp) {
                    tracing::error!("failed to broadcast rtp: {}", e);
                }
            }
        }

        tracing::debug!(
            "MediaTrack RTP event loop has finished for {}, {}: {}",
            track_id,
            track.payload_type(),
            track.codec().capability.mime_type
        );
    }
}

pub(crate) fn detect_mime_type(mime_type: String) -> MediaType {
    if mime_type.contains("video") || mime_type.contains("Video") {
        MediaType::Video
    } else {
        MediaType::Audio
    }
}

pub(crate) enum MediaType {
    Video,
    Audio,
}
