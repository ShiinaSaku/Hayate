//! hayate-engine: completion-based I/O engine for Hayate.
//!
//! Thread-per-core, zero-copy where the OS allows, with compio as the
//! executor and compio-quic (quinn-proto) for QUIC transport.

#![warn(clippy::all, clippy::pedantic)]
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
pub mod network;
pub mod protocol;
pub mod tar;
pub mod transfer;

pub use error::EngineError;
