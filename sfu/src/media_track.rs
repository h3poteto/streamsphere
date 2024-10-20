use std::sync::Arc;

use enclose::enc;
use tokio::sync::oneshot;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::{
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::{track_local::track_local_static_rtp::TrackLocalStaticRTP, track_remote::TrackRemote},
};

use crate::transport;

#[derive(Clone)]
pub struct MediaTrack {
    pub id: String,
    pub track: Arc<TrackLocalStaticRTP>,
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
        let local_track =
            TrackLocalStaticRTP::new(track.codec().capability, track.id(), track.stream_id());

        let arc_track = Arc::new(local_track);
        let cloned = arc_track.clone();
        let (closed_sender, closed_receiver) = oneshot::channel();
        tokio::spawn(enc!((track, cloned) async move {
            Self::rtp_event_loop(track, cloned).await;
            let _ = closed_sender.send(true);
        }));

        (
            Self {
                id,
                track: arc_track,
                _rtp_receiver: rtp_receiver,
                _rtp_transceiver: rtp_transceiver,
                rtcp_sender,
            },
            closed_receiver,
        )
    }

    async fn rtp_event_loop(track: Arc<TrackRemote>, local_track: Arc<TrackLocalStaticRTP>) {
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
            );
            if let Err(e) = local_track.write_rtp(&rtp).await {
                tracing::error!("failed to write rtp: {}", e);
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
