//! Reticulum Network Stack transport driver.
//!
//! Connects to Reticulum destinations through the local Reticulum Transport
//! Node (RTN). Reticulum provides cryptographic addressing and multi-hop
//! mesh routing with automatic path discovery.
//!
//! # Requirements
//!
//! - Reticulum daemon (`rnsd`) running locally with TCP interface enabled
//! - Shared instance accessible (default: `127.0.0.1:37428`)
//!
//! # Protocol
//!
//! Uses Reticulum's TCP client interface to send/receive packets:
//! - Frame format: `[length: 2 bytes BE][payload]`
//! - Announce: broadcasts identity hash for discovery
//! - Link: establishes encrypted bidirectional channel to destination hash

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default Reticulum shared instance TCP interface address.
const DEFAULT_INSTANCE_ADDR: &str = "127.0.0.1:37428";

/// Reticulum destination hash length in hex characters (16 bytes = 32 hex).
const DEST_HASH_HEX_LEN: usize = 32;

/// Frame header size (2-byte length prefix).
const FRAME_HEADER_SIZE: usize = 2;

/// Maximum frame payload size.
const MAX_FRAME_SIZE: usize = 500;

/// Transport driver that routes connections through the Reticulum Network Stack.
pub struct ReticulumDriver {
    /// Reticulum shared instance TCP address.
    instance_addr: String,
}

impl ReticulumDriver {
    /// Create a new Reticulum driver using the default instance address.
    pub fn new() -> Self {
        Self {
            instance_addr: DEFAULT_INSTANCE_ADDR.to_string(),
        }
    }

    /// Create a new Reticulum driver with a custom instance address.
    pub fn with_instance_addr(addr: impl Into<String>) -> Self {
        Self {
            instance_addr: addr.into(),
        }
    }

    /// Validate a Reticulum destination hash (32 hex characters).
    pub fn validate_destination_hash(hash: &str) -> Result<(), TransportError> {
        if hash.len() != DEST_HASH_HEX_LEN {
            return Err(TransportError::Connection(format!(
                "Reticulum destination hash must be {} hex characters, got {}",
                DEST_HASH_HEX_LEN,
                hash.len()
            )));
        }
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TransportError::Connection(
                "Reticulum destination hash must contain only hex characters".to_string(),
            ));
        }
        Ok(())
    }

    /// Encode a frame with 2-byte length prefix.
    fn encode_frame(payload: &[u8]) -> Vec<u8> {
        let len = payload.len() as u16;
        let mut frame = Vec::with_capacity(FRAME_HEADER_SIZE + payload.len());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    /// Decode a frame length from a 2-byte header.
    fn decode_frame_length(header: &[u8; 2]) -> usize {
        u16::from_be_bytes(*header) as usize
    }

    /// Send an announce packet to the Reticulum network.
    async fn send_announce(
        stream: &mut TcpStream,
        identity_hash: &[u8],
    ) -> Result<(), TransportError> {
        // Announce frame: [0x01 (announce type)][identity_hash]
        let mut payload = Vec::with_capacity(1 + identity_hash.len());
        payload.push(0x01); // Announce packet type
        payload.extend_from_slice(identity_hash);
        let frame = Self::encode_frame(&payload);
        stream
            .write_all(&frame)
            .await
            .map_err(|e| TransportError::Connection(format!("failed to send announce: {e}")))?;
        stream
            .flush()
            .await
            .map_err(|e| TransportError::Connection(format!("failed to flush announce: {e}")))?;
        Ok(())
    }

    /// Send a link request to a destination hash.
    async fn send_link_request(
        stream: &mut TcpStream,
        dest_hash: &[u8],
    ) -> Result<(), TransportError> {
        // Link request frame: [0x02 (link type)][dest_hash]
        let mut payload = Vec::with_capacity(1 + dest_hash.len());
        payload.push(0x02); // Link request packet type
        payload.extend_from_slice(dest_hash);
        let frame = Self::encode_frame(&payload);
        stream
            .write_all(&frame)
            .await
            .map_err(|e| TransportError::Connection(format!("failed to send link request: {e}")))?;
        stream.flush().await.map_err(|e| {
            TransportError::Connection(format!("failed to flush link request: {e}"))
        })?;
        Ok(())
    }

    /// Read a response frame from the Reticulum instance.
    async fn read_response(stream: &mut TcpStream) -> Result<Vec<u8>, TransportError> {
        let mut header = [0u8; 2];
        stream
            .read_exact(&mut header)
            .await
            .map_err(|e| TransportError::Connection(format!("failed to read frame header: {e}")))?;
        let len = Self::decode_frame_length(&header);
        if len > MAX_FRAME_SIZE {
            return Err(TransportError::Connection(format!(
                "frame too large: {len} > {MAX_FRAME_SIZE}"
            )));
        }
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.map_err(|e| {
            TransportError::Connection(format!("failed to read frame payload: {e}"))
        })?;
        Ok(payload)
    }
}

