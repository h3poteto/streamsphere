import { EventEmitter } from 'events'

const offerOptions: RTCOfferOptions = {
  offerToReceiveAudio: false,
  offerToReceiveVideo: false
}

export class Transport extends EventEmitter {
  private _peerConnection: RTCPeerConnection
  private readonly _stream: MediaStream
  private _queue: Array<RTCIceCandidateInit>

  constructor(config: RTCConfiguration) {
    super()
    this._peerConnection = new RTCPeerConnection(config)
    this._stream = new MediaStream()
    this._queue = []

    this._peerConnection.onicecandidate = (event: RTCPeerConnectionIceEvent) => {
      if (event.candidate) {
        this.emit('icecandidate', event.candidate)
      }
    }
    this._peerConnection.onicegatheringstatechange = event => {
      this.emit('icegatheringstatechange', event)
    }
    this._peerConnection.ontrack = async (event: RTCTrackEvent) => {
      this.emit('track', event)
      await this.dequeueIceCandidate()
    }
  }

  public async publish(track: MediaStreamTrack): Promise<RTCSessionDescriptionInit> {
    this._peerConnection.addTransceiver(track, {
      direction: 'sendonly',
      streams: [this._stream],
      sendEncodings: [{ maxBitrate: 5000000 }]
    })

    const offer = await this._peerConnection.createOffer(offerOptions)
    await this._peerConnection.setLocalDescription(offer)
    return offer
  }

  public async setAnswer(answer: RTCSessionDescription) {
    await this._peerConnection.setRemoteDescription(answer)
  }

  public async getAnswer(offer: RTCSessionDescription): Promise<RTCSessionDescription> {
    await this._peerConnection.setRemoteDescription(offer)
    const answer = await this._peerConnection.createAnswer()
    await this._peerConnection.setLocalDescription(answer)
    const sdp = this._peerConnection.localDescription

    await this.dequeueIceCandidate()

    if (sdp) {
      return sdp
    }
    throw new Error('faild to create answer')
  }

  public async addIceCandidate(candidate: RTCIceCandidateInit) {
    if (this._peerConnection.remoteDescription) {
      await this._peerConnection.addIceCandidate(candidate)
    } else {
      this._queue.push(candidate)
    }
  }

  public close() {
    this._peerConnection.close()
  }

  private async dequeueIceCandidate() {
    if (this._queue.length > 0 && this._peerConnection.remoteDescription) {
      const next = this._queue.shift()
      if (next) {
        this.addIceCandidate(next)
      }
    }
  }
}
