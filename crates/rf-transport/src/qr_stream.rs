//! QR-stream visual channel transport driver.
//!
//! Encodes data as sequences of QR codes displayed on screen, captured by
//! camera on the receiving end. Provides one-way or bidirectional communication
//! between air-gapped devices using visual channel.
//!
//! # Use Cases
//!
//! - Air-gapped key exchange (bootstrap enrollment)
//! - One-way command injection to isolated systems
//! - Bidirectional if both sides have screen + camera
//!
//! # Protocol
//!
//! - Each QR code encodes: [seq: 2 bytes][total: 2 bytes][payload chunk]
//! - Display rate: configurable (default: 5 QR/sec)
//! - Error correction: Level H (30% recovery)
//! - Max payload per QR: ~1200 bytes (Version 25, binary mode, ECC H)

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Maximum bytes per QR code (Version 25, binary, ECC H).
const MAX_QR_PAYLOAD: usize = 1200;

/// Default QR codes per second display rate.
const DEFAULT_DISPLAY_RATE: u8 = 5;

/// QR Error Correction Level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EccLevel {
    /// ~7% recovery.
    Low,
    /// ~15% recovery.
    Medium,
    /// ~25% recovery.
    Quartile,
    /// ~30% recovery.
    High,
}

impl EccLevel {
    /// Maximum binary bytes for Version 25 at this ECC level.
    pub fn max_bytes_v25(&self) -> usize {
        match self {
            EccLevel::Low => 2520,
            EccLevel::Medium => 1966,
            EccLevel::Quartile => 1394,
            EccLevel::High => 1200,
        }
    }
}

/// A single QR frame in the stream.
#[derive(Debug, Clone)]
pub struct QrFrame {
    /// Sequence number (0-based).
    pub seq: u16,
    /// Total number of frames.
    pub total: u16,
    /// Payload bytes for this frame.
    pub payload: Vec<u8>,
}

impl QrFrame {
    /// Serialize frame to bytes for QR encoding.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.payload.len());
        buf.extend_from_slice(&self.seq.to_be_bytes());
        buf.extend_from_slice(&self.total.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialize frame from QR-decoded bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TransportError> {
        if data.len() < 4 {
            return Err(TransportError::Connection(
                "QR frame too short: need at least 4 bytes for header".to_string(),
            ));
        }
        let seq = u16::from_be_bytes([data[0], data[1]]);
        let total = u16::from_be_bytes([data[2], data[3]]);
        let payload = data[4..].to_vec();
        Ok(Self {
            seq,
            total,
            payload,
        })
    }
}

/// Transport driver that uses QR code sequences for visual data transmission.
#[allow(dead_code)]
pub struct QrStreamDriver {
    /// Display rate (QR codes per second).
    display_rate: u8,
    /// Error correction level.
    ecc_level: EccLevel,
    /// Maximum payload bytes per QR code.
    chunk_size: usize,
    /// Proxy address for screen/camera bridge.
    proxy_addr: String,
}

impl QrStreamDriver {
    /// Create a new QR stream driver with default settings.
    pub fn new() -> Self {
        Self {
            display_rate: DEFAULT_DISPLAY_RATE,
            ecc_level: EccLevel::High,
            chunk_size: MAX_QR_PAYLOAD - 4, // minus header
            proxy_addr: "127.0.0.1:7730".to_string(),
        }
    }

    /// Create with custom display rate.
    pub fn with_rate(rate: u8) -> Self {
        Self {
            display_rate: rate.clamp(1, 30),
            ..Self::new()
        }
    }

    /// Create with custom proxy address.
    pub fn with_proxy(addr: impl Into<String>) -> Self {
        Self {
            proxy_addr: addr.into(),
            ..Self::new()
        }
    }

    /// Fragment data into QR frames.
    pub fn fragment(&self, data: &[u8]) -> Vec<QrFrame> {
        if data.is_empty() {
            return vec![QrFrame {
                seq: 0,
                total: 1,
                payload: Vec::new(),
            }];
        }

        let chunks: Vec<&[u8]> = data.chunks(self.chunk_size).collect();
        let total = chunks.len() as u16;
        chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| QrFrame {
                seq: i as u16,
                total,
                payload: chunk.to_vec(),
            })
            .collect()
    }

    /// Reassemble QR frames into complete data.
    pub fn reassemble(frames: &[QrFrame]) -> Result<Vec<u8>, TransportError> {
        if frames.is_empty() {
            return Ok(Vec::new());
        }

        let total = frames[0].total as usize;
        if frames.len() != total {
            return Err(TransportError::Connection(format!(
                "incomplete QR stream: got {} frames, expected {}",
                frames.len(),
                total
            )));
        }

        // Verify sequence continuity
        let mut sorted: Vec<&QrFrame> = frames.iter().collect();
        sorted.sort_by_key(|f| f.seq);

        let mut data = Vec::new();
        for (i, frame) in sorted.iter().enumerate() {
            if frame.seq as usize != i {
                return Err(TransportError::Connection(format!(
                    "missing QR frame: expected seq {i}, got {}",
                    frame.seq
                )));
            }
            data.extend_from_slice(&frame.payload);
        }
        Ok(data)
    }

