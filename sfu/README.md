# Rheomesh
Rheomesh is a WebRTC SFU library that provides a simple API for building real-time communication applications. This provides an SDK to help you build a WebRTC SFU server.
[Here](examples/media_server.rs) is an example SFU server for video streaming.

## Usage
Add your `Cargo.toml` like this:
```
[dependencies]
rheomesh = { version = "0" }
```

### Create router and transports
First of all, please create a router. Router accommodates multiple transports and they can communicate with each other. That means transports belonging to the same Router can send/receive their media. Router is like a meeting room.

```rust
use rheomesh::config::MediaConfig;
use rheomesh::router::Router;

//...

async fn new() {
  let config = MediaConfig::default();
  let router = Router::new(config);
}
```

Next, please create publish and subscribe transports.

```rust
use rheomesh::config::WebRTCTransportConfig

async fn new() {
  //...
  let mut config = WebRTCTransportConfig::default();
  config.configuration.ice_servers = vec![RTCIceServer {
    urls: vec!["stun:stun.l.google.com:19302".to_owned()],
    ..Default::default()
  }];
  let publish_transport = router.create_publish_transport(config.clone()).await;
  let subscribe_transport = router.create_subscribe_transport(config.clone()).await;
}
```

### Handle publish events
#### Bind `on_ice_candidate` callback
```rust
publish_transport
  .on_ice_candidate(Box::new(move |candidate| {
      let init = candidate.to_json().expect("failed to parse candidate");
      // Send `init` message to client. The client have to call `addIceCandidate` method with this parameter.
  }))
  .await;
```
#### Handle `RTCIceCandidateInit`
On the other hand, you will receive `RTCIceCandidateInit` message from client when client-side have `onicecandidate` events.
```rust
let _ = publish_transport
  .add_ice_candidate(candidate)
  .await
  .expect("failed to add ICE candidate");
```
#### Handle `offer` message
Then, client sends offer to publish.
```rust
let answer = publish_transport
  .get_answer(offer)
  .await
  .expect("failed to connect publish_transport");
// Send `answer` message to client. The client have to call `setAnswer` method.
```
#### Publish
Finally, please handle publish event with `track_id`.
```rust
let publisher = publish_transport.publish(track_id).await;
```

### Handle subscribe events
#### Bind `on_ice_candidate` and `on_negotiation_needed` callback
```rust
subscribe_transport
  .on_ice_candidate(Box::new(move |candidate| {
      let init = candidate.to_json().expect("failed to parse candidate");
      // Send `init` message to client. The client have to call `addIceCandidate` method with this parameter.
  }))
    .await;
subscribe_transport
  .on_negotiation_needed(Box::new(move |offer| {
    // Send `offer` message to client. The client have to call `setOffer` method.
  }))
```
#### Handle `RTCIceCandidateInit`
On the other hand, you will receive `RTCIceCandidateInit` message from client when client-side have `onicecandidate` events.
```rust
let _ = subscribe_transport
  .add_ice_candidate(candidate)
  .await
  .expect("failed to add ICE candidate");
```
#### Subscribe
Then, please call `subscribe` method.
```rust
let (subscriber, offer) = subscribe_transport
  .subscribe(track_id)
  .await
  .expect("failed to connect subscribe_transport");
// Send `offer` message to client. The client have to call `setOffer` method.
```
#### Handle `answer` message
Finally, you will receive `answer` from client after client-side calls `setOffer` method.
```rust
let _ = subscribe_transport
  .set_answer(answer)
  .await
  .expect("failed to set answer");
```