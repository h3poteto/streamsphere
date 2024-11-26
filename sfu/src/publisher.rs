use std::sync::Arc;

use enclose::enc;
use tokio::sync::{mpsc, oneshot, Mutex};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::{
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::router::RouterEvent;
use crate::transport;

#[derive(Clone, Debug)]
pub struct Publisher {
    /// The ID is the same as published track_id.
    pub id: String,
    pub track: Arc<TrackRemote>,
    _rtp_receiver: Arc<RTCRtpReceiver>,
    _rtp_transceiver: Arc<RTCRtpTransceiver>,
    pub(crate) rtcp_sender: Arc<transport::RtcpSender>,
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
    pub(crate) local_track: Arc<TrackLocalStaticRTP>,
}

impl Publisher {
    pub(crate) async fn new(
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        rtp_transceiver: Arc<RTCRtpTransceiver>,
        rtcp_sender: Arc<transport::RtcpSender>,
        router_sender: mpsc::UnboundedSender<RouterEvent>,
    ) -> Self {
        let id = track.id();
        let ssrc = track.ssrc();

        let (tx, rx) = mpsc::unbounded_channel();
        let closed_receiver = Arc::new(Mutex::new(rx));
        let cloned_id = id.clone();

        let (local_track_tx, local_track_rx) = oneshot::channel();

        tokio::spawn(enc!((track) async move {
            let local_track = Arc::new(TrackLocalStaticRTP::new(
                track.codec().capability,
                track.id(),
                track.stream_id(),
            ));
            let _ = local_track_tx.send(local_track.clone());

            Self::rtp_event_loop(local_track, track, closed_receiver).await;
            let _ = router_sender.send(RouterEvent::TrackRemoved(cloned_id));
        }));

        let local_track = local_track_rx.await.unwrap();

        tracing::debug!("Publisher id={} is created for ssrc={}", id, ssrc);

        let publisher = Self {
            id,
            track,
            _rtp_receiver: rtp_receiver,
            _rtp_transceiver: rtp_transceiver,
            rtcp_sender,
            closed_sender: Arc::new(tx),
            local_track,
        };

        publisher
    }

    async fn rtp_event_loop(
        local_track: Arc<TrackLocalStaticRTP>,
        track: Arc<TrackRemote>,
        publisher_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    ) {
        let track_id = track.id().clone();
        let ssrc = track.ssrc();
        tracing::debug!(
            "Publisher RTP event loop has started for track_id={}, ssrc={}, {}: {}",
            track_id,
            ssrc,
            track.payload_type(),
            track.codec().capability.mime_type
        );

        loop {
            let mut publisher_closed = publisher_closed.lock().await;
            tokio::select! {
                _closed = publisher_closed.recv() => {
                    break;
                }
                res = track.read_rtp() => {
                    match res {
                        Ok((rtp, _attr)) => {
                            tracing::trace!(
                                "Publisher track_id={} received RTP ssrc={} seq={} timestamp={}",
                                track_id,
                                rtp.header.ssrc,
                                rtp.header.sequence_number,
                                rtp.header.timestamp
                            );

                            if let Err(err) = local_track.write_rtp(&rtp).await {
                                tracing::error!("failed to write rtp for id={}: {}", track_id, err);
                            }
                        }
                        Err(err) => {
                            tracing::error!("Publisher id={} failed to read rtp: {}", track_id, err);
                            break;
                        }
                    }
                }
            }
        }

        tracing::debug!(
            "Publisher RTP event loop has finished for track_id={}, ssrc={}, {}: {}",
            track_id,
            ssrc,
            track.payload_type(),
            track.codec().capability.mime_type
        );
    }

    pub async fn close(&self) {
        self.closed_sender.send(true).unwrap();
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

impl Drop for Publisher {
    fn drop(&mut self) {
        tracing::debug!("Publisher id={} is dropped", self.id);
    }
}
