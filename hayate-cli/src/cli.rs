// CLI argument definitions using clap derive.

use std::net::IpAddr;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand, ValueEnum};

pub fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Yellow.on_default())
}

/// Explicit color policy, following the convention set by `git`, `ripgrep`, and
/// `eza`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ColorMode {
    /// Color when stdout is a terminal; honors `NO_COLOR` / `CLICOLOR` /
    /// `CLICOLOR_FORCE`.
    Auto,
    /// Always emit color, even when redirected to a file or pipe.
    Always,
    /// Never emit color.
    Never,
}

/// Output format for machine-readable consumers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable styled output (default).
    Pretty,
    /// Compact text without styling or progress bars.
    Plain,
    /// Newline-delimited JSON events.
    Json,
}

/// Integrity hash algorithm supported by the transfer engine.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum HashAlgorithm {
    /// BLAKE3 — fast, cryptographically strong (default).
    Blake3,
    /// SHA-256 — widely available, FIPS-friendly.
    Sha256,
}

impl HashAlgorithm {
    /// Returns the wire/protocol string for the selected algorithm.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Sha256 => "sha256",
        }
    }
}

/// Hayate — encrypted, compressed LAN file transfer.
#[derive(Parser, Debug)]
#[command(
    name = "hayate",
    version = env!("CARGO_PKG_VERSION"),
    about = "Encrypted LAN file transfer over QUIC",
    long_about = "Hayate sends encrypted files and directories directly across a local network.\n\
                  Pair with a short code phrase (no IP needed) or dial a receiver by address.\n\
                  No cloud, no accounts, no SSH.",
    after_long_help = "\
Examples:
  hayate receive --output ./downloads
  hayate send ./photo.jpg 192.168.1.20:50001
  hayate send ./project --code alpha-bravo-charlie-delta
  hayate receive --code alpha-bravo-charlie-delta
  hayate discover --timeout 5
  hayate completions zsh --install
  hayate docs
  hayate docs --web

Full guide:  hayate docs
Online:      hayate docs --web",
    disable_help_subcommand = false,
    styles = cli_styles(),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Control colored output.
    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,

    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Pretty)]
    pub format: OutputFormat,

    /// Suppress informational output; show warnings and errors only.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub quiet: u8,

    /// Increase verbosity; repeat for more detail (e.g. -vv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start a receiver and wait for an incoming file or directory.
    #[command(
        alias = "recv",
        alias = "rx",
        long_about = "Listen for a direct transfer or join a pairing session with --code.\n\
                      Prompts before accepting unless --auto-accept is set."
    )]
    Receive(ReceiveArgs),

    /// Send a file or directory to a receiver.
    #[command(
        alias = "tx",
        long_about = "Send a file or directory. Omit TARGET to print a pairing code and wait;\n\
                      pass ip:port for a direct connection."
    )]
    Send(SendArgs),

    /// Scan the local network for active Hayate receivers.
    #[command(
        alias = "scan",
        long_about = "Probe local subnets for Hayate receivers via short QUIC checks.\n\
                      Discovery is unauthenticated; use pairing or trust the LAN."
    )]
    Discover(DiscoverArgs),

    /// Generate shell completion scripts.
    #[command(
        long_about = "Print or install shell completions for bash, zsh, fish, or PowerShell.\n\
                      Use --install for a conventional user path plus setup hints.\n\
                      See also: hayate docs completions"
    )]
    Completions(CompletionsArgs),

    /// Print the in-terminal guide, or open the website.
    #[command(long_about = "Print a styled handbook in the terminal (default), or open the\n\
                      online docs with --web. Optional topic: start, send, receive,\n\
                      discover, completions, exit, env, security, web.")]
    Docs(DocsArgs),
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

    /// Cryptographic code-phrase for pairing.
    #[arg(long)]
    pub code: Option<String>,

    /// Compress chunks with zstd level 1 before encrypting (default on).
    #[arg(
        short = 'z',
        long,
        default_value_t = true,
        num_args = 0..=1,
        default_missing_value = "true",
        action = clap::ArgAction::Set
    )]
    pub compress: bool,

    /// Disable zstd compression (alias for `--compress=false`).
    #[arg(long, conflicts_with = "compress")]
    pub no_compress: bool,

    /// Hash algorithm for payload integrity.
    #[arg(long, value_enum, default_value_t = HashAlgorithm::Blake3)]
    pub hash: HashAlgorithm,

    /// Suppress the progress bar.
    #[arg(long, alias = "no-tui")]
    pub no_progress: bool,
}

#[derive(clap::Args, Debug)]
pub struct DiscoverArgs {
    /// Network scan timeout in seconds.
    #[arg(short, long, default_value_t = 15)]
    pub timeout: u64,

    /// Override the subnet CIDR to scan (e.g. 192.168.1.0/24).
    #[arg(long)]
    pub cidr: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,

    /// Install the completion script to a conventional user path and print
    /// shell-specific setup steps (instead of writing the script to stdout).
    #[arg(long)]
    pub install: bool,
}

#[derive(clap::Args, Debug)]
pub struct DocsArgs {
    /// Open the online documentation in your browser.
    #[arg(long, short = 'w')]
    pub web: bool,

    /// Topic to show (default: full guide).
    ///
    /// One of: all, start, send, receive, discover, completions, exit, env,
    /// security, web.
    #[arg(value_name = "TOPIC")]
    pub topic: Option<String>,
}
