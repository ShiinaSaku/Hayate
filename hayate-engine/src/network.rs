//! QUIC network layer built on compio-quic (quinn-proto sans tokio).
//!
//! TLS certificates are ephemeral self-signed; peers trust on first use.
//! The sender/receiver generate fresh certs every run; the remote peer
//! accepts any cert (InsecureSkipVerify — the application layer key
//! exchange provides the actual channel binding).

use std::{
    net::SocketAddr,
    sync::{Arc, OnceLock},
};

use compio_quic::{ClientConfig, Endpoint, ServerConfig};
use rcgen::KeyPair;
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

use crate::EngineError;

static SERVER_CFG: OnceLock<ServerConfig> = OnceLock::new();
static CLIENT_CFG: OnceLock<ClientConfig> = OnceLock::new();

/// Generates an ephemeral self-signed TLS cert + key.
pub fn generate_self_signed() -> Result<
    (
        Vec<CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ),
    EngineError,
> {
    let key_pair = KeyPair::generate().map_err(|e| EngineError::Handshake(e.to_string()))?;
    let params = rcgen::CertificateParams::new(vec!["hayate.local".to_owned()])
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;

    let der = CertificateDer::from(cert.der().to_vec());
    let key = rustls::pki_types::PrivateKeyDer::try_from(key_pair.serialize_der())
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    Ok((vec![der], key))
}

/// Builds a `ServerConfig` for the receiver endpoint.
pub fn server_config() -> Result<ServerConfig, EngineError> {
    SERVER_CFG.get_or_init(|| {
        let (certs, key) = generate_self_signed().expect("failed to generate ephemeral TLS cert");
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("failed to build Server TLS config");
        tls.alpn_protocols = vec![b"hayate".to_vec()];
        let quic_server = compio_quic::crypto::rustls::QuicServerConfig::try_from(tls)
            .expect("failed to create QUIC server config");
        let mut server_cfg = ServerConfig::with_crypto(Arc::new(quic_server));

        let mut transport = quinn_proto::TransportConfig::default();
        transport.stream_receive_window(quinn_proto::VarInt::from_u32(32 * 1024 * 1024));
        transport.receive_window(quinn_proto::VarInt::from_u32(64 * 1024 * 1024));
        transport.send_window(64 * 1024 * 1024);
        transport.max_idle_timeout(Some(quinn_proto::VarInt::from_u32(60_000).into()));
        server_cfg.transport_config(Arc::new(transport));

        server_cfg
    });
    Ok(SERVER_CFG.get().unwrap().clone())
}

/// Builds a `ClientConfig` that accepts any server certificate.
pub fn client_config() -> Result<ClientConfig, EngineError> {
    CLIENT_CFG.get_or_init(|| {
        let mut tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipCertVerification))
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"hayate".to_vec()];
        let quic_client = compio_quic::crypto::rustls::QuicClientConfig::try_from(tls)
            .expect("failed to create QUIC client config");

        let mut client_cfg = ClientConfig::new(Arc::new(quic_client));
        let mut transport = quinn_proto::TransportConfig::default();
        transport.stream_receive_window(quinn_proto::VarInt::from_u32(32 * 1024 * 1024));
        transport.receive_window(quinn_proto::VarInt::from_u32(64 * 1024 * 1024));
        transport.send_window(64 * 1024 * 1024);
        transport.max_idle_timeout(Some(quinn_proto::VarInt::from_u32(60_000).into()));
        client_cfg.transport_config(Arc::new(transport));

        client_cfg
    });
    Ok(CLIENT_CFG.get().unwrap().clone())
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

    // Set 25MB buffers and non-blocking
    socket.set_nonblocking(true).map_err(EngineError::Io)?;
    socket
        .set_recv_buffer_size(26_214_400)
        .map_err(EngineError::Io)?;
    socket
        .set_send_buffer_size(26_214_400)
        .map_err(EngineError::Io)?;
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

    // Set 25MB buffers and non-blocking
    socket.set_nonblocking(true).map_err(EngineError::Io)?;
    socket
        .set_recv_buffer_size(26_214_400)
        .map_err(EngineError::Io)?;
    socket
        .set_send_buffer_size(26_214_400)
        .map_err(EngineError::Io)?;
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
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}
