//! `hayate send` subcommand.

use std::{io, net::ToSocketAddrs, path::Path, time::Instant};

use anyhow::{Context, Result, bail};
use compio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use hayate_engine::{
    EngineError, crypto, network,
    protocol::{Metadata, PROTOCOL_VERSION, TRANSFER_DIR, TRANSFER_FILE},
    transfer,
};

use crate::{cli::SendArgs, output};

pub async fn run(args: SendArgs) -> Result<()> {
    output::print_banner();

    let path = &args.path;
    if !path.exists() {
        bail!("Path does not exist: {}", path.display());
    }

    let target = args.peer.as_ref().or(args.target.as_ref());

    let (phrase, print_instruction) = if let Some(code) = &args.code {
        (code.clone(), false)
    } else if target.is_none() {
        let p = crate::words::generate_phrase();
        (p, true)
    } else {
        (String::new(), false)
    };

    let (conn, passphrase) = if let Some(target_str) = target {
        let target_addr = target_str
            .to_socket_addrs()
            .context("invalid target address")?
            .next()
            .context("could not resolve target")?;

        output::info(&format!("Connecting to {target_addr}..."));

        let endpoint = network::bind_client().await?;
        let conn = endpoint
            .connect(target_addr, "hayate.local", Some(network::client_config()?))?
            .await?;
        (conn, args.code.clone())
    } else {
        if print_instruction {
            output::warn(&format!(
                "Waiting for receiver. Run:\n   hayate receive --code \"{}\"\n",
                phrase
            ));
        } else {
            output::info(&format!(
                "Waiting for receiver pairing with code \"{}\"...",
                phrase
            ));
        }

        let bind_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
        let endpoint = network::bind_server(bind_addr).await?;
        let local_port = endpoint.local_addr()?.port();

        let phrase_clone = phrase.clone();
        compio::runtime::spawn(async move {
            let channel_id = hayate_engine::discovery::derive_channel_id(&phrase_clone);
            let _ = hayate_engine::discovery::start_broadcaster(channel_id, local_port).await;
        })
        .detach();

        let incoming = endpoint
            .wait_incoming()
            .await
            .context("endpoint closed while waiting for pairing")?;
        let conn = incoming.await?;
        (conn, Some(phrase))
    };

    output::ok(&format!("Connected to {}", conn.remote_address()));

    let (mut send_stream, mut recv_stream) = conn.open_bi()?;

    let (meta, total_size) = build_metadata(path)?;
    output::info(&format!(
        "Sending: {} ({} bytes, compress={})",
        meta.filename,
        meta.total_size,
        if args.compress { "zstd" } else { "off" }
    ));

    let key = handshake_sender_split(
        &mut send_stream,
        &mut recv_stream,
        &meta,
        passphrase.as_deref(),
    )
    .await?;

    let pb = if args.no_progress || total_size == 0 {
        None
    } else {
        Some(output::progress_bar(total_size))
    };

    let start = Instant::now();

    let checksum = if path.is_dir() {
        send_directory(path, &key, &mut send_stream, args.compress, |b| {
            if let Some(pb) = &pb {
                pb.set_position(b);
            }
        })
        .await?
    } else {
        send_file(path, &key, &mut send_stream, args.compress, |b| {
            if let Some(pb) = &pb {
                pb.set_position(b);
            }
        })
        .await?
    };

    send_stream.finish()?;

    // Wait for receiver to finish processing and close the connection
    let drain_buf = vec![0u8; 1];
    let _ = recv_stream.read(drain_buf).await;

    if let Some(pb) = &pb {
        pb.finish_and_clear();
    }

    let elapsed = start.elapsed().as_secs_f64();
    output::print_transfer_summary(
        &meta.filename,
        total_size,
        elapsed,
        &checksum,
        args.compress,
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_metadata(path: &Path) -> Result<(Metadata, u64)> {
    let filename = path
        .file_name()
        .context("path has no filename")?
        .to_string_lossy()
        .into_owned();

    if path.is_dir() {
        let total = hayate_engine::tar::estimate_dir_size(path);
        Ok((
            Metadata {
                filename,
                total_size: total,
                transfer_type: TRANSFER_DIR,
            },
            total,
        ))
    } else {
        let total = std::fs::metadata(path)?.len();
        Ok((
            Metadata {
                filename,
                total_size: total,
                transfer_type: TRANSFER_FILE,
            },
            total,
        ))
    }
}

async fn handshake_sender_split(
    send: &mut compio_quic::SendStream,
    recv: &mut compio_quic::RecvStream,
    meta: &Metadata,
    passphrase: Option<&str>,
) -> Result<[u8; 32]> {
    // 1. Version
    let compio::BufResult(result, _) = send
        .write_all(PROTOCOL_VERSION.to_be_bytes().to_vec())
        .await;
    result.map_err(EngineError::Io)?;

    // 2. Key exchange
    let (secret, our_pub) = crypto::generate_keypair();
    let compio::BufResult(result, _) = send.write_all(our_pub.to_vec()).await;
    result.map_err(EngineError::Io)?;

    let compio::BufResult(result, peer_pub_vec) = recv.read_exact(vec![0u8; 32]).await;
    result.map_err(EngineError::Io)?;
    let mut peer_pub = [0u8; 32];
    peer_pub.copy_from_slice(&peer_pub_vec);
    let key = crypto::derive_key(secret, &peer_pub, passphrase)?;

    // 3. Encrypted metadata
    let enc = crypto::encrypt_metadata(&key, &meta.encode())?;
    let compio::BufResult(result, _) = send
        .write_all((enc.len() as u32).to_be_bytes().to_vec())
        .await;
    result.map_err(EngineError::Io)?;
    let compio::BufResult(result, _) = send.write_all(enc).await;
    result.map_err(EngineError::Io)?;

    // 4. Consent
    let compio::BufResult(result, consent) = recv.read_exact(vec![0u8; 1]).await;
    result.map_err(EngineError::Io)?;
    match consent[0] {
        0x01 => Ok(key),
        0x00 => Err(EngineError::TransferRejected.into()),
        b => Err(EngineError::InvalidFrame(format!("bad consent 0x{b:02x}")).into()),
    }
}

async fn send_file(
    path: &Path,
    key: &[u8; 32],
    stream: &mut compio_quic::SendStream,
    compress: bool,
    progress_cb: impl FnMut(u64),
) -> Result<String> {
    let file = compio::fs::File::open(path).await?;
    let source = hayate_engine::transfer::PayloadSource::File { file, pos: 0 };
    let filename = path.file_name().and_then(|s| s.to_str());
    Ok(transfer::send_payload_write(key, source, stream, compress, filename, progress_cb).await?)
}

async fn send_directory(
    dir: &Path,
    key: &[u8; 32],
    stream: &mut compio_quic::SendStream,
    compress: bool,
    progress_cb: impl FnMut(u64),
) -> Result<String> {
    let (tx, rx) = flume::bounded::<Result<Vec<u8>, io::Error>>(8);
    let dir_clone = dir.to_path_buf();

    std::thread::spawn(move || {
        struct ChanWriter {
            tx: flume::Sender<Result<Vec<u8>, io::Error>>,
        }
        impl io::Write for ChanWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.tx
                    .send(Ok(buf.to_vec()))
                    .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "receiver gone"))?;
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = ChanWriter { tx: tx.clone() };
        if let Err(e) = hayate_engine::tar::write_tar_sync(&dir_clone, &mut writer) {
            let _ = tx.send(Err(e));
        }
    });

    let source = hayate_engine::transfer::PayloadSource::Channel(rx);
    Ok(transfer::send_payload_write(key, source, stream, compress, None, progress_cb).await?)
}
