use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    WebRTCError(#[from] webrtc::Error),
    #[error(transparent)]
    TransportError(#[from] TransportError),
}

#[derive(thiserror::Error)]
#[error("{kind}: {message}")]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TransportErrorKind {
    #[error("peer connection error")]
    PeerConnectionError,
    #[error("local description error")]
    LocalDescriptionError,
    #[error("ice candidate error")]
    ICECandidateError,
}

impl Error {
    pub fn new_transport(message: String, kind: TransportErrorKind) -> Error {
        Error::TransportError(TransportError { kind, message })
    }
}

impl fmt::Debug for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_struct("streamsphere::TransportError");

        builder.field("kind", &self.kind);
        builder.field("message", &self.message);

        builder.finish()
    }
}
