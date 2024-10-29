let publishPeer: RTCPeerConnection;
let subscribePeer: RTCPeerConnection;

let localVideo: HTMLVideoElement;
let remoteVideo: HTMLVideoElement;
let localStream: MediaStream;

let connectButton: HTMLButtonElement;
let captureButton: HTMLButtonElement;
let stopButton: HTMLButtonElement;

let ws: WebSocket;

let queue: Array<any> = [];

const offerOptions: RTCOfferOptions = {
  offerToReceiveVideo: false,
  offerToReceiveAudio: false,
};

const peerConnectionConfig: RTCConfiguration = {
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};

export function setup() {
  localVideo = document.getElementById("localVideo") as HTMLVideoElement;
  remoteVideo = document.getElementById("remoteVideo") as HTMLVideoElement;

  connectButton = document.getElementById("connect") as HTMLButtonElement;
  captureButton = document.getElementById("capture") as HTMLButtonElement;
  stopButton = document.getElementById("stop") as HTMLButtonElement;
  captureButton.disabled = true;
  stopButton.disabled = true;
  captureButton.addEventListener("click", capture);
  connectButton.addEventListener("click", connect);
  stopButton.addEventListener("click", disconnect);
}

async function connect() {
  console.log("Starting connection");
  ws = new WebSocket("ws://localhost:4000/socket?room=example");
  ws.onopen = () => {
    console.log("Connected to server");
    connectButton.disabled = true;
    captureButton.disabled = false;
    stopButton.disabled = false;
    startPublishPeer();
    startSubscribePeer();
  };
  ws.close = () => {
    console.log("Disconnected from server");
  };
  ws.onmessage = messageHandler;
  setInterval(() => {
    if (ws.readyState === ws.OPEN) {
      ws.send(JSON.stringify({ action: "Ping" }));
    }
  }, 5000);
}

async function capture() {
  console.log("Requesting local stream");
  captureButton.disabled = true;
  try {
    const stream = await navigator.mediaDevices.getDisplayMedia({
      video: true,
      audio: false,
    });
    console.log("Received local stream");
    localVideo.srcObject = stream;
    localStream = stream;
    ws.send(JSON.stringify({ action: "RequestPublish" }));
  } catch (e) {
    console.error("Error accessing media devices.", e);
  }
}

async function disconnect() {
  console.log("Stopping");
  close();
}

function close() {
  ws.send(JSON.stringify({ action: "Close" }));
  publishPeer?.close();
  subscribePeer?.close();
  localStream?.getTracks().forEach((track) => track.stop());
  ws.close();
  stopButton.disabled = true;
  captureButton.disabled = true;
  connectButton.disabled = false;
}

function startPublishPeer() {
  if (!publishPeer) {
    publishPeer = new RTCPeerConnection(peerConnectionConfig);
    ws.send(JSON.stringify({ action: "PublisherInit" }));
    publishPeer.onicecandidate = (event) => {
      if (event.candidate) {
        ws.send(
          JSON.stringify({
            action: "PublisherIce",
            candidate: event.candidate,
          }),
        );
      }
    };
  }
}

function startSubscribePeer() {
  if (!subscribePeer) {
    queue = new Array();
    subscribePeer = new RTCPeerConnection(peerConnectionConfig);
    ws.send(JSON.stringify({ action: "SubscriberInit" }));
    subscribePeer.onicecandidate = (event) => {
      if (event.candidate) {
        ws.send(
          JSON.stringify({
            action: "SubscriberIce",
            candidate: event.candidate,
          }),
        );
      }
    };

    subscribePeer.ontrack = (event) => {
      console.log("Received remote stream");
      remoteVideo.srcObject = event.streams[0];
    };
  }
}

function publish() {
  if (localStream) {
    localStream.getTracks().forEach((track) => {
      publishPeer.addTrack(track, localStream);
    });
  }
  publishPeer
    .createOffer(offerOptions)
    .then(async (offer) => {
      return publishPeer.setLocalDescription(offer).then(() => {
        ws.send(
          JSON.stringify({
            action: "Offer",
            sdp: offer,
          }),
        );
      });
    })
    .then(() => {
      localStream.getTracks().forEach((track) => {
        ws.send(JSON.stringify({ action: "Publish", trackId: track.id }));
      });
    })
    .catch((err) => console.error("Error creating offer: ", err));
}

function messageHandler(event: MessageEvent) {
  console.debug("Received message: ", event.data);
  const message = JSON.parse(event.data);
  switch (message.action) {
    case "StartAsPublisher":
      publish();
      break;
    case "Offer":
      subscribePeer
        .setRemoteDescription(message.sdp)
        .then(() => subscribePeer.createAnswer())
        .then(async (answer) => {
          return subscribePeer.setLocalDescription(answer).then(() => {
            ws.send(JSON.stringify({ action: "Answer", sdp: answer }));
          });
        })
        .catch((err) => console.error(err));
      break;
    case "Answer":
      publishPeer
        .setRemoteDescription(message.sdp)
        .catch((err) => console.error(err));
      break;
    case "SubscriberIce":
      if (subscribePeer.remoteDescription) {
        subscribePeer
          .addIceCandidate(new RTCIceCandidate(message.candidate))
          .catch((err) => console.error("Error adding ice candidate: ", err));
      } else {
        queue.push(event);
        return;
      }
      break;
    case "PublisherIce":
      publishPeer
        .addIceCandidate(new RTCIceCandidate(message.candidate))
        .catch((err) => console.error("Error adding ice candidate: ", err));
      break;
    case "Published":
      ws.send(
        JSON.stringify({
          action: "Subscribe",
          trackId: message.trackId,
        }),
      );
      break;
    case "Close":
      close();
      return;
    case "Pong":
      console.debug("pong");
      break;
    default:
      console.error("Unknown message type: ", message);
      break;
  }

  if (queue.length > 0 && subscribePeer.remoteDescription) {
    messageHandler(queue.shift());
  }
}
