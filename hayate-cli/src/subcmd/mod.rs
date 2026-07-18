pub mod completions;
pub mod discover;
pub mod docs;
pub mod receive;
pub mod send;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::Result;

use crate::cli::{Cli, Command};

/// Top-level dispatcher. The `cancelled` flag is polled by subcommands at
/// key yield points to enable graceful Ctrl+C shutdown.
pub async fn dispatch(cli: Cli, cancelled: Arc<AtomicBool>) -> Result<()> {
    match cli.command.expect("command is checked before dispatch") {
        Command::Receive(args) => receive::run(args, cancelled).await,
        Command::Send(args) => send::run(args, cancelled).await,
        Command::Discover(args) => discover::run(args, cancelled).await,
        Command::Completions(args) => completions::run(args),
        Command::Docs(args) => docs::run(args),
    }
}
