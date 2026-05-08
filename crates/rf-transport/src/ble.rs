//! Bluetooth Low Energy (BLE) transport driver.
//!
//! Provides short-range communication over BLE GATT characteristics.
//! Uses a custom GATT service with RX/TX characteristics for bidirectional
//! data transfer. Designed for proximity-based agent communication.
//!
//! # Requirements
//!
//! - BLE adapter available (typically via D-Bus/BlueZ on Linux, CoreBluetooth on macOS)
//! - GATT server/client capability
//!
//! # Protocol
//!
//! Custom GATT Service UUID: `6e400001-b5a3-f393-e0a9-e50e24dcca9e` (Nordic UART)
//! - TX Characteristic (notify): agent → client
//! - RX Characteristic (write): client → agent
//! - MTU negotiation up to 512 bytes
//! - Frame reassembly for payloads > MTU

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Nordic UART Service UUID.
const NUS_SERVICE_UUID: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";

/// TX Characteristic UUID (notify, agent → client).
#[allow(dead_code)]
const NUS_TX_UUID: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

/// RX Characteristic UUID (write, client → agent).
#[allow(dead_code)]
const NUS_RX_UUID: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";

/// Default BLE MTU.
const DEFAULT_MTU: usize = 23;

/// Maximum negotiable MTU.
const MAX_MTU: usize = 512;

/// BLE adapter proxy address (for TCP bridge to BlueZ D-Bus or platform BLE stack).
const DEFAULT_PROXY_ADDR: &str = "127.0.0.1:7700";

/// Transport driver that communicates over Bluetooth Low Energy.
pub struct BleDriver {
    /// TCP proxy address for BLE adapter access.
    proxy_addr: String,
    /// Negotiated MTU size.
    mtu: usize,
}

impl BleDriver {
    /// Create a new BLE driver with default settings.
    pub fn new() -> Self {
        Self {
            proxy_addr: DEFAULT_PROXY_ADDR.to_string(),
            mtu: DEFAULT_MTU,
        }
    }

    /// Create a new BLE driver with custom proxy address.
    pub fn with_proxy(addr: impl Into<String>) -> Self {
        Self {
            proxy_addr: addr.into(),
            mtu: DEFAULT_MTU,
        }
    }

    /// Create a new BLE driver with custom MTU.
    pub fn with_mtu(mtu: usize) -> Self {
        Self {
            proxy_addr: DEFAULT_PROXY_ADDR.to_string(),
            mtu: mtu.clamp(DEFAULT_MTU, MAX_MTU),
        }
    }

    /// Validate a BLE MAC address (XX:XX:XX:XX:XX:XX format).
    pub fn validate_mac_address(mac: &str) -> Result<(), TransportError> {
        let parts: Vec<&str> = mac.split(':').collect();
        if parts.len() != 6 {
            return Err(TransportError::Connection(format!(
                "BLE MAC address must have 6 octets separated by ':', got {} parts",
                parts.len()
            )));
        }
        for part in &parts {
            if part.len() != 2 || !part.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(TransportError::Connection(format!(
                    "invalid BLE MAC address octet: '{part}'"
                )));
            }
        }
        Ok(())
    }

    /// Fragment a payload into MTU-sized chunks.
    pub fn fragment_payload(&self, data: &[u8]) -> Vec<Vec<u8>> {
        // Reserve 3 bytes for header: [seq: 1][flags: 1][len: 1]
        let chunk_size = self.mtu.saturating_sub(3).max(1);
        let mut fragments = Vec::new();
        let mut offset = 0;
        let mut seq: u8 = 0;

        while offset < data.len() {
            let end = (offset + chunk_size).min(data.len());
            let is_last = end == data.len();
            let mut fragment = Vec::with_capacity(3 + (end - offset));
            fragment.push(seq);
            fragment.push(if is_last { 0x01 } else { 0x00 }); // flags: 0x01 = last
            fragment.push((end - offset) as u8);
            fragment.extend_from_slice(&data[offset..end]);
            fragments.push(fragment);
            offset = end;
            seq = seq.wrapping_add(1);
        }

        if fragments.is_empty() {
            // Empty payload gets a single empty fragment
            fragments.push(vec![0x00, 0x01, 0x00]);
        }

        fragments
    }

    /// Reassemble fragments into a complete payload.
    pub fn reassemble_fragments(fragments: &[Vec<u8>]) -> Result<Vec<u8>, TransportError> {
        let mut data = Vec::new();
        for (i, fragment) in fragments.iter().enumerate() {
            if fragment.len() < 3 {
                return Err(TransportError::Connection(format!(
                    "fragment {i} too short: {} bytes",
                    fragment.len()
                )));
            }
            let seq = fragment[0];
            if seq as usize != i {
                return Err(TransportError::Connection(format!(
                    "fragment sequence mismatch: expected {i}, got {seq}"
                )));
            }
            let len = fragment[2] as usize;
            if fragment.len() < 3 + len {
                return Err(TransportError::Connection(format!(
                    "fragment {i} payload truncated"
                )));
            }
            data.extend_from_slice(&fragment[3..3 + len]);
        }
        Ok(data)
    }
}

impl Default for BleDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for BleDriver {
    fn name(&self) -> &str {
        "ble"
    }

    fn available(&self) -> bool {
        // BLE is available if proxy address is syntactically valid
        self.proxy_addr.contains(':')
            && self
                .proxy_addr
                .split(':')
                .next_back()
                .and_then(|p| p.parse::<u16>().ok())
                .is_some()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Extract BLE MAC address from config or relay_url
        let mac = config
            .get("mac_address")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("ble://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("BLE MAC address not specified".to_string())
            })?;

        Self::validate_mac_address(&mac)?;

