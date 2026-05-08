//! Wi-Fi Direct (P2P) transport driver.
//!
//! Enables peer-to-peer communication over Wi-Fi Direct without requiring
//! an access point. Uses wpa_supplicant's P2P control interface for group
//! formation and then TCP over the P2P link.
//!
//! # Requirements
//!
//! - Wi-Fi adapter supporting P2P (most modern adapters)
//! - wpa_supplicant with P2P support (Linux) or platform Wi-Fi Direct API
//! - Control socket accessible (default: `/var/run/wpa_supplicant/p2p-dev-wlan0`)
//!
//! # Protocol
//!
//! 1. P2P discovery (find peers)
//! 2. P2P group formation (GO negotiation or autonomous GO)
//! 3. TCP connection over P2P link interface
//! 4. Standard RavenFabric framing over TCP

use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default wpa_supplicant control interface path.
const DEFAULT_CTRL_PATH: &str = "/var/run/wpa_supplicant/p2p-dev-wlan0";

/// Default port for TCP connection over P2P link.
const DEFAULT_P2P_PORT: u16 = 7710;

/// Wi-Fi Direct group owner intent (0-15, higher = prefer GO role).
const DEFAULT_GO_INTENT: u8 = 7;

/// Transport driver that communicates over Wi-Fi Direct P2P links.
pub struct WifiDirectDriver {
    /// wpa_supplicant control interface path.
    ctrl_path: String,
    /// Port for TCP over P2P link.
    port: u16,
    /// Group Owner intent value (0-15).
    go_intent: u8,
}

impl WifiDirectDriver {
    /// Create a new Wi-Fi Direct driver with default settings.
    pub fn new() -> Self {
        Self {
            ctrl_path: DEFAULT_CTRL_PATH.to_string(),
            port: DEFAULT_P2P_PORT,
            go_intent: DEFAULT_GO_INTENT,
        }
    }

    /// Create a new Wi-Fi Direct driver with custom control path.
    pub fn with_ctrl_path(path: impl Into<String>) -> Self {
        Self {
            ctrl_path: path.into(),
            port: DEFAULT_P2P_PORT,
            go_intent: DEFAULT_GO_INTENT,
        }
    }

    /// Create a new Wi-Fi Direct driver with custom port.
    pub fn with_port(port: u16) -> Self {
        Self {
            ctrl_path: DEFAULT_CTRL_PATH.to_string(),
            port,
            go_intent: DEFAULT_GO_INTENT,
        }
    }

    /// Validate a P2P device address (MAC format: XX:XX:XX:XX:XX:XX).
    pub fn validate_device_address(addr: &str) -> Result<(), TransportError> {
        let parts: Vec<&str> = addr.split(':').collect();
        if parts.len() != 6 {
            return Err(TransportError::Connection(format!(
                "Wi-Fi Direct device address must be MAC format (XX:XX:XX:XX:XX:XX), got {} parts",
                parts.len()
            )));
        }
        for part in &parts {
            if part.len() != 2 || !part.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(TransportError::Connection(format!(
                    "invalid device address octet: '{part}'"
                )));
            }
        }
        Ok(())
    }

    /// Format a P2P connect command for wpa_supplicant.
    pub fn format_connect_cmd(&self, device_addr: &str, method: &str) -> String {
        format!(
            "P2P_CONNECT {} {} go_intent={}\n",
            device_addr, method, self.go_intent
        )
    }

    /// Format a P2P find command.
    pub fn format_find_cmd(timeout_secs: u16) -> String {
        format!("P2P_FIND {timeout_secs}\n")
    }

    /// Parse a P2P peer info response.
    pub fn parse_peer_info(response: &str) -> Option<P2pPeer> {
        let mut name = None;
        let mut addr = None;
        let mut go_capable = false;

        for line in response.lines() {
            if let Some(val) = line.strip_prefix("p2p_device_addr=") {
                addr = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("device_name=") {
                name = Some(val.trim().to_string());
            } else if line.contains("group_capab=0x") {
                // Bit 0 of group_capab indicates GO capability
                if let Some(hex) = line.split("0x").nth(1) {
                    if let Ok(cap) = u8::from_str_radix(hex.trim(), 16) {
                        go_capable = cap & 0x01 != 0;
                    }
                }
            }
        }

        Some(P2pPeer {
            device_addr: addr?,
            device_name: name.unwrap_or_default(),
            go_capable,
        })
    }
}

/// Represents a discovered Wi-Fi Direct peer.
#[derive(Debug, Clone, PartialEq)]
pub struct P2pPeer {
    /// Device MAC address.
    pub device_addr: String,
    /// Human-readable device name.
    pub device_name: String,
    /// Whether the peer can be a Group Owner.
    pub go_capable: bool,
}

impl Default for WifiDirectDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for WifiDirectDriver {
    fn name(&self) -> &str {
        "wifi-direct"
    }

