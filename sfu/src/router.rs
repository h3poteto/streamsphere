use std::{collections::HashMap, sync::Arc};

use crate::{
    config::{MediaConfig, WebRTCTransportConfig},
    media_track::MediaTrack,
    publish_transport::PublishTransport,
    subscribe_transport::SubscribeTransport,
};
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct Router {
    pub id: String,
    tracks: HashMap<String, Arc<MediaTrack>>,
    subscribers: HashMap<String, Arc<SubscribeTransport>>,
    router_event_sender: mpsc::UnboundedSender<RouterEvent>,
    media_config: MediaConfig,
}

impl Router {
    pub fn new(media_config: MediaConfig) -> Arc<Mutex<Router>> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<RouterEvent>();

        let r = Router {
            id: id.clone(),
            tracks: HashMap::new(),
            subscribers: HashMap::new(),
            router_event_sender: tx,
            media_config,
        };

        tracing::trace!("Router {} is created", id);

        let router = Arc::new(Mutex::new(r));
        let copied = Arc::clone(&router);
        tokio::spawn(async move {
            Router::router_event_loop(id, copied, rx).await;
        });

        router
    }

    pub fn track_ids(&self) -> Vec<String> {
        self.tracks.clone().into_iter().map(|(k, _)| k).collect()
    }

    pub async fn create_publish_transport(
        &self,
        transport_config: WebRTCTransportConfig,
    ) -> PublishTransport {
        let tx = self.router_event_sender.clone();
        PublishTransport::new(tx, self.media_config.clone(), transport_config).await
    }

    pub async fn create_subscribe_transport(
        &self,
        transport_config: WebRTCTransportConfig,
    ) -> Arc<SubscribeTransport> {
        let tx = self.router_event_sender.clone();
        SubscribeTransport::new(tx, self.media_config.clone(), transport_config).await
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
                    r.tracks.insert(track_id, Arc::new(track));
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
                RouterEvent::GetMediaTrack(track_id, reply_sender) => {
                    let r = router.lock().await;
                    let track = r.tracks.get(&track_id);
                    let data = track.cloned();
                    let _ = reply_sender.send(data);
                }
                RouterEvent::Closed => {
                    break;
                }
            }
        }
        tracing::info!("Router {} event loop finished", id);
    }

    pub fn close(&self) {
        let _ = self.router_event_sender.send(RouterEvent::Closed);
    }
}

pub enum RouterEvent {
    TrackPublished(MediaTrack),
    TrackRemoved(String),
    SubscriberAdded(Arc<SubscribeTransport>),
    SubscriberRemoved(String),
    GetMediaTrack(String, oneshot::Sender<Option<Arc<MediaTrack>>>),
    Closed,
}

impl Drop for Router {
    fn drop(&mut self) {
        tracing::trace!("Router {} is dropped", self.id);
    }
}
