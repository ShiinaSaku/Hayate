//! Hayate CLI application.

mod cli;
mod exit_code;
mod output;
mod policy;
mod subcmd;
mod words;

use std::process::{ExitCode, Termination};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::Cli;
use compio::runtime::spawn;

/// Scans raw args for an explicit `--color <mode>` / `--color=<mode>` override.
///
/// Applied before `Cli::try_parse()` so that `always`/`never` take effect even
/// on the `--help` / parse-error exit path, which prints the banner before a
/// full [`Cli`] value exists. `auto` (or no flag at all) is a no-op: `console`
/// already auto-detects `NO_COLOR` / `CLICOLOR` / `CLICOLOR_FORCE` / tty /
/// `TERM=dumb` correctly on its own — this only needs to handle the two cases
/// where the user wants to override that detection.
fn apply_color_override(args: &[String]) {
    let value = args.iter().enumerate().find_map(|(i, arg)| {
        arg.strip_prefix("--color=")
            .map(str::to_owned)
            .or_else(|| (arg == "--color").then(|| args.get(i + 1).cloned()).flatten())
    });
    match value.as_deref() {
        Some("always") => {
            console::set_colors_enabled(true);
            console::set_colors_enabled_stderr(true);
            console::set_true_colors_enabled(true);
            console::set_true_colors_enabled_stderr(true);
        },
        Some("never") => {
            console::set_colors_enabled(false);
            console::set_colors_enabled_stderr(false);
        },
        _ => {
            // "auto", unrecognized, or unset: leave console's own detection
            // alone.
        },
    }
}

fn main() -> impl Termination {
    match run() {
        Ok(exit_code) => exit_code,
        Err(err) => {
            output::print_error(&err);
            ExitCode::from(exit_code::CliExitCode::from_anyhow(&err))
        },
    }
}

fn run() -> Result<ExitCode> {
    let args: Vec<String> = std::env::args().collect();
    apply_color_override(&args);

    if args.iter().any(|arg| arg == "-V") {
        println!("v{}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("v{} (commit: {})", env!("CARGO_PKG_VERSION"), env!("GIT_COMMIT_HASH"));
        return Ok(ExitCode::SUCCESS);
    }

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(err) => {
            let kind = err.kind();
            if kind == clap::error::ErrorKind::DisplayHelp
                || kind == clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                || kind == clap::error::ErrorKind::MissingSubcommand
            {
                output::print_banner();
            }
            err.exit();
        },
    };

    policy::init(&cli);

    if cli.command.is_none() {
        output::print_banner();
        Cli::command().print_help()?;
        println!();
        return Ok(ExitCode::SUCCESS);
    }

    // Shared cancellation flag for graceful shutdown (Ctrl+C).
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = Arc::clone(&cancelled);

    // compio thread-per-core runtime.
    let runtime = compio::runtime::Runtime::new()?;
    runtime.block_on(async {
        // Spawn a signal handler that sets the cancellation flag.
        spawn(async move {
            // On Unix, compio::signal::ctrl_c() works. On Windows, it wraps
            // SetConsoleCtrlHandler. Both should be reliable with compio.
            let _ = compio::signal::ctrl_c().await;
            cancelled_clone.store(true, Ordering::SeqCst);
            // Small grace period for logs to flush, then force exit.
            compio::time::sleep(std::time::Duration::from_millis(1500)).await;
            exit_code::CliExitCode::Interrupted.exit();
        })
        .detach();

        let res = subcmd::dispatch(cli, cancelled).await;
        if let Err(err) = res {
            output::print_error(&err);
            return Ok(ExitCode::from(exit_code::CliExitCode::from_anyhow(&err)));
        }
        Ok(ExitCode::SUCCESS)
    })
}
