//! `hayate receive` subcommand.

use std::{
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use compio::io::{AsyncReadExt, AsyncWriteExt};
use hayate_engine::{
    EngineError, crypto, network,
    protocol::{MAX_METADATA_ENCRYPTED, Metadata, PROTOCOL_VERSION, TRANSFER_DIR},
    transfer,
};

use crate::{cli::ReceiveArgs, output};

pub async fn run(args: ReceiveArgs) -> Result<()> {
    output::print_banner();

    if let Some(code) = &args.code {
        output::info(&format!(
            "Scanning network for pairing peer with code \"{}\"...",
            code
        ));
        let peer_addr = match hayate_engine::discovery::listen_for_broadcast(
            Some(code.clone()),
            Duration::from_secs(30),
        )
        .await?
        {
            Some((_name, addr, _os)) => addr,
            None => bail!("Timed out waiting for sender broadcast."),
        };

        output::info(&format!("Connecting to sender at {peer_addr}..."));
        let endpoint = network::bind_client().await?;
        let conn = endpoint
            .connect(peer_addr, "hayate.local", Some(network::client_config()?))?
            .await?;

        let peer = conn.remote_address();
        output::ok(&format!("Connected to {peer}"));

        let (mut send_stream, mut recv_stream) = conn.accept_bi().await?;
        let (key, meta) =
            handshake_receiver_split(&mut send_stream, &mut recv_stream, Some(code.as_str()))
                .await?;

        let accept = if args.auto_accept {
            true
        } else {
            prompt_accept(&meta, peer)?
        };

        transfer::send_consent_write(&mut send_stream, accept).await?;
        if !accept {
            output::warn("Transfer rejected.");
            conn.close(0u32.into(), b"rejected");
            return Ok(());
        }

        output::ok(&format!("Receiving: {}", meta.filename));
        let dest = resolve_output(&args.output, &meta)?;
        let start = Instant::now();

        let pb = if args.no_progress || meta.total_size == 0 {
            None
        } else {
            Some(output::progress_bar(meta.total_size))
        };

        let pb_clone = pb.clone();
        let checksum = transfer::receive_payload_split(
            &key,
            &mut recv_stream,
            &dest,
            meta.transfer_type,
            move |bytes| {
                if let Some(pb) = &pb_clone {
                    pb.set_position(bytes);
                }
            },
        )
        .await?;

        if let Some(pb) = &pb {
            pb.finish_and_clear();
        }

        let elapsed = start.elapsed().as_secs_f64();
        output::print_transfer_summary(&meta.filename, meta.total_size, elapsed, &checksum, false);
        conn.close(0u32.into(), b"complete");
        return Ok(());
    }

    let bind_addr = SocketAddr::new(args.bind, args.port);
    let endpoint = network::bind_server(bind_addr).await?;
    output::info(&format!(
        "Listening on {} (QUIC / io_uring)",
        endpoint.local_addr()?
    ));
    output::info("Waiting for incoming connection...");

    loop {
        let incoming = match endpoint.wait_incoming().await {
            Some(i) => i,
            None => break,
        };
        let conn = match incoming.await {
            Ok(c) => c,
            Err(e) => {
                output::err(&format!("Connection failed: {e}"));
                continue;
            }
        };
        let peer = conn.remote_address();
        output::ok(&format!("Connection from {peer}"));

        let (mut send_stream, mut recv_stream) = match conn.accept_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                output::err(&format!("Failed to accept streams: {e}"));
                continue;
            }
        };

        let (key, meta) =
            match handshake_receiver_split(&mut send_stream, &mut recv_stream, None).await {
                Ok(r) => r,
                Err(e) => {
                    output::err(&format!("Handshake failed: {e}"));
                    continue;
                }
            };

        let accept = if args.auto_accept {
            true
        } else {
            prompt_accept(&meta, peer)?
        };

        transfer::send_consent_write(&mut send_stream, accept).await?;
        if !accept {
            output::warn("Transfer rejected.");
            conn.close(0u32.into(), b"rejected");
            continue;
        }

        output::ok(&format!("Receiving: {}", meta.filename));

        let dest = resolve_output(&args.output, &meta)?;
        let start = Instant::now();

        let pb = if args.no_progress || meta.total_size == 0 {
            None
        } else {
            Some(output::progress_bar(meta.total_size))
        };

        let pb_clone = pb.clone();
        let checksum = transfer::receive_payload_split(
            &key,
            &mut recv_stream,
            &dest,
            meta.transfer_type,
            move |bytes| {
                if let Some(pb) = &pb_clone {
                    pb.set_position(bytes);
                }
            },
        )
        .await?;

        if let Some(pb) = &pb {
            pb.finish_and_clear();
        }

        let elapsed = start.elapsed().as_secs_f64();
        output::print_transfer_summary(&meta.filename, meta.total_size, elapsed, &checksum, false);
        conn.close(0u32.into(), b"complete");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn handshake_receiver_split(
    send: &mut compio_quic::SendStream,
    recv: &mut compio_quic::RecvStream,
    passphrase: Option<&str>,
) -> Result<([u8; 32], Metadata), EngineError> {
    // 1. Version
    let compio::BufResult(result, vbuf) = recv.read_exact(vec![0u8; 2]).await;
    result.map_err(EngineError::Io)?;
    let remote_ver = u16::from_be_bytes([vbuf[0], vbuf[1]]);
    if remote_ver != PROTOCOL_VERSION {
        return Err(EngineError::ProtocolMismatch {
            local: PROTOCOL_VERSION,
            remote: remote_ver,
        });
    }

    // 2. Key exchange
    let compio::BufResult(result, peer_pub_vec) = recv.read_exact(vec![0u8; 32]).await;
    result.map_err(EngineError::Io)?;
    let mut peer_pub = [0u8; 32];
    peer_pub.copy_from_slice(&peer_pub_vec);

    let (secret, our_pub) = crypto::generate_keypair();
    let compio::BufResult(result, _) = send.write_all(our_pub.to_vec()).await;
    result.map_err(EngineError::Io)?;
    let key = crypto::derive_key(secret, &peer_pub, passphrase)?;

    // 3. Metadata
    let compio::BufResult(result, lbuf) = recv.read_exact(vec![0u8; 4]).await;
    result.map_err(EngineError::Io)?;
    let enc_len = u32::from_be_bytes([lbuf[0], lbuf[1], lbuf[2], lbuf[3]]) as usize;
    if enc_len == 0 || enc_len > MAX_METADATA_ENCRYPTED {
        return Err(EngineError::InvalidFrame(format!(
            "invalid metadata length: {enc_len}"
        )));
    }
    let compio::BufResult(result, enc) = recv.read_exact(vec![0u8; enc_len]).await;
    result.map_err(EngineError::Io)?;
    let plain = match crypto::decrypt_metadata(&key, &enc) {
        Ok(p) => p,
        Err(e) => {
            if passphrase.is_some() {
                return Err(EngineError::InvalidPassphrase);
            } else {
                return Err(e);
            }
        }
    };
    let meta = Metadata::decode(&plain)?;

    Ok((key, meta))
}

fn prompt_accept(meta: &Metadata, peer: SocketAddr) -> Result<bool> {
    let kind = if meta.transfer_type == TRANSFER_DIR {
        "directory"
    } else {
        "file"
    };
    output::info(&format!(
        "Incoming {kind}: \"{}\" from {peer}",
        meta.filename
    ));
    print!("   Accept? [y/N]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn resolve_output(output_dir: &std::path::Path, meta: &Metadata) -> Result<PathBuf> {
    let name = std::path::Path::new(&meta.filename)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
    Ok(output_dir.join(name))
}
