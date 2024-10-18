use std::collections::HashMap;
use std::sync::Arc;

use actix::{Actor, Message, StreamHandler};
use actix::{AsyncContext, Handler};
use actix_web::web::{Data, Query};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use serde::{Deserialize, Serialize};
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
    transport: Arc<streamsphere::transport::Transport>,
    publisher: Arc<streamsphere::publisher::Publisher>,
    subscriber: Arc<streamsphere::subscriber::Subscriber>,
}

impl WebSocket {
    pub async fn new(room: Arc<Room>) -> Self {
        let r = room.router.clone();
        let router = r.lock().await;
        let config = streamsphere::config::WebRTCTransportConfig::default();
        let transport = router.create_transport(config).await;
        let arc = Arc::new(transport);
        let publisher = streamsphere::publisher::Publisher::new(arc.clone());
        let subscriber = streamsphere::subscriber::Subscriber::new(arc.clone());
        Self {
            room,
            transport: arc,
            publisher,
            subscriber,
        }
    }
}

impl Actor for WebSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("WebSocket started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("WebSocket stopped");
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
            ReceivedMessage::Ice { candidate } => {
                let transport = self.transport.clone();
                actix::spawn(async move {
                    let _ = transport
                        .add_ice_candidate(candidate)
                        .await
                        .expect("failed to add ICE candidate");
                });
            }
            ReceivedMessage::Offer { sdp } => {
                let publisher = self.publisher.clone();
                actix::spawn(async move {
                    let answer = publisher
                        .connect(sdp)
                        .await
                        .expect("failed to connect publishder");

                    address.do_send(SendingMessage::Answer { sdp: answer });
                });
            }
        }
    }
}

impl Handler<SendingMessage> for WebSocket {
    type Result = ();

    fn handle(&mut self, msg: SendingMessage, ctx: &mut Self::Context) -> Self::Result {
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
    Ice { candidate: RTCIceCandidateInit },
    #[serde(rename_all = "camelCase")]
    Offer { sdp: RTCSessionDescription },
}

#[derive(Serialize, Message, Debug)]
#[serde(tag = "action")]
#[rtype(result = "()")]
enum SendingMessage {
    #[serde(rename_all = "camelCase")]
    Pong,
    #[serde(rename_all = "camelCase")]
    Answer { sdp: RTCSessionDescription },
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
    id: String,
    pub router: Arc<Mutex<streamsphere::router::Router>>,
}

impl Room {
    pub fn new(id: String, router: Arc<Mutex<streamsphere::router::Router>>) -> Self {
        Self { id, router }
    }
}
