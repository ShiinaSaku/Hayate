//! `hayate receive` subcommand.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hayate::protocol::{Metadata, TransferKind};
use hayate::{EngineError, HayateReceiver, TransferStage, is_benign_peer_close, local_addr};
use indicatif::ProgressBar;

use crate::cli::ReceiveArgs;
use crate::{output, policy};

pub async fn run(args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    // ESC / q listener — polls tty in raw mode, exits cleanly via cancelled flag.
    spawn_esc_listener(Arc::clone(&cancelled));

    if let Some(code) = args.code.clone() {
        return run_pairing(code, args, cancelled).await;
    }

    run_listen(args, cancelled).await
}

// ---------------------------------------------------------------------------
// Pairing-code mode (one-shot)
// ---------------------------------------------------------------------------

async fn run_pairing(code: String, args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        bail!("cancelled");
    }

    let spinner: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let progress: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
    let no_progress = args.no_progress || policy::get().no_progress();
    let auto_accept = args.auto_accept;
    let output_dir = args.output.clone();
    let transfer_start = Arc::new(Mutex::new(None));
    let prompt_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));

    let spinner_s = Arc::clone(&spinner);
    let progress_s = Arc::clone(&progress);
    let cancelled_stage = Arc::clone(&cancelled);
    let cancelled_progress = Arc::clone(&cancelled);
    let transfer_start_stage = Arc::clone(&transfer_start);

    let mut builder = HayateReceiver::new().code(code);
    if auto_accept {
        builder = builder.auto_accept(true);
    }

    let outcome = builder
        .receive_with(
            &output_dir,
            move |stage| {
                if cancelled_stage.load(Ordering::SeqCst) {
                    return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                }
                match stage {
                    TransferStage::Discovering { code } => {
                        output::stage("pairing", format!("scanning for code \"{code}\""));
                        if !no_progress {
                            *spinner_s.lock().unwrap() = Some(output::spinner(
                                "Discovering",
                                "listening for sender broadcast…",
                            ));
                        }
                    },
                    TransferStage::Connecting { peer } => {
                        clear_spinner(&spinner_s);
                        output::stage("connect", format!("dialing sender at {peer}"));
                        if !no_progress {
                            *spinner_s.lock().unwrap() =
                                Some(output::spinner("Connecting", &peer.to_string()));
                        }
                    },
                    TransferStage::Connected { peer } => {
                        clear_spinner(&spinner_s);
                        output::ok(&format!("Connected to {peer}"));
                    },
                    TransferStage::Handshaking => {
                        output::stage("handshake", "negotiating cipher…");
                    },
                    TransferStage::Offer { meta, cipher_id, peer } => {
                        let kind = if meta.transfer_type == TransferKind::Directory {
                            "directory"
                        } else {
                            "file"
                        };
                        output::print_transfer_offer(
                            &meta.filename,
                            meta.total_size,
                            kind,
                            peer,
                            output::cipher_name(cipher_id),
                            &meta.hash_algo,
                        );
                    },
                    TransferStage::Transferring { filename, total_size } => {
                        *transfer_start_stage.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(Instant::now());
                        output::stage("receive", &filename);
                        if !no_progress && total_size > 0 {
                            *progress_s.lock().unwrap() =
                                Some(output::transfer_progress_bar("receive", total_size));
                        }
                    },
                    TransferStage::Finishing
                    | TransferStage::WaitingForPeer
                    | TransferStage::Pairing { .. }
                    | TransferStage::Ready { .. } => {},
                    _ => {},
                }
                Ok(())
            },
            {
                let output_dir = output_dir.clone();
                let prompt_error = Arc::clone(&prompt_error);
                move |meta, peer| {
                    if auto_accept {
                        return Some(resolve_output(&output_dir, meta));
                    }
                    match output::suspend_for_prompt(|| prompt_accept(meta, peer, &output_dir)) {
                        Ok(path) => path,
                        Err(e) => {
                            *prompt_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                            None
                        },
                    }
                }
            },
            {
                let progress = Arc::clone(&progress);
                move |bytes| {
                    if cancelled_progress.load(Ordering::SeqCst) {
                        return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                    }
                    if let Some(pb) = progress.lock().unwrap().as_ref() {
                        output::set_transfer_position(pb, bytes);
                    }
                    Ok(())
                }
            },
        )
        .await;

    clear_spinner(&spinner);

    match outcome {
        Ok(outcome) => {
            if let Some(pb) = progress.lock().unwrap().take() {
                output::finish_transfer_progress(&pb, outcome.meta.total_size);
            }
            output::key_value("output", outcome.path.display());
            let elapsed = transfer_start
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .map_or(0.0, |start| start.elapsed().as_secs_f64());
            output::print_transfer_summary(
                &outcome.meta.filename,
                outcome.meta.total_size,
                elapsed,
                &outcome.checksum,
                false,
                output::cipher_name(outcome.cipher_id),
            );
            Ok(())
        },
        Err(EngineError::TransferRejected) => {
            if let Some(error) = prompt_error.lock().unwrap_or_else(|e| e.into_inner()).take() {
                return Err(error).context("receive prompt failed");
            }
            output::warn("Transfer rejected.");
            Ok(())
        },
        Err(EngineError::Cancelled(_)) => {
            clear_progress(&progress);
            bail!("cancelled");
        },
        Err(e) => {
            clear_progress(&progress);
            Err(e).context("receive failed")
        },
    }
}