impl Default for ReticulumDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for ReticulumDriver {
    fn name(&self) -> &str {
        "reticulum"
    }

    fn available(&self) -> bool {
        // Check if the instance address is syntactically valid
        self.instance_addr.contains(':')
            && self
                .instance_addr
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
        // Extract destination hash from relay_url or config
        let dest_hash = config
            .get("destination_hash")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .map(|u| u.strip_prefix("reticulum://").unwrap_or(u).to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("destination_hash not specified".to_string())
            })?;

        Self::validate_destination_hash(&dest_hash)?;

        // Connect to local Reticulum shared instance
        let mut stream = TcpStream::connect(&self.instance_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Reticulum instance at {}: {e}",
                self.instance_addr
            ))
        })?;

        // Decode destination hash from hex
        let dest_bytes = hex_decode(&dest_hash).map_err(|e| {
            TransportError::Connection(format!("invalid destination hash hex: {e}"))
        })?;

        // Send link request to establish connection
        Self::send_link_request(&mut stream, &dest_bytes).await?;

        // Read link established response
        let response = Self::read_response(&mut stream).await?;
        if response.first() != Some(&0x03) {
            return Err(TransportError::Connection(format!(
                "unexpected response type: expected 0x03 (link established), got {:?}",
                response.first()
            )));
        }

        Ok(Box::new(stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Connect to local Reticulum shared instance for listening
        let mut stream = TcpStream::connect(&self.instance_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Reticulum instance at {}: {e}",
                self.instance_addr
            ))
        })?;

        // Generate identity hash from addr (used as announce identity)
        let identity_hash = simple_hash(addr.as_bytes());

        // Send announce to make ourselves discoverable
        Self::send_announce(&mut stream, &identity_hash).await?;

        Ok(Box::new(ReticulumListener {
            instance_addr: self.instance_addr.clone(),
        }))
    }
}

/// Listener that accepts incoming Reticulum link requests.
struct ReticulumListener {
    instance_addr: String,
}

#[async_trait::async_trait]
impl Listener for ReticulumListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Connect to instance and wait for incoming link
        let mut stream = TcpStream::connect(&self.instance_addr).await.map_err(|e| {
            TransportError::Connection(format!("failed to connect to Reticulum instance: {e}",))
        })?;

        // Read incoming link request
        let mut header = [0u8; 2];
        stream.read_exact(&mut header).await.map_err(|e| {
            TransportError::Connection(format!("failed to read incoming frame: {e}"))
        })?;
        let len = ReticulumDriver::decode_frame_length(&header);
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.map_err(|e| {
            TransportError::Connection(format!("failed to read incoming payload: {e}"))
        })?;

        // Accept the link
        let accept_frame = ReticulumDriver::encode_frame(&[0x04]); // Link accept
        stream
            .write_all(&accept_frame)
            .await
            .map_err(|e| TransportError::Connection(format!("failed to send link accept: {e}")))?;

        Ok(Box::new(stream))
    }
}

/// Simple hex decoder (no external dependency).
fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("odd-length hex string".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| format!("invalid hex at position {i}: {e}"))
        })
        .collect()
}

