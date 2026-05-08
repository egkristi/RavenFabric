//! LoRa/Meshtastic transport driver.
//!
//! Communicates over LoRa radio via Meshtastic-compatible devices.
//! Uses the Meshtastic serial/TCP protocol to send and receive mesh packets.
//!
//! # Requirements
//!
//! - Meshtastic-compatible device (T-Beam, Heltec, RAK, etc.)
//! - Device connected via serial USB or TCP (default: serial /dev/ttyUSB0 or TCP 4403)
//!
//! # Protocol
//!
//! - Meshtastic protobuf-over-serial protocol
//! - Frame: [0x94 0xC3][length: 2 LE][protobuf payload]
//! - Channel-based routing with PSK encryption
//! - Max payload: 237 bytes per packet (after Meshtastic overhead)

use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Meshtastic serial magic bytes.
const MESHTASTIC_MAGIC: [u8; 2] = [0x94, 0xC3];

/// Default TCP interface address for Meshtastic device.
const DEFAULT_TCP_ADDR: &str = "127.0.0.1:4403";

/// Maximum packet payload (after Meshtastic headers).
const MAX_PAYLOAD_SIZE: usize = 237;

/// Default channel index.
const DEFAULT_CHANNEL: u8 = 0;

/// LoRa spreading factors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpreadingFactor {
    SF7,
    SF8,
    SF9,
    SF10,
    SF11,
    SF12,
}

impl SpreadingFactor {
    /// Approximate air-time per byte at this spreading factor (milliseconds).
    pub fn ms_per_byte(&self) -> f32 {
        match self {
            SpreadingFactor::SF7 => 0.5,
            SpreadingFactor::SF8 => 1.0,
            SpreadingFactor::SF9 => 2.0,
            SpreadingFactor::SF10 => 4.0,
            SpreadingFactor::SF11 => 8.0,
            SpreadingFactor::SF12 => 16.0,
        }
    }

    /// Maximum range estimate (km, line of sight).
    pub fn range_km(&self) -> u16 {
        match self {
            SpreadingFactor::SF7 => 2,
            SpreadingFactor::SF8 => 4,
            SpreadingFactor::SF9 => 6,
            SpreadingFactor::SF10 => 10,
            SpreadingFactor::SF11 => 15,
            SpreadingFactor::SF12 => 20,
        }
    }
}

/// Transport driver that communicates over LoRa via Meshtastic devices.
#[allow(dead_code)]
pub struct LoraDriver {
    /// TCP address of the Meshtastic device interface.
    device_addr: String,
    /// LoRa channel index.
    channel: u8,
}

impl LoraDriver {
    /// Create a new LoRa driver with default settings.
    pub fn new() -> Self {
        Self {
            device_addr: DEFAULT_TCP_ADDR.to_string(),
            channel: DEFAULT_CHANNEL,
        }
    }

    /// Create with custom device TCP address.
    pub fn with_device(addr: impl Into<String>) -> Self {
        Self {
            device_addr: addr.into(),
            channel: DEFAULT_CHANNEL,
        }
    }

    /// Create with custom channel.
    pub fn with_channel(channel: u8) -> Self {
        Self {
            device_addr: DEFAULT_TCP_ADDR.to_string(),
            channel,
        }
    }

    /// Encode a Meshtastic serial frame.
    pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, TransportError> {
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(TransportError::Connection(format!(
                "LoRa payload too large: {} > {} bytes",
                payload.len(),
                MAX_PAYLOAD_SIZE
            )));
        }
        let len = payload.len() as u16;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&MESHTASTIC_MAGIC);
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(payload);
        Ok(frame)
    }

    /// Decode a Meshtastic serial frame, returning the payload.
    pub fn decode_frame(data: &[u8]) -> Result<Vec<u8>, TransportError> {
        if data.len() < 4 {
            return Err(TransportError::Connection(
                "LoRa frame too short: need at least 4 bytes".to_string(),
            ));
        }
        if data[0] != MESHTASTIC_MAGIC[0] || data[1] != MESHTASTIC_MAGIC[1] {
            return Err(TransportError::Connection(format!(
                "invalid Meshtastic magic: expected {:02X}{:02X}, got {:02X}{:02X}",
                MESHTASTIC_MAGIC[0], MESHTASTIC_MAGIC[1], data[0], data[1]
            )));
        }
        let len = u16::from_le_bytes([data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(TransportError::Connection(format!(
                "LoRa frame truncated: expected {} payload bytes, got {}",
                len,
                data.len() - 4
            )));
        }
        Ok(data[4..4 + len].to_vec())
    }

    /// Validate a Meshtastic node ID (8 hex chars = 4 bytes).
    pub fn validate_node_id(node_id: &str) -> Result<(), TransportError> {
        let cleaned = node_id.strip_prefix('!').unwrap_or(node_id);
        if cleaned.len() != 8 {
            return Err(TransportError::Connection(format!(
                "Meshtastic node ID must be 8 hex characters (with optional '!' prefix), got {}",
                node_id.len()
            )));
        }
        if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TransportError::Connection(
                "Meshtastic node ID must contain only hex characters".to_string(),
            ));
        }
        Ok(())
    }

    /// Estimate air-time for a payload at given spreading factor.
    pub fn estimate_airtime_ms(payload_len: usize, sf: SpreadingFactor) -> f32 {
        // Preamble + header + payload
        let preamble_ms = 12.0 * sf.ms_per_byte();
        let header_bytes = 13; // Meshtastic header overhead
        let total_bytes = header_bytes + payload_len;
        preamble_ms + (total_bytes as f32 * sf.ms_per_byte())
    }
}

