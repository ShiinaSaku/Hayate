//! Error types for hayate-engine.

use thiserror::Error;

/// Top-level error type for the engine.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol version mismatch: local {local}, remote {remote}")]
    ProtocolMismatch { local: u16, remote: u16 },

    #[error("transfer rejected by receiver")]
    TransferRejected,

    #[error("invalid passphrase: key exchange authentication failed")]
    InvalidPassphrase,

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("invalid frame: {0}")]
    InvalidFrame(String),

    #[error("handshake error: {0}")]
    Handshake(String),

    #[error("QUIC error: {0}")]
    Quic(String),

    #[error("compression error: {0}")]
    Compression(String),

    #[error("path traversal attack detected in archive entry")]
    PathTraversal,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
