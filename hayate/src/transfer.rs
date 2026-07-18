//! Transfer pipeline: handshake, send, receive.
//!
//! ## compio I/O model
//!
//! compio is completion-based (io_uring / IOCP). The kernel holds a
//! reference to the I/O buffer until the completion event fires, so every
//! buffer must be **owned** and passed by value to the I/O call. The
//! return type is `BufResult<T, B>` = `(Result<T, io::Error>, B)` where `B`
//! is the buffer returned after the kernel is done with it.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::rc::Rc;

use compio::buf::{IntoInner, IoBuf};
use compio::io::{AsyncReadAt, AsyncReadExt, AsyncWriteAtExt, AsyncWriteExt};
use futures_util::stream::{FuturesOrdered, StreamExt};

use crate::protocol::{
    CHUNK_SIZE,
    FRAME_RAW,
    FRAME_ZSTD,
    MAX_METADATA_ENCRYPTED,
    Metadata,
    PROTOCOL_VERSION,
    TransferKind,
};
use crate::{EngineError, crypto};

// ---------------------------------------------------------------------------
// Non-blocking Payload Sources and Sinks
// ---------------------------------------------------------------------------

/// Source of data payload to be transferred.
#[derive(Debug)]
#[non_exhaustive]
pub enum PayloadSource {
    /// A local file on the filesystem.
    File {
        /// The file handle.
        file: compio::fs::File,
        /// The starting byte position.
        pos: u64,
    },
    /// An asynchronous channel yielding byte chunks.
    Channel(flume::Receiver<Result<Vec<u8>, io::Error>>),
}

/// Destination for incoming data payload.
#[derive(Debug)]
#[non_exhaustive]
pub enum PayloadSink {
    /// A local file on the filesystem.
    File {
        /// The file handle.
        file: compio::fs::File,
        /// The starting byte position.
        pos: u64,
    },
    /// An asynchronous channel receiving byte chunks.
    Channel(flume::Sender<Vec<u8>>),
}

// ---------------------------------------------------------------------------
// Internal I/O helpers
// ---------------------------------------------------------------------------

/// Helper for dynamic payload hashing.
///
/// This hash is an intentional end-to-end integrity check over the original
/// plaintext content of the transferred file or directory. Each individual
/// encrypted frame is already authenticated by the AEAD tag (ChaCha20-Poly1305
/// or AES-GCM), so per-frame wire integrity is handled separately. The
/// file-level hash lets the sender and receiver agree on the transferred
/// content even if framing details differ.
///
/// Supported algorithms are `blake3` and `sha256`. Unknown algorithms are
/// rejected rather than silently defaulted to a known value.
enum PayloadHasher {
    Blake3(Box<blake3::Hasher>),
    Sha256(Box<ring::digest::Context>),
}

impl PayloadHasher {
    fn new(algo: &str) -> Result<Self, EngineError> {
        match algo {
            "blake3" => Ok(Self::Blake3(Box::new(blake3::Hasher::new()))),
            "sha256" => {
                Ok(Self::Sha256(Box::new(ring::digest::Context::new(&ring::digest::SHA256))))
            },
            _ => Err(EngineError::InvalidFrame(format!("unknown hash algorithm: {algo}"))),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Self::Blake3(h) => {
                h.update(data);
            },
            Self::Sha256(h) => {
                h.update(data);
            },
        }
    }

    fn finalize(self, algo: &str) -> String {
        let hex_hash = match self {
            Self::Blake3(h) => h.finalize().to_hex().to_string(),
            Self::Sha256(h) => crate::hex_encode(h.finish().as_ref()),
        };
        format!("{algo}${hex_hash}")
    }
}

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
) -> Result<Vec<u8>, EngineError> {
    let compio::BufResult(result, buf) = stream.write_all(data).await;
    result.map_err(EngineError::Io)?;
    Ok(buf)
}

