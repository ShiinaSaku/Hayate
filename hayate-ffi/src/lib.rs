//! C-compatible FFI shim for hayate-engine.
//! Reserved for future Flutter / Android / iOS integration.

#![allow(clippy::missing_safety_doc)]

/// Returns the current engine protocol version.
#[unsafe(no_mangle)]
pub extern "C" fn hayate_protocol_version() -> u16 {
    hayate_engine::protocol::PROTOCOL_VERSION
}