impl Default for LoraDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for LoraDriver {
    fn name(&self) -> &str {
        "lora"
    }

    fn available(&self) -> bool {
        !self.device_addr.is_empty()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Extract node ID
        let node_id = config
            .get("node_id")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("lora://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("LoRa node ID not specified".to_string())
            })?;

        Self::validate_node_id(&node_id)?;

        // Connect to Meshtastic device TCP interface
        let stream = TcpStream::connect(&self.device_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to Meshtastic device at {}: {e}",
                    self.device_addr
                ))
            })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = TcpStream::connect(&self.device_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to Meshtastic device at {} for listen: {e}",
                    self.device_addr
                ))
            })?;

        Ok(Box::new(LoraListener {
            device_addr: self.device_addr.clone(),
        }))
    }
}

/// Listener for incoming LoRa mesh packets.
struct LoraListener {
    device_addr: String,
}

#[async_trait::async_trait]
impl Listener for LoraListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.device_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!("LoRa accept failed: {e}"))
            })?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = LoraDriver::new();
        assert_eq!(driver.name(), "lora");
    }

    #[test]
    fn test_default_settings() {
        let driver = LoraDriver::new();
        assert_eq!(driver.device_addr, "127.0.0.1:4403");
        assert_eq!(driver.channel, 0);
    }

    #[test]
    fn test_available() {
        let driver = LoraDriver::new();
        assert!(driver.available());
        let empty = LoraDriver::with_device("");
        assert!(!empty.available());
    }

    #[test]
    fn test_encode_frame() {
        let payload = b"hello";
        let frame = LoraDriver::encode_frame(payload).unwrap();
        assert_eq!(&frame[0..2], &MESHTASTIC_MAGIC);
        assert_eq!(frame[2], 5); // length low byte
        assert_eq!(frame[3], 0); // length high byte
        assert_eq!(&frame[4..], b"hello");
    }

    #[test]
    fn test_encode_frame_too_large() {
        let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        let result = LoraDriver::encode_frame(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_frame_valid() {
        let mut data = vec![0x94, 0xC3, 0x03, 0x00]; // magic + length 3
        data.extend_from_slice(b"abc");
        let payload = LoraDriver::decode_frame(&data).unwrap();
        assert_eq!(payload, b"abc");
    }

    #[test]
    fn test_decode_frame_invalid_magic() {
        let data = vec![0x00, 0x00, 0x01, 0x00, 0xAA];
        let result = LoraDriver::decode_frame(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid Meshtastic magic"));
    }

    #[test]
    fn test_decode_frame_truncated() {
        let data = vec![0x94, 0xC3, 0x05, 0x00, 0xAA]; // claims 5 bytes but only 1
        let result = LoraDriver::decode_frame(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("truncated"));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let payload = b"mesh packet data";
        let frame = LoraDriver::encode_frame(payload).unwrap();
        let decoded = LoraDriver::decode_frame(&frame).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_validate_node_id_valid() {
        assert!(LoraDriver::validate_node_id("!abcd1234").is_ok());
        assert!(LoraDriver::validate_node_id("DEADBEEF").is_ok());
    }

    #[test]
    fn test_validate_node_id_invalid_length() {
        let err = LoraDriver::validate_node_id("abc").unwrap_err();
        assert!(err.to_string().contains("8 hex characters"));
    }

    #[test]
    fn test_validate_node_id_invalid_chars() {
        let err = LoraDriver::validate_node_id("!ghijklmn").unwrap_err();
        assert!(err.to_string().contains("only hex characters"));
    }

    #[test]
    fn test_spreading_factor_range() {
        assert_eq!(SpreadingFactor::SF7.range_km(), 2);
        assert_eq!(SpreadingFactor::SF12.range_km(), 20);
    }

    #[test]
    fn test_estimate_airtime() {
        let time_sf7 = LoraDriver::estimate_airtime_ms(100, SpreadingFactor::SF7);
        let time_sf12 = LoraDriver::estimate_airtime_ms(100, SpreadingFactor::SF12);
        assert!(time_sf12 > time_sf7); // Higher SF = longer airtime
    }

    #[tokio::test]
    async fn test_dial_missing_node_id() {
        let driver = LoraDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("node ID not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_node_id() {
        let driver = LoraDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("lora://xyz".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("8 hex characters")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_device_unavailable() {
        let driver = LoraDriver::with_device("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("node_id".to_string(), "!abcd1234".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to Meshtastic")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_device_unavailable() {
        let driver = LoraDriver::with_device("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to Meshtastic")),
            Ok(_) => panic!("expected error"),
        }
    }
}