// ---------------------------------------------------------------------------
// Direct listener mode (multi-accept loop)
// ---------------------------------------------------------------------------

async fn run_listen(args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    let bind_addr = SocketAddr::new(args.bind, args.port);
    let mut builder = HayateReceiver::new().bind(bind_addr);
    if args.auto_accept {
        builder = builder.auto_accept(true);
    }
    let listener = builder.listen().await.context("Failed to bind listener")?;
    let local_port = listener.local_addr()?.port();

    if bind_addr.ip().is_unspecified() {
        output::print_bound(format!("0.0.0.0:{local_port}"));
        let ips = local_addr::local_ipv4s();
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

    let no_progress = args.no_progress || policy::get().no_progress();
    let auto_accept = args.auto_accept;
    let output_dir = args.output.clone();

    let waiting: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(if no_progress {
        None
    } else {
        Some(output::spinner("Waiting", "for incoming connection…"))
    }));

    loop {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }

        let progress: Arc<Mutex<Option<ProgressBar>>> = Arc::new(Mutex::new(None));
        let progress_s = Arc::clone(&progress);
        let waiting_s = Arc::clone(&waiting);
        let transfer_start = Arc::new(Mutex::new(None));
        let prompt_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));
        let transfer_start_stage = Arc::clone(&transfer_start);

        let result = listener
            .try_accept_one(
                Duration::from_millis(500),
                &output_dir,
                {
                    let cancelled = Arc::clone(&cancelled);
                    move |stage| {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err(EngineError::Cancelled(
                                "transfer cancelled by user".into(),
                            ));
                        }
                        match stage {
                            TransferStage::WaitingForPeer => {},
                            TransferStage::Connected { peer } => {
                                clear_spinner(&waiting_s);
                                output::ok(&format!("Connection from {peer}"));
                            },
                            TransferStage::Handshaking => {
                                output::stage("handshake", "negotiating cipher…");
                            },
                            TransferStage::Offer { meta, cipher_id, peer } => {
                                let kind = if meta.transfer_type == TransferKind::Directory {
                                    "directory"
                                } else {
                                    "file"
                                };
                                output::print_transfer_offer(
                                    &meta.filename,
                                    meta.total_size,
                                    kind,
                                    peer,
                                    output::cipher_name(cipher_id),
                                    &meta.hash_algo,
                                );
                            },
                            TransferStage::Transferring { filename, total_size } => {
                                *transfer_start_stage.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(Instant::now());
                                output::stage("receive", &filename);
                                if !no_progress && total_size > 0 {
                                    *progress_s.lock().unwrap() =
                                        Some(output::transfer_progress_bar("receive", total_size));
                                }
                            },
                            TransferStage::Finishing
                            | TransferStage::Connecting { .. }
                            | TransferStage::Pairing { .. }
                            | TransferStage::Discovering { .. }
                            | TransferStage::Ready { .. } => {},
                            _ => {},
                        }
                        Ok(())
                    }
                },
                {
                    let output_dir = output_dir.clone();
                    let prompt_error = Arc::clone(&prompt_error);
                    move |meta, peer| {
                        if auto_accept {
                            return Some(resolve_output(&output_dir, meta));
                        }
                        match output::suspend_for_prompt(|| prompt_accept(meta, peer, &output_dir))
                        {
                            Ok(path) => path,
                            Err(e) => {
                                *prompt_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                                None
                            },
                        }
                    }
                },
                {
                    let cancelled = Arc::clone(&cancelled);
                    let progress = Arc::clone(&progress);
                    move |bytes| {
                        if cancelled.load(Ordering::SeqCst) {
                            return Err(EngineError::Cancelled(
                                "transfer cancelled by user".into(),
                            ));
                        }
                        if let Some(pb) = progress.lock().unwrap().as_ref() {
                            output::set_transfer_position(pb, bytes);
                        }
                        Ok(())
                    }
                },
            )
            .await;

        match result {
            Ok(None) => continue,
            Ok(Some(outcome)) => {
                if let Some(pb) = progress.lock().unwrap().take() {
                    output::finish_transfer_progress(&pb, outcome.meta.total_size);
                }
                output::key_value("output", outcome.path.display());
                let elapsed = transfer_start
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .map_or(0.0, |start| start.elapsed().as_secs_f64());
                output::print_transfer_summary(
                    &outcome.meta.filename,
                    outcome.meta.total_size,
                    elapsed,
                    &outcome.checksum,
                    false,
                    output::cipher_name(outcome.cipher_id),
                );
                break;
            },
            Err(EngineError::TransferRejected) => {
                clear_progress(&progress);
                if let Some(error) = prompt_error.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    return Err(error).context("receive prompt failed");
                }
                output::warn("Transfer rejected.");
                respawn_waiting(no_progress, &waiting);
                continue;
            },
            Err(EngineError::Cancelled(_)) => {
                clear_progress(&progress);
                output::err("Transfer cancelled");
                bail!("cancelled");
            },
            Err(EngineError::Handshake(message)) if message == "Endpoint closed" => {
                clear_spinner(&waiting);
                break;
            },
            Err(e) if is_benign_peer_close(&e) => {
                respawn_waiting(no_progress, &waiting);
                continue;
            },
            Err(e) => {
                clear_progress(&progress);
                if let Some(error) = prompt_error.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    return Err(error).context("receive prompt failed");
                }
                output::err(&format!("{e}"));
                respawn_waiting(no_progress, &waiting);
                continue;
            },
        }
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

