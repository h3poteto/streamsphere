use crate::error::{Error, TransportErrorKind};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::transport::Transport;

#[derive(Clone)]
pub struct Publisher {
    pub id: String,
    transport: Arc<Transport>,
    pub published_receiver: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
}

impl Publisher {
    pub async fn new(transport: Arc<Transport>) -> Arc<Publisher> {
        let id = Uuid::new_v4().to_string();
        let (published_sender, published_receiver) = mpsc::unbounded_channel();
        transport.set_published_sender(published_sender).await;
        let publisher = Publisher {
            id,
            transport,
            published_receiver: Arc::new(Mutex::new(published_receiver)),
        };
        Arc::new(publisher)
    }

    pub async fn connect(
        &self,
        sdp: RTCSessionDescription,
    ) -> Result<RTCSessionDescription, Error> {
        let answer = self.get_answer_for_offer(sdp).await?;
        Ok(answer)
    }

    async fn get_answer_for_offer(
        &self,
        offer: RTCSessionDescription,
    ) -> Result<RTCSessionDescription, Error> {
        tracing::debug!("publisher set remote description");
        self.transport.set_remote_description(offer).await?;
        let answer = self.transport.create_answer(None).await?;
        self.transport.set_local_description(answer).await?;
        match self.transport.local_description().await? {
            Some(answer) => Ok(answer),
            None => Err(Error::new_transport(
                "Failed to set local description".to_string(),
                TransportErrorKind::LocalDescriptionError,
            )),
        }
    }
}
