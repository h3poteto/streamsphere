use std::sync::Arc;

use chrono::Utc;
use enclose::enc;
use tokio::sync::{broadcast, mpsc, Mutex};
use uuid::Uuid;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::{
    rtcp::{
        self,
        header::{PacketType, FORMAT_PLI, FORMAT_REMB},
        payload_feedbacks::picture_loss_indication::PictureLossIndication,
    },
    rtp,
    rtp_transceiver::rtp_sender::RTCRtpSender,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    publisher::{detect_mime_type, MediaType},
    transport,
};

#[derive(Clone)]
pub struct Subscriber {
    pub id: String,
    closed_sender: Arc<mpsc::UnboundedSender<bool>>,
}

impl Subscriber {
    pub fn new(
        rtcp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
        mime_type: String,
        media_ssrc: u32,
        local_track: Arc<TrackLocalStaticRTP>,
        rtp_buffer: broadcast::Sender<rtp::packet::Packet>,
        track_id: String,
        transport_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
        peer: Arc<RTCPeerConnection>,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let closed_receiver = Arc::new(Mutex::new(rx));
        let subscriber = Self {
            id,
            closed_sender: Arc::new(tx),
        };

        tokio::spawn(enc!((rtcp_sender, publisher_rtcp_sender) async move {
            Self::rtcp_event_loop(rtcp_sender, publisher_rtcp_sender, mime_type, media_ssrc).await;
        }));

        tokio::spawn(enc!((local_track, rtp_buffer) async move{
            let receiver = rtp_buffer.subscribe();
            // We have to drop sender to receive close event when the track has been closed.
            drop(rtp_buffer);

            Self::rtp_event_loop(track_id, local_track, receiver, transport_closed, closed_receiver).await;
            // Upstream track has been closed, so we should remove subscribed track from peer connection.
            let _ = peer.remove_track(&rtcp_sender).await;
        }));

        tracing::debug!("Subscriber {} is created", subscriber.id);
        subscriber
    }

    pub async fn rtcp_event_loop(
        rtcp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
        mime_type: String,
        media_ssrc: u32,
    ) {
        let media_type = detect_mime_type(mime_type);
        let start_timestamp = Utc::now();

        while let Ok((rtcp_packets, attr)) = rtcp_sender.read_rtcp().await {
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
                                    media_ssrc,
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

        tracing::debug!("Subscriber RTCP event loop finished");
    }

    pub async fn rtp_event_loop(
        track_id: String,
        local_track: Arc<TrackLocalStaticRTP>,
        mut rtp_receiver: broadcast::Receiver<rtp::packet::Packet>,
        transport_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
        subscriber_closed: Arc<Mutex<mpsc::UnboundedReceiver<bool>>>,
    ) {
        tracing::debug!("Subscriber RTP event loop has started for {}", track_id);

        let mut curr_timestamp = 0;

        loop {
            let mut transport_closed = transport_closed.lock().await;
            let mut subscriber_closed = subscriber_closed.lock().await;
            tokio::select! {
                _closed = transport_closed.recv() => {
                    break;
                }
                _closed = subscriber_closed.recv() => {
                    break;
                }
                res = rtp_receiver.recv() => {
                    match res {
                        Ok(mut packet) => {
                            curr_timestamp += packet.header.timestamp;
                            packet.header.timestamp = curr_timestamp;

                            tracing::trace!(
                                "Subscriber write RTP ssrc={} seq={} timestamp={}",
                                packet.header.ssrc,
                                packet.header.sequence_number,
                                packet.header.timestamp
                            );

                            if let Err(err) = local_track.write_rtp(&packet).await {
                                tracing::error!("Subscriber failed to write rtp: {}", err);
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break;
                        }
                        Err(err) => {
                            tracing::error!("Subscriber failed to receive rtp: {}", err);
                            break;
                        }
                    }

                }
            }
        }

        tracing::debug!("Subscriber RTP event loop has finished for {}", track_id);
    }

    pub async fn close(&self) {
        if !self.closed_sender.is_closed() {
            self.closed_sender.send(true).unwrap();
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        tracing::debug!("Subscriber {} is dropped", self.id);
    }
}
