use std::{collections::HashMap, sync::Arc};

use crate::{
    config::WebRTCTransportConfig, media_track::MediaTrack, subscriber::Subscriber,
    transport::Transport,
};
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct Router {
    pub id: String,
    tracks: HashMap<String, MediaTrack>,
    subscribers: HashMap<String, Arc<Subscriber>>,
    router_event_sender: mpsc::UnboundedSender<RouterEvent>,
}

impl Router {
    pub fn new() -> Arc<Mutex<Router>> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<RouterEvent>();

        let r = Router {
            id: id.clone(),
            tracks: HashMap::new(),
            subscribers: HashMap::new(),
            router_event_sender: tx,
        };

        let router = Arc::new(Mutex::new(r));
        let copied = Arc::clone(&router);
        tokio::spawn(async move {
            Router::router_event_loop(id, copied, rx).await;
        });

        router
    }

    pub async fn create_transport(&self, transport_config: WebRTCTransportConfig) -> Transport {
        let tx = self.router_event_sender.clone();
        Transport::new(tx, transport_config).await
    }

    pub async fn router_event_loop(
        id: String,
        router: Arc<Mutex<Router>>,
        mut event_receiver: mpsc::UnboundedReceiver<RouterEvent>,
    ) {
        while let Some(event) = event_receiver.recv().await {
            match event {
                RouterEvent::TrackPublished(track) => {
                    let mut r = router.lock().await;
                    let track_id = track.id.clone();
                    r.tracks.insert(track_id, track);
                }
                RouterEvent::TrackRemoved(track_id) => {
                    let mut r = router.lock().await;
                    r.tracks.remove(&track_id);
                }
                RouterEvent::SubscriberAdded(subscriber) => {
                    let mut r = router.lock().await;
                    let subscriber_id = subscriber.id.clone();
                    r.subscribers.insert(subscriber_id, subscriber);
                }
                RouterEvent::SubscriberRemoved(subscriber_id) => {
                    let mut r = router.lock().await;
                    r.subscribers.remove(&subscriber_id);
                }
            }
        }
        tracing::info!("Router {} event loop finished", id);
    }
}

pub enum RouterEvent {
    TrackPublished(MediaTrack),
    TrackRemoved(String),
    SubscriberAdded(Arc<Subscriber>),
    SubscriberRemoved(String),
}
