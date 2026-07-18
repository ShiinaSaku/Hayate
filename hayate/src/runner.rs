//! High-level developer-friendly API runners for sending and receiving files.
//!
//! This module provides the [`HayateSender`] and [`HayateReceiver`] builders,
//! which abstract away low-level QUIC socket bindings, stream negotiations,
//! cryptographic handshakes, consent flows, and file/directory transfers
//! (including automatic tar packaging/extraction).
//!
//! ## Staged transfers
//!
//! Prefer [`HayateSender::send_with`] / [`HayateReceiver::receive_with`] (or
//! [`ListeningReceiver`] for multi-accept listeners) when a UI needs to react
//! to connect / handshake / offer / progress stages. The plain
//! [`HayateSender::send`] and [`HayateReceiver::receive`] methods are thin
//! wrappers that ignore stages.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use compio::io::AsyncRead;
use compio_quic::Endpoint;

use crate::protocol::{Metadata, TransferKind};
use crate::{EngineError, network, transfer};

// ---------------------------------------------------------------------------
// Stages and outcomes
// ---------------------------------------------------------------------------

/// Lifecycle stage of a send or receive transfer.
///
/// UI layers map these to spinners, status lines, and info cards. The library
/// owns connect / handshake / payload sequencing; the caller only observes.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransferStage {
    /// Dialing a known receiver address.
    Connecting {
        /// Address being dialed.
        peer: SocketAddr,
    },
    /// Broadcasting a pairing code and waiting for the peer.
    Pairing {
        /// Shared pairing phrase.
        code: String,
    },
    /// Receiver is listening for an incoming connection (direct mode).
    WaitingForPeer,
    /// Discovery: scanning for a pairing-code broadcast.
    Discovering {
        /// Phrase being scanned for.
        code: String,
    },
    /// QUIC connection established.
    Connected {
        /// Remote peer address.
        peer: SocketAddr,
    },
    /// Application-layer version, cipher, and key exchange in progress.
    Handshaking,
    /// Sender: handshake finished; payload is about to start.
    Ready {
        /// Transfer metadata agreed during the handshake.
        meta: Metadata,
        /// Negotiated AEAD cipher id.
        cipher_id: u8,
        /// Remote peer address.
        peer: SocketAddr,
        /// Estimated total payload bytes (0 for unknown directory streams).
        total_size: u64,
    },
    /// Receiver: encrypted metadata available; consent is next.
    Offer {
        /// Incoming transfer metadata.
        meta: Metadata,
        /// Negotiated AEAD cipher id.
        cipher_id: u8,
        /// Remote peer address.
        peer: SocketAddr,
    },
    /// Payload bytes are flowing.
    Transferring {
        /// Display name of the payload.
        filename: String,
        /// Estimated total bytes (may be 0 for streaming directories).
        total_size: u64,
    },
    /// Streams are being finished / drained.
    Finishing,
}

/// Result of a successful send.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SendOutcome {
    /// Integrity checksum (`algo$hex`).
    pub checksum: String,
    /// Metadata that was sent during the handshake.
    pub meta: Metadata,
    /// Negotiated cipher suite id.
    pub cipher_id: u8,
    /// Remote peer address.
    pub peer: SocketAddr,
    /// Estimated total size reported at send time.
    pub total_size: u64,
}

/// Result of a successful receive.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReceiveOutcome {
    /// Integrity checksum (`algo$hex`).
    pub checksum: String,
    /// Path where the payload was written.
    pub path: PathBuf,
    /// Metadata received during the handshake.
    pub meta: Metadata,
    /// Negotiated cipher suite id.
    pub cipher_id: u8,
    /// Remote peer address.
    pub peer: SocketAddr,
}

// ---------------------------------------------------------------------------
// Sender
// ---------------------------------------------------------------------------

/// High-level builder for sending a file or directory over the network.
///
/// `HayateSender` handles both direct IP transfers and pairing-code-based
/// discovery.
///
/// # Examples
///
/// ```no_run
/// use std::net::SocketAddr;
/// use hayate::runner::HayateSender;
///
/// # async fn run() -> Result<(), hayate::EngineError> {
/// let target: SocketAddr = "192.168.1.50:50001".parse().unwrap();
/// let sender = HayateSender::new()
///     .target(target)
///     .compress(true);
///
/// let checksum = sender.send("path/to/file.txt", |progress| {
///     println!("Sent {progress} bytes");
///     Ok(())
/// }).await?;
/// println!("Transfer complete. Checksum: {checksum}");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct HayateSender {
    target: Option<SocketAddr>,
    code: Option<String>,
    /// Optional KDF passphrase. When unset, falls back to [`Self::code`].
    passphrase: Option<String>,
    compress: bool,
    hash_algo: String,
}

impl Default for HayateSender {
    fn default() -> Self {
        Self {
            target: None,
            code: None,
            passphrase: None,
            compress: true,
            hash_algo: "blake3".to_owned(),
        }
    }
}

