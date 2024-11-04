import { PublishTransport, SubscribeTransport } from "rheomesh";

let publishTransport: PublishTransport;
let subscribeTransport: SubscribeTransport;

let localVideo: HTMLVideoElement;
let remoteVideo: HTMLVideoElement;
let localStream: MediaStream;
let subscriberId: string;

let connectButton: HTMLButtonElement;
let captureButton: HTMLButtonElement;
let stopButton: HTMLButtonElement;

let ws: WebSocket;

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
  stopButton.addEventListener("click", stop);
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
  stopButton.disabled = false;
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

async function stop() {
  console.log("Stopping");
  localStream?.getTracks().forEach((track) => {
    ws.send(JSON.stringify({ action: "StopPublish", publisherId: track.id }));
    track.stop();
  });
  if (subscriberId) {
    ws.send(
      JSON.stringify({
        action: "StopSubscribe",
        subscriberId: subscriberId,
      }),
    );
  }
  stopButton.disabled = true;
  captureButton.disabled = false;
  connectButton.disabled = true;
}

function startPublishPeer() {
  if (!publishTransport) {
    publishTransport = new PublishTransport(peerConnectionConfig);
    ws.send(JSON.stringify({ action: "PublisherInit" }));
    publishTransport.on("icecandidate", (candidate) => {
      ws.send(
        JSON.stringify({
          action: "PublisherIce",
          candidate: candidate,
        }),
      );
    });
  }
}

function startSubscribePeer() {
  if (!subscribeTransport) {
    subscribeTransport = new SubscribeTransport(peerConnectionConfig);
    ws.send(JSON.stringify({ action: "SubscriberInit" }));
    subscribeTransport.on("icecandidate", (candidate) => {
      ws.send(
        JSON.stringify({
          action: "SubscriberIce",
          candidate: candidate,
        }),
      );
    });
  }
}

async function publish() {
  if (localStream) {
    const offer = await publishTransport.publish(localStream);
    ws.send(
      JSON.stringify({
        action: "Offer",
        sdp: offer,
      }),
    );
    localStream.getTracks().forEach((track) => {
      ws.send(JSON.stringify({ action: "Publish", trackId: track.id }));
    });
  }
}

function messageHandler(event: MessageEvent) {
  console.debug("Received message: ", event.data);
  const message = JSON.parse(event.data);
  switch (message.action) {
    case "StartAsPublisher":
      publish();
      break;
    case "Offer":
      subscribeTransport.setOffer(message.sdp).then((answer) => {
        ws.send(JSON.stringify({ action: "Answer", sdp: answer }));
      });
      break;
    case "Answer":
      publishTransport.setAnswer(message.sdp);
      break;
    case "SubscriberIce":
      subscribeTransport.addIceCandidate(message.candidate);
      break;
    case "PublisherIce":
      publishTransport.addIceCandidate(message.candidate);
      break;
    case "Published":
      ws.send(
        JSON.stringify({
          action: "Subscribe",
          publisherId: message.publisherId,
        }),
      );
      subscribeTransport.subscribe(message.publisherId).then((track) => {
        const stream = new MediaStream([track]);
        remoteVideo.srcObject = stream;
      });
      break;
    case "Subscribed":
      subscriberId = message.subscriberId;
      stopButton.disabled = false;
      break;
    case "Pong":
      console.debug("pong");
      break;
    default:
      console.error("Unknown message type: ", message);
      break;
  }
}
