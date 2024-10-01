use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    WebRTCError(#[from] webrtc::Error),
    #[error(transparent)]
    TransportError(#[from] TransportError),
    #[error(transparent)]
    SubscriberError(#[from] SubscriberError),
}

#[derive(thiserror::Error)]
#[error("{kind}: {message}")]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

#[derive(thiserror::Error)]
#[error("{kind}: {message}")]
pub struct SubscriberError {
    pub kind: SubscriberErrorKind,
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

#[derive(Debug, thiserror::Error)]
pub enum SubscriberErrorKind {
    #[error("track not found error")]
    TrackNotFoundError,
}

impl Error {
    pub fn new_transport(message: String, kind: TransportErrorKind) -> Error {
        Error::TransportError(TransportError { kind, message })
    }

    pub fn new_subscriber(message: String, kind: SubscriberErrorKind) -> Error {
        Error::SubscriberError(SubscriberError { kind, message })
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

impl fmt::Debug for SubscriberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = f.debug_struct("streamsphere::SubscriberError");

        builder.field("kind", &self.kind);
        builder.field("message", &self.message);

        builder.finish()
    }
}
