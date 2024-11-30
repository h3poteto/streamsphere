use std::sync::Arc;
use std::time::Duration;

use enclose::enc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::sleep;
use webrtc::rtp;
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
    pub(crate) rtp_packet_sender: broadcast::Sender<rtp::packet::Packet>,
}

impl Publisher {
    pub(crate) fn new(
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        rtp_transceiver: Arc<RTCRtpTransceiver>,
        rtcp_sender: Arc<transport::RtcpSender>,
        router_sender: mpsc::UnboundedSender<RouterEvent>,
    ) -> Self {
        let id = track.id();
        let ssrc = track.ssrc();

        let (sender, _reader) = broadcast::channel::<rtp::packet::Packet>(1024);
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let id = id.clone();
            let closed_receiver = Arc::new(Mutex::new(rx));
            tokio::spawn(enc!((sender, track) async move {
                Self::rtp_event_loop(id.clone(), ssrc, sender, track, closed_receiver).await;
                let _ = router_sender.send(RouterEvent::TrackRemoved(id));
            }));
        }

        tracing::debug!("Publisher id={} is created for ssrc={}", id, ssrc);

        let publisher = Self {
            id,
            track,
            _rtp_receiver: rtp_receiver,
            _rtp_transceiver: rtp_transceiver,
            rtcp_sender,
            closed_sender: Arc::new(tx),
            rtp_packet_sender: sender,
        };

        publisher
    }

    async fn rtp_event_loop(
        id: String,
        ssrc: u32,
        rtp_sender: broadcast::Sender<rtp::packet::Packet>,
        track: Arc<TrackRemote>,
        publisher_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    ) {
        tracing::debug!(
            "Publisher id={} ssrc={} RTP event loop has started, payload_type={}, mime_type={}",
            id,
            ssrc,
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
                                "Publisher id={} received RTP ssrc={} seq={} timestamp={}",
                                id,
                                rtp.header.ssrc,
                                rtp.header.sequence_number,
                                rtp.header.timestamp
                            );

                            if rtp_sender.receiver_count() > 0 {
                                if let Err(err) = rtp_sender.send(rtp) {
                                    tracing::error!("Publisher id={} failed to send rtp: {}", id, err);
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!("Publisher id={} failed to read rtp: {}", id, err);
                            break;
                        }
                    }
                }
            }
            sleep(Duration::from_millis(1)).await;
        }

        tracing::debug!(
            "Publisher id={} ssrc={} RTP event loop has finished",
            id,
            ssrc
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
