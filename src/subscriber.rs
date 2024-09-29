use std::sync::Arc;

use tokio::sync::mpsc;
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::{error::Error, router::RouterEvent, transport::Transport};

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
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let _ = self
            .router_event_sender
            .send(RouterEvent::SubscriberRemoved(self.id.clone()));
    }
}