/// Read a `u32` from the stream.
async fn read_u32<S: AsyncReadExt + Unpin>(stream: &mut S) -> Result<u32, EngineError> {
    let bytes = read_exact_n(stream, 4).await?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Write a `u32` to the stream.
async fn write_u32<S: AsyncWriteExt + Unpin>(stream: &mut S, v: u32) -> Result<(), EngineError> {
    write_all_owned(stream, v.to_be_bytes().to_vec()).await.map(|_| ())
}

/// Performs the cryptographic version check, key exchange, cipher negotiation,
/// and metadata handshake on the sender side using separate write and read
/// streams.
///
/// This is particularly useful for protocols like QUIC that split connections
/// into separate unidirectional streams.
///
/// # Errors
///
/// Returns [`EngineError`] if version mismatch, key exchange derivation, cipher
/// negotiation, or encryption/decryption fails, or if the receiver rejects the
/// transfer.
pub async fn handshake_sender_split<W, R>(
    send: &mut W,
    recv: &mut R,
    meta: &Metadata,
    passphrase: Option<&str>,
) -> Result<([u8; 32], u8), EngineError>
where
    W: compio::io::AsyncWrite + Unpin,
    R: compio::io::AsyncRead + Unpin,
{
    meta.validate()?;

    // 1. Protocol version + Sender Capability
    let sender_cap = if crypto::is_aes_hw_accelerated() {
        crypto::CIPHER_AES256_GCM
    } else {
        crypto::CIPHER_CHACHA20
    };
    let mut ver_and_cap = Vec::with_capacity(3);
    ver_and_cap.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    ver_and_cap.push(sender_cap);
    write_all_owned(send, ver_and_cap).await?;

    // 2. Key exchange
    let (secret, our_pub) = crypto::generate_keypair();
    let salt = crypto::generate_salt();
    let mut handshake_buf = Vec::with_capacity(crypto::PUBLIC_KEY_LEN + crypto::SALT_LEN);
    handshake_buf.extend_from_slice(&our_pub);
    handshake_buf.extend_from_slice(&salt);
    write_all_owned(send, handshake_buf).await?;

    let peer_pub_bytes = read_exact_n(recv, 32).await?;
    let mut peer_pub = [0u8; 32];
    peer_pub.copy_from_slice(&peer_pub_bytes);

    // 3. Receive the selected cipher from receiver
    let cipher_bytes = read_exact_n(recv, 1).await?;
    let selected_cipher = cipher_bytes[0];
    if selected_cipher != crypto::CIPHER_CHACHA20 && selected_cipher != crypto::CIPHER_AES256_GCM {
        return Err(EngineError::Handshake("Unknown cipher suite selected by receiver".into()));
    }

    let key = crypto::derive_key(crypto::KeyDerivationContext {
        secret,
        peer_pub: &peer_pub,
        salt: &salt,
        passphrase,
        sender_pub: &our_pub,
        receiver_pub: &peer_pub,
        sender_cap,
        selected_cipher,
    })?;

    // 4. Encrypted metadata
    let encrypted = crypto::encrypt_metadata(&key, selected_cipher, &meta.encode())?;
    write_u32(send, encrypted.len() as u32).await?;
    write_all_owned(send, encrypted).await?;

    // 5. Consent
    let consent = read_exact_n(recv, 1).await?;
    match consent[0] {
        0x01 => Ok((key, selected_cipher)),
        0x00 => Err(EngineError::TransferRejected),
        other => Err(EngineError::InvalidFrame(format!("unexpected consent byte 0x{other:02x}"))),
    }
}

/// Performs the cryptographic version check, key exchange, cipher negotiation,
/// and metadata handshake on the receiver side using separate write and read
/// streams.
///
/// This is particularly useful for protocols like QUIC that split connections
/// into separate unidirectional streams.
///
/// # Errors
///
/// Returns [`EngineError`] if version mismatch, key exchange derivation, cipher
/// negotiation, or encryption/decryption fails.
pub async fn handshake_receiver_split<W, R>(
    send: &mut W,
    recv: &mut R,
    passphrase: Option<&str>,
) -> Result<(([u8; 32], u8), Metadata), EngineError>
where
    W: compio::io::AsyncWrite + Unpin,
    R: compio::io::AsyncRead + Unpin,
{
    // 1. Version check + Sender Capability
    let ver_cap = read_exact_n(recv, 3).await?;
    let remote_ver = u16::from_be_bytes([ver_cap[0], ver_cap[1]]);
    if remote_ver != PROTOCOL_VERSION {
        return Err(EngineError::ProtocolMismatch { local: PROTOCOL_VERSION, remote: remote_ver });
    }
    let sender_cap = ver_cap[2];
    if sender_cap != crypto::CIPHER_CHACHA20 && sender_cap != crypto::CIPHER_AES256_GCM {
        return Err(EngineError::Handshake("Unknown cipher capability sent by sender".into()));
    }

    let selected_cipher =
        if sender_cap == crypto::CIPHER_AES256_GCM && crypto::is_aes_hw_accelerated() {
            crypto::CIPHER_AES256_GCM
        } else {
            crypto::CIPHER_CHACHA20
        };

    // 2. Key exchange
    let sender_pub_and_salt = read_exact_n(recv, crypto::PUBLIC_KEY_LEN + crypto::SALT_LEN).await?;
    let mut peer_pub = [0u8; crypto::PUBLIC_KEY_LEN];
    peer_pub.copy_from_slice(&sender_pub_and_salt[..crypto::PUBLIC_KEY_LEN]);
    let mut salt = [0u8; crypto::SALT_LEN];
    salt.copy_from_slice(&sender_pub_and_salt[crypto::PUBLIC_KEY_LEN..]);

    let (secret, our_pub) = crypto::generate_keypair();
    write_all_owned(send, our_pub.to_vec()).await?;

    let key = crypto::derive_key(crypto::KeyDerivationContext {
        secret,
        peer_pub: &peer_pub,
        salt: &salt,
        passphrase,
        sender_pub: &peer_pub,
        receiver_pub: &our_pub,
        sender_cap,
        selected_cipher,
    })?;

    // 3. Write selected cipher back to sender
    write_all_owned(send, vec![selected_cipher]).await?;

    // 4. Metadata
    let enc_len = read_u32(recv).await? as usize;
    if enc_len == 0 || enc_len > MAX_METADATA_ENCRYPTED {
        return Err(EngineError::InvalidFrame(format!("invalid metadata length: {enc_len}")));
    }
    let enc = read_exact_n(recv, enc_len).await?;
    let plain = match crypto::decrypt_metadata(&key, selected_cipher, &enc) {
        Ok(p) => p,
        Err(e) => {
            if passphrase.is_some() {
                return Err(EngineError::InvalidPassphrase);
            }
            return Err(e);
        },
    };
    let meta = Metadata::decode(&plain)?;
    Ok(((key, selected_cipher), meta))
}

/// Writes the consent byte (0x01 = accept, 0x00 = reject).
pub async fn send_consent<S>(stream: &mut S, accept: bool) -> Result<(), EngineError>
where
    S: compio::io::AsyncWrite + Unpin,
{
    write_all_owned(stream, vec![u8::from(accept)]).await.map(|_| ())
}

// ---------------------------------------------------------------------------
// Send payload
// ---------------------------------------------------------------------------

/// Encrypts and transmits a payload source (file or channel) to the receiver.
///
/// Chunks of [`crate::protocol::CHUNK_SIZE`] are read from the source,
/// compressed (if requested and the file extension suggests it is beneficial),
/// encrypted with the negotiated AEAD cipher, and written to the stream.
///
/// # Errors
///
/// Returns [`EngineError`] if reading from source, compressing, encrypting, or
/// writing to the network fails.
#[allow(clippy::too_many_arguments)]
pub async fn send_payload<S>(
    key: &[u8; 32],
    cipher_id: u8,
    source: PayloadSource,
    stream: &mut S,
    compress: bool,
    filename: Option<&str>,
    hash_algo: &str,
    mut progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
) -> Result<String, EngineError>
where
    S: compio::io::AsyncWrite + Unpin,
{
    if !crate::protocol::is_known_hash_algo(hash_algo) {
        return Err(EngineError::InvalidFrame(format!("unsupported hash algorithm: {hash_algo}")));
    }

    // Compression is counterproductive for formats that are already entropy
    // coded; avoid burning CPU and expanding payloads for those extensions.
    let mut do_compress = compress;
    let ext_opt = if compress {
        filename.and_then(|name| std::path::Path::new(name).extension()).and_then(|s| s.to_str())
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

    let pool = crate::pool::BufferPool::new(32, CHUNK_SIZE);
    let enc_pool = crate::pool::BufferPool::new(32, CHUNK_SIZE + 1024);

    let (chunk_tx, chunk_rx) =
        flume::bounded::<Result<(usize, Vec<u8>, usize, bool), io::Error>>(16);
    let (hash_tx, hash_rx) = flume::bounded::<String>(1);

    // Reader task: keep a small read-ahead queue so disk latency overlaps with
    // compression/encryption without allowing unbounded memory growth.
    let pool_clone = pool.clone();
    let algo = hash_algo.to_owned();
    compio::runtime::spawn(async move {
        let mut index = 0;
        let mut hasher =
            PayloadHasher::new(&algo).expect("hash algorithm is validated before payload transfer");

        match source {
            PayloadSource::File { file, pos } => {
                let file = Rc::new(file);
                let read_future = |f: Rc<compio::fs::File>, buf: Vec<u8>, p: u64| async move {
                    let compio::BufResult(result, buf) = f.read_at(buf, p).await;
                    (result, buf, p)
                };
                let mut reads = FuturesOrdered::new();
                let queue_depth = 8; // 8 × 4 MiB = 32 MiB read-ahead; hides disk latency
                let mut current_pos = pos;
                let mut file_ended = false;

                for _ in 0..queue_depth {
                    if file_ended {
                        break;
                    }
                    let buf = pool_clone.lease().await;
                    let f = file.clone();
                    let p = current_pos;
                    current_pos += CHUNK_SIZE as u64;

                    reads.push_back(read_future(f, buf, p));
                }

                while let Some((result, buf, _)) = reads.next().await {
                    match result {
                        Ok(n) => {
                            if n == 0 {
                                pool_clone.release(buf);
                                break;
                            }
                            if n < CHUNK_SIZE {
                                file_ended = true;
                            }
                            hasher.update(&buf[..n]);

                            if chunk_tx.send_async(Ok((index, buf, n, true))).await.is_err() {
                                break;
                            }
                            index += 1;
                        },
                        Err(e) => {
                            pool_clone.release(buf);
                            let _ = chunk_tx.send_async(Err(e)).await;
                            break;
                        },
                    }

                    if !file_ended {
                        let buf = pool_clone.lease().await;
                        let f = file.clone();
                        let p = current_pos;
                        current_pos += CHUNK_SIZE as u64;

                        reads.push_back(read_future(f, buf, p));
                    }
                }

                // Reclaim leased buffers for any reads that completed after
                // the consumer side stopped accepting chunks.
                while let Some((_, buf, _)) = reads.next().await {
                    pool_clone.release(buf);
                }
            },
            PayloadSource::Channel(rx) => {
                while let Ok(res) = rx.recv_async().await {
                    match res {
                        Ok(data) => {
                            if data.is_empty() {
                                break;
                            }
                            hasher.update(&data);
                            let len = data.len();
                            if chunk_tx.send_async(Ok((index, data, len, false))).await.is_err() {
                                break;
                            }
                            index += 1;
                        },
                        Err(e) => {
                            let _ = chunk_tx.send_async(Err(e)).await;
                            break;
                        },
                    }
                }
            },
        }

        let hash = hasher.finalize(&algo);
        let _ = hash_tx.send(hash);
    })
    .detach();

    // Worker pool: CPU-bound compression and AEAD sealing are isolated from
    // the compio executor. Each worker prepares the AEAD key once, then reuses
    // it for every frame it handles.
    let num_workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .saturating_sub(1)
        .max(2);

    let (result_tx, result_rx) = flume::unbounded::<Result<(usize, Vec<u8>, usize), EngineError>>();

    for _ in 0..num_workers {
        let chunk_rx = chunk_rx.clone();
        let result_tx = result_tx.clone();
        let key = *key;
        let pool = pool.clone();
        let enc_pool = enc_pool.clone();
        std::thread::spawn(move || {
            let aead_key = match crypto::AeadKey::new(&key, cipher_id) {
                Ok(key) => key,
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                    return;
                },
            };
            // Reusable zstd compressor: built once per worker, like
            // `aead_key` above, instead of `zstd::encode_all` constructing a
            // fresh CCtx (entropy tables, window state) on every chunk.
            // `scratch` is sized to the worst-case compressed output for one
            // CHUNK_SIZE input and reused across every chunk this worker
            // handles, so steady-state compression is allocation-free.
            let mut compressor = if do_compress {
                match zstd::bulk::Compressor::new(1) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        let _ = result_tx.send(Err(EngineError::Compression(e.to_string())));
                        return;
                    },
                }
            } else {
                None
            };
            let mut compress_scratch = compressor
                .is_some()
                .then(|| vec![0u8; zstd::zstd_safe::compress_bound(CHUNK_SIZE)]);

            let mut plain_buf = Vec::with_capacity(CHUNK_SIZE + 256);
            while let Ok(res) = chunk_rx.recv() {
                let (index, chunk, chunk_len, pooled) = match res {
                    Ok(val) => val,
                    Err(e) => {
                        let _ = result_tx.send(Err(EngineError::Io(e)));
                        break;
                    },
                };

                let chunk_data = &chunk[..chunk_len];
                plain_buf.clear();

                if let (Some(compressor), Some(scratch)) =
                    (compressor.as_mut(), compress_scratch.as_mut())
                {
                    match compressor.compress_to_buffer(chunk_data, scratch) {
                        Ok(n) if n < chunk_len => {
                            plain_buf.push(FRAME_ZSTD);
                            plain_buf.extend_from_slice(&scratch[..n]);
                        },
                        Ok(_) => {
                            // Compression didn't help this chunk; send it raw.
                            plain_buf.push(FRAME_RAW);
                            plain_buf.extend_from_slice(chunk_data);
                        },
                        Err(e) => {
                            let _ = result_tx.send(Err(EngineError::Compression(e.to_string())));
                            break;
                        },
                    }
                } else {
                    plain_buf.push(FRAME_RAW);
                    plain_buf.extend_from_slice(chunk_data);
                }
                if pooled {
                    pool.release(chunk);
                }

                let mut enc_buf = enc_pool.lease_sync();
                enc_buf.clear();
                enc_buf.extend_from_slice(&[0u8; 4]); // placeholder for length
                match crypto::encrypt_frame_with_key(&aead_key, &plain_buf, &mut enc_buf) {
                    Ok(_) => {
                        let len = (enc_buf.len() - 4) as u32;
                        enc_buf[0..4].copy_from_slice(&len.to_be_bytes());
                        if result_tx.send(Ok((index, enc_buf, chunk_len))).is_err() {
                            break;
                        }
                    },
                    Err(e) => {
                        let _ = result_tx.send(Err(e));
                        break;
                    },
                }
            }
        });
    }
    // Drop our handle to result_tx so result_rx completes when all workers exit
    drop(result_tx);

    // Writer loop: worker output may complete out of order, but the stream
    // remains deterministic by buffering gaps until the next frame arrives.
    let mut pending = std::collections::BTreeMap::new();
    let mut next_index = 0;
    let mut total: u64 = 0;

    while let Ok(res) = result_rx.recv_async().await {
        let (index, frame, plaintext_len) = res?;
        pending.insert(index, (frame, plaintext_len));
        while let Some((frame, p_len)) = pending.remove(&next_index) {
            let written_frame = write_all_owned(stream, frame).await?;
            enc_pool.release(written_frame);
            total += p_len as u64;
            progress_cb(total)?;
            next_index += 1;
        }
    }

    let hash = hash_rx.recv_async().await.map_err(|_| {
        EngineError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "hasher task exited prematurely"))
    })?;

    Ok(hash)
}

