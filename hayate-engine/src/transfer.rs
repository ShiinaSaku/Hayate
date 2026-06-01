//! Transfer pipeline: handshake, send, receive.
//!
//! ## compio I/O model
//!
//! compio is completion-based (io_uring / IOCP).  The kernel holds a
//! reference to the I/O buffer until the completion event fires, so every
//! buffer must be **owned** and passed by value to the I/O call.  The
//! return type is `BufResult<T, B>` = `(Result<T, io::Error>, B)` where `B`
//! is the buffer returned after the kernel is done with it.

use std::{io, path::Path};

use compio::io::{AsyncReadAt, AsyncReadExt, AsyncWriteAtExt, AsyncWriteExt};

use crate::{
    EngineError, crypto,
    protocol::{
        CHUNK_SIZE, FRAME_RAW, FRAME_ZSTD, MAX_METADATA_ENCRYPTED, Metadata, PROTOCOL_VERSION,
        TRANSFER_DIR, TRANSFER_FILE,
    },
};

// ---------------------------------------------------------------------------
// Non-blocking Payload Sources and Sinks
// ---------------------------------------------------------------------------

pub enum PayloadSource {
    File { file: compio::fs::File, pos: u64 },
    Channel(flume::Receiver<Result<Vec<u8>, io::Error>>),
}

pub enum PayloadSink {
    File { file: compio::fs::File, pos: u64 },
    Channel(flume::Sender<Vec<u8>>),
}

// ---------------------------------------------------------------------------
// Internal I/O helpers
// ---------------------------------------------------------------------------

/// Read exactly `N` bytes from `stream` into a fresh `Vec<u8>`.
async fn read_exact_n<S: AsyncReadExt + Unpin>(
    stream: &mut S,
    n: usize,
) -> Result<Vec<u8>, EngineError> {
    let buf = vec![0u8; n];
    let compio::BufResult(result, buf) = stream.read_exact(buf).await;
    result.map_err(EngineError::Io)?;
    Ok(buf)
}

/// Write `data` to `stream`.
async fn write_all_owned<S: AsyncWriteExt + Unpin>(
    stream: &mut S,
    data: Vec<u8>,
) -> Result<(), EngineError> {
    let compio::BufResult(result, _) = stream.write_all(data).await;
    result.map_err(EngineError::Io)
}

