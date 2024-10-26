use crate::error::{Error, PublisherErrorKind, TransportErrorKind};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::transport::Transport;

#[derive(Clone)]
pub struct Publisher {
    pub id: String,
    transport: Arc<Transport>,
    published_receiver: Arc<Mutex<broadcast::Receiver<String>>>,
}

impl Publisher {
    pub async fn new(transport: Arc<Transport>) -> Arc<Publisher> {
        let id = Uuid::new_v4().to_string();
        let published_receiver = transport.get_published_receiver();
        let publisher = Publisher {
            id,
            transport,
            published_receiver: Arc::new(Mutex::new(published_receiver)),
        };
        Arc::new(publisher)
    }

    pub async fn get_answer(
        &self,
        sdp: RTCSessionDescription,
    ) -> Result<RTCSessionDescription, Error> {
        let answer = self.get_answer_for_offer(sdp).await?;
        Ok(answer)
    }

    pub async fn publish(&self, track_id: String) -> Result<String, Error> {
        let receiver = self.published_receiver.clone();
        while let Ok(published_id) = receiver.lock().await.recv().await {
            if published_id == track_id {
                return Ok(published_id);
            }
        }
        Err(Error::new_publisher(
            "Failed to get published track".to_string(),
            PublisherErrorKind::TrackNotPublishedError,
        ))
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
