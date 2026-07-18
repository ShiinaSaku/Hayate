//! Process exit codes used by the Hayate CLI.

use std::process::ExitCode;

use hayate::EngineError;

/// Exit codes returned by the `hayate` binary.
///
/// Stable contract for scripts and automation. Values stay fixed across
/// releases; new codes may be added but existing ones are not renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExitCode {
    /// Operation completed successfully.
    Success = 0,
    /// A generic runtime or transfer error occurred.
    GeneralError = 1,
    /// Invalid usage or argument parsing failure.
    Usage = 2,
    /// The receiver rejected the transfer offer.
    TransferRejected = 3,
    /// Local and remote protocol versions do not match.
    ProtocolMismatch = 4,
    /// Pairing passphrase failed key-exchange authentication.
    InvalidPassphrase = 5,
    /// Operation timed out (pairing, discovery, or connect).
    TimedOut = 6,
    /// Operation cancelled by the user (soft cancel path).
    Cancelled = 7,
    /// The user interrupted the process (Ctrl+C or Esc/q after grace period).
    Interrupted = 130,
}

impl CliExitCode {
    /// Map a library [`EngineError`] to a process exit code.
    #[must_use]
    pub fn from_engine(err: &EngineError) -> Self {
        match err {
            EngineError::TransferRejected => Self::TransferRejected,
            EngineError::ProtocolMismatch { .. } => Self::ProtocolMismatch,
            EngineError::InvalidPassphrase => Self::InvalidPassphrase,
            EngineError::TimedOut(_) => Self::TimedOut,
            EngineError::Cancelled(_) => Self::Cancelled,
            EngineError::Handshake(msg) if msg.to_ascii_lowercase().contains("timed out") => {
                Self::TimedOut
            },
            EngineError::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => Self::TimedOut,
            EngineError::Io(e) if e.kind() == std::io::ErrorKind::Interrupted => Self::Cancelled,
            _ => Self::GeneralError,
        }
    }

    /// Walk an [`anyhow::Error`] chain and map the first [`EngineError`] found.
    #[must_use]
    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        for cause in err.chain() {
            if let Some(engine) = cause.downcast_ref::<EngineError>() {
                return Self::from_engine(engine);
            }
        }
        let msg = err.to_string().to_ascii_lowercase();
        if msg.contains("cancelled") || msg.contains("canceled") {
            return Self::Cancelled;
        }
        if msg.contains("timed out") || msg.contains("timeout") {
            return Self::TimedOut;
        }
        if msg.contains("rejected") {
            return Self::TransferRejected;
        }
        Self::GeneralError
    }

    /// Exit the process with this code.
    ///
    /// On Unix, restores the default SIGINT handler before exiting with the
    /// interrupt code so the parent shell sees the conventional `130` signal
    /// origin. This mirrors the `fd` pattern.
    pub fn exit(self) -> ! {
        #[cfg(unix)]
        if self == CliExitCode::Interrupted {
            // SAFETY: restoring the default SIGINT handler then re-raising is the
            // standard way to report 130 to the parent shell after we handled Ctrl+C.
            unsafe {
                let _ = libc::signal(libc::SIGINT, libc::SIG_DFL);
                let _ = libc::raise(libc::SIGINT);
            }
        }
        std::process::exit(i32::from(self as u8));
    }
}

impl From<CliExitCode> for ExitCode {
    fn from(code: CliExitCode) -> Self {
        ExitCode::from(code as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_zero() {
        assert_eq!(CliExitCode::Success as u8, 0);
    }

    #[test]
    fn error_is_one() {
        assert_eq!(CliExitCode::GeneralError as u8, 1);
    }

    #[test]
    fn interrupted_is_130() {
        assert_eq!(CliExitCode::Interrupted as u8, 130);
    }

    #[test]
    fn maps_engine_variants() {
        assert_eq!(
            CliExitCode::from_engine(&EngineError::TransferRejected),
            CliExitCode::TransferRejected
        );
        assert_eq!(
            CliExitCode::from_engine(&EngineError::ProtocolMismatch { local: 6, remote: 5 }),
            CliExitCode::ProtocolMismatch
        );
        assert_eq!(
            CliExitCode::from_engine(&EngineError::InvalidPassphrase),
            CliExitCode::InvalidPassphrase
        );
        assert_eq!(
            CliExitCode::from_engine(&EngineError::TimedOut("x".into())),
            CliExitCode::TimedOut
        );
        assert_eq!(
            CliExitCode::from_engine(&EngineError::Cancelled("x".into())),
            CliExitCode::Cancelled
        );
        assert_eq!(
            CliExitCode::from_engine(&EngineError::Handshake(
                "timed out waiting for sender".into()
            )),
            CliExitCode::TimedOut
        );
    }

    #[test]
    fn maps_anyhow_chain() {
        let err = anyhow::Error::new(EngineError::TransferRejected).context("send failed");
        assert_eq!(CliExitCode::from_anyhow(&err), CliExitCode::TransferRejected);
    }
}