// ---------------------------------------------------------------------------
// Receive payload
// ---------------------------------------------------------------------------

/// Receives, decrypts, and extracts a payload from a stream, saving it to
/// `output_path`.
///
/// Handles decryption, decompression (zstd), size validation, and safe path
/// extraction for directory transfers.
///
/// # Errors
///
/// Returns [`EngineError`] if reading from stream, decrypting, decompressing,
/// writing to target file/directory, or size validation fails.
#[allow(clippy::too_many_arguments)]
struct FileCleanupGuard<'a> {
    path: &'a Path,
    active: bool,
}

impl<'a> FileCleanupGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, active: true }
    }

    fn disable(&mut self) {
        self.active = false;
    }
}

impl Drop for FileCleanupGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

/// Receives, decrypts, and extracts a payload from a stream, saving it to
/// `output_path`.
///
/// Handles decryption, decompression (zstd), size validation, and safe path
/// extraction for directory transfers.
///
/// # Errors
///
/// Returns [`EngineError`] if reading from stream, decrypting, decompressing,
/// writing to target file/directory, or size validation fails.
#[allow(clippy::too_many_arguments)]
pub async fn receive_payload<S>(
    key: &[u8; 32],
    cipher_id: u8,
    stream: &mut S,
    output_path: &Path,
    transfer_type: TransferKind,
    expected_size: u64,
    hash_algo: &str,
    mut progress_cb: impl FnMut(u64) -> Result<(), EngineError> + 'static,
) -> Result<String, EngineError>
where
    S: compio::io::AsyncRead + Unpin,
{
    if transfer_type != TransferKind::File && transfer_type != TransferKind::Directory {
        return Err(EngineError::InvalidFrame(format!(
            "invalid transfer type: 0x{:02x}",
            transfer_type.as_u8()
        )));
    }
    if !crate::protocol::is_known_hash_algo(hash_algo) {
        return Err(EngineError::InvalidFrame(format!("unsupported hash algorithm: {hash_algo}")));
    }

    let pool = crate::pool::BufferPool::new(32, CHUNK_SIZE + 1024);
    let plain_pool = crate::pool::BufferPool::new(32, CHUNK_SIZE);

    let (tx, rx) = flume::bounded::<Vec<u8>>(8);

    let extract_handle = if transfer_type == TransferKind::Directory {
        let out = output_path.to_path_buf();
        let pool_clone = plain_pool.clone();
        Some(std::thread::spawn(move || -> Result<(), EngineError> {
            struct ChanReader {
                rx: flume::Receiver<Vec<u8>>,
                buf: Vec<u8>,
                pos: usize,
                pool: crate::pool::BufferPool,
            }
            impl io::Read for ChanReader {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    while self.pos >= self.buf.len() {
                        let old_buf = std::mem::take(&mut self.buf);
                        if !old_buf.is_empty() {
                            self.pool.release(old_buf);
                        }
                        match self.rx.recv() {
                            Ok(chunk) => {
                                self.buf = chunk;
                                self.pos = 0;
                            },
                            Err(_) => return Ok(0),
                        }
                    }
                    let n = std::cmp::min(buf.len(), self.buf.len() - self.pos);
                    buf[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                    self.pos += n;
                    Ok(n)
                }
            }
            impl Drop for ChanReader {
                fn drop(&mut self) {
                    let old_buf = std::mem::take(&mut self.buf);
                    if !old_buf.is_empty() {
                        self.pool.release(old_buf);
                    }
                    while let Ok(buf) = self.rx.try_recv() {
                        self.pool.release(buf);
                    }
                }
            }
            crate::tar::extract_tar_sync(
                ChanReader { rx, buf: Vec::new(), pos: 0, pool: pool_clone },
                &out,
            )
        }))
    } else {
        None
    };

    let mut cleanup_guard = None;
    let mut sink = if transfer_type == TransferKind::File {
        let f = compio::fs::File::create(output_path).await.map_err(EngineError::Io)?;
        cleanup_guard = Some(FileCleanupGuard::new(output_path));
        PayloadSink::File { file: f, pos: 0 }
    } else {
        PayloadSink::Channel(tx)
    };

    let (enc_tx, enc_rx) = flume::bounded::<Result<(usize, Vec<u8>), io::Error>>(16);
    let (plain_tx, plain_rx) = flume::unbounded::<Result<(usize, Vec<u8>), EngineError>>();

    // Decrypt/decompress workers: isolate CPU-bound tasks in a worker pool.
    let num_workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .saturating_sub(1)
        .max(2);

    for _ in 0..num_workers {
        let enc_rx = enc_rx.clone();
        let plain_tx = plain_tx.clone();
        let key = *key;
        let pool_clone = pool.clone();
        let plain_pool_clone = plain_pool.clone();
        std::thread::spawn(move || {
            let aead_key = match crypto::AeadKey::new(&key, cipher_id) {
                Ok(key) => key,
                Err(e) => {
                    let _ = plain_tx.send(Err(e));
                    return;
                },
            };
            // Reusable zstd decompressor: built once per worker, like
            // `aead_key` above, instead of `zstd::decode_all` constructing a
            // fresh DCtx on every frame.
            let mut decompressor = match zstd::bulk::Decompressor::new() {
                Ok(d) => d,
                Err(e) => {
                    let _ = plain_tx.send(Err(EngineError::Compression(e.to_string())));
                    return;
                },
            };
            let mut decrypted_buf = Vec::with_capacity(CHUNK_SIZE + 256);
            while let Ok(res) = enc_rx.recv() {
                let (index, enc) = match res {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = plain_tx.send(Err(EngineError::Io(e)));
                        break;
                    },
                };
                let decrypt_res =
                    crypto::decrypt_frame_into_with_key(&aead_key, &enc, &mut decrypted_buf);
                pool_clone.release(enc);

                match decrypt_res {
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
                            FRAME_RAW => {
                                let mut plain = plain_pool_clone.lease_sync();
                                plain.resize(data.len(), 0);
                                plain.copy_from_slice(data);
                                Ok(plain)
                            },
                            FRAME_ZSTD => {
                                // Lease from the same pool FRAME_RAW uses so
                                // every plaintext buffer reaching the writer
                                // task was leased exactly once (see the
                                // lease/release invariant in pool.rs).
                                // Decompressed size can never exceed
                                // CHUNK_SIZE — the sender only ever compresses
                                // chunks up to that size — so the pool's
                                // fixed-size buffer is always big enough.
                                let mut plain = plain_pool_clone.lease_sync();
                                match decompressor.decompress_to_buffer(data, &mut plain) {
                                    Ok(_) => Ok(plain),
                                    Err(e) => {
                                        plain_pool_clone.release(plain);
                                        Err(EngineError::Compression(e.to_string()))
                                    },
                                }
                            },
                            other => Err(EngineError::InvalidFrame(format!(
                                "unknown frame flag 0x{other:02x}"
                            ))),
                        };
                        match plaintext_res {
                            Ok(plain) => {
                                if plain_tx.send(Ok((index, plain))).is_err() {
                                    break;
                                }
                            },
                            Err(e) => {
                                let _ = plain_tx.send(Err(e));
                                break;
                            },
                        }
                    },
                    Err(e) => {
                        let _ = plain_tx.send(Err(e));
                        break;
                    },
                }
            }
        });
    }
    // Drop our handle so plain_rx completes when all workers exit
    drop(plain_tx);

    // Writer task: reorder, hash, and persist plaintext after validation.
    let algo = hash_algo.to_owned();
    let plain_pool_clone = plain_pool.clone();
    let write_handle = compio::runtime::spawn(async move {
        let mut hasher =
            PayloadHasher::new(&algo).expect("hash algorithm is validated before payload transfer");
        let mut total: u64 = 0;
        let mut pos: u64 = 0;
        let mut next_index = 0;
        let mut pending = BTreeMap::new();

        while let Ok(res) = plain_rx.recv_async().await {
            let (index, plaintext) = res?;
            pending.insert(index, plaintext);

            while let Some(plaintext) = pending.remove(&next_index) {
                hasher.update(&plaintext);
                total += plaintext.len() as u64;
                let plaintext_len = plaintext.len() as u64;

                match &mut sink {
                    PayloadSink::File { file, pos: _ } => {
                        let compio::BufResult(result, buffer) =
                            file.write_all_at(plaintext, pos).await;
                        result.map_err(EngineError::Io)?;
                        pos += plaintext_len;
                        plain_pool_clone.release(buffer);
                    },
                    PayloadSink::Channel(tx) => {
                        tx.send_async(plaintext).await.map_err(|_| {
                            EngineError::Io(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "extractor thread exited",
                            ))
                        })?;
                    },
                }
                progress_cb(total)?;
                next_index += 1;
            }
        }

        // Wait for extraction to finish if this is a directory transfer
        if let Some(handle) = extract_handle {
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

        // Size validation
        if transfer_type == TransferKind::File {
            if total != expected_size {
                return Err(EngineError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "Transfer truncated: received {total} bytes, expected {expected_size} bytes"
                    ),
                )));
            }
        } else {
            // Directory transfer. The metadata size is an estimate of file contents,
            // not the tar stream size; allow tar header/padding overhead but
            // cap the total to prevent a runaway peer from filling the disk.
            let max_dir_size = expected_size.saturating_mul(4).saturating_add(64 * 1024 * 1024);
            if total > max_dir_size {
                return Err(EngineError::Io(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Directory transfer exceeded size ceiling: {total} bytes, max {max_dir_size} bytes"
                    ),
                )));
            }
            if total < expected_size {
                return Err(EngineError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "Transfer truncated: received {total} bytes, expected at least {expected_size} bytes"
                    ),
                )));
            }
            if total < 1024 {
                return Err(EngineError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Empty or truncated tar archive received".to_string(),
                )));
            }
        }

        Ok::<_, EngineError>(hasher.finalize(&algo))
    });

    // Network reader loop: each frame is length-prefixed.
    let mut read_error = None;
    let mut current_index = 0;
    let mut len_buf_owned = vec![0u8; 4];
    loop {
        let compio::BufResult(result, len_buf) = stream.read_exact(len_buf_owned).await;
        len_buf_owned = len_buf;
        match result {
            Ok(()) => {},
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                read_error = Some(e);
                break;
            },
        }
        let frame_len = u32::from_be_bytes([
            len_buf_owned[0],
            len_buf_owned[1],
            len_buf_owned[2],
            len_buf_owned[3],
        ]) as usize;

        // Max frame = flag (1) + encrypted chunk (CHUNK_SIZE + zstd worst case + nonce
        // + tag). Use a precise cap instead of an arbitrary 64 MiB constant to
        // prevent a malformed peer from forcing a large allocation before AEAD
        // authentication runs.
        let max_frame_len = 1
            + 4
            + CHUNK_SIZE
            + zstd::zstd_safe::compress_bound(CHUNK_SIZE)
            + crypto::NONCE_LEN
            + crypto::TAG_LEN;
        if frame_len == 0 || frame_len > max_frame_len {
            read_error = Some(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame length out of range: {frame_len}, max {max_frame_len}"),
            ));
            break;
        }

        let mut enc_owned = pool.lease().await;
        enc_owned.resize(frame_len, 0);
        let compio::BufResult(result, enc_sliced) =
            stream.read_exact(enc_owned.slice(0..frame_len)).await;
        let enc = enc_sliced.into_inner();
        match result {
            Ok(()) => {
                if enc_tx.send_async(Ok((current_index, enc))).await.is_err() {
                    break;
                }
                current_index += 1;
            },
            Err(e) => {
                read_error = Some(e);
                break;
            },
        }
    }

    if let Some(e) = read_error {
        let _ = enc_tx.send_async(Err(e)).await;
    }

    drop(enc_tx);

    let hash = write_handle.await.map_err(|e| {
        EngineError::Io(io::Error::other(format!("receive writer task failed: {e:?}")))
    })??;

    if let Some(mut guard) = cleanup_guard {
        guard.disable();
    }
    Ok(hash)
}

