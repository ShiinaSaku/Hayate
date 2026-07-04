//! Process exit codes used by the Hayate CLI.

use std::process::ExitCode;

/// Exit codes returned by the `hayate` binary.
///
/// Following the conventions of `fd`, `ripgrep`, and `cargo-binstall`, we use a
/// small, typed enum so every exit path is explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExitCode {
    /// Operation completed successfully.
    Success = 0,
    /// A generic runtime or transfer error occurred.
    GeneralError = 1,
    /// The user interrupted the process (e.g. Ctrl+C or Esc/q).
    Interrupted = 130,
}

impl CliExitCode {
    /// Exit the process with this code.
    ///
    /// On Unix, restores the default SIGINT handler before exiting with the
    /// interrupt code so the parent shell sees the conventional `130` signal
    /// origin. This mirrors the `fd` pattern.
    pub fn exit(self) -> ! {
        #[cfg(unix)]
        if self == CliExitCode::Interrupted {
            unsafe {
                let _ = libc::signal(libc::SIGINT, libc::SIG_DFL);
                let _ = libc::raise(libc::SIGINT);
            }
        }
        std::process::exit(self as i32);
    }
}

impl From<CliExitCode> for ExitCode {
    fn from(code: CliExitCode) -> Self {
        // ExitCode::from(u8) is available on Rust 1.61+.
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
}
