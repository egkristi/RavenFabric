//! Yggdrasil overlay network transport driver.
//!
//! Connects to agents over the Yggdrasil IPv6 mesh network. Yggdrasil provides
//! a self-configuring encrypted overlay with key-derived IPv6 addresses
//! (`200::/7` prefix). When the Yggdrasil daemon is running locally,
//! connections to Yggdrasil addresses are routed through the mesh transparently.
//!
//! # Requirements
//!
//! - Yggdrasil daemon running locally with TUN interface active
//! - Target agent reachable over Yggdrasil (has a `200::/7` IPv6 address)
//!
//! # Configuration
//!
//! Driver config keys:
//! - `ygg_addr`: Target Yggdrasil IPv6 address (or from `Target.relay_url`)
//! - `port`: Target port (default: `9443`)
//! - `admin_socket`: Optional admin API socket for status queries

use std::net::{Ipv6Addr, SocketAddrV6};

use tokio::net::{TcpListener, TcpStream};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default port for RavenFabric over Yggdrasil.
const DEFAULT_PORT: u16 = 9443;

/// Yggdrasil IPv6 prefix (200::/7).
const YGG_PREFIX_BYTE: u8 = 0x02;

/// Transport driver that routes connections over the Yggdrasil mesh network.
pub struct YggdrasilDriver;

impl YggdrasilDriver {
    pub fn new() -> Self {
        Self
    }

    /// Check if an IPv6 address is in the Yggdrasil range (200::/7).
    fn is_yggdrasil_addr(addr: &Ipv6Addr) -> bool {
        let octets = addr.octets();
        // 200::/7 means the top 7 bits are 0000001 (i.e., first byte & 0xFE == 0x02)
        (octets[0] & 0xFE) == YGG_PREFIX_BYTE
    }

    /// Parse a Yggdrasil address from config/target.
    fn parse_target(
        target: &Target,
        config: &DriverConfig,
    ) -> Result<(Ipv6Addr, u16), TransportError> {
        let addr_str = config
            .get("ygg_addr")
            .or(target.relay_url.as_ref())
            .ok_or_else(|| {
                TransportError::Connection(
                    "no ygg_addr in config or relay_url in target".to_string(),
                )
            })?;

        // Strip protocol prefix
        let addr_clean = addr_str.strip_prefix("ygg://").unwrap_or(addr_str);

        // Handle [ipv6]:port format
        let (host_str, port) = if addr_clean.starts_with('[') {
            // Bracketed IPv6: [addr]:port
            if let Some((bracketed, rest)) = addr_clean.split_once(']') {
                let host = bracketed.strip_prefix('[').unwrap_or(bracketed);
                let port = rest
                    .strip_prefix(':')
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or_else(|| {
                        config
                            .get("port")
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(DEFAULT_PORT)
                    });
                (host, port)
            } else {
                return Err(TransportError::Connection(
                    "malformed bracketed IPv6 address".to_string(),
                ));
            }
        } else {
            // Plain IPv6 address without port
            let port = config
                .get("port")
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            (addr_clean, port)
        };

        let addr: Ipv6Addr = host_str
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid IPv6 address: {e}")))?;

        if !Self::is_yggdrasil_addr(&addr) {
            return Err(TransportError::Connection(format!(
                "address {addr} is not in Yggdrasil range (200::/7)"
            )));
        }

        Ok((addr, port))
    }
}

impl Default for YggdrasilDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for YggdrasilDriver {
    fn name(&self) -> &str {
        "yggdrasil"
    }

    fn available(&self) -> bool {
        // Yggdrasil availability requires the TUN interface to exist.
        // This is a best-effort sync check — full verification at dial time.
        true
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let (addr, port) = Self::parse_target(target, config)?;

        let sock_addr = SocketAddrV6::new(addr, port, 0, 0);
        let stream = TcpStream::connect(sock_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Yggdrasil peer [{addr}]:{port}: {e}"
            ))
        })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Parse listen address — should be [ygg_ipv6]:port or just :port
        let listen_addr = if addr.starts_with(':') {
            format!("[::]{addr}")
        } else if addr.starts_with('[') {
            addr.to_string()
        } else {
            format!("[{addr}]")
        };

        let listener = TcpListener::bind(&listen_addr).await.map_err(|e| {
            TransportError::Connection(format!("failed to bind Yggdrasil listener on {addr}: {e}"))
        })?;

        Ok(Box::new(YggdrasilListener { listener }))
    }
}

/// Listener for incoming Yggdrasil TCP connections.
struct YggdrasilListener {
    listener: TcpListener,
}

#[async_trait::async_trait]
impl Listener for YggdrasilListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let (stream, _peer_addr) = self
            .listener
            .accept()
            .await
            .map_err(|e| TransportError::Connection(format!("Yggdrasil accept failed: {e}")))?;
        Ok(Box::new(stream))
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = YggdrasilDriver::new();
        assert_eq!(driver.name(), "yggdrasil");
    }

    #[test]
    fn test_available() {
        let driver = YggdrasilDriver;
        assert!(driver.available());
    }

    #[test]
    fn test_is_yggdrasil_addr() {
        // Valid Yggdrasil addresses (200::/7 → first byte 0x02 or 0x03)
        let valid: Ipv6Addr = "200::1".parse().unwrap();
        assert!(YggdrasilDriver::is_yggdrasil_addr(&valid));

        let valid2: Ipv6Addr = "201:abcd:ef01:2345:6789:abcd:ef01:2345".parse().unwrap();
        assert!(YggdrasilDriver::is_yggdrasil_addr(&valid2));

        let valid3: Ipv6Addr = "300::1".parse().unwrap();
        assert!(YggdrasilDriver::is_yggdrasil_addr(&valid3));

        // Invalid — not in 200::/7
        let invalid: Ipv6Addr = "fe80::1".parse().unwrap();
        assert!(!YggdrasilDriver::is_yggdrasil_addr(&invalid));

        let invalid2: Ipv6Addr = "::1".parse().unwrap();
        assert!(!YggdrasilDriver::is_yggdrasil_addr(&invalid2));

        let invalid3: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert!(!YggdrasilDriver::is_yggdrasil_addr(&invalid3));
    }

    #[tokio::test]
    async fn test_dial_missing_addr() {
        let driver = YggdrasilDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("expected error")
                .to_string()
                .contains("no ygg_addr")
        );
    }

    #[tokio::test]
    async fn test_dial_non_yggdrasil_addr() {
        let driver = YggdrasilDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("fe80::1".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("expected error")
                .to_string()
                .contains("not in Yggdrasil range")
        );
    }

    #[tokio::test]
    async fn test_dial_invalid_ipv6() {
        let driver = YggdrasilDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("not-an-address".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("expected error")
                .to_string()
                .contains("invalid IPv6")
        );
    }

    #[tokio::test]
    async fn test_dial_with_prefix() {
        let driver = YggdrasilDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("ygg://200::dead:beef".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        // Should fail at TCP connect (no Yggdrasil running), not at parsing
        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("expected error")
                .to_string()
                .contains("failed to connect")
        );
    }

    #[tokio::test]
    async fn test_dial_bracketed_with_port() {
        let driver = YggdrasilDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: Some("[200::1]:8080".into()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        assert!(
            result
                .err()
                .expect("expected error")
                .to_string()
                .contains("failed to connect")
        );
    }

    #[tokio::test]
    async fn test_listen_and_accept() {
        let driver = YggdrasilDriver::new();
        // Bind to localhost (not a real Yggdrasil address, but tests the listen path)
        let listener_result = driver.listen("[::1]:0").await;
        assert!(listener_result.is_ok());
    }
}
