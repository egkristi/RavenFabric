//! Native TLS termination for the relay (ROADMAP R0.3 / finding F3).
//!
//! The relay historically accepted raw TCP, forcing operators to terminate TLS
//! in a reverse proxy (which also broke per-IP rate limiting because the source
//! address seen by the relay was the proxy's, not the client's). This module
//! lets the relay terminate TLS natively using `rustls` so a single binary can
//! serve WSS on 443 directly.
//!
//! Only **manual** certificate mode is implemented here (`--tls-cert`,
//! `--tls-key`). ACME auto-provisioning and PROXY-protocol support are follow-on
//! enhancements tracked in the roadmap.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

/// How TLS is configured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS disabled — raw TCP (behind a reverse proxy).
    #[default]
    Off,
    /// Manual TLS with a certificate/key pair on disk.
    Manual { cert_path: String, key_path: String },
}

/// A loaded rustls server configuration, ready to accept TLS connections.
#[derive(Clone)]
pub struct RelayTlsConfig {
    acceptor: TlsAcceptor,
}

impl std::fmt::Debug for RelayTlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayTlsConfig").finish_non_exhaustive()
    }
}

impl RelayTlsConfig {
    /// Load a TLS server config from a certificate and private key PEM pair.
    pub fn from_files(cert_path: &Path, key_path: &Path) -> anyhow::Result<Self> {
        let certs = load_certs(cert_path)?;
        let key = load_key(key_path)?;

        // Explicitly select the `ring` crypto provider. Feature unification can
        // enable both `ring` and `aws-lc-rs` (via tokio-tungstenite's
        // `rustls-tls-native-roots`), in which case `builder()` cannot
        // auto-determine a provider and panics at runtime.
        let provider = rustls::crypto::ring::default_provider();
        let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .map_err(|e| anyhow::anyhow!("failed to select TLS protocol versions: {e}"))?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| anyhow::anyhow!("invalid TLS server config: {e}"))?;

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(server_config)),
        })
    }

    /// Accept a TCP stream and perform the TLS handshake, returning the TLS
    /// stream on success.
    pub async fn accept(
        &self,
        tcp: TcpStream,
    ) -> Result<tokio_rustls::server::TlsStream<TcpStream>, std::io::Error> {
        self.acceptor.accept(tcp).await
    }
}

/// Load the certificate chain from a PEM file.
fn load_certs(path: &Path) -> anyhow::Result<Vec<CertificateDer<'static>>> {
    let data = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read cert file {}: {e}", path.display()))?;
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut &data[..])
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("failed to parse cert file {}: {e}", path.display()))?;
    if certs.is_empty() {
        anyhow::bail!("no certificates found in {}", path.display());
    }
    Ok(certs)
}

/// Load the private key from a PEM file.
fn load_key(path: &Path) -> anyhow::Result<PrivateKeyDer<'static>> {
    let data = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read key file {}: {e}", path.display()))?;
    let key = rustls_pemfile::private_key(&mut &data[..])
        .map_err(|e| anyhow::anyhow!("failed to parse key file {}: {e}", path.display()))?
        .ok_or_else(|| anyhow::anyhow!("no private key found in {}", path.display()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Generate a self-signed certificate/key pair, written to temp files.
    fn write_test_cert() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cert =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("cert generation");
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");
        std::fs::write(&cert_path, cert.cert.pem()).expect("write cert");
        std::fs::write(&key_path, cert.key_pair.serialize_pem()).expect("write key");
        (dir, cert_path, key_path)
    }

    /// A TLS client config that skips server certificate verification.
    fn skip_verify_client_config() -> Arc<rustls::ClientConfig> {
        #[derive(Debug)]
        struct SkipVerification;
        impl rustls::client::danger::ServerCertVerifier for SkipVerification {
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
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
            }
            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
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

        let provider = rustls::crypto::ring::default_provider();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
            .with_safe_default_protocol_versions()
            .expect("valid protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerification))
            .with_no_client_auth();
        Arc::new(config)
    }

    #[test]
    fn test_tls_mode_default_is_off() {
        assert_eq!(TlsMode::default(), TlsMode::Off);
    }

    #[test]
    fn test_load_missing_cert_fails() {
        let err = RelayTlsConfig::from_files(
            Path::new("/nonexistent/cert.pem"),
            Path::new("/nonexistent/key.pem"),
        );
        assert!(err.is_err());
    }

    #[test]
    fn test_load_valid_cert_succeeds() {
        let (_dir, cert_path, key_path) = write_test_cert();
        let config = RelayTlsConfig::from_files(&cert_path, &key_path);
        assert!(config.is_ok(), "valid cert/key should load: {config:?}");
    }

    /// End-to-end test: a TLS server terminates a handshake and echoes bytes
    /// back to a TLS client, validating `RelayTlsConfig::accept()`.
    #[tokio::test]
    async fn test_tls_accept_roundtrip() {
        let (_dir, cert_path, key_path) = write_test_cert();
        let config = RelayTlsConfig::from_files(&cert_path, &key_path).expect("load config");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");

        // Server task: accept → TLS handshake → echo.
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("accept");
            let mut tls = config.accept(tcp).await.expect("tls handshake");
            let mut buf = [0u8; 5];
            tls.read_exact(&mut buf).await.expect("read");
            assert_eq!(&buf, b"hello");
            tls.write_all(b"world").await.expect("write");
        });

        // Client task: connect → TLS handshake → send → read echo.
        let client = tokio::spawn(async move {
            let tcp = tokio::net::TcpStream::connect(addr).await.expect("connect");
            let connector = tokio_rustls::TlsConnector::from(skip_verify_client_config());
            let name = rustls::pki_types::ServerName::try_from("localhost")
                .expect("server name")
                .to_owned();
            let mut tls = connector.connect(name, tcp).await.expect("client tls");
            tls.write_all(b"hello").await.expect("write");
            let mut buf = [0u8; 5];
            tls.read_exact(&mut buf).await.expect("read");
            assert_eq!(&buf, b"world");
        });

        let (server_res, client_res) = tokio::join!(server, client);
        server_res.expect("server task");
        client_res.expect("client task");
    }
}
