import { EventEmitter } from "events";

const offerOptions: RTCOfferOptions = {
  offerToReceiveVideo: false,
  offerToReceiveAudio: false,
};

export class PublishTransport extends EventEmitter {
  private _peerConnection: RTCPeerConnection;
  private _signalingLock: boolean;

  constructor(config: RTCConfiguration) {
    super();
    const peer = new RTCPeerConnection(config);

    this._peerConnection = peer;
    this._signalingLock = false;

    this._peerConnection.onicecandidate = (event) => {
      if (event.candidate) {
        this.emit("icecandidate", event.candidate);
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

    this._peerConnection.onnegotiationneeded = async (_event) => {
      while (this._signalingLock) {
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      this._signalingLock = true;
      const offer = await this._peerConnection.createOffer(offerOptions);
      await this._peerConnection.setLocalDescription(offer);

      this.emit("negotiationneeded", offer);
    };
  }

  public async publish(
    track: MediaStreamTrack,
  ): Promise<RTCSessionDescriptionInit> {
    while (this._signalingLock) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    this._signalingLock = true;
    this._peerConnection.addTrack(track);

    const offer = await this._peerConnection.createOffer(offerOptions);
    await this._peerConnection.setLocalDescription(offer);

    await this.waitForIceGatheringComplete(this._peerConnection);

    const res = this._peerConnection.localDescription;
    if (!res) {
      throw new Error("empty localDescription");
    }
    return res;
  }

  public async setAnswer(answer: RTCSessionDescription): Promise<void> {
    await this._peerConnection.setRemoteDescription(answer);
  }

  public async addIceCandidate(candidate: RTCIceCandidateInit): Promise<void> {
    await this._peerConnection.addIceCandidate(new RTCIceCandidate(candidate));
  }

  public async publishData(): Promise<
    [RTCDataChannel, RTCSessionDescriptionInit]
  > {
    const label = crypto.randomUUID();
    const channel = this._peerConnection.createDataChannel(label, {
      ordered: true,
    });
    while (this._signalingLock) {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    this._signalingLock = true;
    const offer = await this._peerConnection.createOffer(offerOptions);
    await this._peerConnection.setLocalDescription(offer);

    return [channel, offer];
  }

  private waitForIceGatheringComplete(peerConnection: RTCPeerConnection) {
    return new Promise((resolve) => {
      if (peerConnection.iceGatheringState === "complete") {
        resolve(true);
      } else {
        const checkState = () => {
          if (peerConnection.iceGatheringState === "complete") {
            peerConnection.removeEventListener(
              "icegatheringstatechange",
              checkState,
            );
            resolve(true);
          }
        };
        peerConnection.addEventListener("icegatheringstatechange", checkState);
      }
    });
  }
}
