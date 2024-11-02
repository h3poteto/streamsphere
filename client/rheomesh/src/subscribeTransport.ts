import { EventEmitter } from "events";

export class SubscribeTransport extends EventEmitter {
  private _peerConnection: RTCPeerConnection;
  private _queue: Array<RTCIceCandidateInit>;

  constructor(config: RTCConfiguration) {
    super();
    const peer = new RTCPeerConnection(config);

    this._peerConnection = peer;
    this._queue = [];

    this._peerConnection.onicecandidate = (event) => {
      if (event.candidate) {
        this.emit("icecandidate", event.candidate);
      }
    };

    this._peerConnection.ontrack = (event) => {
      // todo: should we prepare subscrbie method?
      this.emit("track", event.streams);
    };
  }

  public async setOffer(
    sdp: RTCSessionDescriptionInit,
  ): Promise<RTCSessionDescriptionInit> {
    await this._peerConnection.setRemoteDescription(sdp);
    const answer = await this._peerConnection.createAnswer();
    await this._peerConnection.setLocalDescription(answer);

    if (this._queue.length > 0 && this._peerConnection.remoteDescription) {
      const candidate = this._queue.shift();
      if (candidate) {
        await this.addIceCandidate(candidate);
      }
    }

    return answer;
  }

  public async addIceCandidate(candidate: RTCIceCandidateInit): Promise<void> {
    if (this._peerConnection.remoteDescription) {
      await this._peerConnection.addIceCandidate(
        new RTCIceCandidate(candidate),
      );
    } else {
      this._queue.push(candidate);
    }
  }
}