impl HayateSender {
    /// Creates a new `HayateSender` with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the target receiver address.
    ///
    /// Use this for direct IP transfers. May be combined with
    /// [`Self::passphrase`] for an out-of-band secret without enabling
    /// pairing discovery.
    #[must_use]
    pub fn target(mut self, target: SocketAddr) -> Self {
        self.target = Some(target);
        self
    }

    /// Sets a cryptographic code-phrase for pairing discovery.
    ///
    /// When no [`Self::target`] is set, the sender broadcasts over the LAN so
    /// the receiver can find it. The phrase is also used as the KDF
    /// passphrase unless [`Self::passphrase`] overrides it.
    #[must_use]
    pub fn code(mut self, code: String) -> Self {
        self.code = Some(code);
        self
    }

    /// Sets the passphrase mixed into application-layer key derivation.
    ///
    /// Useful with [`Self::target`] when peers share a secret without pairing
    /// discovery. When unset, [`Self::code`] is used if present.
    #[must_use]
    pub fn passphrase(mut self, passphrase: String) -> Self {
        self.passphrase = Some(passphrase);
        self
    }

    /// Enables or disables zstd compression for the transfer (enabled by
    /// default).
    #[must_use]
    pub fn compress(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Sets the hash algorithm for payload integrity (default is "blake3").
    #[must_use]
    pub fn hash_algo(mut self, algo: String) -> Self {
        self.hash_algo = algo;
        self
    }

    fn kdf_passphrase(&self) -> Option<&str> {
        self.passphrase.as_deref().or(self.code.as_deref()).filter(|s| !s.is_empty())
    }

    /// Initiates the transfer of the file or directory at `path`.
    ///
    /// The `progress_cb` closure is periodically called with the total number
    /// of bytes written to the network.
    ///
    /// Returns the checksum of the transferred payload.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the path is invalid, network
    /// connection/handshake fails, or the transfer is rejected by the
    /// receiver.
    pub async fn send(
        self,
        path: impl AsRef<Path>,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<String, EngineError> {
        self.send_with(path, |_| Ok(()), progress_cb).await.map(|o| o.checksum)
    }

    /// Like [`Self::send`], but reports lifecycle [`TransferStage`]s for UI
    /// layering.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the path is invalid, network
    /// connection/handshake fails, a stage callback fails, or the transfer
    /// is rejected by the receiver.
    pub async fn send_with(
        self,
        path: impl AsRef<Path>,
        mut on_stage: impl FnMut(TransferStage) -> Result<(), EngineError>,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<SendOutcome, EngineError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Path does not exist: {}", path.display()),
            )));
        }

        let (meta, total_size) = self.build_metadata(path)?;
        meta.validate()?;

        let passphrase = self.kdf_passphrase().map(str::to_owned);

        // Establish the QUIC connection
        let (_endpoint, conn) = if let Some(target_addr) = self.target {
            on_stage(TransferStage::Connecting { peer: target_addr })?;
            let endpoint = network::bind_client().await?;
            let client_cfg = network::client_config()?;
            let connecting = endpoint.connect(target_addr, "hayate.local", Some(client_cfg))?;
            let conn = connecting.await?;
            (endpoint, conn)
        } else {
            let phrase = self.code.as_ref().ok_or_else(|| {
                EngineError::Handshake("Neither target nor code specified".into())
            })?;

            on_stage(TransferStage::Pairing { code: phrase.clone() })?;

            let bind_addr =
                SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0);
            let endpoint = network::bind_server(bind_addr).await?;
            let local_port = endpoint.local_addr()?.port();

            let channel_id = crate::discovery::derive_channel_id(phrase);
            let os_name = std::env::consts::OS.to_owned();
            let _broadcaster_guard =
                crate::discovery::start_broadcaster_hybrid(&channel_id, local_port, &os_name)
                    .map_err(|e| {
                        EngineError::Handshake(format!("broadcaster start failed: {e}"))
                    })?;

            let incoming = compio::time::timeout(Duration::from_mins(1), endpoint.wait_incoming())
                .await
                .map_err(|_| {
                    EngineError::Handshake("timed out waiting for receiver during pairing".into())
                })?
                .ok_or_else(|| EngineError::Handshake("Endpoint closed during pairing".into()))?;
            let conn = incoming.await?;
            (endpoint, conn)
        };

        let peer = conn.remote_address();
        on_stage(TransferStage::Connected { peer })?;

        let (mut send_stream, mut recv_stream) = conn.open_bi()?;

        on_stage(TransferStage::Handshaking)?;
        let (key, cipher_id) = transfer::handshake_sender_split(
            &mut send_stream,
            &mut recv_stream,
            &meta,
            passphrase.as_deref(),
        )
        .await?;

        on_stage(TransferStage::Ready { meta: meta.clone(), cipher_id, peer, total_size })?;

        on_stage(TransferStage::Transferring { filename: meta.filename.clone(), total_size })?;

        let checksum = if path.is_dir() {
            self.send_directory(
                path,
                &key,
                cipher_id,
                &self.hash_algo,
                &mut send_stream,
                progress_cb,
            )
            .await?
        } else {
            self.send_file(path, &key, cipher_id, &self.hash_algo, &mut send_stream, progress_cb)
                .await?
        };

        on_stage(TransferStage::Finishing)?;
        send_stream.finish()?;

        // Wait for the receiver to acknowledge completion with a time-bounded
        // read. If the receiver has closed the connection, reading returns EOF
        // or an error. The timeout prevents hanging if the receiver disappears.
        let drain_buf = vec![0u8; 1];
        let _ =
            compio::time::timeout(std::time::Duration::from_secs(10), recv_stream.read(drain_buf))
                .await;

        conn.close(0u32.into(), b"complete");
        Ok(SendOutcome { checksum, meta, cipher_id, peer, total_size })
    }

    /// Builds [`Metadata`] and estimates total byte size for `path`.
    ///
    /// Callers who perform their own QUIC connection and handshake can use this
    /// instead of [`Self::send`] to interleave terminal UI between stages.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the path has no filename or metadata cannot
    /// be read.
    pub fn build_metadata(&self, path: &Path) -> Result<(Metadata, u64), EngineError> {
        let filename = path
            .file_name()
            .ok_or_else(|| {
                EngineError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Path has no filename",
                ))
            })?
            .to_string_lossy()
            .into_owned();

        if path.is_dir() {
            let total = crate::tar::estimate_dir_size(path);
            Ok((
                Metadata::new(filename, total, TransferKind::Directory, self.hash_algo.clone()),
                total,
            ))
        } else {
            let total = std::fs::metadata(path).map_err(EngineError::Io)?.len();
            Ok((Metadata::new(filename, total, TransferKind::File, self.hash_algo.clone()), total))
        }
    }

    /// Sends a single file over an already-established QUIC send stream.
    ///
    /// Prefer [`Self::send_with`] unless you own the connection lifecycle.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if file I/O, compression, encryption, or network
    /// write fails.
    pub async fn send_file(
        &self,
        path: &Path,
        key: &[u8; 32],
        cipher_id: u8,
        hash_algo: &str,
        stream: &mut compio_quic::SendStream,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<String, EngineError> {
        let file = compio::fs::File::open(path).await.map_err(EngineError::Io)?;
        let source = transfer::PayloadSource::File { file, pos: 0 };
        let filename = path.file_name().and_then(|s| s.to_str());
        transfer::send_payload_write(
            key,
            cipher_id,
            source,
            stream,
            self.compress,
            filename,
            hash_algo,
            progress_cb,
        )
        .await
    }

    /// Sends a directory as a tar stream over an already-established QUIC send
    /// stream.
    ///
    /// Prefer [`Self::send_with`] unless you own the connection lifecycle.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if tar packaging, compression, encryption, or
    /// network write fails.
    pub async fn send_directory(
        &self,
        dir: &Path,
        key: &[u8; 32],
        cipher_id: u8,
        hash_algo: &str,
        stream: &mut compio_quic::SendStream,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<String, EngineError> {
        let (tx, rx) = flume::bounded::<Result<Vec<u8>, std::io::Error>>(8);
        let dir_clone = dir.to_path_buf();

        std::thread::spawn(move || {
            use std::io::Write;
            struct ChanWriter {
                tx: flume::Sender<Result<Vec<u8>, std::io::Error>>,
            }
            impl std::io::Write for ChanWriter {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    self.tx.send(Ok(buf.to_vec())).map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "receiver gone")
                    })?;
                    Ok(buf.len())
                }

                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let writer = ChanWriter { tx: tx.clone() };
            let mut buffered_writer = std::io::BufWriter::with_capacity(128 * 1024, writer);
            let mut run = move || -> Result<(), std::io::Error> {
                crate::tar::write_tar_sync(&dir_clone, &mut buffered_writer)?;
                buffered_writer.flush()?;
                Ok(())
            };
            if let Err(e) = run() {
                let _ = tx.send(Err(e));
            }
        });

        let source = transfer::PayloadSource::Channel(rx);
        transfer::send_payload_write(
            key,
            cipher_id,
            source,
            stream,
            self.compress,
            None,
            hash_algo,
            progress_cb,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Receiver
// ---------------------------------------------------------------------------

/// High-level builder for receiving a file or directory over the network.
///
/// `HayateReceiver` handles both direct IP listening and pairing-code-based
/// discovery.
///
/// # Examples
///
/// ```no_run
/// use std::net::SocketAddr;
/// use hayate::runner::HayateReceiver;
///
/// # async fn run() -> Result<(), hayate::EngineError> {
/// let bind_addr: SocketAddr = "0.0.0.0:50001".parse().unwrap();
/// let receiver = HayateReceiver::new()
///     .bind(bind_addr);
///
/// let (checksum, path) = receiver.receive("downloads", |meta| {
///     println!("Accepting {} ({} bytes)?", meta.filename, meta.total_size);
///     true // Accept the transfer
/// }, |progress| {
///     println!("Received {progress} bytes");
///     Ok(())
/// }).await?;
///
/// println!("Successfully saved to {} (Checksum: {})", path.display(), checksum);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct HayateReceiver {
    bind_addr: SocketAddr,
    code: Option<String>,
    /// Optional KDF passphrase. When unset, falls back to `code`.
    passphrase: Option<String>,
    auto_accept: bool,
}