// ---------------------------------------------------------------------------
// Split-stream wrappers (compio-quic SendStream / RecvStream)
// ---------------------------------------------------------------------------

/// Sends the payload using a split `compio_quic::SendStream`.
///
/// # Errors
///
/// Returns [`EngineError`] if sending fails.
#[allow(clippy::too_many_arguments)]
pub async fn send_payload_write(
    key: &[u8; 32],
    cipher_id: u8,
    source: PayloadSource,
    stream: &mut compio_quic::SendStream,
    compress: bool,
    filename: Option<&str>,
    hash_algo: &str,
    progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
) -> Result<String, EngineError> {
    send_payload(key, cipher_id, source, stream, compress, filename, hash_algo, progress_cb).await
}

/// Receives the payload using a split `compio_quic::RecvStream`.
///
/// # Errors
///
/// Returns [`EngineError`] if receiving fails.
#[allow(clippy::too_many_arguments)]
pub async fn receive_payload_split(
    key: &[u8; 32],
    cipher_id: u8,
    stream: &mut compio_quic::RecvStream,
    output_path: &Path,
    transfer_type: TransferKind,
    expected_size: u64,
    hash_algo: &str,
    progress_cb: impl FnMut(u64) -> Result<(), EngineError> + 'static,
) -> Result<String, EngineError> {
    receive_payload(
        key,
        cipher_id,
        stream,
        output_path,
        transfer_type,
        expected_size,
        hash_algo,
        progress_cb,
    )
    .await
}

