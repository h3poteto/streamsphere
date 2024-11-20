use std::sync::Arc;

use enclose::enc;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use webrtc::rtp::packet::Packet;
use webrtc::{
    rtp_transceiver::{rtp_receiver::RTCRtpReceiver, RTCRtpTransceiver},
    track::track_remote::TrackRemote,
};

use crate::transport;

#[derive(Clone, Debug)]
pub struct Publisher {
    /// The ID is the same as published track_id.
    pub id: String,
    pub track: Arc<TrackRemote>,
    _rtp_receiver: Arc<RTCRtpReceiver>,
    _rtp_transceiver: Arc<RTCRtpTransceiver>,
    pub(crate) rtcp_sender: Arc<transport::RtcpSender>,
    pub(crate) rtp_sender: broadcast::Sender<Packet>,
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
}

impl Publisher {
    pub(crate) fn new(
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        rtp_transceiver: Arc<RTCRtpTransceiver>,
        rtcp_sender: Arc<transport::RtcpSender>,
    ) -> (Arc<Self>, oneshot::Receiver<bool>) {
        let id = track.id();

        let (finished_sender, finished_receiver) = oneshot::channel();
        let (rtp_sender, _) = broadcast::channel(1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let closed_receiver = Arc::new(Mutex::new(rx));
        tokio::spawn(enc!((track, rtp_sender) async move {
            Self::rtp_event_loop(track, rtp_sender, closed_receiver).await;
            let _ = finished_sender.send(true);
        }));

        tracing::debug!("Publisher {} is created", id);

        let publisher = Self {
            id,
            track,
            _rtp_receiver: rtp_receiver,
            _rtp_transceiver: rtp_transceiver,
            rtcp_sender,
            rtp_sender,
            closed_sender: Arc::new(tx),
        };

        (Arc::new(publisher), finished_receiver)
    }

    async fn rtp_event_loop(
        track: Arc<TrackRemote>,
        rtp_sender: broadcast::Sender<Packet>,
        publisher_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    ) {
        let track_id = track.id().clone();
        tracing::debug!(
            "Publisher RTP event loop has started for {}, {}: {}",
            track_id,
            track.payload_type(),
            track.codec().capability.mime_type
        );

        let mut last_timestamp = 0;

        loop {
            let mut publisher_closed = publisher_closed.lock().await;
            tokio::select! {
                _closed = publisher_closed.recv() => {
                    break;
                }
                res = track.read_rtp() => {
                    match res {
                        Ok((mut rtp, _attr)) => {
                            let old_timestamp = rtp.header.timestamp;
                            if last_timestamp == 0 {
                                rtp.header.timestamp = 0
                            } else {
                                rtp.header.timestamp -= last_timestamp;
                            }
                            last_timestamp = old_timestamp;

                            tracing::trace!(
                                "Publisher {} received RTP ssrc={} seq={} timestamp={}",
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
                        Err(err) => {
                            tracing::error!("Publisher failed to read rtp: {}", err);
                            break;
                        }
                    }
                }
            }
        }

        // When the track is finished, we should notify the subscriber
        // Subscriber should stop rtp_event_loop and rtcp_event_loop after it.
        drop(rtp_sender);

        tracing::debug!(
            "Publisher RTP event loop has finished for {}, {}: {}",
            track_id,
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
        tracing::debug!("Publisher {} is dropped", self.id);
    }
}