impl Default for HayateReceiver {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:50001".parse().unwrap(),
            code: None,
            passphrase: None,
            auto_accept: false,
        }
    }
}

impl HayateReceiver {
    /// Creates a new `HayateReceiver` with default configuration (binding to
    /// `0.0.0.0:50001`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the local address and port to bind to.
    #[must_use]
    pub fn bind(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = addr;
        self
    }

    /// Sets a cryptographic code-phrase for pairing.
    ///
    /// The receiver will listen for UDP broadcast announcements matching this
    /// code-phrase, locate the sender, and connect automatically.
    #[must_use]
    pub fn code(mut self, code: String) -> Self {
        self.code = Some(code);
        self
    }

    /// Sets the passphrase mixed into application-layer key derivation.
    ///
    /// Mirrors [`HayateSender::passphrase`]: required when the sender sets an
    /// explicit passphrase in direct-target mode. When unset, [`Self::code`]
    /// is used if present.
    #[must_use]
    pub fn passphrase(mut self, passphrase: String) -> Self {
        self.passphrase = Some(passphrase);
        self
    }

    /// Automatically accepts all incoming transfers without calling the consent
    /// callback.
    #[must_use]
    pub fn auto_accept(mut self, auto_accept: bool) -> Self {
        self.auto_accept = auto_accept;
        self
    }