fn clear_progress(progress: &Arc<Mutex<Option<ProgressBar>>>) {
    if let Some(pb) = progress.lock().unwrap_or_else(|e| e.into_inner()).take() {
        pb.finish_and_clear();
    }
}

/// Re-creates a "Waiting" spinner after handling a failed connection.
fn respawn_waiting(no_progress: bool, waiting: &Arc<Mutex<Option<ProgressBar>>>) {
    clear_spinner(waiting);
    if !no_progress {
        *waiting.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(crate::output::spinner("Waiting", "for incoming connection…"));
    }
}

#[derive(Clone)]
struct DirCompleter;

impl inquire::Autocomplete for DirCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        let path = std::path::Path::new(input);
        let (dir_path, prefix) = if input.ends_with('/') || input.is_empty() {
            (path, "")
        } else {
            (
                path.parent().unwrap_or_else(|| std::path::Path::new("")),
                path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            )
        };

        let dir_to_read =
            if dir_path.as_os_str().is_empty() { std::path::Path::new(".") } else { dir_path };

        let mut suggestions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir_to_read) {
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name_str) = name.to_str() else {
                    continue;
                };
                if !name_str.starts_with(prefix) {
                    continue;
                }

                let full_path = dir_path.join(name_str);
                let mut path_str = full_path.to_string_lossy().into_owned();
                if !path_str.ends_with('/') {
                    path_str.push('/');
                }
                suggestions.push(path_str);
            }
        }

        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        Ok(highlighted_suggestion)
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
            .with_autocomplete(DirCompleter)
            .prompt()
        {
            Ok(val) => val,
            Err(inquire::InquireError::OperationInterrupted) => {
                return Ok(None);
            },
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        let dest = PathBuf::from(dest_dir);
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
