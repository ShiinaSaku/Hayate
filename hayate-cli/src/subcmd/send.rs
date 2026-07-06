//! `hayate send` subcommand.

use std::{
    net::ToSocketAddrs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use anyhow::{Context, Result, bail};
use compio::io::AsyncRead;
use hayate::{EngineError, HayateSender, network, protocol::TransferKind, transfer};

use crate::{cli::SendArgs, output, policy};

pub async fn run(args: SendArgs, cancelled: Arc<AtomicBool>) -> Result<()> {
    if cancelled.load(Ordering::SeqCst) {
        bail!("cancelled");
    }
    let path = &args.path;
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    let target = args.target.as_ref();

    let (phrase, print_instruction) = if let Some(code) = &args.code {
        (code.clone(), false)
    } else if target.is_none() {
        let p = crate::words::generate_phrase();
        (p, policy::get().normal())
    } else {
        (String::new(), false)
    };

    // ── Stage 1: Connect ─────────────────────────────────────────────
    let (conn, passphrase) = if let Some(target_str) = target {
        let target_addr = target_str
            .to_socket_addrs()
            .context("invalid target address")?
            .next()
            .context("could not resolve target")?;

        output::stage("connect", format!("dialing {target_addr}"));

        let endpoint = network::bind_client()
            .await
            .context("Failed to bind UDP socket for client")?;
        let client_config =
            network::client_config().context("Failed to build client configuration")?;
        let spinner = if args.no_progress {
            None
        } else {
            Some(output::spinner("Connecting", &target_addr.to_string()))
        };
        let conn_result: Result<_> =
            match endpoint.connect(target_addr, "hayate.local", Some(client_config)) {
                Ok(connecting) => connecting
                    .await
                    .context("Failed to establish connection to receiver"),
                Err(e) => Err(e.into()),
            };
        if let Some(spinner) = &spinner {
            spinner.finish_and_clear();
        }
        let conn = conn_result?;
        (conn, args.code.clone())
    } else {
        if print_instruction {
            output::pairing_code(&phrase, &format!("hayate receive --code \"{phrase}\""));
        } else {
            output::stage("pairing", format!("waiting with code \"{phrase}\""));
        }

        let bind_addr =
            std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
        let endpoint = network::bind_server(bind_addr)
            .await
            .context("Failed to bind server socket")?;
        let local_port = endpoint.local_addr()?.port();

        let os_name = std::env::consts::OS.to_owned();
        let channel_id = hayate::discovery::derive_channel_id(&phrase);
        let _broadcaster_guard =
            hayate::discovery::start_broadcaster_hybrid(&channel_id, local_port, &os_name)
                .context("Failed to start hybrid broadcaster")?;

        let spinner = if args.no_progress {
            None
        } else {
            Some(output::spinner("Pairing", "waiting for receiver…"))
        };
        let incoming = endpoint
            .wait_incoming()
            .await
            .context("endpoint closed while waiting for pairing");
        let incoming = match incoming {
            Ok(incoming) => incoming,
            Err(e) => {
                if let Some(spinner) = &spinner {
                    spinner.finish_and_clear();
                }
                return Err(e);
            }
        };
        if let Some(spinner) = &spinner {
            output::spinner_update(spinner, "Pairing", "receiver connected");
        }
        let conn_result = incoming
            .await
            .context("Connection handshake failed with receiver");
        if let Some(spinner) = &spinner {
            spinner.finish_and_clear();
        }
        let conn = conn_result?;
        (conn, Some(phrase))
    };

    output::ok(&format!("Connected to {}", conn.remote_address()));

    let (mut send_stream, mut recv_stream) = conn
        .open_bi()
        .context("Failed to open streams for handshake")?;

    // ── Stage 2: Prepare ─────────────────────────────────────────────
    let sender = HayateSender::new()
        .compress(args.compress)
        .hash_algo(args.hash.clone());
    let (meta, total_size) = sender.build_metadata(path)?;

    // ── Stage 3: Handshake ───────────────────────────────────────────
    output::stage("handshake", "negotiating cipher…");
    let (key, cipher_id) = transfer::handshake_sender_split(
        &mut send_stream,
        &mut recv_stream,
        &meta,
        passphrase.as_deref(),
    )
    .await
    .context("Handshake cipher negotiation failed")?;

    // ── Show transfer info card ──────────────────────────────────────
    let kind = if meta.transfer_type == TransferKind::Directory {
        "directory"
    } else {
        "file"
    };
    output::print_info_card(
        "Sending",
        &[
            ("file", meta.filename.clone()),
            ("type", kind.to_owned()),
            ("size", output::format_bytes(total_size)),
            (
                "compress",
                if args.compress {
                    "zstd level 1".to_owned()
                } else {
                    "off".to_owned()
                },
            ),
            ("hash", args.hash.clone()),
            ("cipher", output::cipher_name(cipher_id).to_owned()),
            ("peer", conn.remote_address().to_string()),
        ],
    );

    // ── Stage 4: Transfer ────────────────────────────────────────────
        let pb = if args.no_progress || total_size == 0 || policy::get().no_progress() {
            None
        } else {
            let pb = output::transfer_progress_bar("send", total_size);
            Some(pb)
        };

    let start = Instant::now();
    let cancelled_transfer = Arc::clone(&cancelled);
    let pb_clone = pb.clone();

    let checksum = if path.is_dir() {
        sender
            .send_directory(path, &key, cipher_id, &args.hash, &mut send_stream, {
                let pb = pb_clone.clone();
                move |b| {
                    if cancelled_transfer.load(Ordering::SeqCst) {
                        return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                    }
                    if let Some(pb) = &pb {
                        output::set_transfer_position(pb, b);
                    }
                    Ok(())
                }
            })
            .await
            .context("Failed to send directory contents")?
    } else {
        sender
            .send_file(path, &key, cipher_id, &args.hash, &mut send_stream, {
                let pb = pb_clone;
                move |b| {
                    if cancelled_transfer.load(Ordering::SeqCst) {
                        return Err(EngineError::Cancelled("transfer cancelled by user".into()));
                    }
                    if let Some(pb) = &pb {
                        output::set_transfer_position(pb, b);
                    }
                    Ok(())
                }
            })
            .await
            .context("Failed to send file contents")?
    };

    // Finish the send stream and notify receiver we're done sending.
    send_stream
        .finish()
        .context("Failed to finalize send stream")?;

    // Wait for the receiver to acknowledge completion with a time-bounded read.
    // If the receiver has closed the connection, reading will either return
    // EOF (Ok(0)) or an error. We use a timeout to avoid hanging if the
    // receiver disappears.
    let drain_buf = vec![0u8; 1];
    let _ = compio::time::timeout(
        std::time::Duration::from_secs(10),
        recv_stream.read(drain_buf),
    )
    .await;

    if let Some(pb) = &pb {
        output::finish_transfer_progress(pb, total_size);
    }

    // ── Stage 5: Summary ─────────────────────────────────────────────
    let elapsed = start.elapsed().as_secs_f64();
    output::print_transfer_summary(
        &meta.filename,
        total_size,
        elapsed,
        &checksum,
        args.compress,
        output::cipher_name(cipher_id),
    );

    conn.close(0u32.into(), b"complete");
    Ok(())
}
