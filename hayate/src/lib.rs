//! # Hayate Engine
//!
//! Hayate is a completion-based transfer engine for encrypted file and
//! directory movement across local networks. It is the library behind the
//! Hayate CLI, but it is designed to be embedded directly in Rust applications.
//!
//! The engine combines:
//!
//! * QUIC transport through `compio-quic`.
//! * Completion-based async I/O through `compio`.
//! * Ephemeral X25519 key agreement.
//! * HKDF-SHA256 key derivation.
//! * Blake3 or SHA256 payload integrity.
//! * ChaCha20-Poly1305 or AES-256-GCM frame encryption.
//! * Optional zstd compression.
//! * Safe tar streaming for directory payloads (symlinks rejected; hard links
//!   supported by replay after the target file is extracted).
//!
//! Most applications should start with [`HayateSender`] and [`HayateReceiver`].
//! Lower-level modules remain public for custom transports, protocol testing,
//! or UI integrations that need direct control over handshakes and payload
//! streams.
//!
//! ## Direct Transfer
//!
//! Direct mode connects to a known receiver address.
//!
//! ```no_run
//! use std::net::SocketAddr;
//! use hayate::HayateSender;
//!
//! # async fn run() -> Result<(), hayate::EngineError> {
//! let target: SocketAddr = "192.168.1.50:50001".parse().unwrap();
//!
//! let sender = HayateSender::new()
//!     .target(target)
//!     .compress(true);
//!
//! let checksum = sender.send("photos", |bytes_sent| {
//!     println!("sent {bytes_sent} bytes");
//!     Ok(())
//! }).await?;
//!
//! println!("checksum {checksum}");
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use std::net::SocketAddr;
//! use hayate::HayateReceiver;
//!
//! # async fn run() -> Result<(), hayate::EngineError> {
//! let bind_addr: SocketAddr = "0.0.0.0:50001".parse().unwrap();
//!
//! let receiver = HayateReceiver::new()
//!     .bind(bind_addr);
//!
//! let (checksum, path) = receiver.receive(
//!     "./downloads",
//!     |meta| {
//!         println!("incoming {} ({} bytes)", meta.filename, meta.total_size);
//!         true
//!     },
//!     |bytes_received| {
//!         println!("received {bytes_received} bytes");
//!         Ok(())
//!     },
//! ).await?;
//!
//! println!("saved to {}", path.display());
//! println!("checksum {checksum}");
//! # Ok(())
//! # }
//! ```
//!
//! ## Pairing-Code Transfer
//!
//! Pairing mode lets a sender and receiver find each other through LAN
//! broadcast using a shared phrase. The same phrase is also used during key
//! derivation, so peers that do not know it cannot derive the application-layer
//! metadata and payload key.
//!
//! ```no_run
//! use hayate::HayateSender;
//!
//! # async fn run() -> Result<(), hayate::EngineError> {
//! let sender = HayateSender::new()
//!     .code("apple-bravo-charlie".to_owned());
//!
//! sender.send("report.pdf", |_| Ok(())).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use hayate::HayateReceiver;
//!
//! # async fn run() -> Result<(), hayate::EngineError> {
//! let receiver = HayateReceiver::new()
//!     .code("apple-bravo-charlie".to_owned())
//!     .auto_accept(true);
//!
//! let (_checksum, _path) = receiver.receive("./downloads", |_| true, |_| Ok(())).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Module Guide
//!
//! * [`runner`] provides the builder-style high-level API, including staged
//!   transfers ([`TransferStage`], [`HayateSender::send_with`],
//!   [`HayateReceiver::receive_with`], [`ListeningReceiver`]).
//! * [`transfer`] implements handshake, consent, payload send, and payload
//!   receive.
//! * [`protocol`] defines wire constants, frame flags, and
//!   [`protocol::Metadata`].
//! * [`crypto`] contains key agreement, HKDF, AEAD sealing, and AEAD opening.
//! * [`network`] binds QUIC endpoints and builds ephemeral TLS/transport
//!   config.
//! * [`discovery`] handles pairing-code broadcast and discovery.
//! * [`tar`] packages directories and safely extracts directory payloads.
//! * [`local_addr`] exposes local IPv4 helpers for UI and discovery.
//! * [`error`] defines [`EngineError`].
//!
//! ## Protocol Shape
//!
//! A transfer establishes QUIC, opens a bidirectional stream, negotiates
//! protocol version and cipher capability, performs X25519 key agreement,
//! derives a 32-byte AEAD key, encrypts metadata (including chosen hash
//! algorithm), sends receiver consent, and then streams length-prefixed
//! encrypted payload frames. File transfers must finish with the
//! exact announced byte count; directory transfers are tar streams with
//! containment checks during extraction.
//!
//! ## Safety Guarantees
//!
//! Hayate treats peer input as untrusted:
//!
//! * Metadata is encrypted and authenticated before use.
//! * Unknown transfer types are rejected before receive routing.
//! * Payload frames are length-capped and AEAD-authenticated before writes.
//! * Directory extraction rejects absolute paths, `..`, and symlinks; hard
//!   links are collected and replayed after their targets are extracted.
//! * Each transfer uses a fresh random HKDF salt and a transcript-bound key
//!   derivation. When a passphrase is used, it is mixed into the IKM; this
//!   authenticates the session against an eavesdropper but is **not a PAKE** on
//!   its own — offline brute-force resistance depends on the word-list entropy
//!   (see the CLI word-list hardening in the matching Tier 2 change).
//! * Receive task failures are returned as [`EngineError`] instead of
//!   panicking.
//!
//! ## Runtime
//!
//! Public async APIs are intended to run inside a `compio` runtime, commonly
//! via `#[compio::main]`. Low-level callers must respect `compio` buffer
//! ownership: buffers are moved into I/O operations and returned through
//! `compio::BufResult`.

#![warn(clippy::all, clippy::pedantic, missing_docs)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::must_use_candidate
)]

pub mod crypto;
pub mod discovery;
pub mod error;
pub mod local_addr;
pub mod network;
pub mod pool;
pub mod protocol;
pub mod runner;
pub mod tar;
pub mod transfer;

pub use discovery::{BroadcasterGuard, DiscoveredPeer};
pub use error::EngineError;
pub use protocol::{Metadata, TransferKind};
pub use runner::{
    HayateReceiver,
    HayateSender,
    ListeningReceiver,
    ReceiveOutcome,
    SendOutcome,
    TransferStage,
    is_benign_peer_close,
};

/// Encode bytes as a lowercase hex string. Replaces the external `hex` crate.
#[inline]
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Two lookups into a 16-char table, no formatting overhead.
        const TABLE: &[u8; 16] = b"0123456789abcdef";
        s.push(TABLE[(b >> 4) as usize] as char);
        s.push(TABLE[(b & 0xf) as usize] as char);
    }
    s
}
