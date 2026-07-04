//! `hayate receive` subcommand.

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use compio_quic::ConnectionError as QuicConnectionError;
use hayate::{
    EngineError, local_addr, network,
    protocol::{Metadata, TransferKind},
    transfer,
};

use crate::{cli::ReceiveArgs, output};

pub async fn run(args: ReceiveArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    // ESC / q listener — polls tty in raw mode, exits cleanly via cancelled flag.
    spawn_esc_listener(Arc::clone(&cancelled));

    if let Some(code) = &args.code {
        // ── Pairing-code mode ────────────────────────────────────────
        output::stage("pairing", format!("scanning for code \"{code}\""));
        let spinner = if args.no_progress {
            None
        } else {
            Some(output::spinner(
                "Discovering",
                "listening for sender broadcast…",
            ))
        };

        if cancelled.load(Ordering::SeqCst) {
            bail!("cancelled");
        }

        let peer_addr = match hayate::discovery::listen_for_broadcast(
            Some(code.as_str()),
            Duration::from_secs(60),
        )? {
            Some((_name, addr, _os)) => {
                if let Some(s) = &spinner {
                    s.finish_and_clear();
                }
                addr
            }
            None => {
                if let Some(s) = &spinner {
                    s.finish_and_clear();
                }
                bail!("Timed out waiting for sender broadcast.");
            }
        };

        output::stage("connect", format!("dialing sender at {peer_addr}"));
        let endpoint = network::bind_client().await?;
        let client_config = network::client_config()?;
        let spinner = if args.no_progress {
            None
        } else {
            Some(output::spinner("Connecting", &peer_addr.to_string()))
        };
        let conn_result: Result<_> =
            match endpoint.connect(peer_addr, "hayate.local", Some(client_config)) {
                Ok(connecting) => connecting
                    .await
                    .context("Failed to establish QUIC connection to the sender"),
                Err(e) => Err(e.into()),
            };
        if let Some(spinner) = &spinner {
            spinner.finish_and_clear();
        }
        let conn = conn_result?;

        let peer = conn.remote_address();
        output::ok(&format!("Connected to {peer}"));

        let (mut send_stream, mut recv_stream) = conn
            .accept_bi()
            .await
            .context("Failed to accept bidirectional streams from sender")?;

        // ── Handshake ────────────────────────────────────────────────
        output::stage("handshake", "negotiating cipher…");
        let ((key, cipher_id), meta) = transfer::handshake_receiver_split(
            &mut send_stream,
            &mut recv_stream,
            Some(code.as_str()),
        )
        .await
        .context("Handshake cipher negotiation failed")?;

        // ── Transfer offer card ──────────────────────────────────────
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

        let dest = if args.auto_accept {
            Some(resolve_output(&args.output, &meta))
        } else {
            if let Some(pb) = &spinner {
                output::hide_progress(pb);
            }
            let result = prompt_accept(&meta, peer, &args.output);
            if let Some(pb) = &spinner {
                output::show_progress(pb);
            }
            result?
        };

        let accept = dest.is_some();
        if cancelled.load(Ordering::SeqCst) {
            bail!("cancelled");
        }
        transfer::send_consent_write(&mut send_stream, accept)
            .await
            .context("Failed to send transfer acceptance to peer")?;
        if !accept {
            output::warn("Transfer rejected.");
            conn.close(0u32.into(), b"rejected");
            return Ok(());
        }
        let dest = dest.unwrap();

        // ── Receive ──────────────────────────────────────────────────
        output::stage("receive", &meta.filename);
        output::key_value("output", dest.display());
        let start = Instant::now();

        let pb = if args.no_progress || meta.total_size == 0 {
            None
        } else {
            let pb = output::transfer_progress_bar("receive", meta.total_size);
            Some(pb)
        };

        let pb_clone = pb.clone();
        let cancelled_clone = Arc::clone(&cancelled);
        let checksum_result = transfer::receive_payload_split(
            &key,
            cipher_id,
            &mut recv_stream,
            &dest,
            meta.transfer_type,
            meta.total_size,
            &meta.hash_algo,
            move |bytes| {
                if cancelled_clone.load(Ordering::SeqCst) {
                    return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                }
                if let Some(pb) = &pb_clone {
                    output::set_transfer_position(pb, bytes);
                }
                Ok(())
            },
        )
        .await
        .context("File transfer failed during payload delivery");

        if let Some(pb) = &pb {
            output::finish_transfer_progress(pb, meta.total_size);
        }

        let checksum = checksum_result?;

        let elapsed = start.elapsed().as_secs_f64();
        output::print_transfer_summary(
            &meta.filename,
            meta.total_size,
            elapsed,
            &checksum,
            false,
            output::cipher_name(cipher_id),
        );

        // Finish our send stream to signal the sender we're done, then
        // close the connection gracefully.
        let _ = send_stream.finish();
        compio::time::sleep(std::time::Duration::from_millis(200)).await;
        conn.close(0u32.into(), b"complete");
        return Ok(());
    }

    // ── Direct listener mode ─────────────────────────────────────────
    let bind_addr = SocketAddr::new(args.bind, args.port);
    let endpoint = network::bind_server(bind_addr).await?;
    let local_port = endpoint.local_addr()?.port();

    if bind_addr.ip().is_unspecified() {
        // Single bound line + compact interface table so the user knows
        // which addresses peers can connect to, without looking like we
        // started multiple servers.
        output::print_bound(format!("0.0.0.0:{local_port}"));
        let ips = local_addr::local_ipv4s();
        if !ips.is_empty() {
            let addrs_with_names: Vec<_> = ips
                .into_iter()
                .map(|ip| {
                    // Try to find the interface name for each IP.
                    let name = if_addrs::get_if_addrs()
                        .ok()
                        .and_then(|ifaces| {
                            ifaces
                                .into_iter()
                                .find(|iface| iface.ip() == std::net::IpAddr::V4(ip))
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
        output::print_bound(endpoint.local_addr()?);
    }

    let mut spinner = if args.no_progress {
        None
    } else {
        Some(output::spinner("Waiting", "for incoming connection…"))
    };

    loop {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }

        // Wait with a 500 ms timeout so the cancelled flag is polled.
        let incoming =
            match compio::time::timeout(Duration::from_millis(500), endpoint.wait_incoming()).await
            {
                Ok(Some(i)) => {
                    if let Some(s) = &spinner {
                        s.finish_and_clear();
                    }
                    i
                }
                Ok(None) => break,
                Err(_timeout) => continue,
            };
        let conn = match incoming.await {
            Ok(c) => c,
            Err(e) => {
                if !is_peer_close(&e) {
                    output::err(&format!("Connection failed: {e}"));
                }
                respawn_spinner(args.no_progress, &mut spinner);
                continue;
            }
        };
        let peer = conn.remote_address();
        output::ok(&format!("Connection from {peer}"));

        let (mut send_stream, mut recv_stream) = match conn.accept_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                if !is_peer_close(&e) {
                    output::err(&format!("Failed to accept streams: {e}"));
                }
                respawn_spinner(args.no_progress, &mut spinner);
                continue;
            }
        };

        output::stage("handshake", "negotiating cipher…");
        let ((key, cipher_id), meta) = match transfer::handshake_receiver_split(
            &mut send_stream,
            &mut recv_stream,
            None,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                output::err(&format!("Handshake failed: {e}"));
                respawn_spinner(args.no_progress, &mut spinner);
                continue;
            }
        };

        // ── Transfer offer card ──────────────────────────────────────
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

        let dest = if args.auto_accept {
            Some(resolve_output(&args.output, &meta))
        } else {
            if let Some(pb) = &spinner {
                output::hide_progress(pb);
            }
            let result = prompt_accept(&meta, peer, &args.output);
            if let Some(pb) = &spinner {
                output::show_progress(pb);
            }
            result?
        };

        let accept = dest.is_some();
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        if let Err(e) = transfer::send_consent_write(&mut send_stream, accept).await {
            output::err(&format!("Failed to send transfer consent: {e}"));
            respawn_spinner(args.no_progress, &mut spinner);
            continue;
        }
        if !accept {
            output::warn("Transfer rejected.");
            conn.close(0u32.into(), b"rejected");
            respawn_spinner(args.no_progress, &mut spinner);
            continue;
        }
        let dest = dest.unwrap();

        output::stage("receive", &meta.filename);
        output::key_value("output", dest.display());
        let start = Instant::now();

        let pb = if args.no_progress || meta.total_size == 0 {
            None
        } else {
            let pb = output::transfer_progress_bar("receive", meta.total_size);
            Some(pb)
        };

        let pb_clone = pb.clone();
        let cancelled_clone = Arc::clone(&cancelled);
        let receive_result = transfer::receive_payload_split(
            &key,
            cipher_id,
            &mut recv_stream,
            &dest,
            meta.transfer_type,
            meta.total_size,
            &meta.hash_algo,
            move |bytes| {
                if cancelled_clone.load(Ordering::SeqCst) {
                    return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                }
                if let Some(pb) = &pb_clone {
                    output::set_transfer_position(pb, bytes);
                }
                Ok(())
            },
        )
        .await;

        if let Some(pb) = &pb {
            output::finish_transfer_progress(pb, meta.total_size);
        }

        let checksum = match receive_result {
            Ok(checksum) => checksum,
            Err(EngineError::Cancelled(_)) => {
                output::err("Transfer cancelled");
                break;
            }
            Err(e) => {
                output::err(&format!("Transfer failed: {e}"));
                conn.close(1u32.into(), b"failed");
                respawn_spinner(args.no_progress, &mut spinner);
                continue;
            }
        };

        let elapsed = start.elapsed().as_secs_f64();
        output::print_transfer_summary(
            &meta.filename,
            meta.total_size,
            elapsed,
            &checksum,
            false,
            output::cipher_name(cipher_id),
        );

        // Finish our send stream to signal the sender, then close.
        let _ = send_stream.finish();
        compio::time::sleep(std::time::Duration::from_millis(200)).await;
        conn.close(0u32.into(), b"complete");
        break;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if the error is a peer-initiated connection close (e.g. a
/// discover probe or remote shutdown). These are benign and should be
/// silently ignored so the listener keeps waiting.
fn is_peer_close(e: &QuicConnectionError) -> bool {
    matches!(
        e,
        QuicConnectionError::ApplicationClosed(_) | QuicConnectionError::ConnectionClosed(_)
    )
}

/// Re-creates a "Waiting" spinner after handling a failed connection.
/// Any previous spinner is finished and cleared first so only one live
/// spinner appears at a time.
fn respawn_spinner(
    no_progress: bool,
    current: &mut Option<indicatif::ProgressBar>,
) -> Option<indicatif::ProgressBar> {
    if let Some(pb) = current.take() {
        pb.finish_and_clear();
    }
    if no_progress {
        None
    } else {
        Some(crate::output::spinner(
            "Waiting",
            "for incoming connection…",
        ))
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

        let dir_to_read = if dir_path.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            dir_path
        };

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
    let kind = if meta.transfer_type == TransferKind::Directory {
        "directory"
    } else {
        "file"
    };

    let prompt = format!(
        "   Accept {kind} \"{}\" ({}) from {peer}?",
        meta.filename,
        output::format_bytes(meta.total_size)
    );

    let accept = match inquire::Confirm::new(&prompt).with_default(false).prompt() {
        Ok(val) => val,
        Err(inquire::InquireError::OperationInterrupted) => {
            return Ok(None);
        }
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
            }
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
                    let exit = matches!(
                        k.code,
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q')
                    );
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