    fn kdf_passphrase(&self) -> Option<&str> {
        self.passphrase.as_deref().or(self.code.as_deref()).filter(|s| !s.is_empty())
    }

    /// Starts the receiver and waits for a single incoming connection.
    ///
    /// Once connected, it performs the handshake, invokes `consent_cb` with the
    /// metadata of the incoming transfer, and if accepted, downloads the
    /// files to `output_dir`.
    ///
    /// The `progress_cb` closure is periodically called with the total number
    /// of bytes received from the network.
    ///
    /// Returns a tuple containing the checksum of the payload and the actual
    /// path where it was written.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if pairing times out, connection or handshake
    /// fails, or the transfer is rejected.
    pub async fn receive(
        self,
        output_dir: impl AsRef<Path>,
        consent_cb: impl FnOnce(&Metadata) -> bool,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<(String, PathBuf), EngineError> {
        let output_dir = output_dir.as_ref().to_path_buf();
        let output_for_consent = output_dir.clone();
        self.receive_with(
            &output_dir,
            |_| Ok(()),
            move |meta, _peer| {
                if consent_cb(meta) {
                    Some(resolve_output(&output_for_consent, meta))
                } else {
                    None
                }
            },
            progress_cb,
        )
        .await
        .map(|o| (o.checksum, o.path))
    }

    /// Like [`Self::receive`], but reports lifecycle stages and lets consent
    /// choose the destination path.
    ///
    /// `consent_cb` is invoked after [`TransferStage::Offer`]. Return
    /// `Some(path)` to accept and write there, or `None` to reject. When
    /// [`Self::auto_accept`] is set, `consent_cb` is not called and the
    /// file is written under `output_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if pairing times out, connection or handshake
    /// fails, a stage callback fails, or the transfer is rejected.
    pub async fn receive_with(
        self,
        output_dir: impl AsRef<Path>,
        mut on_stage: impl FnMut(TransferStage) -> Result<(), EngineError>,
        consent_cb: impl FnOnce(&Metadata, SocketAddr) -> Option<PathBuf>,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<ReceiveOutcome, EngineError> {
        let output_dir = output_dir.as_ref();

        let (_endpoint, conn) = if let Some(phrase) = &self.code {
            on_stage(TransferStage::Discovering { code: phrase.clone() })?;
            // `listen_for_broadcast` blocks its calling thread on a
            // `recv_timeout`, so run it on a blocking worker instead of the
            // compio completion executor.
            let phrase_clone = phrase.clone();
            let discovered = compio::runtime::spawn_blocking(move || {
                crate::discovery::listen_for_broadcast(
                    Some(phrase_clone.as_str()),
                    Duration::from_mins(1),
                )
            })
            .await
            .map_err(|e| EngineError::Handshake(format!("discovery task failed: {e}")))?
            .map_err(EngineError::Io)?;
            let Some((_name, peer_addr, _os)) = discovered else {
                return Err(EngineError::Handshake(
                    "Timed out waiting for sender broadcast".into(),
                ));
            };

            on_stage(TransferStage::Connecting { peer: peer_addr })?;
            let endpoint = network::bind_client().await?;
            let client_cfg = network::client_config()?;
            let connecting = endpoint.connect(peer_addr, "hayate.local", Some(client_cfg))?;
            let conn = connecting.await?;
            (endpoint, conn)
        } else {
            on_stage(TransferStage::WaitingForPeer)?;
            let endpoint = network::bind_server(self.bind_addr).await?;
            let incoming = compio::time::timeout(Duration::from_mins(1), endpoint.wait_incoming())
                .await
                .map_err(|_| EngineError::Handshake("timed out waiting for sender".into()))?
                .ok_or_else(|| EngineError::Handshake("Endpoint closed".into()))?;
            let conn = incoming.await?;
            (endpoint, conn)
        };

        let peer = conn.remote_address();
        on_stage(TransferStage::Connected { peer })?;

        complete_receive_session(
            conn,
            self.kdf_passphrase(),
            self.auto_accept,
            output_dir,
            on_stage,
            consent_cb,
            progress_cb,
        )
        .await
    }

    /// Binds a server endpoint for multi-accept direct receives.
    ///
    /// Pairing-code mode is not supported here — use [`Self::receive_with`]
    /// instead.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if binding the UDP/QUIC endpoint fails, or if a
    /// pairing code was configured (use the one-shot receive path instead).
    pub async fn listen(self) -> Result<ListeningReceiver, EngineError> {
        if self.code.is_some() {
            return Err(EngineError::Handshake(
                "listen() is for direct mode only; use receive_with() for pairing codes".into(),
            ));
        }
        let endpoint = network::bind_server(self.bind_addr).await?;
        Ok(ListeningReceiver {
            endpoint,
            passphrase: self.passphrase,
            auto_accept: self.auto_accept,
        })
    }
}

/// Bound QUIC listener that can accept multiple transfers sequentially.
///
/// Obtained from [`HayateReceiver::listen`]. The CLI uses this for the direct
/// receive loop so connect/handshake/payload stay inside the library.
pub struct ListeningReceiver {
    endpoint: Endpoint,
    passphrase: Option<String>,
    auto_accept: bool,
}

impl ListeningReceiver {
    /// Local address the listener is bound to.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the endpoint local address cannot be read.
    pub fn local_addr(&self) -> Result<SocketAddr, EngineError> {
        self.endpoint.local_addr().map_err(EngineError::Io)
    }

    /// Wait up to `timeout` for one complete transfer.
    ///
    /// Returns `Ok(None)` when the timeout elapses with no incoming connection
    /// (callers typically poll a cancellation flag and try again).
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on connection, handshake, consent, or payload
    /// failures. Benign peer closes surface as [`EngineError::Connection`];
    /// callers may filter those and continue listening.
    pub async fn try_accept_one(
        &self,
        timeout: Duration,
        output_dir: impl AsRef<Path>,
        mut on_stage: impl FnMut(TransferStage) -> Result<(), EngineError>,
        consent_cb: impl FnOnce(&Metadata, SocketAddr) -> Option<PathBuf>,
        progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
    ) -> Result<Option<ReceiveOutcome>, EngineError> {
        on_stage(TransferStage::WaitingForPeer)?;
        let incoming = match compio::time::timeout(timeout, self.endpoint.wait_incoming()).await {
            Ok(Some(i)) => i,
            Ok(None) => {
                return Err(EngineError::Handshake("Endpoint closed".into()));
            },
            Err(_timeout) => return Ok(None),
        };
        let conn = incoming.await?;
        let peer = conn.remote_address();
        on_stage(TransferStage::Connected { peer })?;

        let outcome = complete_receive_session(
            conn,
            self.passphrase.as_deref(),
            self.auto_accept,
            output_dir.as_ref(),
            on_stage,
            consent_cb,
            progress_cb,
        )
        .await?;
        Ok(Some(outcome))
    }
}

/// Runs handshake → consent → payload on an established receiver connection.
async fn complete_receive_session(
    conn: compio_quic::Connection,
    passphrase: Option<&str>,
    auto_accept: bool,
    output_dir: &Path,
    mut on_stage: impl FnMut(TransferStage) -> Result<(), EngineError>,
    consent_cb: impl FnOnce(&Metadata, SocketAddr) -> Option<PathBuf>,
    progress_cb: impl FnMut(u64) -> Result<(), EngineError> + Send + 'static,
) -> Result<ReceiveOutcome, EngineError> {
    let peer = conn.remote_address();
    let (mut send_stream, mut recv_stream) = conn.accept_bi().await?;

    on_stage(TransferStage::Handshaking)?;
    let ((key, cipher_id), meta) =
        transfer::handshake_receiver_split(&mut send_stream, &mut recv_stream, passphrase).await?;

    on_stage(TransferStage::Offer { meta: meta.clone(), cipher_id, peer })?;

    let dest =
        if auto_accept { Some(resolve_output(output_dir, &meta)) } else { consent_cb(&meta, peer) };

    let accept = dest.is_some();
    transfer::send_consent_write(&mut send_stream, accept).await?;

    let Some(dest) = dest else {
        conn.close(0u32.into(), b"rejected");
        return Err(EngineError::TransferRejected);
    };

    on_stage(TransferStage::Transferring {
        filename: meta.filename.clone(),
        total_size: meta.total_size,
    })?;

    let checksum = transfer::receive_payload_split(
        &key,
        cipher_id,
        &mut recv_stream,
        &dest,
        meta.transfer_type,
        meta.total_size,
        &meta.hash_algo,
        progress_cb,
    )
    .await?;

    on_stage(TransferStage::Finishing)?;
    let _ = send_stream.finish();
    // Brief pause so the peer can observe stream finish before close.
    compio::time::sleep(Duration::from_millis(50)).await;
    conn.close(0u32.into(), b"complete");

    Ok(ReceiveOutcome { checksum, path: dest, meta, cipher_id, peer })
}

/// Helper to resolve the output path for received files/directories.
fn resolve_output(output_dir: &Path, meta: &Metadata) -> PathBuf {
    let name = Path::new(&meta.filename)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("received_file"));
    output_dir.join(name)
}

/// Returns true when a connection error is a benign peer-initiated close
/// (discover probes, remote shutdown). Listeners can ignore these and keep
/// waiting.
#[must_use]
pub fn is_benign_peer_close(err: &EngineError) -> bool {
    match err {
        EngineError::Connection(e) => matches!(
            e,
            compio_quic::ConnectionError::ApplicationClosed(_)
                | compio_quic::ConnectionError::ConnectionClosed(_)
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        HayateReceiver,
        HayateSender,
        ReceiveOutcome,
        TransferStage,
        is_benign_peer_close,
    };
    use crate::EngineError;

    fn pick_free_port() -> u16 {
        let listener = std::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .expect("bind should succeed");
        listener.local_addr().unwrap().port()
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("hayate-{prefix}-{now}"))
    }

    fn write_random_file(path: &Path, len: usize) {
        let mut file = std::fs::File::create(path).unwrap();
        let mut state: u64 = 0x1234_5678_9abc_def0;
        let mut written = 0;
        while written < len {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let chunk_len = std::cmp::min(1024, len - written);
            let bytes: Vec<u8> = (0..chunk_len)
                .map(|i| ((state >> (8 * (i % 8))) as u8).wrapping_add(i as u8))
                .collect();
            file.write_all(&bytes).unwrap();
            written += chunk_len;
        }
        file.flush().unwrap();
    }

    fn stage_name(stage: &TransferStage) -> &'static str {
        match stage {
            TransferStage::Connecting { .. } => "connecting",
            TransferStage::Pairing { .. } => "pairing",
            TransferStage::WaitingForPeer => "waiting",
            TransferStage::Discovering { .. } => "discovering",
            TransferStage::Connected { .. } => "connected",
            TransferStage::Handshaking => "handshaking",
            TransferStage::Ready { .. } => "ready",
            TransferStage::Offer { .. } => "offer",
            TransferStage::Transferring { .. } => "transferring",
            TransferStage::Finishing => "finishing",
        }
    }

