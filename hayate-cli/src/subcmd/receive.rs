//! `hayate receive` subcommand.

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use hayate::protocol::{Metadata, TransferKind};
use hayate::{EngineError, HayateReceiver, ReceiveOutcome, TransferStage, is_benign_peer_close};
use indicatif::ProgressBar;

use crate::cli::ReceiveArgs;
use crate::ui::{PathCompleter, TransferUi};
use crate::{history, output};

pub async fn run(args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    // ESC / q listener — polls tty in raw mode, exits cleanly via cancelled
    // flag. Only meaningful on a real terminal; spawning it for piped stdin
    // burns a thread and can toggle raw mode inside scripts.
    if std::io::stdin().is_terminal() {
        spawn_esc_listener(Arc::clone(&cancelled));
    }

    ensure_output_dir(&args.output)?;

    if let Some(code) = args.code.as_deref() {
        let code = crate::subcmd::normalize_code(code)?;
        return run_pairing(code, args, cancelled).await;
    }

    run_listen(args, cancelled).await
}

// ---------------------------------------------------------------------------
// Shared stage handling
// ---------------------------------------------------------------------------

/// Renders the stages both receive paths handle identically (offer card,
/// resume notice, transfer bar). Returns `false` for unhandled stages so the
/// caller can deal with its own connection-lifecycle stages.
fn handle_common_stage(stage: &TransferStage, ui: &TransferUi) -> bool {
    match stage {
        TransferStage::Handshaking => {
            output::stage("handshake", "negotiating cipher…");
        },
        TransferStage::Offer { meta, cipher_id, peer } => {
            let kind =
                if meta.transfer_type == TransferKind::Directory { "directory" } else { "file" };
            output::print_transfer_offer(
                &meta.filename,
                meta.total_size,
                kind,
                *peer,
                output::cipher_name(*cipher_id),
                &meta.hash_algo,
            );
        },
        TransferStage::Resuming { offset } => {
            output::stage("resume", format!("continuing from {}", output::format_bytes(*offset)));
            ui.set_resume_offset(*offset);
        },
        TransferStage::Transferring { filename, total_size } => {
            output::stage("receive", filename);
            ui.start_transfer("receive", *total_size);
        },
        _ => return false,
    }
    true
}

/// Success path shared by both receive modes: summary, integrity report,
/// history.
fn report_receive_success(outcome: &ReceiveOutcome, ui: &TransferUi) {
    ui.finish_progress(outcome.meta.total_size);
    output::key_value("output", outcome.path.display());
    let elapsed = ui.elapsed();
    output::print_transfer_summary(
        &outcome.meta.filename,
        outcome.meta.total_size,
        elapsed,
        &outcome.checksum,
        outcome.compressed,
        output::cipher_name(outcome.cipher_id),
    );
    output::integrity_verified(&outcome.checksum);
    history::record_transfer(
        "receive",
        &outcome.meta.filename,
        outcome.meta.total_size,
        elapsed,
        &outcome.checksum,
        output::cipher_name(outcome.cipher_id),
        &outcome.peer.to_string(),
        Some(outcome.path.display().to_string()),
    );
}

/// What to do when the destination path already exists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OverwritePolicy {
    /// Prompt on a terminal, otherwise refuse the transfer.
    Ask,
    /// Replace the existing path without asking.
    Allow,
    /// Refuse the transfer.
    Deny,
}

fn overwrite_policy(args: &ReceiveArgs) -> OverwritePolicy {
    if args.overwrite {
        OverwritePolicy::Allow
    } else if args.no_overwrite {
        OverwritePolicy::Deny
    } else {
        OverwritePolicy::Ask
    }
}

