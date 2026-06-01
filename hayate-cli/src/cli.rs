//! CLI argument definitions using clap derive.

use std::{net::IpAddr, path::PathBuf};

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};

pub fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Yellow.on_default())
}

/// Hayate — encrypted, compressed, blazing-fast LAN file transfer.
#[derive(Parser, Debug)]
#[command(
    name = "hayate",
    version = env!("CARGO_PKG_VERSION"),
    about = "Swift cross-device file transfer",
    long_about = None,
    disable_help_subcommand = false,
    styles = cli_styles(),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start a receiver and wait for an incoming file or directory.
    Receive(ReceiveArgs),

    /// Send a file or directory to a receiver.
    Send(SendArgs),

    /// Scan the local network for active Hayate receivers.
    Discover(DiscoverArgs),
}

#[derive(clap::Args, Debug)]
pub struct ReceiveArgs {
    /// IP address to bind the QUIC listener.
    #[arg(short, long, env = "HAYATE_BIND", default_value = "0.0.0.0")]
    pub bind: IpAddr,

    /// Port to listen on.
    #[arg(short, long, env = "HAYATE_PORT", default_value_t = 50001)]
    pub port: u16,

    /// Directory to save received files into.
    #[arg(short, long, default_value = ".")]
    pub output: PathBuf,

    /// Auto-accept all incoming transfers without prompting.
    #[arg(long)]
    pub auto_accept: bool,

    /// Suppress the progress bar (useful in Termux / headless).
    #[arg(long, alias = "no-tui")]
    pub no_progress: bool,

    /// Cryptographic code-phrase for pairing.
    #[arg(long)]
    pub code: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct SendArgs {
    /// Path to the file or directory to send.
    pub path: PathBuf,

    /// Receiver address in the form `ip:port` or `hostname:port`.
    pub target: Option<String>,

    /// Receiver address in the form `ip:port` or `hostname:port` (compat option).
    #[arg(long)]
    pub peer: Option<String>,

    /// Cryptographic code-phrase for pairing.
    #[arg(long)]
    pub code: Option<String>,

    /// Compress chunks with zstd level 1 before encrypting.
    #[arg(short = 'z', long)]
    pub compress: bool,

    /// Suppress the progress bar.
    #[arg(long, alias = "no-tui")]
    pub no_progress: bool,
}

#[derive(clap::Args, Debug)]
pub struct DiscoverArgs {
    /// Network scan timeout in seconds.
    #[arg(short, long, default_value_t = 3)]
    pub timeout: u64,

    /// Override the subnet CIDR to scan (e.g. 192.168.1.0/24).
    #[arg(long)]
    pub cidr: Option<String>,
}
