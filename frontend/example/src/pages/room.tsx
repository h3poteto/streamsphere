import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

const peerConnectionConfig: RTCConfiguration = {
  iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
};

const offerOptions: RTCOfferOptions = {
  offerToReceiveVideo: true,
  offerToReceiveAudio: false,
};

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const [peerConnection, setPeerConnection] = useState<RTCPeerConnection>();
  const [recevingStream, setRecevingStream] = useState<{
    [trackId: string]: MediaStream;
  }>({});

  const ws = useRef<WebSocket>();
  const conn = useRef(peerConnection);
  const localVideo = useRef<HTMLVideoElement>();
  const queue = useRef<Array<any>>([]);

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

  const onMessage = async (e: MessageEvent) => {
    const message: ReceivedMessage = JSON.parse(e.data);
    switch (message.action) {
      case "Pong": {
        console.debug("pong");
        break;
      }
      case "Answer": {
        console.debug("received answer", message.sdp);
        await conn.current!.setRemoteDescription(message.sdp);
        break;
      }
      case "Offer": {
        console.debug("received offer", message.sdp);
        await conn.current!.setRemoteDescription(message.sdp);
        const answer = await conn.current!.createAnswer();
        await conn.current!.setLocalDescription(answer);
        const a = conn.current!.localDescription;
        if (a) {
          const payload = {
            action: "Answer",
            sdp: a,
          };
          ws.current!.send(JSON.stringify(payload));
        } else {
          console.error("failed to set local description");
        }

        break;
      }
      case "Ice": {
        console.debug("received ice", message.candidate);
        if (conn.current!.remoteDescription) {
          await conn.current!.addIceCandidate(message.candidate);
        } else {
          queue.current.push(e);
          return;
        }
        break;
      }
      case "Published": {
        const trackId = message.trackId;
        await subscribe(trackId);
        break;
      }
      default: {
        console.warn("Unknown message received: ", message);
        break;
      }
    }

    if (queue.current.length > 0 && conn.current?.remoteDescription) {
      onMessage(queue.current.shift());
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
    c.onicegatheringstatechange = (event) => {
      console.debug("gathering change", event);
    };
    c.ontrack = (event) => {
      const track = event.track;
      const stream = new MediaStream([track]);
      setRecevingStream((prev) => ({
        ...prev,
        [track.id]: stream,
      }));
    };
  };

  const capture = async () => {
    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: true,
        audio: false,
      });
      if (localVideo.current) {
        localVideo.current.srcObject = stream;
      }
      stream.getTracks().forEach((track) => {
        // use this track id for subscribers
        console.log("adding track", track.id);
        conn.current!.addTrack(track);
        const payload = {
          action: "Published",
          trackId: track.id,
        };
        ws.current?.send(JSON.stringify(payload));
      });
    } catch (e) {
      console.error(e);
    }

    await publish();
  };

  const publish = async () => {
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

  const subscribe = async (trackId: string) => {
    const payload = {
      action: "Subscribe",
      trackId: trackId,
    };
    ws.current?.send(JSON.stringify(payload));
  };

  return (
    <>
      <button onClick={startConn} disabled={peerConnection !== undefined}>
        Peer connection
      </button>
      <button onClick={capture} disabled={!peerConnection}>
        Capture
      </button>
      <div>
        <h3>sending video</h3>
        <video ref={localVideo} width="480" controls autoPlay />
      </div>
      <div>
        <h3>receving video</h3>
        {Object.keys(recevingStream).map((key) => (
          <div key={key}>
            {recevingStream[key] && (
              <video
                id={key}
                muted
                autoPlay
                ref={(video) => {
                  if (video && recevingStream[key]) {
                    video.srcObject = recevingStream[key];
                  } else {
                    console.warn("video element or track is null");
                  }
                }}
                width={480}
              ></video>
            )}
          </div>
        ))}
      </div>
    </>
  );
}

type ReceivedMessage =
  | ReceivedPong
  | ReceivedAnswer
  | ReceivedOffer
  | ReceivedIce
  | ReceivedPublished;

type ReceivedPong = {
  action: "Pong";
};

type ReceivedAnswer = {
  action: "Answer";
  sdp: RTCSessionDescription;
};

type ReceivedOffer = {
  action: "Offer";
  sdp: RTCSessionDescription;
};

type ReceivedIce = {
  action: "Ice";
  candidate: RTCIceCandidateInit;
};

type ReceivedPublished = {
  action: "Published";
  trackId: string;
};
