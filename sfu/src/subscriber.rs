use std::sync::Arc;

use enclose::enc;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;
use webrtc::track::track_local::TrackLocalWriter;
use webrtc::{
    peer_connection::{
        offer_answer_options::RTCOfferOptions, sdp::session_description::RTCSessionDescription,
    },
    rtcp::{
        self,
        header::{PacketType, FORMAT_PLI},
    },
    rtp::packet::Packet,
    rtp_transceiver::rtp_sender::RTCRtpSender,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

use crate::{
    error::{Error, SubscriberErrorKind},
    media_track::MediaTrack,
    router::RouterEvent,
    transport::{self, Transport},
};

#[derive(Clone)]
pub struct Subscriber {
    pub id: String,
    transport: Arc<Transport>,
    router_event_sender: mpsc::UnboundedSender<RouterEvent>,
    offer_options: RTCOfferOptions,
}

impl Subscriber {
    pub fn new(transport: Arc<Transport>) -> Arc<Subscriber> {
        let id = Uuid::new_v4().to_string();
        let sender = transport.router_event_sender.clone();
        let subscriber = Subscriber {
            id,
            transport,
            router_event_sender: sender,
            offer_options: RTCOfferOptions {
                ice_restart: false,
                voice_activity_detection: false,
            },
        };

        let subscriber = Arc::new(subscriber);
        let copied = Arc::clone(&subscriber);
        let sender = subscriber.router_event_sender.clone();
        let _ = sender.send(RouterEvent::SubscriberAdded(copied));

        subscriber
    }

    pub async fn subscribe(&self, track_id: String) -> Result<RTCSessionDescription, Error> {
        // We have to add a track before creating offer.
        // https://datatracker.ietf.org/doc/html/rfc3264
        // https://github.com/webrtc-rs/webrtc/issues/115#issuecomment-1958137875
        let (tx, rx) = oneshot::channel();

        let _ = self
            .router_event_sender
            .send(RouterEvent::GetMediaTrack(track_id.clone(), tx));

        let reply = rx.await.unwrap();
        match reply {
            None => {
                return Err(Error::new_subscriber(
                    format!("Media track for {} is not found", track_id),
                    SubscriberErrorKind::TrackNotFoundError,
                ))
            }
            Some(track) => self.subscribe_track(track).await?,
        }

        let offer = self.create_offer().await?;
        Ok(offer)
    }

    async fn create_offer(&self) -> Result<RTCSessionDescription, Error> {
        tracing::debug!("subscriber creates offer");

        let offer = self
            .transport
            .create_offer(Some(self.offer_options.clone()))
            .await?;
        self.transport.set_local_description(offer).await?;

        let receiver = self.transport.ice_gathering_complete_receiver.clone();
        let mut r = receiver.lock().await;
        let _ = r.recv().await;

        match self.transport.local_description().await? {
            Some(offer) => Ok(offer),
            None => Err(Error::new_transport(
                "Failed to set local description".to_string(),
                crate::error::TransportErrorKind::LocalDescriptionError,
            )),
        }
    }

    pub async fn set_answer(&self, answer: RTCSessionDescription) -> Result<(), Error> {
        tracing::debug!("subscriber set answer");
        self.transport.set_remote_description(answer).await?;

        // Perhaps, we need to run add_ice_candidate for delay tricle ice, like
        // https://github.com/billylindeman/switchboard/blob/94295c082be25f20e4144b29dfbb5a26c2c6c970/switchboard-sfu/src/sfu/peer.rs#L133

        Ok(())
    }

    async fn subscribe_track(&self, media_track: Arc<MediaTrack>) -> Result<(), Error> {
        let publisher_rtcp_sender = media_track.rtcp_sender.clone();
        let track_id = media_track.track.id();
        let local_track = TrackLocalStaticRTP::new(
            media_track.track.codec().capability,
            media_track.track.id(),
            media_track.track.stream_id(),
        );

        let local_track = Arc::new(local_track);
        let cloned_track = local_track.clone();
        let rtp_sender = self.transport.add_track(local_track).await?;

        // TODO: when peer connection has been closed, remove track using rtp_sender.

        tokio::spawn(enc!((rtp_sender, publisher_rtcp_sender) async move {
            Self::rtcp_event_loop(rtp_sender, publisher_rtcp_sender).await;
        }));

        let rtp_buffer = media_track.rtp_sender.clone();
        tokio::spawn(enc!((cloned_track, rtp_buffer) async move{
            let receiver = rtp_buffer.subscribe();
            Self::rtp_event_loop(track_id, cloned_track, receiver).await;
        }));

        Ok(())
    }

    pub async fn rtcp_event_loop(
        rtp_sender: Arc<RTCRtpSender>,
        publisher_rtcp_sender: Arc<transport::RtcpSender>,
    ) {
        while let Ok((rtcp_packets, attr)) = rtp_sender.read_rtcp().await {
            for rtcp in rtcp_packets.into_iter() {
                tracing::trace!("Receive RTCP rtcp={:#?}, attr={:#?}", rtcp, attr);

                let header = rtcp.header();
                match header.packet_type {
                    PacketType::ReceiverReport => {
                        if let Some(rr) = rtcp
                            .as_any()
                            .downcast_ref::<rtcp::receiver_report::ReceiverReport>() {
                                let rr = rr.clone();
                                tracing::debug!("Receive RTCP ReceiverReport: {:#?}", rr);
                            }
                    }
                    PacketType::PayloadSpecificFeedback => match header.count {
                        FORMAT_PLI => {
                            if let Some(pli) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication>() {
                                let pli = pli.clone();
                                match publisher_rtcp_sender.send(Box::new(pli)) {
                                    Ok(_) => tracing::trace!("send rtcp"),
                                    Err(err) => tracing::error!("failed to send rtcp: {}", err)
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
        mut rtp_receiver: broadcast::Receiver<Packet>,
    ) {
        tracing::debug!("Subscriber RTP event loop has started for {}", track_id);

        let mut curr_timestamp = 0;
        let mut i = 0;

        while let Ok(mut packet) = rtp_receiver.recv().await {
            curr_timestamp += packet.header.timestamp;
            packet.header.timestamp = curr_timestamp;
            // Keep an increasing sequence number
            packet.header.sequence_number = i;

            tracing::trace!(
                "Subscriber write RTP ssrc={} seq={} timestamp={}",
                packet.header.ssrc,
                packet.header.sequence_number,
                packet.header.timestamp
            );

            if let Err(err) = local_track.write_rtp(&packet).await {
                tracing::error!("Subscriber failed to write rtp: {}", err);
            }
            i += 1;
        }

        tracing::debug!("Subscriber RTP event loop has finished for {}", track_id);
    }

    pub fn close(&self) {
        let _ = self
            .router_event_sender
            .send(RouterEvent::SubscriberRemoved(self.id.clone()));
    }
}
