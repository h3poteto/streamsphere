const EXTMAP_ORDER: { [key: string]: number } = {
  "urn:ietf:params:rtp-hdrext:ssrc-audio-level": 1,
  "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time": 2,
  "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01": 3,
  "urn:ietf:params:rtp-hdrext:sdes:mid": 4,
  "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id": 10,
  "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id": 11,
  "urn:3gpp:video-orientation": 13,
  "urn:ietf:params:rtp-hdrext:toffset": 14,
};

export function findExtmapOrder(uri: string): number | null {
  const order = EXTMAP_ORDER[uri];
  if (order) {
    return order;
  } else {
    return null;
  }
}
