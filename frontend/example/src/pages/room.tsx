import { useRouter } from "next/router";
import { useEffect, useRef, useState } from "react";

export default function Room() {
  const router = useRouter();

  const [room, setRoom] = useState("");
  const ws = useRef<WebSocket>();

  useEffect(() => {
    if (router.query.room) {
      setRoom(router.query.room as string);
      const w = new WebSocket("ws://localhost:4000/socket");
      ws.current = w;
      ws.current.onopen = onOpen;
      ws.current.onclose = onClose;
      ws.current.onmessage = onMessage;
    }
  }, [router.query.room]);

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
      default: {
        console.warn("Unknown message received: ", message);
        break;
      }
    }
  };
}

type ReceivedMessage = ReceivedPong;

type ReceivedPong = {
  action: "Pong";
};
