use crate::{config::WebRTCTransportConfig, transport::Transport};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct Router {
    pub id: String,
}

impl Router {
    pub fn new() -> Router {
        let id = Uuid::new_v4().to_string();

        Router { id }
    }

    pub async fn create_transport(self, transport_config: WebRTCTransportConfig) -> Transport {
        let router = Arc::new(self.clone());
        Transport::new(router, transport_config).await
    }
}