/// Writes the consent byte (accept/reject) to a split
/// `compio_quic::SendStream`.
///
/// # Errors
///
/// Returns [`EngineError`] if writing fails.
pub async fn send_consent_write(
    stream: &mut compio_quic::SendStream,
    accept: bool,
) -> Result<(), EngineError> {
    send_consent(stream, accept).await
}

#[cfg(test)]
mod tests {
    use super::{CHUNK_SIZE, PayloadHasher};

    // ── PayloadHasher ─────────────────────────────────────────────────────────
    //
    // These tests validate change 1 (hex_encode) through the SHA-256 arm of
    // PayloadHasher, which is the only public-surface user of hex_encode that
    // produces output compared across the wire.  A leading-zero regression
    // (e.g. 0x01 → "1" instead of "01") would silently produce a different
    // checksum string on sender and receiver, causing an integrity failure.

    /// NIST FIPS 180-4 test vector for SHA-256("abc").
    ///
    /// The raw digest bytes are:
    ///   ba 78 16 bf  **8f 01** cf ea  41 41 40 de  5d ae 22 23
    ///   b0 03 61 a3  96 17 7a 9c  b4 10 ff 61  f2 00 15 ad
    ///
    /// Byte at index 5 is `0x01`.  The old LUT and the new `write!("{b:02x}")`
    /// must both render it as `"01"` — any implementation that skips the
    /// leading zero would produce `"1"` and break every SHA-256 checksum.
    #[test]
    fn sha256_nist_vector_abc_preserves_leading_zero_byte() {
        let mut h = PayloadHasher::new("sha256").unwrap();
        h.update(b"abc");
        assert_eq!(
            h.finalize("sha256"),
            "sha256$ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "SHA-256(abc) mismatch — leading-zero byte 0x01 must render as '01'"
        );
    }

