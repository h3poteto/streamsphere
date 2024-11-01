# Streamsphere
Streamsphere is a WebRTC SFU ([Selective Forwarding Unit](https://bloggeek.me/webrtcglossary/sfu/)) library written by Rust. This provides an SDK to help you build a WebRTC SFU server. And this provides client-side library with TypeScript.


## Current status
This library is under development.


## Example
### Server side
```
$ cd sfu
$ cargo run --example server
```

WebScoket signaling server will launch with `0.0.0.0:4000`.

### Client side
```
$ cd client
$ npm run dev
```

You can access the frontend service with `localhost:5173`.

# License
The software is available as open source under the terms of the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
