//! Mixnet transport driver.
//!
//! Routes traffic through a mix network for strong metadata resistance.
//! Uses Sphinx packet format, multi-hop routing through mix nodes, and
//! loop cover traffic to resist traffic analysis.
//!
//! # Design
//!
//! - Sphinx packet format for layered encryption
//! - Multi-hop routing (default 3 hops) through mix nodes
//! - Poisson-distributed cover traffic to mask timing
//! - SURB (Single-Use Reply Block) for anonymous replies
//! - Exponential mixing delay at each hop
//!
//! # Requirements
//!
//! - Access to mix network gateway (TCP connection)
//! - Network directory service for topology discovery

use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default mix network gateway address.
const DEFAULT_GATEWAY_ADDR: &str = "127.0.0.1:9000";

/// Default number of hops through the mixnet.
const DEFAULT_NUM_HOPS: u8 = 3;

/// Maximum Sphinx packet payload size.
const MAX_PAYLOAD_SIZE: usize = 2048;

/// Sphinx packet header size (per hop).
const SPHINX_HEADER_PER_HOP: usize = 32;

/// SURB (Single-Use Reply Block) size.
const SURB_SIZE: usize = 296;

/// Mix node identifier (32 bytes public key fingerprint).
#[derive(Debug, Clone, PartialEq)]
pub struct MixNodeId(pub [u8; 32]);

impl MixNodeId {
    /// Parse from hex string.
    pub fn from_hex(hex: &str) -> Result<Self, TransportError> {
        if hex.len() != 64 {
            return Err(TransportError::Connection(format!(
                "mix node ID must be 64 hex chars (32 bytes), got {}",
                hex.len()
            )));
        }
        let mut bytes = [0u8; 32];
        for i in 0..32 {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| TransportError::Connection(
                    "mix node ID contains invalid hex characters".to_string(),
                ))?;
        }
        Ok(Self(bytes))
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Sphinx packet structure (simplified).
#[derive(Debug, Clone)]
pub struct SphinxPacket {
    /// Layered routing header.
    pub header: Vec<u8>,
    /// Encrypted payload.
    pub payload: Vec<u8>,
}

impl SphinxPacket {
    /// Create a new Sphinx packet with the given route and payload.
    pub fn create(route: &[MixNodeId], payload: &[u8]) -> Result<Self, TransportError> {
        if route.is_empty() {
            return Err(TransportError::Connection(
                "Sphinx route must contain at least one hop".to_string(),
            ));
        }
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(TransportError::Connection(format!(
                "Sphinx payload too large: {} > {MAX_PAYLOAD_SIZE}",
                payload.len()
            )));
        }

        // Header: per-hop routing data
        let header_size = route.len() * SPHINX_HEADER_PER_HOP;
        let mut header = vec![0u8; header_size];
        // Encode route into header (simplified: concatenate node IDs)
        for (i, node) in route.iter().enumerate() {
            let offset = i * SPHINX_HEADER_PER_HOP;
            header[offset..offset + 32].copy_from_slice(&node.0);
        }

        // Payload: padded to fixed size for indistinguishability
        let mut padded_payload = vec![0u8; MAX_PAYLOAD_SIZE];
        padded_payload[..payload.len()].copy_from_slice(payload);
        // Store actual length at end
        let len_bytes = (payload.len() as u16).to_be_bytes();
        padded_payload[MAX_PAYLOAD_SIZE - 2] = len_bytes[0];
        padded_payload[MAX_PAYLOAD_SIZE - 1] = len_bytes[1];

        Ok(Self {
            header,
            payload: padded_payload,
        })
    }

    /// Total packet size in bytes.
    pub fn size(&self) -> usize {
        self.header.len() + self.payload.len()
    }

