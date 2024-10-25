import { useRouter } from 'next/router'
import { useEffect, useRef, useState } from 'react'
import { Transport } from 'streamsphere'

const peerConnectionConfig: RTCConfiguration = {
  iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
}

export default function Room() {
  const router = useRouter()

  const [room, setRoom] = useState('')
  const [transport, setTransport] = useState<Transport>()
  const [recevingStream, setRecevingStream] = useState<{
    [trackId: string]: MediaStream
  }>({})

  const ws = useRef<WebSocket>()
  const conn = useRef(transport)
  const localVideo = useRef<HTMLVideoElement>()

  useEffect(() => {
    if (router.query.room) {
      const r = router.query.room as string
      setRoom(r)
      const w = new WebSocket(`ws://localhost:4000/socket?room=${r}`)
      ws.current = w
      ws.current.onopen = onOpen
      ws.current.onclose = onClose
      ws.current.onmessage = onMessage
    }
  }, [router.query.room])

  useEffect(() => {
    conn.current = transport
  }, [transport])

  const onOpen = () => {
    setInterval(() => {
      const ping = {
        action: 'Ping'
      }
      ws.current?.send(JSON.stringify(ping))
    }, 10000)

    startConn()
  }

  const onClose = () => {
    console.log('WS connection has been closed')
  }

  const onMessage = async (e: MessageEvent) => {
    const message: ReceivedMessage = JSON.parse(e.data)
    switch (message.action) {
      case 'Pong': {
        console.debug('pong')
        break
      }
      case 'Answer': {
        console.debug('received answer', message.sdp)
        await conn.current?.setAnswer(message.sdp)
        break
      }
      case 'Offer': {
        console.debug('received offer', message.sdp)
        const answer = await conn.current?.getAnswer(message.sdp)
        if (answer) {
          const payload = {
            action: 'Answer',
            sdp: answer
          }
          ws.current!.send(JSON.stringify(payload))
        } else {
          console.error('failed to set local description')
        }

        break
      }
      case 'Ice': {
        console.debug('received ice', message.candidate)
        await conn.current?.addIceCandidate(message.candidate)
        break
      }
      case 'Published': {
        const trackId = message.trackId
        const payload = {
          action: 'Subscribe',
          trackId: trackId
        }
        ws.current?.send(JSON.stringify(payload))
        break
      }
      default: {
        console.warn('Unknown message received: ', message)
        break
      }
    }
  }

  const startConn = () => {
    const t = new Transport(peerConnectionConfig)
    setTransport(t)
    t.on('icecandidate', event => {
      const payload = {
        action: 'Ice',
        candidate: event
      }
      ws.current?.send(JSON.stringify(payload))
    })
    t.on('icegatheringstatechange', event => {
      console.debug('gathering change', event)
    })
    t.on('track', event => {
      console.debug('track added', event)
      const track = event.track
      const stream = new MediaStream([track])
      setRecevingStream(prev => ({
        ...prev,
        [track.id]: stream
      }))
    })
  }

  const capture = async () => {
    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: true,
        audio: false
      })
      if (localVideo.current) {
        localVideo.current.srcObject = stream
      }

      stream.getTracks().forEach(async track => {
        console.log('adding track', track.id)
        const offer = await conn.current?.publish(track)

        const offerPayload = {
          action: 'Offer',
          sdp: offer
        }
        ws.current?.send(JSON.stringify(offerPayload))

        const payload = {
          action: 'Publish',
          trackId: track.id
        }
        ws.current?.send(JSON.stringify(payload))
      })
    } catch (e) {
      console.error(e)
    }
  }

  return (
    <>
      <button onClick={capture} disabled={!transport}>
        Capture
      </button>
      <div>
        <h3>sending video</h3>
        <video ref={localVideo} width="480" controls autoPlay />
      </div>
      <div>
        <h3>receving video</h3>
        {Object.keys(recevingStream).map(key => (
          <div key={key}>
            <h4>{key}</h4>
            {recevingStream[key] && (
              <video
                id={key}
                muted
                autoPlay
                controls
                ref={video => {
                  if (video && recevingStream[key]) {
                    video.srcObject = recevingStream[key]
                  } else {
                    console.warn('video element or track is null')
                  }
                }}
                width={480}
              ></video>
            )}
          </div>
        ))}
      </div>
    </>
  )
}

type ReceivedMessage = ReceivedPong | ReceivedAnswer | ReceivedOffer | ReceivedIce | ReceivedPublished

type ReceivedPong = {
  action: 'Pong'
}

type ReceivedAnswer = {
  action: 'Answer'
  sdp: RTCSessionDescription
}

type ReceivedOffer = {
  action: 'Offer'
  sdp: RTCSessionDescription
}

type ReceivedIce = {
  action: 'Ice'
  candidate: RTCIceCandidateInit
}

type ReceivedPublished = {
  action: 'Published'
  trackId: string
}