/// Builds the consent closure shared by both receive modes: auto-accept
/// resolves directly, otherwise suspend bars and prompt. The resolved path is
/// then checked against the overwrite policy so an existing file is never
/// truncated silently.
fn make_consent(
    auto_accept: bool,
    output_dir: PathBuf,
    policy: OverwritePolicy,
    resume: bool,
    prompt_error: Arc<Mutex<Option<anyhow::Error>>>,
) -> impl FnOnce(&Metadata, SocketAddr) -> Option<PathBuf> {
    move |meta, peer| {
        let dest = if auto_accept {
            Some(resolve_output(&output_dir, meta))
        } else {
            match output::suspend_for_prompt(|| prompt_accept(meta, peer, &output_dir)) {
                Ok(path) => path,
                Err(e) => {
                    *prompt_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                    return None;
                },
            }
        }?;

        // Only single-file transfers can append to an existing partial file;
        // directories always extract into a fresh (or merged) tree.
        let can_resume = resume && meta.transfer_type == TransferKind::File;
        match confirm_overwrite(&dest, policy, can_resume) {
            Ok(true) => Some(dest),
            Ok(false) => None,
            Err(e) => {
                *prompt_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                None
            },
        }
    }
}

/// Ensures the destination can be written: creates missing output directories
/// and rejects paths that exist but are not directories.
fn ensure_output_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("could not create output directory {}", dir.display()))?;
    if !dir.is_dir() {
        bail!("output path {} is not a directory", dir.display());
    }
    Ok(())
}

/// Applies the overwrite policy to an already-resolved destination path.
fn confirm_overwrite(dest: &Path, policy: OverwritePolicy, can_resume: bool) -> Result<bool> {
    if policy == OverwritePolicy::Allow || can_resume || !dest.exists() {
        return Ok(true);
    }
    if policy == OverwritePolicy::Deny || !output::is_tty() {
        output::warn(&format!(
            "{} already exists; pass --overwrite to replace it or --resume to continue a partial transfer",
            dest.display()
        ));
        return Ok(false);
    }

    let prompt = format!("   {} exists. Overwrite?", dest.display());
    match output::suspend_for_prompt(|| inquire::Confirm::new(&prompt).with_default(false).prompt())
    {
        Ok(value) => Ok(value),
        Err(inquire::InquireError::OperationInterrupted) => Ok(false),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Builds the progress closure shared by both receive modes.
fn make_progress(ui: &TransferUi) -> impl FnMut(u64) -> Result<(), EngineError> + Send + 'static {
    let ui = ui.clone();
    move |bytes| {
        ui.check_cancelled()?;
        ui.set_position(bytes);
        Ok(())
    }
}

/// Surfaces a stored prompt error, if any, as the real failure.
fn take_prompt_error(prompt_error: &Arc<Mutex<Option<anyhow::Error>>>) -> Option<anyhow::Error> {
    prompt_error.lock().unwrap_or_else(|e| e.into_inner()).take()
}

// ---------------------------------------------------------------------------
// Pairing-code mode (one-shot)
// ---------------------------------------------------------------------------

async fn run_pairing(code: String, args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        bail!("cancelled");
    }

    let ui = TransferUi::new(Arc::clone(&cancelled), args.no_progress);
    let auto_accept = args.auto_accept;
    let output_dir = args.output.clone();
    let policy = overwrite_policy(&args);
    let resume = args.resume;
    let prompt_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));

    // Deliberately leave the engine's `auto_accept` off: the library
    // short-circuits the consent callback in that mode, which would bypass the
    // overwrite policy. `make_consent` already resolves paths without a prompt
    // when `--auto-accept` is set.
    let builder = HayateReceiver::new().code(code).resume(args.resume);

    let outcome = builder
        .receive_with(
            &output_dir,
            {
                let ui = ui.clone();
                move |stage| {
                    ui.check_cancelled()?;
                    if handle_common_stage(&stage, &ui) {
                        return Ok(());
                    }
                    match stage {
                        TransferStage::Discovering { code } => {
                            output::stage("pairing", format!("scanning for code \"{code}\""));
                            ui.spinner("Discovering", "listening for sender broadcast…");
                        },
                        TransferStage::Connecting { peer } => {
                            ui.clear_spinner();
                            output::stage("connect", format!("dialing sender at {peer}"));
                            ui.spinner("Connecting", &peer.to_string());
                        },
                        TransferStage::Connected { peer } => {
                            ui.clear_spinner();
                            output::ok(&format!("Connected to {peer}"));
                        },
                        _ => {},
                    }
                    Ok(())
                }
            },
            make_consent(
                auto_accept,
                output_dir.clone(),
                policy,
                resume,
                Arc::clone(&prompt_error),
            ),
            make_progress(&ui),
        )
        .await;

    ui.clear_spinner();

    match outcome {
        Ok(outcome) => {
            report_receive_success(&outcome, &ui);
            Ok(())
        },
        Err(EngineError::TransferRejected) => {
            if let Some(error) = take_prompt_error(&prompt_error) {
                return Err(error).context("receive prompt failed");
            }
            // A rejected offer is a documented non-zero outcome (exit code 3),
            // not a successful command.
            Err(EngineError::TransferRejected.into())
        },
        Err(EngineError::Cancelled(_)) => {
            ui.clear_all();
            bail!("cancelled");
        },
        Err(e) => {
            ui.clear_all();
            Err(e).context("receive failed")
        },
    }
}

