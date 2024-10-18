import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

const peerConnectionConfig: RTCConfiguration = {
  iceServers: [
    { urls: "stun:stun.l.google.com:19302" },
    { urls: "stun:stun1.l.google.com:19302" },
    { urls: "stun:stun2.l.google.com:19302" },
  ],
};

const offerOptions: RTCOfferOptions = {
  offerToReceiveVideo: true,
  offerToReceiveAudio: false,
};

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const [publisher, setPublisher] = useState(false);
  const [peerConnection, setPeerConnection] = useState<RTCPeerConnection>();

  const ws = useRef<WebSocket>();
  const conn = useRef(peerConnection);
  const localVideo = useRef<HTMLVideoElement>();

  useEffect(() => {
    if (router.query.room) {
      const r = router.query.room as string;
      setRoom(r);
      const w = new WebSocket(`ws://localhost:4000/socket?room=${r}`);
      ws.current = w;
      ws.current.onopen = onOpen;
      ws.current.onclose = onClose;
      ws.current.onmessage = onMessage;
    }
  }, [router.query.room]);

  useEffect(() => {
    conn.current = peerConnection;
  }, [peerConnection]);

  const onOpen = () => {
    setInterval(() => {
      const ping = {
        action: "Ping",
      };
      ws.current?.send(JSON.stringify(ping));
    }, 10000);
  };

  const onClose = () => {
    console.log("WS connection has been closed");
  };

  const onMessage = (e: MessageEvent) => {
    const message: ReceivedMessage = JSON.parse(e.data);
    switch (message.action) {
      case "Pong": {
        console.debug("pong");
        break;
      }
      case "Answer": {
        console.debug("received answer", message.sdp);
        conn.current!.setRemoteDescription(message.sdp);
        setPublisher(true);
        break;
      }
      default: {
        console.warn("Unknown message received: ", message);
        break;
      }
    }
  };

  const startConn = async () => {
    const c = new RTCPeerConnection(peerConnectionConfig);
    setPeerConnection(c);
    c.onicecandidate = (event) => {
      if (event.candidate) {
        const payload = {
          action: "Ice",
          candidate: event.candidate,
        };
        ws.current?.send(JSON.stringify(payload));
      }
    };
  };

  const startPublish = async () => {
    conn
      .current!.createOffer(offerOptions)
      .then((description: RTCSessionDescriptionInit) => {
        conn.current!.setLocalDescription(description).then(() => {
          const payload = {
            action: "Offer",
            sdp: description,
          };
          ws.current?.send(JSON.stringify(payload));
        });
      });
  };

  const addTrack = async () => {
    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: true,
        audio: false,
      });
      if (localVideo.current) {
        localVideo.current.srcObject = stream;
      }
      stream.getTracks().forEach((track) => {
        conn.current!.addTrack(track);
      });
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <>
      <button onClick={startConn} disabled={peerConnection !== undefined}>
        Peer connection
      </button>
      <button onClick={startPublish} disabled={!peerConnection || publisher}>
        Publisher
      </button>
      <button onClick={addTrack} disabled={!publisher}>
        Add track
      </button>
      <div>
        <video ref={localVideo} width="400" controls />
      </div>
    </>
  );
}

type ReceivedMessage = ReceivedPong | ReceivedAnswer;

type ReceivedPong = {
  action: "Pong";
};

type ReceivedAnswer = {
  action: "Answer";
  sdp: RTCSessionDescription;
};