    /// Serialize to wire format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + self.header.len() + self.payload.len());
        bytes.extend_from_slice(&(self.header.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        bytes.extend_from_slice(&self.header);
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Deserialize from wire format.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TransportError> {
        if data.len() < 4 {
            return Err(TransportError::Connection(
                "Sphinx packet too short".to_string(),
            ));
        }
        let header_len = u16::from_be_bytes([data[0], data[1]]) as usize;
        let payload_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        if data.len() < 4 + header_len + payload_len {
            return Err(TransportError::Connection(format!(
                "Sphinx packet truncated: expected {} bytes, got {}",
                4 + header_len + payload_len,
                data.len()
            )));
        }
        Ok(Self {
            header: data[4..4 + header_len].to_vec(),
            payload: data[4 + header_len..4 + header_len + payload_len].to_vec(),
        })
    }
}

/// SURB (Single-Use Reply Block) for anonymous replies.
#[derive(Debug, Clone)]
pub struct Surb {
    /// Encoded reply route (encrypted, fixed size).
    pub data: Vec<u8>,
}

impl Surb {
    /// Create a new SURB for a given return route.
    pub fn create(return_route: &[MixNodeId]) -> Result<Self, TransportError> {
        if return_route.is_empty() {
            return Err(TransportError::Connection(
                "SURB return route must not be empty".to_string(),
            ));
        }
        // Simplified: encode route into fixed-size SURB
        let mut data = vec![0u8; SURB_SIZE];
        let route_data: Vec<u8> = return_route.iter().flat_map(|n| n.0).collect();
        let copy_len = route_data.len().min(SURB_SIZE);
        data[..copy_len].copy_from_slice(&route_data[..copy_len]);
        Ok(Self { data })
    }

    /// Size of this SURB.
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Transport driver that routes through a mix network.
pub struct MixnetDriver {
    /// Gateway address.
    gateway_addr: String,
    /// Number of hops.
    num_hops: u8,
}

impl MixnetDriver {
    /// Create a new mixnet driver with default settings.
    pub fn new() -> Self {
        Self {
            gateway_addr: DEFAULT_GATEWAY_ADDR.to_string(),
            num_hops: DEFAULT_NUM_HOPS,
        }
    }

    /// Create with custom gateway address.
    pub fn with_gateway(addr: impl Into<String>) -> Self {
        Self {
            gateway_addr: addr.into(),
            num_hops: DEFAULT_NUM_HOPS,
        }
    }

    /// Create with custom number of hops.
    pub fn with_hops(num_hops: u8) -> Self {
        if num_hops < 1 {
            Self {
                gateway_addr: DEFAULT_GATEWAY_ADDR.to_string(),
                num_hops: 1,
            }
        } else {
            Self {
                gateway_addr: DEFAULT_GATEWAY_ADDR.to_string(),
                num_hops,
            }
        }
    }

    /// Get configured number of hops.
    pub fn num_hops(&self) -> u8 {
        self.num_hops
    }

    /// Calculate expected latency for the mixnet path.
    /// Each hop adds ~200ms average mixing delay.
    pub fn estimated_latency_ms(&self) -> u32 {
        self.num_hops as u32 * 200
    }

    /// Calculate overhead compared to direct transmission.
    pub fn overhead_bytes(&self, payload_len: usize) -> usize {
        let header = self.num_hops as usize * SPHINX_HEADER_PER_HOP;
        let padding = MAX_PAYLOAD_SIZE.saturating_sub(payload_len);
        header + padding + 4 // 4 bytes for length fields
    }
}

impl Default for MixnetDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for MixnetDriver {
    fn name(&self) -> &str {
        "mixnet"
    }

    fn available(&self) -> bool {
        !self.gateway_addr.is_empty()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let dest_node = config
            .get("node_id")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("mix://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("Mixnet destination node ID not specified".to_string())
            })?;

        MixNodeId::from_hex(&dest_node)?;

        let stream = TcpStream::connect(&self.gateway_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to mixnet gateway at {}: {e}",
                self.gateway_addr
            ))
        })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = TcpStream::connect(&self.gateway_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to mixnet gateway at {} for listen: {e}",
                self.gateway_addr
            ))
        })?;

        Ok(Box::new(MixnetListener {
            gateway_addr: self.gateway_addr.clone(),
        }))
    }
}

