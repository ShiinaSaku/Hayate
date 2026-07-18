//! QUIC network layer built on compio-quic (quinn-proto sans tokio).
//!
//! TLS certificates are ephemeral self-signed; peers trust on first use.
//! The sender/receiver generate fresh certs every run; the remote peer
//! accepts any cert (InsecureSkipVerify — the application layer key
//! exchange provides the actual channel binding).

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use compio_quic::{ClientConfig, Endpoint, ServerConfig};
use rcgen::KeyPair;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use crate::EngineError;

/// TLS configs are built once and cached. Build failures surface as
/// [`EngineError`] rather than panicking the calling thread.
static SERVER_CFG: OnceLock<ServerConfig> = OnceLock::new();
static CLIENT_CFG: OnceLock<ClientConfig> = OnceLock::new();

/// Generates an ephemeral self-signed TLS cert + key.
pub fn generate_self_signed()
-> Result<(Vec<CertificateDer<'static>>, rustls::pki_types::PrivateKeyDer<'static>), EngineError> {
    let key_pair = KeyPair::generate().map_err(|e| EngineError::Handshake(e.to_string()))?;
    let params = rcgen::CertificateParams::new(vec!["hayate.local".to_owned()])
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    let cert = params.self_signed(&key_pair).map_err(|e| EngineError::Handshake(e.to_string()))?;

    let der = CertificateDer::from(cert.der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    Ok((vec![der], key))
}

/// Helper to construct OS-aware Quinn TransportConfig.
pub fn build_transport_config() -> Arc<quinn_proto::TransportConfig> {
    let mut config = quinn_proto::TransportConfig::default();

    // 1. Congestion Control
    // We use the default Cubic congestion controller. BBR requires precise
    // user-space pacing timers which incur substantial timer system-call
    // overhead on macOS/Android, resulting in high CPU usage and lower
    // throughput on high-speed local Wi-Fi/LANs.

    // 2. Asymmetric Flow Control Windows
    #[cfg(target_os = "android")]
    {
        // Android/Termux: Optimized flow control windows to prevent kernel buffer drops
        // while allowing enough outstanding data to saturate Wi-Fi links.
        config.stream_receive_window(quinn_proto::VarInt::from_u32(4_194_304)); // 4 MB
        config.receive_window(quinn_proto::VarInt::from_u32(8_388_608)); // 8 MB
        config.send_window(8_388_608); // 8 MB
    }

    #[cfg(not(target_os = "android"))]
    {
        // macOS/Linux/Windows: Large buffers for high-speed LAN transfers.
        // On Windows, IOCP may need slightly smaller windows to avoid
        // non-paged pool exhaustion with many concurrent streams.
        #[cfg(target_os = "windows")]
        {
            config.stream_receive_window(quinn_proto::VarInt::from_u32(16_777_216)); // 16 MB
            config.receive_window(quinn_proto::VarInt::from_u32(33_554_432)); // 32 MB
            config.send_window(33_554_432); // 32 MB
        }
        #[cfg(not(target_os = "windows"))]
        {
            config.stream_receive_window(quinn_proto::VarInt::from_u32(25_165_824)); // 24 MB
            config.receive_window(quinn_proto::VarInt::from_u32(50_331_648)); // 48 MB
            config.send_window(50_331_648); // 48 MB
        }
    }

    // 3. Keep-alive and timeout settings — tuned for large file transfers.
    // Keep-alive pings every 3 s prevent NAT/firewall timeouts during
    // transfers that saturate the link but produce no QUIC ACKs.
    config.keep_alive_interval(Some(std::time::Duration::from_secs(3)));

    // Idle timeout extended to 5 minutes so slow links and very large
    // files (100+ GB) don't trigger spurious disconnects.
    config.max_idle_timeout(Some(
        quinn_proto::VarInt::from_u32(300_000).into(), // 5 min
    ));

    config.initial_mtu(1450);
    config.enable_segmentation_offload(true);

    Arc::new(config)
}

/// Builds a `ServerConfig` for the receiver endpoint.
///
/// The config is built once and cached. Certificate generation or config
/// assembly errors are propagated as [`EngineError`] rather than panicking.
pub fn server_config() -> Result<ServerConfig, EngineError> {
    if let Some(cfg) = SERVER_CFG.get() {
        return Ok(cfg.clone());
    }
    let cfg = build_server_config()?;
    Ok(SERVER_CFG.get_or_init(|| cfg).clone())
}

/// Builds a `ClientConfig` that accepts any server certificate.
///
/// The config is built once and cached. Config assembly errors are propagated
/// as [`EngineError`] rather than panicking.
pub fn client_config() -> Result<ClientConfig, EngineError> {
    if let Some(cfg) = CLIENT_CFG.get() {
        return Ok(cfg.clone());
    }
    let cfg = build_client_config()?;
    Ok(CLIENT_CFG.get_or_init(|| cfg).clone())
}

/// Assembles the receiver server config, returning an error instead of
/// panicking if cert generation or config assembly fails.
fn build_server_config() -> Result<ServerConfig, EngineError> {
    let (certs, key) = generate_self_signed()?;
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    tls.alpn_protocols = vec![b"hayate".to_vec()];
    let quic_server = compio_quic::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    let mut server_cfg = ServerConfig::with_crypto(Arc::new(quic_server));
    server_cfg.transport_config(build_transport_config());
    Ok(server_cfg)
}

/// Assembles the client config, returning an error instead of panicking if
/// config assembly fails.
fn build_client_config() -> Result<ClientConfig, EngineError> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipCertVerification))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"hayate".to_vec()];
    let quic_client = compio_quic::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    let mut client_cfg = ClientConfig::new(Arc::new(quic_client));
    client_cfg.transport_config(build_transport_config());
    Ok(client_cfg)
}

