//! `hayate completions` subcommand — prints a shell completion script to stdout.

use anyhow::Result;
use clap::CommandFactory;

use crate::cli::{Cli, CompletionsArgs};

/// Generates a completion script for the requested shell and writes it to stdout.
///
/// # Errors
///
/// This function currently always succeeds; the `Result` return type matches the
/// other subcommand entrypoints so [`crate::subcmd::dispatch`] can treat every
/// variant uniformly.
pub fn run(args: CompletionsArgs) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_owned();
    clap_complete::generate(args.shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
