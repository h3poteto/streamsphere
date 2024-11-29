global.RTCSessionDescription = class {
  constructor(description) {
    this.type = description.type;
    this.sdp = description.sdp;
  }
};

global.RTCPeerConnection = class {
  constructor() {
    this.localDescription = null;
    this.remoteDescription = null;
    this.iceCandidates = [];
  }

  setLocalDescription(description) {
    this.localDescription = description;
    return Promise.resolve();
  }

  setRemoteDescription(description) {
    this.remoteDescription = description;
    return Promise.resolve();
  }

  addIceCandidate(candidate) {
    this.iceCandidates.push(candidate);
    return Promise.resolve();
  }

  createOffer() {
    return Promise.resolve(
      new RTCSessionDescription({ type: "offer", sdp: "mock-sdp" }),
    );
  }

  createAnswer() {
    return Promise.resolve(
      new RTCSessionDescription({ type: "answer", sdp: "mock-sdp" }),
    );
  }
};