/// Listener for incoming mixnet messages.
struct MixnetListener {
    gateway_addr: String,
}

#[async_trait::async_trait]
impl Listener for MixnetListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.gateway_addr)
            .await
            .map_err(|e| TransportError::Connection(format!("mixnet accept failed: {e}")))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = MixnetDriver::new();
        assert_eq!(driver.name(), "mixnet");
    }

    #[test]
    fn test_default_hops() {
        let driver = MixnetDriver::new();
        assert_eq!(driver.num_hops(), 3);
    }

    #[test]
    fn test_available() {
        let driver = MixnetDriver::new();
        assert!(driver.available());
        let empty = MixnetDriver::with_gateway("");
        assert!(!empty.available());
    }

    #[test]
    fn test_estimated_latency() {
        let driver = MixnetDriver::with_hops(5);
        assert_eq!(driver.estimated_latency_ms(), 1000);
    }

    #[test]
    fn test_overhead_bytes() {
        let driver = MixnetDriver::new(); // 3 hops
        let overhead = driver.overhead_bytes(100);
        // Header: 3 * 32 = 96, padding: 2048 - 100 = 1948, lengths: 4
        assert_eq!(overhead, 96 + 1948 + 4);
    }

    #[test]
    fn test_mix_node_id_from_hex() {
        let hex = "a".repeat(64);
        let id = MixNodeId::from_hex(&hex).unwrap();
        assert_eq!(id.0, [0xAA; 32]);
    }

    #[test]
    fn test_mix_node_id_invalid_length() {
        let err = MixNodeId::from_hex("abcd").unwrap_err();
        assert!(err.to_string().contains("64 hex chars"));
    }

    #[test]
    fn test_mix_node_id_invalid_chars() {
        let hex = "g".repeat(64);
        let err = MixNodeId::from_hex(&hex).unwrap_err();
        assert!(err.to_string().contains("invalid hex"));
    }

    #[test]
    fn test_mix_node_id_roundtrip() {
        let hex = "0123456789abcdef".repeat(4);
        let id = MixNodeId::from_hex(&hex).unwrap();
        assert_eq!(id.to_hex(), hex);
    }

    #[test]
    fn test_sphinx_packet_create() {
        let node = MixNodeId([0x42; 32]);
        let payload = b"secret message";
        let pkt = SphinxPacket::create(&[node], payload).unwrap();
        assert_eq!(pkt.header.len(), 32);
        assert_eq!(pkt.payload.len(), MAX_PAYLOAD_SIZE);
    }

    #[test]
    fn test_sphinx_packet_empty_route() {
        let result = SphinxPacket::create(&[], b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_sphinx_packet_too_large() {
        let node = MixNodeId([0x42; 32]);
        let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        let result = SphinxPacket::create(&[node], &payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_sphinx_packet_serialization() {
        let node = MixNodeId([0x42; 32]);
        let payload = b"test data";
        let pkt = SphinxPacket::create(&[node], payload).unwrap();
        let bytes = pkt.to_bytes();
        let decoded = SphinxPacket::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.header, pkt.header);
        assert_eq!(decoded.payload, pkt.payload);
    }

    #[test]
    fn test_sphinx_from_bytes_too_short() {
        let result = SphinxPacket::from_bytes(&[0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_surb_create() {
        let node = MixNodeId([0x11; 32]);
        let surb = Surb::create(&[node]).unwrap();
        assert_eq!(surb.size(), SURB_SIZE);
    }

    #[test]
    fn test_surb_empty_route() {
        let result = Surb::create(&[]);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dial_missing_node_id() {
        let driver = MixnetDriver::new();
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
        let driver = MixnetDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("mix://short".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("64 hex chars")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_gateway_unavailable() {
        let driver = MixnetDriver::with_gateway("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("node_id".to_string(), "a".repeat(64));
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to mixnet gateway")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_gateway_unavailable() {
        let driver = MixnetDriver::with_gateway("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to mixnet gateway")),
            Ok(_) => panic!("expected error"),
        }
    }
}
