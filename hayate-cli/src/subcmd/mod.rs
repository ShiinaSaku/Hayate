pub mod completions;
pub mod discover;
pub mod docs;
pub mod man;
pub mod receive;
pub mod send;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Result, bail};

use crate::cli::{Cli, Command};

/// Trims and validates a user-supplied pairing phrase.
///
/// An empty or whitespace-only phrase would silently weaken the handshake, so
/// it is rejected before any socket is opened.
///
/// # Errors
///
/// Returns an error when the phrase is empty, too long, or contains control
/// characters.
pub fn normalize_code(raw: &str) -> Result<String> {
    let code = raw.trim();
    if code.is_empty() {
        bail!("pairing phrase must not be empty");
    }
    if code.len() > 128 {
        bail!("pairing phrase is too long (max 128 characters)");
    }
    if code.chars().any(char::is_control) {
        bail!("pairing phrase contains control characters");
    }
    Ok(code.to_owned())
}

/// Top-level dispatcher. The `cancelled` flag is polled by subcommands at
/// key yield points to enable graceful Ctrl+C shutdown.
pub async fn dispatch(cli: Cli, cancelled: Arc<AtomicBool>) -> Result<()> {
    match cli.command.expect("command is checked before dispatch") {
        Command::Receive(args) => receive::run(args, cancelled).await,
        Command::Send(args) => send::run(args, cancelled).await,
        Command::Discover(args) => discover::run(args, cancelled).await,
        Command::Web(args) => crate::web::run(args, cancelled).await,
        Command::Completions(args) => completions::run(args),
        Command::Docs(args) => docs::run(args),
        Command::History(args) => crate::history::run(args),
        Command::Peers(args) => crate::peers::run(args),
        Command::Man => man::run(),
    }
}
