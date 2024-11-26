use std::sync::Arc;

use chrono::Utc;
use enclose::enc;
use tokio::sync::mpsc;
use uuid::Uuid;
use webrtc::{
    rtcp::{
        self,
        header::{PacketType, FORMAT_PLI, FORMAT_REMB},
        payload_feedbacks::picture_loss_indication::PictureLossIndication,
    },
    rtp_transceiver::rtp_sender::RTCRtpSender,
};

use crate::{
    publisher::{detect_mime_type, MediaType},
    transport,
};

#[derive(Clone, Debug)]
pub struct Subscriber {
    pub id: String,
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
}

impl Subscriber {
    pub(crate) fn new(
        track_id: String,
        rtcp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
        mime_type: String,
        original_track_ssrc: u32,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let subscriber = Self {
            id,
            closed_sender: Arc::new(tx),
        };

        let original_track_id = track_id.clone();
        tokio::spawn(enc!((rtcp_sender, publisher_rtcp_sender) async move {
            Self::rtcp_event_loop(original_track_id, rtcp_sender, publisher_rtcp_sender, mime_type, original_track_ssrc, rx).await;
        }));

        tracing::debug!(
            "Subscriber id={} is created, track_id={}",
            subscriber.id,
            track_id
        );
        subscriber
    }

    pub(crate) async fn rtcp_event_loop(
        track_id: String,
        rtcp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
        mime_type: String,
        original_track_ssrc: u32,
        mut subscriber_closed: mpsc::UnboundedReceiver<bool>,
    ) {
        let media_type = detect_mime_type(mime_type);
        let start_timestamp = Utc::now();
        tracing::debug!(
            "Subscriber RTCP event loop has started for track_id={}",
            track_id
        );

        loop {
            tokio::select! {
                _ = subscriber_closed.recv() => {
                    break;
                }
                res = rtcp_sender.read_rtcp() => {
                    match res {
                        Ok((rtcp_packets, attr)) => {
                            for rtcp in rtcp_packets.into_iter() {
                                tracing::trace!("Receive RTCP rtcp={:#?}, attr={:#?}", rtcp, attr);

                                let header = rtcp.header();
                                match header.packet_type {
                                    PacketType::ReceiverReport => {
                                        if let Some(rr) = rtcp
                                            .as_any()
                                            .downcast_ref::<rtcp::receiver_report::ReceiverReport>()
                                        {
                                            let rr = rr.clone();
                                            match publisher_rtcp_sender.send(Box::new(rr)) {
                                                Ok(_) => tracing::trace!("send rtcp: rr"),
                                                Err(err) => tracing::error!("failed to send rtcp rr: {}", err),
                                            }
                                        }
                                    }
                                    PacketType::PayloadSpecificFeedback => match header.count {
                                        FORMAT_PLI => {
                                            if let Some(_pli) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication>() {
                                                match publisher_rtcp_sender.send(Box::new(PictureLossIndication {
                                                    sender_ssrc: 0,
                                                    media_ssrc: original_track_ssrc,
                                                })) {
                                                    Ok(_) => tracing::trace!("send rtcp: pli"),
                                                    Err(err) => tracing::error!("failed to send rtcp pli: {}", err)
                                                }
                                            }
                                        }
                                        FORMAT_REMB => {
                                            if let Some(remb) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate>() {

                                                let mut remb = remb.clone();
                                                let diff = Utc::now() - start_timestamp;
                                                if diff.num_seconds() < 30 {
                                                    // Min bitrate is 128kbps if it is video and first 30seconds.
                                                    match media_type {
                                                        MediaType::Video => {
                                                            if remb.bitrate < 128000.0 {
                                                                remb.bitrate = 128000.0;
                                                            }
                                                        }
                                                        MediaType::Audio => {
                                                            if remb.bitrate < 64000.0 {
                                                                remb.bitrate = 640000.0
                                                            }
                                                        }
                                                    }
                                                }

                                                match publisher_rtcp_sender.send(Box::new(remb)) {
                                                    Ok(_) => tracing::trace!("send rtcp: remb"),
                                                    Err(err) => tracing::error!("failed to send rtcp remb: {}", err)
                                                }
                                            }
                                        }
                                        _ => {}
                                    },
                                    _ => {}
                                }
                            }

                        }
                        Err(err) => {
                            tracing::error!("failed to read rtcp for track_id={}: {}", track_id, err);
                        }
                    }
                }
            }
        }

        tracing::debug!(
            "Subscriber RTCP event loop for track_id={} finished",
            track_id
        );
    }

    pub async fn close(&self) {
        if !self.closed_sender.is_closed() {
            self.closed_sender.send(true).unwrap();
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        tracing::debug!("Subscriber id={} is dropped", self.id);
    }
}
