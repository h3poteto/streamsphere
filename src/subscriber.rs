use std::sync::Arc;

use enclose::enc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use webrtc::{
    peer_connection::sdp::session_description::RTCSessionDescription,
    rtcp,
    rtcp::header::{PacketType, FORMAT_PLI},
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
}

impl Subscriber {
    pub fn new(transport: Arc<Transport>) -> Arc<Subscriber> {
        let id = Uuid::new_v4().to_string();
        let sender = transport.router_event_sender.clone();
        let subscriber = Subscriber {
            id,
            transport: Arc::clone(&transport),
            router_event_sender: sender,
        };

        let subscriber = Arc::new(subscriber);
        let copied = Arc::clone(&subscriber);
        let sender = subscriber.router_event_sender.clone();
        let _ = sender.send(RouterEvent::SubscriberAdded(copied));

        subscriber
    }

    pub async fn connect(&self) -> Result<RTCSessionDescription, Error> {
        let offer = self.create_offer().await?;
        Ok(offer)
    }

    async fn create_offer(&self) -> Result<RTCSessionDescription, Error> {
        tracing::debug!("subscriber creates offer");
        let offer = self.transport.create_offer(None).await?;
        let mut offer_gathering_complete = self.transport.gathering_complete_promise().await?;
        self.transport.set_local_description(offer).await?;
        let _ = offer_gathering_complete.recv().await;

        match self.transport.local_description().await? {
            Some(offer) => Ok(offer),
            None => Err(Error::new_transport(
                "Failed to set local description".to_string(),
                crate::error::TransportErrorKind::LocalDescriptionError,
            )),
        }
    }

    // TODO: when should we call this method? When subscriber replies answer?
    pub async fn set_answer(&self, answer: RTCSessionDescription) -> Result<(), Error> {
        tracing::debug!("subscriber set answer");
        self.transport.set_remote_description(answer).await?;

        // Perhaps, we need to run add_ice_candidate for delay tricle ice, like
        // https://github.com/billylindeman/switchboard/blob/94295c082be25f20e4144b29dfbb5a26c2c6c970/switchboard-sfu/src/sfu/peer.rs#L133

        Ok(())
    }

    pub async fn subscribe(&self, track_id: String) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();

        let _ = self
            .router_event_sender
            .send(RouterEvent::GetMediaTrack(track_id.clone(), tx));

        let reply = rx.await.unwrap();
        match reply {
            None => Err(Error::new_subscriber(
                format!("Media track for {} is not found", track_id),
                SubscriberErrorKind::TrackNotFoundError,
            )),
            Some(track) => self.subscribe_track(track).await,
        }
    }

    async fn subscribe_track(&self, media_track: Arc<MediaTrack>) -> Result<(), Error> {
        let publisher_rtcp_sender = media_track.rtcp_sender.clone();
        let ssrc = media_track.track.ssrc();
        let local_track = TrackLocalStaticRTP::new(
            media_track.track.codec().capability,
            media_track.track.id(),
            media_track.track.stream_id(),
        );
        let rtp_sender = self.transport.add_track(Arc::new(local_track)).await?;

        tokio::spawn(enc!((rtp_sender, publisher_rtcp_sender) async move {
            Self::rtcp_event_loop(ssrc, rtp_sender, publisher_rtcp_sender).await;
        }));

        Ok(())
    }

    pub async fn rtcp_event_loop(
        ssrc: u32,
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
                                match publisher_rtcp_sender.send(Box::new(rr)) {
                                    Ok(_) => tracing::trace!("send rtcp"),
                                    Err(err) => tracing::error!("failed to send rtcp: {}", err)
                                }
                            }
                    }
                    PacketType::PayloadSpecificFeedback => match header.count {
                        FORMAT_PLI => {
                            if let Some(pli) = rtcp.as_any().downcast_ref::<rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication>() {
                                let mut pli = pli.clone();
                                pli.media_ssrc = ssrc;
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
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let _ = self
            .router_event_sender
            .send(RouterEvent::SubscriberRemoved(self.id.clone()));
    }
}