/// Simple hash function for identity derivation.
fn simple_hash(data: &[u8]) -> Vec<u8> {
    // FNV-1a inspired hash producing 16 bytes
    let mut hash = [0u8; 16];
    let mut h: u64 = 0xcbf29ce484222325;
    for &byte in data {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    hash[0..8].copy_from_slice(&h.to_le_bytes());
    // Second pass with different seed
    let mut h2: u64 = 0x6c62272e07bb0142;
    for &byte in data {
        h2 ^= byte as u64;
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    hash[8..16].copy_from_slice(&h2.to_le_bytes());
    hash.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = ReticulumDriver::new();
        assert_eq!(driver.name(), "reticulum");
    }

    #[test]
    fn test_default_instance_addr() {
        let driver = ReticulumDriver::new();
        assert_eq!(driver.instance_addr, "127.0.0.1:37428");
    }

    #[test]
    fn test_custom_instance_addr() {
        let driver = ReticulumDriver::with_instance_addr("192.168.1.1:37428");
        assert_eq!(driver.instance_addr, "192.168.1.1:37428");
    }

    #[test]
    fn test_available_valid_addr() {
        let driver = ReticulumDriver::new();
        assert!(driver.available());
    }

    #[test]
    fn test_available_invalid_addr() {
        let driver = ReticulumDriver::with_instance_addr("not-a-valid-addr");
        assert!(!driver.available());
    }

    #[test]
    fn test_validate_destination_hash_valid() {
        let hash = "a".repeat(32);
        assert!(ReticulumDriver::validate_destination_hash(&hash).is_ok());
    }

    #[test]
    fn test_validate_destination_hash_too_short() {
        let hash = "abcdef";
        let err = ReticulumDriver::validate_destination_hash(&hash).unwrap_err();
        assert!(err.to_string().contains("32 hex characters"));
    }

    #[test]
    fn test_validate_destination_hash_invalid_chars() {
        let hash = "g".repeat(32);
        let err = ReticulumDriver::validate_destination_hash(&hash).unwrap_err();
        assert!(err.to_string().contains("only hex characters"));
    }

    #[test]
    fn test_encode_frame() {
        let payload = b"hello";
        let frame = ReticulumDriver::encode_frame(payload);
        assert_eq!(frame.len(), 2 + 5);
        assert_eq!(&frame[0..2], &[0x00, 0x05]);
        assert_eq!(&frame[2..], b"hello");
    }

    #[test]
    fn test_decode_frame_length() {
        assert_eq!(ReticulumDriver::decode_frame_length(&[0x00, 0x05]), 5);
        assert_eq!(ReticulumDriver::decode_frame_length(&[0x01, 0xF4]), 500);
        assert_eq!(ReticulumDriver::decode_frame_length(&[0x00, 0x00]), 0);
    }

    #[test]
    fn test_hex_decode_valid() {
        let result = hex_decode("48656c6c6f").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn test_hex_decode_empty() {
        let result = hex_decode("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert!(hex_decode("zz").is_err());
        assert!(hex_decode("abc").is_err()); // odd length
    }

    #[test]
    fn test_simple_hash_deterministic() {
        let h1 = simple_hash(b"test");
        let h2 = simple_hash(b"test");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_simple_hash_different_inputs() {
        let h1 = simple_hash(b"hello");
        let h2 = simple_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn test_dial_missing_destination() {
        let driver = ReticulumDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("destination_hash not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_hash() {
        let driver = ReticulumDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("reticulum://short".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("32 hex characters")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_instance_unavailable() {
        let driver = ReticulumDriver::with_instance_addr("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("destination_hash".to_string(), "a".repeat(32));
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to Reticulum instance")
            ),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_instance_unavailable() {
        let driver = ReticulumDriver::with_instance_addr("127.0.0.1:19999");
        let result = driver.listen("test-identity").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_strip_reticulum_prefix() {
        let url = "reticulum://abcdef1234567890abcdef1234567890";
        let stripped = url.strip_prefix("reticulum://").unwrap_or(url);
        assert_eq!(stripped.len(), 32);
    }
}
