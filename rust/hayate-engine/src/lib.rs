//! hayate-engine: completion-based I/O engine for Hayate.
//!
//! Thread-per-core, zero-copy where the OS allows, with compio as the
//! executor and compio-quic (quinn-proto) for QUIC transport.

#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod crypto;
pub mod discovery;
pub mod error;
pub mod network;
pub mod protocol;
pub mod tar;
pub mod transfer;

pub use error::EngineError;