    /// Calculate estimated transfer time for given data size.
    pub fn estimate_transfer_secs(&self, data_len: usize) -> f32 {
        let num_frames = (data_len + self.chunk_size - 1) / self.chunk_size.max(1);
        num_frames as f32 / self.display_rate as f32
    }

    /// Calculate effective bitrate.
    pub fn effective_bitrate(&self) -> u32 {
        (self.chunk_size as u32) * 8 * (self.display_rate as u32)
    }
}

impl Default for QrStreamDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for QrStreamDriver {
    fn name(&self) -> &str {
        "qr-stream"
    }

    fn available(&self) -> bool {
        !self.proxy_addr.is_empty()
    }

    async fn dial(
        &self,
        _target: &Target,
        _config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to QR stream proxy at {}: {e}",
                    self.proxy_addr
                ))
            })?;
        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to QR stream proxy at {} for listen: {e}",
                    self.proxy_addr
                ))
            })?;
        Ok(Box::new(QrStreamListener {
            proxy_addr: self.proxy_addr.clone(),
        }))
    }
}

/// Listener for incoming QR stream sessions.
struct QrStreamListener {
    proxy_addr: String,
}

#[async_trait::async_trait]
impl Listener for QrStreamListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| TransportError::Connection(format!("QR stream accept failed: {e}")))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = QrStreamDriver::new();
        assert_eq!(driver.name(), "qr-stream");
    }

    #[test]
    fn test_default_config() {
        let driver = QrStreamDriver::new();
        assert_eq!(driver.display_rate, 5);
        assert_eq!(driver.ecc_level, EccLevel::High);
    }

    #[test]
    fn test_ecc_max_bytes() {
        assert_eq!(EccLevel::High.max_bytes_v25(), 1200);
        assert_eq!(EccLevel::Low.max_bytes_v25(), 2520);
    }

    #[test]
    fn test_fragment_small_data() {
        let driver = QrStreamDriver::new();
        let data = b"Hello, QR!";
        let frames = driver.fragment(data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].seq, 0);
        assert_eq!(frames[0].total, 1);
        assert_eq!(frames[0].payload, data);
    }

    #[test]
    fn test_fragment_large_data() {
        let driver = QrStreamDriver::new();
        let data = vec![0xAB; 3000]; // Should produce 3 frames (1196 bytes per chunk)
        let frames = driver.fragment(&data);
        assert!(frames.len() >= 3);
        assert_eq!(frames[0].total, frames.len() as u16);
    }

    #[test]
    fn test_fragment_empty() {
        let driver = QrStreamDriver::new();
        let frames = driver.fragment(b"");
        assert_eq!(frames.len(), 1);
        assert!(frames[0].payload.is_empty());
    }

    #[test]
    fn test_reassemble_roundtrip() {
        let driver = QrStreamDriver::new();
        let data = b"Test data for QR stream reassembly roundtrip!";
        let frames = driver.fragment(data);
        let reassembled = QrStreamDriver::reassemble(&frames).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_reassemble_large_roundtrip() {
        let driver = QrStreamDriver::new();
        let data = vec![0xCD; 5000];
        let frames = driver.fragment(&data);
        let reassembled = QrStreamDriver::reassemble(&frames).unwrap();
        assert_eq!(reassembled, data);
    }

    #[test]
    fn test_reassemble_incomplete() {
        let frames = vec![QrFrame {
            seq: 0,
            total: 3,
            payload: vec![1, 2, 3],
        }];
        let result = QrStreamDriver::reassemble(&frames);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("incomplete"));
    }

    #[test]
    fn test_qr_frame_serialization() {
        let frame = QrFrame {
            seq: 1,
            total: 5,
            payload: vec![0xAA, 0xBB],
        };
        let bytes = frame.to_bytes();
        let decoded = QrFrame::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.seq, 1);
        assert_eq!(decoded.total, 5);
        assert_eq!(decoded.payload, vec![0xAA, 0xBB]);
    }

    #[test]
    fn test_qr_frame_too_short() {
        let result = QrFrame::from_bytes(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn test_estimate_transfer_time() {
        let driver = QrStreamDriver::new();
        let time = driver.estimate_transfer_secs(6000); // ~5 frames at 1196/frame
        assert!(time > 0.0);
        assert!(time < 10.0);
    }

    #[test]
    fn test_effective_bitrate() {
        let driver = QrStreamDriver::new();
        let bitrate = driver.effective_bitrate();
        // chunk_size * 8 * display_rate = 1196 * 8 * 5 = 47840 bps
        assert!(bitrate > 40000);
    }

    #[test]
    fn test_available() {
        let driver = QrStreamDriver::new();
        assert!(driver.available());
        let empty = QrStreamDriver::with_proxy("");
        assert!(!empty.available());
    }

    #[tokio::test]
    async fn test_dial_proxy_unavailable() {
        let driver = QrStreamDriver::with_proxy("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to QR stream proxy")
            ),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_proxy_unavailable() {
        let driver = QrStreamDriver::with_proxy("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to QR stream proxy")
            ),
            Ok(_) => panic!("expected error"),
        }
    }
}
