import { EventEmitter } from "events";

export class SubscribeTransport extends EventEmitter {
  private _peerConnection: RTCPeerConnection;
  private _queue: Array<RTCIceCandidateInit>;
  private _track: { [publisherId: string]: MediaStreamTrack };
  private _channel: { [publisherId: string]: RTCDataChannel };
  private _signalingLock: boolean;

  constructor(config: RTCConfiguration) {
    super();
    const peer = new RTCPeerConnection(config);

    this._peerConnection = peer;
    this._queue = [];
    this._track = {};
    this._channel = {};
    this._signalingLock = false;

    this._peerConnection.onicecandidate = (event) => {
      if (event.candidate) {
        this.emit("icecandidate", event.candidate);
      }
    };

    this._peerConnection.ontrack = (event) => {
      console.debug("ontrack: ", event);
      const track = event.track;
      this._track[track.id] = track;
    };

    this._peerConnection.ondatachannel = (event) => {
      console.debug("ondatachannel: ", event);
      if (event.channel.id) {
        this._channel[event.channel.label] = event.channel;
      }
    };

    this._peerConnection.onsignalingstatechange = (event) => {
      console.debug(
        "onsignalingstatechange: ",
        (event.target as RTCPeerConnection).signalingState,
      );
      if ((event.target as RTCPeerConnection).signalingState === "stable") {
        this._signalingLock = false;
      }
    };
  }

  public async setOffer(
    sdp: RTCSessionDescriptionInit,
  ): Promise<RTCSessionDescriptionInit> {
    while (this._signalingLock) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    this._signalingLock = true;
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

  public async subscribe(publisherId: string): Promise<MediaStreamTrack> {
    return new Promise(async (resolve, reject) => {
      for (let i = 0; i < 10; i++) {
        if (this._track[publisherId]) {
          return resolve(this._track[publisherId]);
        }
        await sleep(500);
      }
      reject("Publisher was not found");
    });
  }

  public async subscribeData(publisherId: string): Promise<RTCDataChannel> {
    return new Promise(async (resolve, reject) => {
      for (let i = 0; i < 10; i++) {
        if (this._channel[publisherId]) {
          return resolve(this._channel[publisherId]);
        }
        await sleep(500);
      }
      reject("Subscriber was not found");
    });
  }

  public close() {
    this._peerConnection.close();
  }
}

function sleep(milliseconds: number) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
