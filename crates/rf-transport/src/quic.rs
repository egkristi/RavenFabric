//! QUIC transport driver.
//!
//! Uses `quinn` for QUIC connections with 0-RTT support, connection
//! migration, and multiplexed streams. Each connection uses a single
//! bi-directional QUIC stream bridged to an AsyncRead+AsyncWrite interface.
//!
//! For RavenFabric, QUIC provides:
//! - UDP-based transport (NAT-friendly, works through more firewalls)
//! - 0-RTT reconnection for known peers
//! - Built-in connection migration (seamless WiFi ↔ cellular)
//! - Multiplexed streams at the transport layer
//!
//! Note: RavenFabric's Noise XX encryption runs *on top* of QUIC's TLS 1.3.
//! QUIC provides transport security and multiplexing; Noise provides mutual
//! authentication independent of the PKI.

use std::net::SocketAddr;
use std::sync::Arc;

use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// QUIC transport driver using quinn.
pub struct QuicDriver;

impl QuicDriver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuicDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a self-signed certificate for QUIC TLS.
/// RavenFabric uses Noise XX on top so TLS identity is not meaningful here,
/// it's just required by the QUIC spec.
fn generate_self_signed_cert() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["ravenfabric.local".into()])
        .expect("cert generation uses valid params");
    let key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    let cert_der = CertificateDer::from(cert.cert);
    (cert_der, PrivateKeyDer::Pkcs8(key))
}

/// Create a server config with a self-signed cert.
fn make_server_config() -> ServerConfig {
    let (cert, key) = generate_self_signed_cert();
    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("valid server TLS config");
    server_crypto.alpn_protocols = vec![b"ravenfabric".to_vec()];
    ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
            .expect("valid QUIC server config"),
    ))
}

/// Create a client config that skips certificate verification.
/// This is safe because RavenFabric uses Noise XX mutual auth on top.
fn make_client_config() -> ClientConfig {
    let mut client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![b"ravenfabric".to_vec()];
    ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
            .expect("valid QUIC client config"),
    ))
}

/// Skip TLS server cert verification — we authenticate via Noise XX instead.
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

#[async_trait::async_trait]
impl Driver for QuicDriver {
    fn name(&self) -> &str {
        "quic"
    }

    fn available(&self) -> bool {
        true
    }

    async fn dial(
        &self,
        target: &Target,
        _config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let url = target
            .relay_url
            .as_ref()
            .ok_or_else(|| TransportError::Connection("no relay_url in target".into()))?;

        // Parse address from URL (quic://host:port or just host:port)
        let addr_str = url.strip_prefix("quic://").unwrap_or(url);
        let addr: SocketAddr = addr_str
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid address: {e}")))?;

        let client_config = make_client_config();
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| TransportError::Connection(format!("bind failed: {e}")))?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(addr, "ravenfabric.local")
            .map_err(|e| TransportError::Connection(format!("connect config: {e}")))?
            .await
            .map_err(|e| TransportError::Connection(format!("connect: {e}")))?;

        // Open a bidirectional stream for RPC
        let (send, recv) = connection
            .open_bi()
            .await
            .map_err(|e| TransportError::Connection(format!("open bi: {e}")))?;

        // Send meet token as first message if provided
        let mut send = send;
        if let Some(token) = &target.meet_token {
            let token_bytes = token.as_bytes();
            let len = (token_bytes.len() as u32).to_be_bytes();
            send.write_all(&len)
                .await
                .map_err(|e| TransportError::Connection(format!("write token len: {e}")))?;
            send.write_all(token_bytes)
                .await
                .map_err(|e| TransportError::Connection(format!("write token: {e}")))?;
        }

        let stream = bridge_quic_bi(send, recv);
        Ok(Box::new(stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid listen addr: {e}")))?;

        let server_config = make_server_config();
        let endpoint = Endpoint::server(server_config, socket_addr)
            .map_err(|e| TransportError::Connection(format!("bind: {e}")))?;

        Ok(Box::new(QuicListener { endpoint }))
    }
}

struct QuicListener {
    endpoint: Endpoint,
}

#[async_trait::async_trait]
impl Listener for QuicListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| TransportError::Connection("endpoint closed".into()))?;

        let connection = incoming
            .await
            .map_err(|e| TransportError::Connection(format!("accept: {e}")))?;

        let (send, recv) = connection
            .accept_bi()
            .await
            .map_err(|e| TransportError::Connection(format!("accept bi: {e}")))?;

        let stream = bridge_quic_bi(send, recv);
        Ok(Box::new(stream))
    }
}

/// Bridge a QUIC bidirectional stream into a tokio DuplexStream.
///
/// Spawns two tasks to shuttle bytes between QUIC send/recv and the app-facing stream.
fn bridge_quic_bi(mut send: quinn::SendStream, mut recv: quinn::RecvStream) -> DuplexStream {
    let (app_stream, bridge_stream) = tokio::io::duplex(65536);
    let (mut bridge_read, mut bridge_write) = tokio::io::split(bridge_stream);

    // QUIC recv → app write
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        while let Ok(Some(n)) = recv.read(&mut buf).await {
            if bridge_write.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });

    // app read → QUIC send
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            match bridge_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if send.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = send.finish();
    });

    app_stream
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_quic_driver_listen_and_dial() {
        let driver = QuicDriver::new();

        // Bind to port 0 — construct server endpoint directly to get bound address
        let server_config = make_server_config();
        let endpoint = Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = endpoint.local_addr().unwrap().to_string();
        let listener = QuicListener { endpoint };

        // Dial from client
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some(addr.clone()),
            meet_token: None,
        };
        let config = DriverConfig::new();

        let accept_handle = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 64];
            let n = stream.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"hello quic");
            stream.write_all(b"hello back").await.unwrap();
        });

        // Small delay to ensure listener is ready
        tokio::task::yield_now().await;

        let mut client_stream = driver.dial(&target, &config).await.unwrap();
        client_stream.write_all(b"hello quic").await.unwrap();
        client_stream.flush().await.unwrap();

        let mut buf = vec![0u8; 64];
        let n = client_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello back");

        accept_handle.await.unwrap();
    }

    #[test]
    fn test_quic_driver_name() {
        let driver = QuicDriver::new();
        assert_eq!(driver.name(), "quic");
        assert!(driver.available());
    }
}
