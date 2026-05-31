pub mod discover;
pub mod receive;
pub mod send;

use anyhow::Result;

use crate::cli::{Cli, Command};

/// Top-level dispatcher.
pub async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Receive(args) => receive::run(args).await,
        Command::Send(args) => send::run(args).await,
        Command::Discover(args) => discover::run(args).await,
    }
}
