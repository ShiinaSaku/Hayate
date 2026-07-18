//! Output format / log-level policy configured by the top-level CLI flags.
//!
//! The CLI supports `--format pretty|plain|json`, `--verbose`, and `--quiet`.
//! This module centralizes the resulting policy so that [`crate::output`] and
//! subcommands can decide whether to print status lines, emit progress bars, or
//! write JSON events without plumbing four booleans everywhere.

use std::sync::OnceLock;

use crate::cli::{Cli, OutputFormat};

/// Effective output policy derived from the CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPolicy {
    /// How status lines and summaries should be rendered.
    pub format: OutputFormat,
    /// Number of `-v` flags supplied.
    pub verbose: u8,
    /// Number of `-q` flags supplied.
    pub quiet: u8,
}

impl OutputPolicy {
    /// Returns true when the current policy is JSON mode.
    #[inline]
    #[must_use]
    pub fn is_json(self) -> bool {
        self.format == OutputFormat::Json
    }

    /// Returns true when the current policy is plain (unstyled) mode.
    #[inline]
    #[must_use]
    pub fn is_plain(self) -> bool {
        self.format == OutputFormat::Plain
    }

    /// Returns true when the current policy is pretty (styled) mode.
    #[inline]
    #[must_use]
    #[allow(dead_code)]
    pub fn is_pretty(self) -> bool {
        self.format == OutputFormat::Pretty
    }

    /// True if progress bars and spinners should be suppressed.
    #[must_use]
    pub fn no_progress(self) -> bool {
        // JSON and plain modes are meant for machines / logs; live bars would
        // corrupt JSON or clutter CI logs. Respect `-q` the same way.
        self.is_json() || self.is_plain() || self.quiet > 0
    }

    /// True if debug-level tracing should be emitted.
    #[must_use]
    #[allow(dead_code)]
    pub fn debug(self) -> bool {
        self.verbose >= 2
    }

    /// True if verbose informational output should be emitted.
    #[must_use]
    #[allow(dead_code)]
    pub fn info(self) -> bool {
        self.verbose >= 1 && self.quiet == 0
    }

    /// True if ordinary informational output should be emitted.
    #[must_use]
    pub fn normal(self) -> bool {
        self.quiet == 0
    }

    /// True if warnings should be suppressed entirely.
    #[must_use]
    #[allow(dead_code)]
    pub fn silent_warnings(self) -> bool {
        self.quiet >= 2
    }

    /// True if only fatal errors should be shown.
    #[must_use]
    #[allow(dead_code)]
    pub fn silent_errors(self) -> bool {
        self.quiet >= 3
    }
}

static POLICY: OnceLock<OutputPolicy> = OnceLock::new();

/// Initialize the process-wide output policy from the parsed CLI.
///
/// Must be called once after `Cli::parse` (or `try_parse`) succeeds and before
/// any subcommand runs. The policy is immutable after initialization.
pub fn init(cli: &Cli) {
    let _ = POLICY.set(OutputPolicy { format: cli.format, verbose: cli.verbose, quiet: cli.quiet });
}

/// Returns the process-wide output policy.
///
/// Panics if [`init`] has not been called. The CLI entrypoint always calls
/// [`init`] right after parsing, so subcommands can rely on this.
#[inline]
#[must_use]
pub fn get() -> OutputPolicy {
    *POLICY.get().expect("output policy not initialized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_progress_in_json_and_plain() {
        assert!(OutputPolicy { format: OutputFormat::Json, verbose: 0, quiet: 0 }.no_progress());
        assert!(OutputPolicy { format: OutputFormat::Plain, verbose: 0, quiet: 0 }.no_progress());
        assert!(!OutputPolicy { format: OutputFormat::Pretty, verbose: 0, quiet: 0 }.no_progress());
    }

    #[test]
    fn quiet_suppresses_progress() {
        assert!(OutputPolicy { format: OutputFormat::Pretty, verbose: 0, quiet: 1 }.no_progress());
    }

    #[test]
    fn verbosity_levels() {
        let base = OutputPolicy { format: OutputFormat::Pretty, verbose: 0, quiet: 0 };
        assert!(!base.debug());
        assert!(!base.info());
        assert!(base.normal());

        let v1 = OutputPolicy { verbose: 1, ..base };
        assert!(v1.info());
        assert!(!v1.debug());

        let v2 = OutputPolicy { verbose: 2, ..base };
        assert!(v2.debug());
    }

    #[test]
    fn quiet_levels() {
        let q1 = OutputPolicy { format: OutputFormat::Pretty, verbose: 0, quiet: 1 };
        assert!(!q1.normal());
        assert!(!q1.silent_warnings());

        let q2 = OutputPolicy { quiet: 2, ..q1 };
        assert!(q2.silent_warnings());
        assert!(!q2.silent_errors());

        let q3 = OutputPolicy { quiet: 3, ..q1 };
        assert!(q3.silent_errors());
    }
}
