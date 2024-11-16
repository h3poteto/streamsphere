import { EventEmitter } from "events";

const offerOptions: RTCOfferOptions = {
  offerToReceiveVideo: false,
  offerToReceiveAudio: false,
};

export class PublishTransport extends EventEmitter {
  private _peerConnection: RTCPeerConnection;

  constructor(config: RTCConfiguration) {
    super();
    const peer = new RTCPeerConnection(config);

    this._peerConnection = peer;

    this._peerConnection.onicecandidate = (event) => {
      if (event.candidate) {
        this.emit("icecandidate", event.candidate);
      }
    };
  }

  public async publish(
    stream: MediaStream,
  ): Promise<RTCSessionDescriptionInit> {
    stream.getTracks().forEach((track) => {
      this._peerConnection.addTrack(track, stream);
    });
    const offer = await this._peerConnection.createOffer(offerOptions);
    await this._peerConnection.setLocalDescription(offer);

    return offer;
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

    const offer = await this._peerConnection.createOffer(offerOptions);
    await this._peerConnection.setLocalDescription(offer);

    return [channel, offer];
  }
}
