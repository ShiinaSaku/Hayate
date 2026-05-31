//! QUIC network layer built on compio-quic (quinn-proto sans tokio).
//!
//! TLS certificates are ephemeral self-signed; peers trust on first use.
//! The sender/receiver generate fresh certs every run; the remote peer
//! accepts any cert (InsecureSkipVerify — the application layer key
//! exchange provides the actual channel binding).

use std::{net::SocketAddr, sync::Arc};

use compio_quic::{ClientConfig, Endpoint, ServerConfig};
use rcgen::KeyPair;
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

use crate::EngineError;

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
    let (certs, key) = generate_self_signed()?;
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    tls.alpn_protocols = vec![b"hayate".to_vec()];
    let quic_server = compio_quic::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    let mut server_cfg = ServerConfig::with_crypto(Arc::new(quic_server));
    let transport = quinn_proto::TransportConfig::default();
    server_cfg.transport_config(Arc::new(transport));
    Ok(server_cfg)
}

/// Builds a `ClientConfig` that accepts any server certificate.
pub fn client_config() -> Result<ClientConfig, EngineError> {
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipCertVerification))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"hayate".to_vec()];
    let quic_client = compio_quic::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|e| EngineError::Handshake(e.to_string()))?;
    Ok(ClientConfig::new(Arc::new(quic_client)))
}

/// Creates a QUIC listener endpoint bound to `addr`.
pub async fn bind_server(addr: SocketAddr) -> Result<Endpoint, EngineError> {
    let cfg = server_config()?;
    Endpoint::server(addr, cfg).await.map_err(EngineError::Io)
}

/// Creates a QUIC client endpoint bound to an ephemeral local port.
pub async fn bind_client() -> Result<Endpoint, EngineError> {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse().expect("static parse");
    Endpoint::client(bind_addr).await.map_err(EngineError::Io)
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