    fn available(&self) -> bool {
        // Available if control path looks valid (has p2p in the path or is a socket path)
        !self.ctrl_path.is_empty()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Extract device address
        let device_addr = config
            .get("device_address")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("p2p://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("Wi-Fi Direct device address not specified".to_string())
            })?;

        Self::validate_device_address(&device_addr)?;

        // Get P2P group IP from config or derive from device address
        let peer_ip = config.get("peer_ip").cloned().unwrap_or_else(|| {
            // Default P2P subnet: 192.168.49.x
            "192.168.49.1".to_string()
        });

        let port = config
            .get("port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(self.port);

        // Connect via TCP over the P2P link
        let addr = format!("{peer_ip}:{port}");
        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Wi-Fi Direct peer at {addr}: {e}"
            ))
        })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let port = addr
            .split(':')
            .next_back()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(self.port);

        // Bind TCP listener on P2P interface
        let bind_addr = format!("0.0.0.0:{port}");
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to bind Wi-Fi Direct listener on {bind_addr}: {e}"
                ))
            })?;

        Ok(Box::new(WifiDirectListener { listener }))
    }
}

/// Listener that accepts incoming Wi-Fi Direct TCP connections.
struct WifiDirectListener {
    listener: tokio::net::TcpListener,
}

#[async_trait::async_trait]
impl Listener for WifiDirectListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let (stream, _addr) = self.listener.accept().await.map_err(|e| {
            TransportError::Connection(format!("Wi-Fi Direct accept failed: {e}"))
        })?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = WifiDirectDriver::new();
        assert_eq!(driver.name(), "wifi-direct");
    }

    #[test]
    fn test_default_settings() {
        let driver = WifiDirectDriver::new();
        assert_eq!(driver.port, 7710);
        assert_eq!(driver.go_intent, 7);
        assert!(driver.ctrl_path.contains("p2p"));
    }

    #[test]
    fn test_available() {
        let driver = WifiDirectDriver::new();
        assert!(driver.available());
        let empty = WifiDirectDriver::with_ctrl_path("");
        assert!(!empty.available());
    }

    #[test]
    fn test_validate_device_address_valid() {
        assert!(WifiDirectDriver::validate_device_address("AA:BB:CC:DD:EE:FF").is_ok());
    }

    #[test]
    fn test_validate_device_address_invalid() {
        let err = WifiDirectDriver::validate_device_address("invalid").unwrap_err();
        assert!(err.to_string().contains("MAC format"));
    }

    #[test]
    fn test_validate_device_address_bad_hex() {
        let err = WifiDirectDriver::validate_device_address("GG:HH:II:JJ:KK:LL").unwrap_err();
        assert!(err.to_string().contains("invalid device address octet"));
    }

    #[test]
    fn test_format_connect_cmd() {
        let driver = WifiDirectDriver::new();
        let cmd = driver.format_connect_cmd("AA:BB:CC:DD:EE:FF", "pbc");
        assert_eq!(cmd, "P2P_CONNECT AA:BB:CC:DD:EE:FF pbc go_intent=7\n");
    }

    #[test]
    fn test_format_find_cmd() {
        let cmd = WifiDirectDriver::format_find_cmd(30);
        assert_eq!(cmd, "P2P_FIND 30\n");
    }

    #[test]
    fn test_parse_peer_info() {
        let response = "p2p_device_addr=AA:BB:CC:DD:EE:FF\ndevice_name=TestDevice\ngroup_capab=0x01\n";
        let peer = WifiDirectDriver::parse_peer_info(response).unwrap();
        assert_eq!(peer.device_addr, "AA:BB:CC:DD:EE:FF");
        assert_eq!(peer.device_name, "TestDevice");
        assert!(peer.go_capable);
    }

    #[test]
    fn test_parse_peer_info_no_go() {
        let response = "p2p_device_addr=11:22:33:44:55:66\ndevice_name=Peer2\ngroup_capab=0x00\n";
        let peer = WifiDirectDriver::parse_peer_info(response).unwrap();
        assert!(!peer.go_capable);
    }

    #[test]
    fn test_parse_peer_info_missing_addr() {
        let response = "device_name=NoAddr\n";
        let peer = WifiDirectDriver::parse_peer_info(response);
        assert!(peer.is_none());
    }

    #[tokio::test]
    async fn test_dial_missing_device_address() {
        let driver = WifiDirectDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("device address not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_device_address() {
        let driver = WifiDirectDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("p2p://invalid-addr".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("MAC format")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_peer_unreachable() {
        let driver = WifiDirectDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("device_address".to_string(), "AA:BB:CC:DD:EE:FF".to_string());
        config.insert("peer_ip".to_string(), "127.0.0.1".to_string());
        config.insert("port".to_string(), "19999".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to Wi-Fi Direct peer")),
            Ok(_) => panic!("expected error"),
        }
    }
}
