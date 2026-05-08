//! Tor hidden service transport driver.
//!
//! Connects to `.onion` addresses through a local Tor SOCKS5 proxy
//! (typically `127.0.0.1:9050`). Provides anonymous, censorship-resistant
//! connectivity without requiring the remote agent to have a public IP.
//!
//! # Requirements
//!
//! - Tor daemon running locally with SOCKS5 proxy enabled
//! - Target agent configured as a Tor hidden service
//!
//! # Configuration
//!
//! Driver config keys:
//! - `socks_proxy`: SOCKS5 proxy address (default: `127.0.0.1:9050`)
//! - `onion_addr`: Target `.onion` address (can also come from `Target.relay_url`)
//! - `port`: Target port (default: `9443`)

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default Tor SOCKS5 proxy address.
const DEFAULT_SOCKS_PROXY: &str = "127.0.0.1:9050";

/// Default target port for RavenFabric over Tor.
const DEFAULT_PORT: u16 = 9443;

/// Transport driver that routes connections through Tor's SOCKS5 proxy.
pub struct TorDriver {
    /// SOCKS5 proxy address (e.g., "127.0.0.1:9050").
    socks_addr: String,
}

impl TorDriver {
    /// Create a new Tor driver using the default SOCKS5 proxy address.
    pub fn new() -> Self {
        Self {
            socks_addr: DEFAULT_SOCKS_PROXY.to_string(),
        }
    }

    /// Create a new Tor driver with a custom SOCKS5 proxy address.
    pub fn with_socks_addr(socks_addr: impl Into<String>) -> Self {
        Self {
            socks_addr: socks_addr.into(),
        }
    }

    /// Perform SOCKS5 CONNECT handshake to reach a .onion address.
    async fn socks5_connect(
        &self,
        onion_host: &str,
        port: u16,
    ) -> Result<TcpStream, TransportError> {
        // Connect to the SOCKS5 proxy
        let proxy_addr: SocketAddr = self
            .socks_addr
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid SOCKS5 addr: {e}")))?;

        let mut stream = TcpStream::connect(proxy_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Tor SOCKS5 proxy at {}: {e}",
                self.socks_addr
            ))
        })?;

        // SOCKS5 greeting: version 5, 1 auth method (no auth)
        stream
            .write_all(&[0x05, 0x01, 0x00])
            .await
            .map_err(|e| TransportError::Connection(format!("SOCKS5 greeting failed: {e}")))?;

        // Read server response (2 bytes: version + chosen method)
        let mut resp = [0u8; 2];
        stream
            .read_exact(&mut resp)
            .await
            .map_err(|e| TransportError::Connection(format!("SOCKS5 response failed: {e}")))?;

        if resp[0] != 0x05 || resp[1] != 0x00 {
            return Err(TransportError::Connection(format!(
                "SOCKS5 auth rejected: version={:#x}, method={:#x}",
                resp[0], resp[1]
            )));
        }

        // SOCKS5 CONNECT request with domain name (type 0x03)
        let host_bytes = onion_host.as_bytes();
        if host_bytes.len() > 255 {
            return Err(TransportError::Connection(
                "onion address too long".to_string(),
            ));
        }

        let mut req = Vec::with_capacity(7 + host_bytes.len());
        req.push(0x05); // version
        req.push(0x01); // CONNECT command
        req.push(0x00); // reserved
        req.push(0x03); // domain name address type
        req.push(host_bytes.len() as u8); // domain length
        req.extend_from_slice(host_bytes); // domain
        req.push((port >> 8) as u8); // port high byte
        req.push((port & 0xff) as u8); // port low byte

        stream
            .write_all(&req)
            .await
            .map_err(|e| TransportError::Connection(format!("SOCKS5 connect req failed: {e}")))?;

        // Read CONNECT response (minimum 10 bytes for IPv4 bind addr)
        let mut connect_resp = [0u8; 10];
        stream.read_exact(&mut connect_resp).await.map_err(|e| {
            TransportError::Connection(format!("SOCKS5 connect response failed: {e}"))
        })?;

        if connect_resp[0] != 0x05 {
            return Err(TransportError::Connection(format!(
                "SOCKS5 bad version in response: {:#x}",
                connect_resp[0]
            )));
        }

        if connect_resp[1] != 0x00 {
            let reason = match connect_resp[1] {
                0x01 => "general SOCKS server failure",
                0x02 => "connection not allowed by ruleset",
                0x03 => "network unreachable",
                0x04 => "host unreachable",
                0x05 => "connection refused",
                0x06 => "TTL expired",
                0x07 => "command not supported",
                0x08 => "address type not supported",
                _ => "unknown error",
            };
            return Err(TransportError::Connection(format!(
                "SOCKS5 CONNECT failed: {reason} (code {:#x})",
                connect_resp[1]
            )));
        }

        // If bind address type is domain (0x03) or IPv6 (0x04), read extra bytes
        match connect_resp[3] {
            0x01 => {} // IPv4: already read all 10 bytes (4 + 2 + 4 addr bytes)
            0x04 => {
                // IPv6: need 12 more bytes (16 addr - 4 already read + 2 port - 2 already counted)
                let mut extra = [0u8; 12];
                stream.read_exact(&mut extra).await.map_err(|e| {
                    TransportError::Connection(format!("SOCKS5 IPv6 bind read failed: {e}"))
                })?;
            }
            0x03 => {
                // Domain: read length byte + domain + port (already got partial data)
                let domain_len = connect_resp[4] as usize;
                if domain_len > 4 {
                    let mut extra = vec![0u8; domain_len - 4 + 2];
                    stream.read_exact(&mut extra).await.map_err(|e| {
                        TransportError::Connection(format!("SOCKS5 domain bind read failed: {e}"))
                    })?;
                }
            }
            _ => {}
        }

        Ok(stream)
    }
}

