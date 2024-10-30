use std::collections::HashMap;
use std::sync::Arc;

use actix::{Actor, Addr, Message, StreamHandler};
use actix::{AsyncContext, Handler};
use actix_web::web::{Data, Query};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
use streamsphere::transport::Transport;
use tokio::sync::Mutex;
use tracing_actix_web::TracingLogger;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let room_owner = RoomOwner::new();
    let room_data = Data::new(Mutex::new(room_owner));

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .service(index)
            .app_data(room_data.clone())
            .route("/socket", web::get().to(socket))
    })
    .bind("0.0.0.0:4000")?
    .run()
    .await
}

#[actix_web::get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("healthy")
}

async fn socket(
    req: HttpRequest,
    room_owner: Data<Mutex<RoomOwner>>,
    stream: web::Payload,
) -> impl Responder {
    let query = req.query_string();

    let parameters =
        Query::<HashMap<String, String>>::from_query(query).expect("Failed to parse query");
    let room_id = parameters.get("room").expect("room is required");
    let find = room_owner
        .as_ref()
        .lock()
        .await
        .find_by_id(room_id.to_string());

    match find {
        Some(room) => {
            tracing::info!("Room found, so joining it: {}", room_id);
            let server = WebSocket::new(room).await;
            ws::start(server, &req, stream)
        }
        None => {
            let owner = room_owner.clone();
            let mut owner = owner.lock().await;
            let router = streamsphere::router::Router::new();
            let room = owner.create_new_room(room_id.to_string(), router).await;
            let server = WebSocket::new(room).await;
            ws::start(server, &req, stream)
        }
    }
}

struct WebSocket {
    room: Arc<Room>,
    publisher: Arc<streamsphere::publisher::PublishTransport>,
    subscriber: Arc<streamsphere::subscriber::SubscribeTransport>,
}

impl WebSocket {
    // This function is called when a new user connect to this server.
    pub async fn new(room: Arc<Room>) -> Self {
        tracing::info!("Starting WebSocket");
        let r = room.router.clone();
        let router = r.lock().await;
        let config = streamsphere::config::WebRTCTransportConfig::default();
        let publisher = router.create_publish_transport(config.clone()).await;
        let subscriber = router.create_subscribe_transport(config).await;
        Self {
            room,
            publisher: Arc::new(publisher),
            subscriber,
        }
    }
}

impl Actor for WebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("New WebSocket connection is started");
        let address = ctx.address();
        self.room.add_user(address.clone());
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        tracing::info!("The WebSocket connection is stopped");
        let address = ctx.address();
        let subscriber = self.subscriber.clone();
        let publisher = self.publisher.clone();
        actix::spawn(async move {
            subscriber
                .close()
                .await
                .expect("failed to close subscriber");
            publisher.close().await.expect("failed to close publisher");
        });
        self.room.remove_user(address);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocket {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => tracing::info!("pong received"),
            Ok(ws::Message::Text(text)) => match serde_json::from_str::<ReceivedMessage>(&text) {
                Ok(message) => {
                    ctx.address().do_send(message);
                }
                Err(error) => {
                    tracing::error!("failed to parse client message: {}\n{}", error, text);
                }
            },
            Ok(ws::Message::Binary(bin)) => ctx.binary(bin),
            Ok(ws::Message::Close(reason)) => ctx.close(reason),
            _ => (),
        }
    }
}

impl Handler<ReceivedMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, msg: ReceivedMessage, ctx: &mut Self::Context) -> Self::Result {
        let address = ctx.address();
        tracing::debug!("received message: {:?}", msg);