/// Creates a QUIC listener endpoint bound to `addr`.
#[allow(clippy::unused_async)]
pub async fn bind_server(addr: SocketAddr) -> Result<Endpoint, EngineError> {
    let cfg = server_config()?;
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(EngineError::Io)?;

    // Set large buffer sizes and non-blocking.
    // On Windows, SO_RCVBUF/SO_SNDBUF behave differently (they set the
    // total buffer, not the per-socket minimum), so we use a value that
    // works across platforms without exceeding OS limits.
    socket.set_nonblocking(true).map_err(EngineError::Io)?;

    #[cfg(target_os = "windows")]
    {
        socket.set_recv_buffer_size(8_388_608).map_err(EngineError::Io)?;
        socket.set_send_buffer_size(8_388_608).map_err(EngineError::Io)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        socket.set_recv_buffer_size(26_214_400).map_err(EngineError::Io)?;
        socket.set_send_buffer_size(26_214_400).map_err(EngineError::Io)?;
    }

    socket.bind(&addr.into()).map_err(EngineError::Io)?;
    let std_socket: std::net::UdpSocket = socket.into();
    let compio_socket = compio::net::UdpSocket::from_std(std_socket).map_err(EngineError::Io)?;

    let endpoint_config = quinn_proto::EndpointConfig::default();
    let endpoint =
        Endpoint::new(compio_socket, endpoint_config, Some(cfg), None).map_err(EngineError::Io)?;
    Ok(endpoint)
}

/// Creates a QUIC client endpoint bound to an ephemeral local port.
#[allow(clippy::unused_async)]
pub async fn bind_client() -> Result<Endpoint, EngineError> {
    let cfg = client_config()?;
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().expect("static parse");
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(bind_addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .map_err(EngineError::Io)?;

    socket.set_nonblocking(true).map_err(EngineError::Io)?;

    #[cfg(target_os = "windows")]
    {
        socket.set_recv_buffer_size(8_388_608).map_err(EngineError::Io)?;
        socket.set_send_buffer_size(8_388_608).map_err(EngineError::Io)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        socket.set_recv_buffer_size(26_214_400).map_err(EngineError::Io)?;
        socket.set_send_buffer_size(26_214_400).map_err(EngineError::Io)?;
    }

    socket.bind(&bind_addr.into()).map_err(EngineError::Io)?;
    let std_socket: std::net::UdpSocket = socket.into();
    let compio_socket = compio::net::UdpSocket::from_std(std_socket).map_err(EngineError::Io)?;

    let endpoint_config = quinn_proto::EndpointConfig::default();
    let endpoint =
        Endpoint::new(compio_socket, endpoint_config, None, Some(cfg)).map_err(EngineError::Io)?;
    Ok(endpoint)
}

// ---------------------------------------------------------------------------
// Certificate verifier that skips validation (LAN trust-on-connect model)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SkipCertVerification;

impl ServerCertVerifier for SkipCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _msg: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _msg: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // These are advertised to the peer during TLS negotiation, not verified.
        // An empty list causes handshake failure, so the verifier still advertises
        // the schemes the server may present.
        vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519]
    }
}