impl Default for TorDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for TorDriver {
    fn name(&self) -> &str {
        "tor"
    }

    fn available(&self) -> bool {
        // Quick check: see if SOCKS5 proxy address is parseable
        // (actual availability requires async connect, checked at dial time)
        self.socks_addr.parse::<SocketAddr>().is_ok()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Determine .onion address from config or target
        let onion_addr = config
            .get("onion_addr")
            .or(target.relay_url.as_ref())
            .ok_or_else(|| {
                TransportError::Connection(
                    "no onion_addr in config or relay_url in target".to_string(),
                )
            })?
            .clone();

        // Strip protocol prefix if present
        let onion_host = onion_addr
            .strip_prefix("tor://")
            .or_else(|| onion_addr.strip_prefix("onion://"))
            .unwrap_or(&onion_addr);

        // Parse port (may be embedded in address as host:port)
        let (host, port) = if let Some((h, p)) = onion_host.rsplit_once(':') {
            let port = p.parse::<u16>().map_err(|e| {
                TransportError::Connection(format!("invalid port in onion address: {e}"))
            })?;
            (h, port)
        } else {
            let port = config
                .get("port")
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(DEFAULT_PORT);
            (onion_host, port)
        };

        // Validate .onion address format
        if !host.ends_with(".onion") {
            return Err(TransportError::Connection(format!(
                "invalid Tor address (must end in .onion): {host}"
            )));
        }

        let stream = self.socks5_connect(host, port).await?;
        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Listening as a Tor hidden service requires Tor control port configuration
        // (ADD_ONION command). This is a future enhancement.
        Err(TransportError::Connection(
            "Tor hidden service listening requires Tor control port (not yet implemented)".into(),
        ))
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = TorDriver::new();
        assert_eq!(driver.name(), "tor");
    }

    #[test]
    fn test_default_socks_addr() {
        let driver = TorDriver::default();
        assert!(driver.available());
        assert_eq!(driver.socks_addr, "127.0.0.1:9050");
    }

    #[test]
    fn test_custom_socks_addr() {
        let driver = TorDriver::with_socks_addr("127.0.0.1:9150");
        assert!(driver.available());
    }

    #[test]
    fn test_invalid_socks_addr_not_available() {
        let driver = TorDriver::with_socks_addr("not-a-valid-addr");
        assert!(!driver.available());
    }

    #[tokio::test]
    async fn test_dial_missing_onion_addr() {
        let driver = TorDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("no onion_addr"));
    }

    #[tokio::test]
    async fn test_dial_invalid_onion_addr() {
        let driver = TorDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("example.com:9443".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("must end in .onion"));
    }

    #[tokio::test]
    async fn test_dial_no_tor_proxy() {
        // Attempt to connect through a non-existent SOCKS5 proxy
        let driver = TorDriver::with_socks_addr("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("abc123xyz.onion:9443".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("failed to connect to Tor SOCKS5 proxy"));
    }

    #[tokio::test]
    async fn test_listen_returns_error() {
        let driver = TorDriver::new();
        let result = driver.listen("0.0.0.0:9443").await;
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("control port"));
    }

    #[tokio::test]
    async fn test_dial_with_protocol_prefix() {
        let driver = TorDriver::with_socks_addr("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("tor://abc123xyz.onion:9443".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        // Should fail at TCP connect (not at address parsing)
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("failed to connect to Tor SOCKS5 proxy"));
    }

    #[tokio::test]
    async fn test_dial_port_from_config() {
        let driver = TorDriver::with_socks_addr("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("abc123xyz.onion".into()),
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("port".into(), "8080".into());
        let result = driver.dial(&target, &config).await;
        // Should fail at TCP connect, not at config parsing
        assert!(result.is_err());
        let err = result.err().expect("expected error").to_string();
        assert!(err.contains("failed to connect to Tor SOCKS5 proxy"));
    }
}