        match msg {
            ReceivedMessage::Ping => {
                address.do_send(SendingMessage::Pong);
            }
            ReceivedMessage::PublisherInit => {
                let publisher = self.publisher.clone();
                tokio::spawn(async move {
                    let addr = address.clone();
                    publisher
                        .on_ice_candidate(Box::new(move |candidate| {
                            let init = candidate.to_json().expect("failed to parse candidate");
                            addr.do_send(SendingMessage::PublisherIce { candidate: init });
                        }))
                        .await;
                });
            }
            ReceivedMessage::SubscriberInit => {
                let subscriber = self.subscriber.clone();
                let room = self.room.clone();
                tokio::spawn(async move {
                    let addr = address.clone();
                    let addr2 = address.clone();
                    subscriber
                        .on_ice_candidate(Box::new(move |candidate| {
                            let init = candidate.to_json().expect("failed to parse candidate");
                            addr.do_send(SendingMessage::SubscriberIce { candidate: init });
                        }))
                        .await;
                    subscriber
                        .on_negotiation_needed(Box::new(move |offer| {
                            addr2.do_send(SendingMessage::Offer { sdp: offer });
                        }))
                        .await;

                    let router = room.router.lock().await;
                    let ids = router.track_ids();
                    tracing::info!("router track ids {:#?}", ids);
                    ids.iter().for_each(|id| {
                        address.do_send(SendingMessage::Published {
                            track_id: id.to_string(),
                        });
                    });
                });
            }

            ReceivedMessage::RequestPublish => address.do_send(SendingMessage::StartAsPublisher),
            ReceivedMessage::PublisherIce { candidate } => {
                let publisher = self.publisher.clone();
                actix::spawn(async move {
                    let _ = publisher
                        .add_ice_candidate(candidate)
                        .await
                        .expect("failed to add ICE candidate");
                });
            }
            ReceivedMessage::SubscriberIce { candidate } => {
                let subscriber = self.subscriber.clone();
                actix::spawn(async move {
                    let _ = subscriber
                        .add_ice_candidate(candidate)
                        .await
                        .expect("failed to add ICE candidate");
                });
            }
            ReceivedMessage::Offer { sdp } => {
                let publisher = self.publisher.clone();
                actix::spawn(async move {
                    let answer = publisher
                        .get_answer(sdp)
                        .await
                        .expect("failed to connect publishder");

                    address.do_send(SendingMessage::Answer { sdp: answer });
                });
            }
            ReceivedMessage::Subscribe { track_id } => {
                let subscriber = self.subscriber.clone();

                actix::spawn(async move {
                    let offer = subscriber
                        .subscribe(track_id)
                        .await
                        .expect("failed to connect subscriber");

                    address.do_send(SendingMessage::Offer { sdp: offer });
                });
            }
            ReceivedMessage::Answer { sdp } => {
                let subscriber = self.subscriber.clone();
                actix::spawn(async move {
                    let _ = subscriber
                        .set_answer(sdp)
                        .await
                        .expect("failed to set answer");
                });
            }
            ReceivedMessage::Publish { track_id } => {
                let room = self.room.clone();
                let publisher = self.publisher.clone();
                actix::spawn(async move {
                    match publisher.publish(track_id).await {
                        Ok(id) => {
                            tracing::debug!("published a track: {}", id);
                            // address.do_send(SendingMessage::Published {
                            //     track_id: id.clone(),
                            // });
                            room.get_peers(&address).iter().for_each(|peer| {
                                peer.do_send(SendingMessage::Published {
                                    track_id: id.clone(),
                                });
                            });
                        }
                        Err(err) => {
                            tracing::error!("{}", err);
                        }
                    }
                });
            }
        }
    }
}

impl Handler<SendingMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, msg: SendingMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::debug!("sending message: {:?}", msg);
        ctx.text(serde_json::to_string(&msg).expect("failed to parse SendingMessage"));
    }
}

impl Handler<InternalMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, _msg: InternalMessage, _ctx: &mut Self::Context) -> Self::Result {}
}

#[derive(Deserialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum ReceivedMessage {
    #[serde(rename_all = "camelCase")]
    Ping,
    #[serde(rename_all = "camelCase")]
    PublisherInit,
    #[serde(rename_all = "camelCase")]
    SubscriberInit,
    #[serde(rename_all = "camelCase")]
    RequestPublish,
    // Seems like client-side (JS) RTCIceCandidate struct is equal RTCIceCandidateInit.
    #[serde(rename_all = "camelCase")]
    PublisherIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    SubscriberIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    Offer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Subscribe { track_id: String },
    #[serde(rename_all = "camelCase")]
    Answer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Publish { track_id: String },
}

#[derive(Serialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum SendingMessage {
    #[serde(rename_all = "camelCase")]
    Pong,
    #[serde(rename_all = "camelCase")]
    StartAsPublisher,
    #[serde(rename_all = "camelCase")]
    Answer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    Offer { sdp: RTCSessionDescription },
    #[serde(rename_all = "camelCase")]
    PublisherIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    SubscriberIce { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    Published { track_id: String },
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
enum InternalMessage {}

pub struct RoomOwner {
    rooms: HashMap<String, Arc<Room>>,
}

impl RoomOwner {
    pub fn new() -> Self {
        RoomOwner {
            rooms: HashMap::<String, Arc<Room>>::new(),
        }
    }

    fn find_by_id(&self, id: String) -> Option<Arc<Room>> {
        self.rooms.get(&id).cloned()
    }

    async fn create_new_room(
        &mut self,
        id: String,
        router: Arc<Mutex<streamsphere::router::Router>>,
    ) -> Arc<Room> {
        let room = Room::new(id.clone(), router);
        let a = Arc::new(room);
        self.rooms.insert(id.clone(), a.clone());
        a
    }
}

struct Room {
    _id: String,
    pub router: Arc<Mutex<streamsphere::router::Router>>,
    users: std::sync::Mutex<Vec<Addr<WebSocket>>>,
}

impl Room {
    pub fn new(_id: String, router: Arc<Mutex<streamsphere::router::Router>>) -> Self {
        Self {
            _id,
            router,
            users: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn add_user(&self, user: Addr<WebSocket>) {
        let mut users = self.users.lock().unwrap();
        users.push(user);
    }

    pub fn remove_user(&self, user: Addr<WebSocket>) {
        let mut users = self.users.lock().unwrap();
        users.retain(|u| u != &user);
    }

    pub fn get_peers(&self, user: &Addr<WebSocket>) -> Vec<Addr<WebSocket>> {
        let users = self.users.lock().unwrap();
        users.iter().filter(|u| u != &user).cloned().collect()
    }
}
