//! Error types for the Hayate engine.

use thiserror::Error;

/// Top-level error type for the engine.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// An underlying I/O error occurred (e.g. file system or socket issues).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The operation timed out before completing.
    #[error("timed out: {0}")]
    TimedOut(String),

    /// The operation was cancelled by the caller.
    #[error("cancelled: {0}")]
    Cancelled(String),

    /// The client and server protocol versions do not match.
    #[error("protocol version mismatch: local {local}, remote {remote}")]
    ProtocolMismatch {
        /// The local protocol version supported by this node.
        local: u16,
        /// The remote protocol version sent by the peer.
        remote: u16,
    },

    /// The receiver explicitly rejected the transfer request.
    #[error("transfer rejected by receiver")]
    TransferRejected,

    /// Key exchange authentication failed due to an invalid pairing passphrase.
    #[error("invalid passphrase: key exchange authentication failed")]
    InvalidPassphrase,

    /// An error occurred during cryptographic operations (e.g.
    /// encryption/decryption/HKDF).
    #[error("crypto error: {0}")]
    Crypto(&'static str),

    /// An invalid frame structure or size was encountered on the stream.
    #[error("invalid frame: {0}")]
    InvalidFrame(String),

    /// An error occurred during the handshake negotiation phase.
    #[error("handshake error: {0}")]
    Handshake(String),

    /// An error occurred during client connection initiation.
    #[error("QUIC connect error: {0}")]
    Connect(#[from] compio_quic::ConnectError),

    /// An error occurred on the active QUIC connection.
    #[error("QUIC connection error: {0}")]
    Connection(#[from] compio_quic::ConnectionError),

    /// An error occurred when opening a stream on the QUIC connection.
    #[error("QUIC open stream error: {0}")]
    OpenStream(#[from] compio_quic::OpenStreamError),

    /// An error occurred during writing to the stream.
    #[error("QUIC write error: {0}")]
    Write(#[from] compio_quic::WriteError),

    /// An error occurred because the stream was already closed.
    #[error("QUIC stream closed error: {0}")]
    ClosedStream(#[from] compio_quic::ClosedStream),

    /// An error occurred during payload compression or decompression.
    #[error("compression error: {0}")]
    Compression(String),

    /// An archive entry attempted path traversal outside the output directory.
    #[error("path traversal attack detected in archive entry")]
    PathTraversal,
}