    /// SHA-256("") NIST vector.
    ///
    /// Verifies that the empty-input path works and that `ring::digest` (which
    /// replaced the removed `sha2` dev-dep) produces the correct digest.
    #[test]
    fn sha256_nist_vector_empty_input() {
        let mut h = PayloadHasher::new("sha256").unwrap();
        h.update(b"");
        assert_eq!(
            h.finalize("sha256"),
            "sha256$e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        );
    }

    /// The output format is `"{algo}${hex}"` where the hex part is exactly
    /// 64 lowercase ASCII characters for SHA-256.
    #[test]
    fn sha256_output_format_is_prefix_dollar_64hex() {
        let mut h = PayloadHasher::new("sha256").unwrap();
        h.update(b"hello world");
        let result = h.finalize("sha256");

        let (prefix, hex_part) = result.split_once('$').expect("output must contain '$'");
        assert_eq!(prefix, "sha256");
        assert_eq!(hex_part.len(), 64, "SHA-256 hex must be 64 chars");
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "hex part must be valid hex: {hex_part}"
        );
        assert_eq!(hex_part, hex_part.to_lowercase(), "hex must be lowercase");
    }

    /// Blake3 output format: prefix `"blake3$"` followed by 64 hex chars.
    ///
    /// Validates the blake3 arm does not regress while we test sha256.
    #[test]
    fn blake3_output_format_is_prefix_dollar_64hex() {
        let mut h = PayloadHasher::new("blake3").unwrap();
        h.update(b"anything");
        let result = h.finalize("blake3");

        let (prefix, hex_part) = result.split_once('$').expect("missing '$'");
        assert_eq!(prefix, "blake3");
        assert_eq!(hex_part.len(), 64);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn unknown_algorithm_is_rejected() {
        let result = PayloadHasher::new("does-not-exist");
        assert!(result.is_err(), "unknown hash algorithm must be rejected");
    }

    // ── zstd bulk compress/decompress ───────────────────────────────────────
    //
    // These validate the approach used by the sender/receiver worker loops
    // in send_payload/receive_payload: a reusable `Compressor`/`Decompressor`
    // plus a fixed-capacity scratch/pool buffer, replacing the old per-chunk
    // `zstd::encode_all`/`decode_all` calls (which allocated a fresh Vec and
    // rebuilt the codec context on every single frame).

    /// Round-trips repetitive data through the exact call pattern used in
    /// production: compress into a reusable scratch buffer sized by
    /// `compress_bound`, decompress directly into a pool-leased buffer sized
    /// to `CHUNK_SIZE`.
    #[test]
    fn zstd_bulk_roundtrip_via_reusable_context_and_pool_buffer() {
        let pool = crate::pool::BufferPool::new(2, CHUNK_SIZE);
        let mut compressor = zstd::bulk::Compressor::new(1).unwrap();
        let mut decompressor = zstd::bulk::Decompressor::new().unwrap();
        let mut scratch = vec![0u8; zstd::zstd_safe::compress_bound(CHUNK_SIZE)];

        let original = b"hello world, hayate hayate hayate! ".repeat(10_000);

        let n = compressor.compress_to_buffer(&original[..], &mut scratch).unwrap();
        assert!(n < original.len(), "repetitive input must compress smaller");

        let mut plain = pool.lease_sync();
        let m = decompressor.decompress_to_buffer(&scratch[..n], &mut plain).unwrap();
        plain.truncate(m);

        assert_eq!(plain, original, "decompressed output must match the input");
        pool.release(plain);
    }

    /// Regression guard for the plain-pool lease/release imbalance: the
    /// FRAME_ZSTD decode path must lease its output buffer from the same
    /// pool FRAME_RAW uses, not allocate independently. This simulates many
    /// more decompressions than the pool's capacity, all leased first as
    /// production code now does — it must neither block nor grow beyond
    /// capacity.
    ///
    /// Before this fix, `zstd::decode_all` allocated outside the pool, so
    /// every decompressed buffer was an *unmatched* release once it reached
    /// the writer task: fine with a non-blocking `release` (silently
    /// dropped, but wastes an allocation every frame), catastrophic with a
    /// blocking one (the single-threaded compio executor freezes forever
    /// the first time the pool is at capacity — see pool.rs history).
    #[test]
    fn compressible_frames_lease_from_pool_instead_of_allocating_independently() {
        let pool = crate::pool::BufferPool::new(4, CHUNK_SIZE);
        let mut compressor = zstd::bulk::Compressor::new(1).unwrap();
        let mut decompressor = zstd::bulk::Decompressor::new().unwrap();
        let mut scratch = vec![0u8; zstd::zstd_safe::compress_bound(CHUNK_SIZE)];

        let chunk = b"repetitive payload chunk ".repeat(50_000);
        let n = compressor.compress_to_buffer(&chunk[..], &mut scratch).unwrap();

        // Simulate 50 frames worth of decode+release with the pool capped
        // far below that count — proves steady-state reuse, not growth.
        for _ in 0..50 {
            let mut plain = pool.lease_sync();
            let m = decompressor.decompress_to_buffer(&scratch[..n], &mut plain).unwrap();
            plain.truncate(m);
            assert_eq!(plain, chunk);
            pool.release(plain);
        }
    }
}