        // Connect to BLE proxy daemon
        let mut stream =
            TcpStream::connect(&self.proxy_addr)
                .await
                .map_err(|e| TransportError::Connection(format!(
                    "failed to connect to BLE proxy at {}: {e}",
                    self.proxy_addr
                )))?;

        // Send GATT connect command: CONNECT <MAC> <SERVICE_UUID>\n
        let cmd = format!("CONNECT {mac} {NUS_SERVICE_UUID}\n");
        stream.write_all(cmd.as_bytes()).await.map_err(|e| {
            TransportError::Connection(format!("failed to send BLE connect command: {e}"))
        })?;

        // Read response
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await.map_err(|e| {
            TransportError::Connection(format!("failed to read BLE connect response: {e}"))
        })?;
        let response = String::from_utf8_lossy(&buf[..n]);
        if !response.starts_with("OK") {
            return Err(TransportError::Connection(format!(
                "BLE connect failed: {response}"
            )));
        }

        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Connect to BLE proxy and start GATT server
        let mut stream =
            TcpStream::connect(&self.proxy_addr)
                .await
                .map_err(|e| TransportError::Connection(format!(
                    "failed to connect to BLE proxy at {}: {e}",
                    self.proxy_addr
                )))?;

        // Register GATT service
        let cmd = format!("SERVE {NUS_SERVICE_UUID}\n");
        stream.write_all(cmd.as_bytes()).await.map_err(|e| {
            TransportError::Connection(format!("failed to register GATT service: {e}"))
        })?;

        Ok(Box::new(BleListener {
            proxy_addr: self.proxy_addr.clone(),
        }))
    }
}

/// Listener that accepts incoming BLE GATT connections.
struct BleListener {
    proxy_addr: String,
}

#[async_trait::async_trait]
impl Listener for BleListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| TransportError::Connection(format!(
                "failed to connect to BLE proxy for accept: {e}",
            )))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = BleDriver::new();
        assert_eq!(driver.name(), "ble");
    }

    #[test]
    fn test_default_mtu() {
        let driver = BleDriver::new();
        assert_eq!(driver.mtu, 23);
    }

    #[test]
    fn test_custom_mtu_clamped() {
        let driver = BleDriver::with_mtu(1024);
        assert_eq!(driver.mtu, MAX_MTU);
        let driver2 = BleDriver::with_mtu(5);
        assert_eq!(driver2.mtu, DEFAULT_MTU);
    }

    #[test]
    fn test_available() {
        let driver = BleDriver::new();
        assert!(driver.available());
        let invalid = BleDriver::with_proxy("bad-addr");
        assert!(!invalid.available());
    }

    #[test]
    fn test_validate_mac_valid() {
        assert!(BleDriver::validate_mac_address("AA:BB:CC:DD:EE:FF").is_ok());
        assert!(BleDriver::validate_mac_address("00:11:22:33:44:55").is_ok());
    }

    #[test]
    fn test_validate_mac_invalid_parts() {
        let err = BleDriver::validate_mac_address("AA:BB:CC").unwrap_err();
        assert!(err.to_string().contains("6 octets"));
    }

    #[test]
    fn test_validate_mac_invalid_hex() {
        let err = BleDriver::validate_mac_address("GG:HH:II:JJ:KK:LL").unwrap_err();
        assert!(err.to_string().contains("invalid BLE MAC address octet"));
    }

    #[test]
    fn test_fragment_small_payload() {
        let driver = BleDriver::with_mtu(23);
        let data = b"Hello, BLE!";
        let fragments = driver.fragment_payload(data);
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0][0], 0); // seq
        assert_eq!(fragments[0][1], 1); // last flag
    }

    #[test]
    fn test_fragment_large_payload() {
        let driver = BleDriver::with_mtu(23);
        let chunk_size = 23 - 3; // 20 bytes per chunk
        let data = vec![0xAB; 50]; // 50 bytes = 3 chunks (20 + 20 + 10)
        let fragments = driver.fragment_payload(&data);
        assert_eq!(fragments.len(), 3);
        assert_eq!(fragments[0][1], 0); // not last
        assert_eq!(fragments[1][1], 0); // not last
        assert_eq!(fragments[2][1], 1); // last
    }

    #[test]
    fn test_fragment_empty_payload() {
        let driver = BleDriver::new();
        let fragments = driver.fragment_payload(b"");
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0], vec![0x00, 0x01, 0x00]);
    }

    #[test]
    fn test_reassemble_roundtrip() {
        let driver = BleDriver::with_mtu(23);
        let data = b"Hello, this is a BLE fragmentation test payload!";
        let fragments = driver.fragment_payload(data);
        let reassembled = BleDriver::reassemble_fragments(&fragments).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_reassemble_sequence_error() {
        let fragments = vec![
            vec![0x00, 0x00, 0x01, 0xAA],
            vec![0x05, 0x01, 0x01, 0xBB], // wrong seq (should be 1)
        ];
        let err = BleDriver::reassemble_fragments(&fragments).unwrap_err();
        assert!(err.to_string().contains("sequence mismatch"));
    }

    #[tokio::test]
    async fn test_dial_missing_mac() {
        let driver = BleDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("MAC address not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_mac() {
        let driver = BleDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("ble://invalid".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("6 octets")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_proxy_unavailable() {
        let driver = BleDriver::with_proxy("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("mac_address".to_string(), "AA:BB:CC:DD:EE:FF".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to BLE proxy")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_proxy_unavailable() {
        let driver = BleDriver::with_proxy("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to BLE proxy")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_service_uuid_format() {
        // UUID v4 format: 8-4-4-4-12
        let parts: Vec<&str> = NUS_SERVICE_UUID.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }
}