    #[test]
    fn send_file_roundtrip_matches_checksum_and_bytes() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src");
        let dst_dir = unique_test_dir("dst");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();

        // Keep under one chunk so the suite stays fast, but still non-trivial.
        let src_file = src_dir.join("payload.bin");
        write_random_file(&src_file, 256 * 1024 + 1234);
        let expected_bytes = std::fs::read(&src_file).unwrap();

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let dst_dir_for_recv = dst_dir.clone();
        let src_file_for_sender = src_file.clone();

        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on(async move {
                let receiver = HayateReceiver::new().bind(receiver_addr).auto_accept(true);
                let recv_handle = compio::runtime::spawn(async move {
                    receiver.receive(&dst_dir_for_recv, |_| true, |_| Ok(())).await
                });

                compio::time::sleep(Duration::from_millis(50)).await;

                let sender_checksum = HayateSender::new()
                    .target(receiver_addr)
                    .send(&src_file_for_sender, |_| Ok(()))
                    .await?;

                let receiver_result = recv_handle
                    .await
                    .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))?;
                let (receiver_checksum, dest_path) = receiver_result?;
                assert_eq!(sender_checksum, receiver_checksum);
                assert_eq!(expected_bytes, std::fs::read(&dest_path).unwrap());
                Ok::<(), EngineError>(())
            })
            .expect("transfer should succeed");
    }

    #[test]
    fn send_directory_roundtrip_extracts_and_matches_checksum() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src");
        let dst_dir = unique_test_dir("dst");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();

        let nested = src_dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        write_random_file(&src_dir.join("top.txt"), 5_000);
        write_random_file(&nested.join("deep.bin"), 2_000);

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let dst_dir_for_recv = dst_dir.clone();
        let src_dir_for_sender = src_dir.clone();

        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on(async move {
                let receiver = HayateReceiver::new().bind(receiver_addr).auto_accept(true);
                let recv_handle = compio::runtime::spawn(async move {
                    receiver.receive(&dst_dir_for_recv, |_| true, |_| Ok(())).await
                });

                compio::time::sleep(Duration::from_millis(50)).await;

                let sender_checksum = HayateSender::new()
                    .target(receiver_addr)
                    .send(&src_dir_for_sender, |_| Ok(()))
                    .await?;

                let receiver_result = recv_handle
                    .await
                    .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))?;
                let (receiver_checksum, received_path) = receiver_result?;
                assert_eq!(sender_checksum, receiver_checksum);

                let top = received_path.join("top.txt");
                let deep = received_path.join("nested").join("deep.bin");
                assert!(top.exists());
                assert!(deep.exists());

                assert_eq!(
                    std::fs::read(src_dir_for_sender.join("top.txt")).unwrap(),
                    std::fs::read(&top).unwrap()
                );
                assert_eq!(
                    std::fs::read(src_dir_for_sender.join("nested").join("deep.bin")).unwrap(),
                    std::fs::read(&deep).unwrap()
                );

                Ok::<(), EngineError>(())
            })
            .expect("transfer should succeed");
    }

    #[test]
    fn send_with_emits_expected_stages() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src-stages");
        let dst_dir = unique_test_dir("dst-stages");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let src_file = src_dir.join("tiny.txt");
        std::fs::write(&src_file, b"stage-check").unwrap();

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
        let sender_stages: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let receiver_stages: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on({
                let sender_stages = Arc::clone(&sender_stages);
                let receiver_stages = Arc::clone(&receiver_stages);
                let src_file = src_file.clone();
                let dst_dir = dst_dir.clone();
                async move {
                    let recv_stages = Arc::clone(&receiver_stages);
                    let receiver = HayateReceiver::new().bind(receiver_addr).auto_accept(true);
                    let recv_handle = compio::runtime::spawn(async move {
                        receiver
                            .receive_with(
                                &dst_dir,
                                move |stage| {
                                    recv_stages.lock().unwrap().push(stage_name(&stage));
                                    Ok(())
                                },
                                |_, _| unreachable!("auto_accept skips consent"),
                                |_| Ok(()),
                            )
                            .await
                    });

                    compio::time::sleep(Duration::from_millis(50)).await;

                    let send_stages = Arc::clone(&sender_stages);
                    let outcome = HayateSender::new()
                        .target(receiver_addr)
                        .send_with(
                            &src_file,
                            move |stage| {
                                send_stages.lock().unwrap().push(stage_name(&stage));
                                Ok(())
                            },
                            |_| Ok(()),
                        )
                        .await?;

                    let recv: ReceiveOutcome = recv_handle
                        .await
                        .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))??;

                    assert_eq!(outcome.checksum, recv.checksum);
                    assert_eq!(outcome.cipher_id, recv.cipher_id);
                    Ok::<(), EngineError>(())
                }
            })
            .expect("staged transfer should succeed");

        let send = sender_stages.lock().unwrap().clone();
        let recv = receiver_stages.lock().unwrap().clone();
        assert!(
            send.windows(2).all(|w| {
                // Stages are emitted in a fixed order; allow only forward progress.
                let order = [
                    "connecting",
                    "connected",
                    "handshaking",
                    "ready",
                    "transferring",
                    "finishing",
                ];
                let i = order.iter().position(|s| *s == w[0]);
                let j = order.iter().position(|s| *s == w[1]);
                matches!((i, j), (Some(a), Some(b)) if a <= b)
            }),
            "unexpected sender stages: {send:?}"
        );
        assert!(send.contains(&"connecting"));
        assert!(send.contains(&"ready"));
        assert!(send.contains(&"finishing"));
        assert!(recv.contains(&"waiting"));
        assert!(recv.contains(&"offer"));
        assert!(recv.contains(&"transferring"));
    }

    #[test]
    fn receiver_reject_surfaces_transfer_rejected() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src-reject");
        let dst_dir = unique_test_dir("dst-reject");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let src_file = src_dir.join("nope.txt");
        std::fs::write(&src_file, b"should not land").unwrap();

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));

        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on(async move {
                let receiver = HayateReceiver::new().bind(receiver_addr);
                let recv_handle = compio::runtime::spawn(async move {
                    receiver
                        .receive_with(&dst_dir, |_| Ok(()), |_meta, _peer| None, |_| Ok(()))
                        .await
                });

                compio::time::sleep(Duration::from_millis(50)).await;

                let send_err = HayateSender::new()
                    .target(receiver_addr)
                    .send(&src_file, |_| Ok(()))
                    .await
                    .unwrap_err();
                // Receiver closes after reject; sender may observe TransferRejected
                // or a connection drop depending on scheduling.
                assert!(
                    matches!(
                        send_err,
                        EngineError::TransferRejected
                            | EngineError::Connection(_)
                            | EngineError::Write(_)
                            | EngineError::Io(_)
                    ),
                    "unexpected send error: {send_err:?}"
                );

                let recv_err = recv_handle
                    .await
                    .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))?
                    .unwrap_err();
                assert!(
                    matches!(recv_err, EngineError::TransferRejected),
                    "unexpected recv error: {recv_err:?}"
                );
                Ok::<(), EngineError>(())
            })
            .expect("reject path should run cleanly");
    }

    #[test]
    fn listening_receiver_try_accept_one_roundtrip() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src-listen");
        let dst_dir = unique_test_dir("dst-listen");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let src_file = src_dir.join("listen.bin");
        write_random_file(&src_file, 4096);

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));

        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on(async move {
                let listener =
                    HayateReceiver::new().bind(receiver_addr).auto_accept(true).listen().await?;
                assert_eq!(listener.local_addr()?.port(), port);

                let dst = dst_dir.clone();
                let recv_handle = compio::runtime::spawn(async move {
                    loop {
                        match listener
                            .try_accept_one(
                                Duration::from_millis(500),
                                &dst,
                                |_| Ok(()),
                                |_, _| unreachable!("auto_accept"),
                                |_| Ok(()),
                            )
                            .await
                        {
                            Ok(Some(outcome)) => return Ok(outcome),
                            Ok(None) => {},
                            Err(e) if is_benign_peer_close(&e) => {},
                            Err(e) => return Err(e),
                        }
                    }
                });

                compio::time::sleep(Duration::from_millis(50)).await;

                let send = HayateSender::new()
                    .target(receiver_addr)
                    .send_with(&src_file, |_| Ok(()), |_| Ok(()))
                    .await?;

                let recv = recv_handle
                    .await
                    .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))??;

                assert_eq!(send.checksum, recv.checksum);
                assert_eq!(std::fs::read(&src_file).unwrap(), std::fs::read(&recv.path).unwrap());
                Ok::<(), EngineError>(())
            })
            .expect("listen path should succeed");
    }

    #[test]
    fn direct_passphrase_roundtrip_and_mismatch() {
        let port = pick_free_port();
        let src_dir = unique_test_dir("src-pass");
        let dst_dir = unique_test_dir("dst-pass");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dst_dir).unwrap();
        let src_file = src_dir.join("secret.bin");
        write_random_file(&src_file, 8192);

        let receiver_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));

        // Matching passphrases on both ends complete the transfer.
        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on({
                let src_file = src_file.clone();
                let dst_dir = dst_dir.clone();
                async move {
                    let receiver = HayateReceiver::new()
                        .bind(receiver_addr)
                        .passphrase("hunter2".to_owned())
                        .auto_accept(true);
                    let recv_handle = compio::runtime::spawn(async move {
                        receiver.receive(&dst_dir, |_| true, |_| Ok(())).await
                    });
                    compio::time::sleep(Duration::from_millis(50)).await;
                    let sender_checksum = HayateSender::new()
                        .target(receiver_addr)
                        .passphrase("hunter2".to_owned())
                        .send(&src_file, |_| Ok(()))
                        .await?;
                    let (receiver_checksum, _) = recv_handle
                        .await
                        .map_err(|e| EngineError::Io(std::io::Error::other(e.to_string())))??;
                    assert_eq!(sender_checksum, receiver_checksum);
                    Ok::<(), EngineError>(())
                }
            })
            .expect("matching passphrases should transfer");

        // Mismatched passphrases must fail the handshake.
        let port2 = pick_free_port();
        let receiver_addr2 = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port2));
        let runtime = compio::runtime::Runtime::new().expect("runtime should build");
        runtime
            .block_on(async move {
                let receiver = HayateReceiver::new()
                    .bind(receiver_addr2)
                    .passphrase("hunter2".to_owned())
                    .auto_accept(true);
                let recv_handle = compio::runtime::spawn(async move {
                    receiver.receive(unique_test_dir("dst-pass2"), |_| true, |_| Ok(())).await
                });
                compio::time::sleep(Duration::from_millis(50)).await;
                let send_err = HayateSender::new()
                    .target(receiver_addr2)
                    .passphrase("wrong".to_owned())
                    .send(&src_file, |_| Ok(()))
                    .await;
                assert!(send_err.is_err(), "mismatched passphrase must fail");
                let _ = recv_handle.await;
                Ok::<(), EngineError>(())
            })
            .expect("mismatch path should run");
    }

    #[test]
    fn is_benign_peer_close_false_for_other_errors() {
        assert!(!is_benign_peer_close(&EngineError::TransferRejected));
        assert!(!is_benign_peer_close(&EngineError::Handshake("nope".into())));
    }
}