// ---------------------------------------------------------------------------
// Direct listener mode (multi-accept loop)
// ---------------------------------------------------------------------------

async fn run_listen(args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    let bind_addr = SocketAddr::new(args.bind, args.port);
    // See the note in `run_pairing`: the consent callback owns auto-accept so
    // the overwrite policy is always applied.
    let builder = HayateReceiver::new().bind(bind_addr).resume(args.resume);
    let listener = builder.listen().await.context("Failed to bind listener")?;
    let local_addr = listener.local_addr()?;
    let local_port = local_addr.port();

    // Advertise the receiver so `hayate discover` and `send --pick` find it
    // without sweeping the subnet. Best-effort: discovery must never block
    // receiving.
    let _announcer = hayate::discovery::start_announcer(local_addr).ok();

    if bind_addr.ip().is_unspecified() {
        output::print_bound(format!("0.0.0.0:{local_port}"));
        let ips = hayate::local_addr::local_ipv4s();
        if !ips.is_empty() {
            let addrs_with_names: Vec<_> = ips
                .into_iter()
                .map(|ip| {
                    let name = if_addrs::get_if_addrs()
                        .ok()
                        .and_then(|ifaces| {
                            ifaces.into_iter().find(|iface| iface.ip() == std::net::IpAddr::V4(ip))
                        })
                        .map(|iface| iface.name)
                        .unwrap_or_default();
                    (ip, name)
                })
                .collect();
            output::print_local_addresses(&addrs_with_names);
        }
        output::print_cancel_hint();
    } else {
        output::print_bound(listener.local_addr()?);
    }

    let auto_accept = args.auto_accept;
    let output_dir = args.output.clone();
    let policy = overwrite_policy(&args);
    let resume = args.resume;

    let waiting: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(if args.no_progress {
        None
    } else {
        Some(output::spinner("Waiting", "for incoming connection…"))
    }));

    loop {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }

        let ui = TransferUi::new(Arc::clone(&cancelled), args.no_progress);
        let prompt_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));

        let result = listener
            .try_accept_one(
                Duration::from_millis(500),
                &output_dir,
                {
                    let ui = ui.clone();
                    let waiting = Arc::clone(&waiting);
                    move |stage| {
                        ui.check_cancelled()?;
                        if handle_common_stage(&stage, &ui) {
                            return Ok(());
                        }
                        if let TransferStage::Connected { peer } = stage {
                            clear_spinner(&waiting);
                            output::ok(&format!("Connection from {peer}"));
                        }
                        Ok(())
                    }
                },
                make_consent(
                    auto_accept,
                    output_dir.clone(),
                    policy,
                    resume,
                    Arc::clone(&prompt_error),
                ),
                make_progress(&ui),
            )
            .await;

        match result {
            Ok(None) => continue,
            Ok(Some(outcome)) => {
                report_receive_success(&outcome, &ui);
                if args.once {
                    break;
                }
                respawn_waiting(args.no_progress, &waiting);
                continue;
            },
            Err(EngineError::TransferRejected) => {
                ui.clear_all();
                if let Some(error) = take_prompt_error(&prompt_error) {
                    return Err(error).context("receive prompt failed");
                }
                output::warn("Transfer rejected.");
                if args.once {
                    return Err(EngineError::TransferRejected.into());
                }
                respawn_waiting(args.no_progress, &waiting);
                continue;
            },
            Err(EngineError::Cancelled(_)) => {
                ui.clear_all();
                output::err("Transfer cancelled");
                bail!("cancelled");
            },
            Err(EngineError::Handshake(message)) if message == "Endpoint closed" => {
                clear_spinner(&waiting);
                break;
            },
            Err(e) if is_benign_peer_close(&e) => {
                respawn_waiting(args.no_progress, &waiting);
                continue;
            },
            Err(e) => {
                ui.clear_all();
                if let Some(error) = take_prompt_error(&prompt_error) {
                    return Err(error).context("receive prompt failed");
                }
                output::err(&format!("{e}"));
                if args.once {
                    return Err(e).context("receive failed");
                }
                respawn_waiting(args.no_progress, &waiting);
                continue;
            },
        }
    }

    if cancelled.load(Ordering::SeqCst) {
        // Ctrl+C / Esc / q: exit non-zero instead of reporting success.
        bail!("cancelled");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn clear_spinner(spinner: &Arc<Mutex<Option<ProgressBar>>>) {
    if let Some(s) = spinner.lock().unwrap_or_else(|e| e.into_inner()).take() {
        s.finish_and_clear();
    }
}

/// Re-creates a "Waiting" spinner after handling a connection.
fn respawn_waiting(no_progress: bool, waiting: &Arc<Mutex<Option<ProgressBar>>>) {
    clear_spinner(waiting);
    if !no_progress {
        *waiting.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(crate::output::spinner("Waiting", "for incoming connection…"));
    }
}

fn prompt_accept(
    meta: &Metadata,
    peer: SocketAddr,
    default_dir: &std::path::Path,
) -> Result<Option<PathBuf>> {
    let kind = if meta.transfer_type == TransferKind::Directory { "directory" } else { "file" };

    let prompt = format!(
        "   Accept {kind} \"{}\" ({}) from {peer}?",
        meta.filename,
        output::format_bytes(meta.total_size)
    );

    let accept = match inquire::Confirm::new(&prompt).with_default(false).prompt() {
        Ok(val) => val,
        Err(inquire::InquireError::OperationInterrupted) => {
            return Ok(None);
        },
        Err(e) => return Err(anyhow::anyhow!(e)),
    };

    if accept {
        let dest_dir = match inquire::Text::new("   Save to directory")
            .with_default(&default_dir.to_string_lossy())
            .with_autocomplete(PathCompleter::dirs_only())
            .prompt()
        {
            Ok(val) => val,
            Err(inquire::InquireError::OperationInterrupted) => {
                return Ok(None);
            },
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        let dest = PathBuf::from(dest_dir);
        ensure_output_dir(&dest)?;
        let name = std::path::Path::new(&meta.filename)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
        Ok(Some(dest.join(name)))
    } else {
        Ok(None)
    }
}

fn resolve_output(output_dir: &std::path::Path, meta: &Metadata) -> PathBuf {
    let name = std::path::Path::new(&meta.filename)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
    output_dir.join(name)
}

// ── ESC / q listener ─────────────────────────────────────────────────────

fn spawn_esc_listener(cancelled: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        use crossterm::event::{Event, KeyCode, KeyModifiers, poll, read};
        loop {
            if cancelled.load(Ordering::SeqCst) {
                break;
            }
            // Prompts own raw mode while active; don't fight inquire.
            if output::prompt_active() {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            if crossterm::terminal::enable_raw_mode().is_ok() {
                if poll(std::time::Duration::from_millis(0)).is_ok_and(|b| b)
                    && let Ok(Event::Key(k)) = read()
                {
                    let exit =
                        matches!(k.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q'));
                    if exit && !k.modifiers.contains(KeyModifiers::CONTROL) {
                        cancelled.store(true, Ordering::SeqCst);
                    }
                }
                let _ = crossterm::terminal::disable_raw_mode();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}