/// Read a `u16` from the stream.
async fn read_u16<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<u16, EngineError> {
    let bytes = read_exact_n(stream, 2).await?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Read a `u32` from the stream.
async fn read_u32<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<u32, EngineError> {
    let bytes = read_exact_n(stream, 4).await?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Write a `u16` to the stream.
async fn write_u16<S: AsyncWriteExt + Unpin>(stream: &mut S, v: u16) -> Result<(), EngineError> {
    write_all_owned(stream, v.to_be_bytes().to_vec()).await
}

/// Write a `u32` to the stream.
async fn write_u32<S: AsyncWriteExt + Unpin>(stream: &mut S, v: u32) -> Result<(), EngineError> {
    write_all_owned(stream, v.to_be_bytes().to_vec()).await
}

// ---------------------------------------------------------------------------
// Handshake — sender side
// ---------------------------------------------------------------------------

pub async fn handshake_sender<S>(
    stream: &mut S,
    meta: &Metadata,
    passphrase: Option<&str>,
) -> Result<[u8; 32], EngineError>
where
    S: compio::io::AsyncRead + compio::io::AsyncWrite + Unpin,
{
    // 1. Protocol version
    write_u16(stream, PROTOCOL_VERSION).await?;

    // 2. Key exchange
    let (secret, our_pub) = crypto::generate_keypair();
    write_all_owned(stream, our_pub.to_vec()).await?;

    let peer_pub_bytes = read_exact_n(stream, 32).await?;
    let mut peer_pub = [0u8; 32];
    peer_pub.copy_from_slice(&peer_pub_bytes);

    let key = crypto::derive_key(secret, &peer_pub, passphrase)?;

    // 3. Encrypted metadata
    let encrypted = crypto::encrypt_metadata(&key, &meta.encode())?;
    write_u32(stream, encrypted.len() as u32).await?;
    write_all_owned(stream, encrypted).await?;

    // 4. Consent
    let consent = read_exact_n(stream, 1).await?;
    match consent[0] {
        0x01 => Ok(key),
        0x00 => Err(EngineError::TransferRejected),
        other => Err(EngineError::InvalidFrame(format!(
            "unexpected consent byte 0x{other:02x}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Handshake — receiver side
// ---------------------------------------------------------------------------

pub async fn handshake_receiver<S>(
    stream: &mut S,
    passphrase: Option<&str>,
) -> Result<([u8; 32], Metadata), EngineError>
where
    S: compio::io::AsyncRead + compio::io::AsyncWrite + Unpin,
{
    // 1. Version check
    let remote = read_u16(stream).await?;
    if remote != PROTOCOL_VERSION {
        return Err(EngineError::ProtocolMismatch {
            local: PROTOCOL_VERSION,
            remote,
        });
    }

    // 2. Key exchange
    let peer_pub_bytes = read_exact_n(stream, 32).await?;
    let mut peer_pub = [0u8; 32];
    peer_pub.copy_from_slice(&peer_pub_bytes);

    let (secret, our_pub) = crypto::generate_keypair();
    write_all_owned(stream, our_pub.to_vec()).await?;

    let key = crypto::derive_key(secret, &peer_pub, passphrase)?;

    // 3. Metadata
    let enc_len = read_u32(stream).await? as usize;
    if enc_len == 0 || enc_len > MAX_METADATA_ENCRYPTED {
        return Err(EngineError::InvalidFrame(format!(
            "invalid metadata length: {enc_len}"
        )));
    }
    let enc = read_exact_n(stream, enc_len).await?;
    let plain = match crypto::decrypt_metadata(&key, &enc) {
        Ok(p) => p,
        Err(e) => {
            if passphrase.is_some() {
                return Err(EngineError::InvalidPassphrase);
            }
            return Err(e);
        }
    };
    let meta = Metadata::decode(&plain)?;
    Ok((key, meta))
}

/// Writes the consent byte (0x01 = accept, 0x00 = reject).
pub async fn send_consent<S>(stream: &mut S, accept: bool) -> Result<(), EngineError>
where
    S: compio::io::AsyncWrite + Unpin,
{
    write_all_owned(stream, vec![u8::from(accept)]).await
}

// ---------------------------------------------------------------------------
// Send payload
// ---------------------------------------------------------------------------

pub async fn send_payload<S>(
    key: &[u8; 32],
    mut source: PayloadSource,
    stream: &mut S,
    compress: bool,
    filename: Option<&str>,
    mut progress_cb: impl FnMut(u64),
) -> Result<String, EngineError>
where
    S: compio::io::AsyncWrite + Unpin,
{
    use ring::digest;

    // Check if we should skip compression based on file extension
    let mut do_compress = compress;
    let ext_opt = if compress {
        filename
            .and_then(|name| std::path::Path::new(name).extension())
            .and_then(|s| s.to_str())
    } else {
        None
    };

    if let Some(ext) = ext_opt {
        let ext_lower = ext.to_lowercase();
        let skipped = [
            "zip", "gz", "zst", "mp4", "mkv", "jpg", "png", "rar", "7z", "bz2", "xz", "br", "webm",
            "webp", "m4v", "mov", "flac", "opus",
        ];
        if skipped.contains(&ext_lower.as_str()) {
            do_compress = false;
        }
    }

    let (chunk_tx, chunk_rx) = flume::bounded::<Result<(usize, Vec<u8>), io::Error>>(16);
    let (hash_tx, hash_rx) = flume::bounded::<String>(1);

    // 1. Spawning Reader Task
    compio::runtime::spawn(async move {
        let mut index = 0;
        let mut ctx = digest::Context::new(&digest::SHA256);
        loop {
            let chunk_res = match &mut source {
                PayloadSource::File { file, pos } => {
                    let buf = vec![0u8; CHUNK_SIZE];
                    let compio::BufResult(result, mut buf) = file.read_at(buf, *pos).await;
                    match result {
                        Ok(n) => {
                            if n == 0 {
                                None
                            } else {
                                *pos += n as u64;
                                buf.truncate(n);
                                Some(Ok(buf))
                            }
                        }
                        Err(e) => Some(Err(e)),
                    }
                }
                PayloadSource::Channel(rx) => match rx.recv_async().await {
                    Ok(Ok(data)) => {
                        if data.is_empty() {
                            None
                        } else {
                            Some(Ok(data))
                        }
                    }
                    Ok(Err(e)) => Some(Err(e)),
                    Err(_) => None,
                },
            };

            match chunk_res {
                Some(Ok(chunk)) => {
                    ctx.update(&chunk);
                    if chunk_tx.send_async(Ok((index, chunk))).await.is_err() {
                        break;
                    }
                    index += 1;
                }
                Some(Err(e)) => {
                    let _ = chunk_tx.send_async(Err(e)).await;
                    break;
                }
                None => break,
            }
        }
        let hash = hex::encode(ctx.finish().as_ref());
        let _ = hash_tx.send(hash);
    })
    .detach();

    // 2. Spawning Worker Pool
    let num_workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .saturating_sub(1)
        .max(2);

    let (result_tx, result_rx) = flume::bounded::<Result<(usize, Vec<u8>, usize), EngineError>>(16);

    for _ in 0..num_workers {
        let chunk_rx = chunk_rx.clone();
        let result_tx = result_tx.clone();
        let key = *key;
        std::thread::spawn(move || {
            while let Ok(res) = chunk_rx.recv() {
                let (index, chunk) = match res {
                    Ok(val) => val,
                    Err(e) => {
                        let _ = result_tx.send(Err(EngineError::Io(e)));
                        break;
                    }
                };

                let plain_frame: Vec<u8> = if do_compress {
                    match zstd::encode_all(chunk.as_slice(), 1) {
                        Ok(compressed) if compressed.len() < chunk.len() => {
                            let mut pf = Vec::with_capacity(1 + compressed.len());
                            pf.push(FRAME_ZSTD);
                            pf.extend_from_slice(&compressed);
                            pf
                        }
                        _ => {
                            let mut pf = Vec::with_capacity(1 + chunk.len());
                            pf.push(FRAME_RAW);
                            pf.extend_from_slice(&chunk);
                            pf
                        }
                    }
                } else {
                    let mut pf = Vec::with_capacity(1 + chunk.len());
                    pf.push(FRAME_RAW);
                    pf.extend_from_slice(&chunk);
                    pf
                };

                let mut enc_buf = Vec::with_capacity(4 + 12 + plain_frame.len() + 16);
                enc_buf.extend_from_slice(&[0u8; 4]); // placeholder for length
                match crypto::encrypt_frame(&key, &plain_frame, &mut enc_buf) {
                    Ok(_) => {
                        let len = (enc_buf.len() - 4) as u32;
                        enc_buf[0..4].copy_from_slice(&len.to_be_bytes());
                        if result_tx.send(Ok((index, enc_buf, chunk.len()))).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = result_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
    }
    // Drop our handle to result_tx so result_rx completes when all workers exit
    drop(result_tx);

    // 3. Ordered writer loop
    let mut pending = std::collections::BTreeMap::new();
    let mut next_index = 0;
    let mut total: u64 = 0;

    while let Ok(res) = result_rx.recv_async().await {
        let (index, frame, plaintext_len) = res?;
        pending.insert(index, (frame, plaintext_len));
        while let Some((frame, p_len)) = pending.remove(&next_index) {
            write_all_owned(stream, frame).await?;
            total += p_len as u64;
            progress_cb(total);
            next_index += 1;
        }
    }

    let hash = hash_rx.recv_async().await.map_err(|_| {
        EngineError::Io(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "hasher task exited prematurely",
        ))
    })?;

    Ok(hash)
}

// ---------------------------------------------------------------------------
// Receive payload
// ---------------------------------------------------------------------------

pub async fn receive_payload<S>(
    key: &[u8; 32],
    stream: &mut S,
    output_path: &Path,
    transfer_type: u8,
    mut progress_cb: impl FnMut(u64) + 'static,
) -> Result<String, EngineError>
where
    S: compio::io::AsyncRead + Unpin,
{
    let (tx, rx) = flume::bounded::<Vec<u8>>(8);

    let extract_handle = if transfer_type == TRANSFER_DIR {
        let out = output_path.to_path_buf();
        Some(std::thread::spawn(move || -> Result<(), EngineError> {
            struct ChanReader {
                rx: flume::Receiver<Vec<u8>>,
                buf: Vec<u8>,
                pos: usize,
            }
            impl io::Read for ChanReader {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    while self.pos >= self.buf.len() {
                        match self.rx.recv() {
                            Ok(chunk) => {
                                self.buf = chunk;
                                self.pos = 0;
                            }
                            Err(_) => return Ok(0),
                        }
                    }
                    let n = std::cmp::min(buf.len(), self.buf.len() - self.pos);
                    buf[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                    self.pos += n;
                    Ok(n)
                }
            }
            crate::tar::extract_tar_sync(
                ChanReader {
                    rx,
                    buf: Vec::new(),
                    pos: 0,
                },
                &out,
            )
        }))
    } else {
        None
    };

    let mut sink = if transfer_type == TRANSFER_FILE {
        let f = compio::fs::File::create(output_path)
            .await
            .map_err(EngineError::Io)?;
        PayloadSink::File { file: f, pos: 0 }
    } else {
        PayloadSink::Channel(tx)
    };

    let (enc_tx, enc_rx) = flume::bounded::<Result<Vec<u8>, io::Error>>(16);
    let (plain_tx, plain_rx) = flume::bounded::<Result<Vec<u8>, EngineError>>(16);

    // 1. Spawning Decryption/Decompression thread
    let key = *key;
    std::thread::spawn(move || {
        let mut decrypted_buf = Vec::with_capacity(CHUNK_SIZE + 256);
        while let Ok(res) = enc_rx.recv() {
            let enc = match res {
                Ok(e) => e,
                Err(e) => {
                    let _ = plain_tx.send(Err(EngineError::Io(e)));
                    break;
                }
            };
            match crypto::decrypt_frame_into(&key, &enc, &mut decrypted_buf) {
                Ok(()) => {
                    if decrypted_buf.is_empty() {
                        let _ = plain_tx.send(Err(EngineError::InvalidFrame(
                            "empty decrypted frame".into(),
                        )));
                        break;
                    }
                    let flag = decrypted_buf[0];
                    let data = &decrypted_buf[1..];
                    let plaintext_res: Result<Vec<u8>, EngineError> = match flag {
                        FRAME_RAW => Ok(data.to_vec()),
                        FRAME_ZSTD => zstd::decode_all(data)
                            .map_err(|e| EngineError::Compression(e.to_string())),
                        other => Err(EngineError::InvalidFrame(format!(
                            "unknown frame flag 0x{other:02x}"
                        ))),
                    };
                    match plaintext_res {
                        Ok(plain) => {
                            if plain_tx.send(Ok(plain)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = plain_tx.send(Err(e));
                            break;
                        }
                    }
                }
                Err(e) => {
                    let _ = plain_tx.send(Err(e));
                    break;
                }
            }
        }
    });

    // 2. Spawning Disk Writer Task
    let write_handle = compio::runtime::spawn(async move {
        use ring::digest;
        let mut ctx = digest::Context::new(&digest::SHA256);
        let mut total: u64 = 0;
        let mut pos: u64 = 0;

        while let Ok(res) = plain_rx.recv_async().await {
            let plaintext = res?;
            ctx.update(&plaintext);
            total += plaintext.len() as u64;
            let plaintext_len = plaintext.len() as u64;

            match &mut sink {
                PayloadSink::File { file, pos: _ } => {
                    let compio::BufResult(result, _) = file.write_all_at(plaintext, pos).await;
                    result.map_err(EngineError::Io)?;
                    pos += plaintext_len;
                }
                PayloadSink::Channel(tx) => {
                    tx.send_async(plaintext).await.map_err(|_| {
                        EngineError::Io(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "extractor thread exited",
                        ))
                    })?;
                }
            }
            progress_cb(total);
        }

        // Wait for extraction to finish if this is a directory transfer
        if let Some(handle) = extract_handle {
            // Drop the channel sender so the extractor thread sees EOF
            if let PayloadSink::Channel(tx) = sink {
                drop(tx);
            }
            handle.join().map_err(|e| {
                let msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    (*s).to_owned()
                } else {
                    "unknown panic".to_owned()
                };
                EngineError::Io(io::Error::other(format!("extractor panicked: {msg}")))
            })??;
        }

        let hash = hex::encode(ctx.finish().as_ref());
        Ok::<_, EngineError>(hash)
    });

    // 3. Main network reader loop
    let mut read_error = None;
    loop {
        let len_buf_owned = vec![0u8; 4];
        let compio::BufResult(result, len_buf) = stream.read_exact(len_buf_owned).await;
        match result {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                read_error = Some(e);
                break;
            }
        }
        let frame_len =
            u32::from_be_bytes([len_buf[0], len_buf[1], len_buf[2], len_buf[3]]) as usize;

        if frame_len == 0 || frame_len > (CHUNK_SIZE * 2 + 256) {
            read_error = Some(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame length out of range: {frame_len}"),
            ));
            break;
        }

        let enc_owned = vec![0u8; frame_len];
        let compio::BufResult(result, enc) = stream.read_exact(enc_owned).await;
        match result {
            Ok(()) => {
                if enc_tx.send_async(Ok(enc)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                read_error = Some(e);
                break;
            }
        }
    }

    if let Some(e) = read_error {
        let _ = enc_tx.send_async(Err(e)).await;
    }

    // Drop sender handles so threads/tasks know EOF is reached
    drop(enc_tx);

    let hash = write_handle.await.unwrap()?;
    Ok(hash)
}

// ---------------------------------------------------------------------------
// Split-stream wrappers (compio-quic SendStream / RecvStream)
// ---------------------------------------------------------------------------

pub async fn send_payload_write(
    key: &[u8; 32],
    source: PayloadSource,
    stream: &mut compio_quic::SendStream,
    compress: bool,
    filename: Option<&str>,
    progress_cb: impl FnMut(u64),
) -> Result<String, EngineError> {
    send_payload(key, source, stream, compress, filename, progress_cb).await
}

pub async fn receive_payload_split(
    key: &[u8; 32],
    stream: &mut compio_quic::RecvStream,
    output_path: &Path,
    transfer_type: u8,
    progress_cb: impl FnMut(u64) + 'static,
) -> Result<String, EngineError> {
    receive_payload(key, stream, output_path, transfer_type, progress_cb).await
}

pub async fn send_consent_write(
    stream: &mut compio_quic::SendStream,
    accept: bool,
) -> Result<(), EngineError> {
    send_consent(stream, accept).await
}
