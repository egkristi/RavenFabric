//! AX.25 packet radio transport driver.
//!
//! Communicates over amateur radio AX.25 protocol using a TNC (Terminal Node
//! Controller) or software modem (Direwolf, soundmodem). Provides store-and-forward
//! packet radio connectivity.
//!
//! # Requirements
//!
//! - TNC or software modem (Direwolf recommended)
//! - KISS TNC interface (TCP or serial)
//! - Valid amateur radio license for transmission
//!
//! # Protocol
//!
//! - KISS TNC framing: [0xC0][cmd][data...][0xC0]
//! - AX.25 UI frames for connectionless delivery
//! - Callsign-based addressing (max 6 chars + SSID 0-15)

use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// KISS frame delimiter.
const KISS_FEND: u8 = 0xC0;

/// KISS frame escape.
const KISS_FESC: u8 = 0xDB;

/// KISS transposed FEND.
const KISS_TFEND: u8 = 0xDC;

/// KISS transposed FESC.
const KISS_TFESC: u8 = 0xDD;

/// KISS data frame command.
const KISS_CMD_DATA: u8 = 0x00;

/// Default KISS TNC TCP address (Direwolf default).
const DEFAULT_TNC_ADDR: &str = "127.0.0.1:8001";

/// Maximum AX.25 information field size.
const MAX_INFO_SIZE: usize = 256;

/// Maximum callsign length (without SSID).
const MAX_CALLSIGN_LEN: usize = 6;

/// AX.25 callsign with SSID.
#[derive(Debug, Clone, PartialEq)]
pub struct Callsign {
    /// Station callsign (1-6 uppercase alphanumeric).
    pub call: String,
    /// SSID (0-15).
    pub ssid: u8,
}

impl Callsign {
    /// Parse a callsign string (e.g., "N0CALL-5").
    pub fn parse(s: &str) -> Result<Self, TransportError> {
        let (call, ssid) = if let Some((c, s)) = s.split_once('-') {
            let ssid = s.parse::<u8>().map_err(|_| {
                TransportError::Connection(format!("invalid SSID in callsign: '{s}'"))
            })?;
            if ssid > 15 {
                return Err(TransportError::Connection(format!(
                    "SSID must be 0-15, got {ssid}"
                )));
            }
            (c.to_string(), ssid)
        } else {
            (s.to_string(), 0)
        };

        if call.is_empty() || call.len() > MAX_CALLSIGN_LEN {
            return Err(TransportError::Connection(format!(
                "callsign must be 1-{MAX_CALLSIGN_LEN} characters, got '{call}'"
            )));
        }
        if !call.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(TransportError::Connection(format!(
                "callsign must be alphanumeric: '{call}'"
            )));
        }

        Ok(Self {
            call: call.to_ascii_uppercase(),
            ssid,
        })
    }

    /// Encode callsign to AX.25 address field (7 bytes: 6 char + SSID byte).
    pub fn to_ax25_bytes(&self) -> [u8; 7] {
        let mut bytes = [0x40u8; 7]; // Space (0x20) shifted left = 0x40
        for (i, ch) in self.call.bytes().enumerate() {
            if i < 6 {
                bytes[i] = ch << 1;
            }
        }
        // SSID byte: [C C SSID(4) 0 E]
        // C=1 for command, E=0 unless last address
        bytes[6] = 0x60 | ((self.ssid & 0x0F) << 1);
        bytes
    }
}

impl std::fmt::Display for Callsign {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ssid == 0 {
            write!(f, "{}", self.call)
        } else {
            write!(f, "{}-{}", self.call, self.ssid)
        }
    }
}

/// Transport driver that communicates over AX.25 packet radio.
#[allow(dead_code)]
pub struct Ax25Driver {
    /// KISS TNC TCP address.
    tnc_addr: String,
    /// Local station callsign.
    mycall: Option<Callsign>,
}

impl Ax25Driver {
    /// Create a new AX.25 driver with default settings.
    pub fn new() -> Self {
        Self {
            tnc_addr: DEFAULT_TNC_ADDR.to_string(),
            mycall: None,
        }
    }

    /// Create with custom TNC address.
    pub fn with_tnc(addr: impl Into<String>) -> Self {
        Self {
            tnc_addr: addr.into(),
            mycall: None,
        }
    }

    /// Create with local callsign.
    pub fn with_callsign(call: &str) -> Result<Self, TransportError> {
        Ok(Self {
            tnc_addr: DEFAULT_TNC_ADDR.to_string(),
            mycall: Some(Callsign::parse(call)?),
        })
    }

    /// Encode data into a KISS frame.
    pub fn kiss_encode(data: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(data.len() * 2 + 3);
        frame.push(KISS_FEND);
        frame.push(KISS_CMD_DATA);
        for &byte in data {
            match byte {
                KISS_FEND => {
                    frame.push(KISS_FESC);
                    frame.push(KISS_TFEND);
                }
                KISS_FESC => {
                    frame.push(KISS_FESC);
                    frame.push(KISS_TFESC);
                }
                _ => frame.push(byte),
            }
        }
        frame.push(KISS_FEND);
        frame
    }

    /// Decode a KISS frame, stripping delimiters and unescaping.
    pub fn kiss_decode(frame: &[u8]) -> Result<Vec<u8>, TransportError> {
        if frame.len() < 3 {
            return Err(TransportError::Connection(
                "KISS frame too short".to_string(),
            ));
        }
        if frame[0] != KISS_FEND || frame[frame.len() - 1] != KISS_FEND {
            return Err(TransportError::Connection(
                "KISS frame missing delimiters".to_string(),
            ));
        }

        // Skip command byte
        let data = &frame[2..frame.len() - 1];
        let mut decoded = Vec::with_capacity(data.len());
        let mut i = 0;
        while i < data.len() {
            if data[i] == KISS_FESC {
                i += 1;
                if i >= data.len() {
                    return Err(TransportError::Connection(
                        "KISS frame: escape at end of frame".to_string(),
                    ));
                }
                match data[i] {
                    KISS_TFEND => decoded.push(KISS_FEND),
                    KISS_TFESC => decoded.push(KISS_FESC),
                    _ => {
                        return Err(TransportError::Connection(format!(
                            "KISS frame: invalid escape sequence: 0x{:02X}",
                            data[i]
                        )));
                    }
                }
            } else {
                decoded.push(data[i]);
            }
            i += 1;
        }
        Ok(decoded)
    }

    /// Build an AX.25 UI frame.
    pub fn build_ui_frame(
        dest: &Callsign,
        src: &Callsign,
        info: &[u8],
    ) -> Result<Vec<u8>, TransportError> {
        if info.len() > MAX_INFO_SIZE {
            return Err(TransportError::Connection(format!(
                "AX.25 info field too large: {} > {MAX_INFO_SIZE}",
                info.len()
            )));
        }

        let mut frame = Vec::with_capacity(16 + info.len());

        // Destination address (7 bytes)
        frame.extend_from_slice(&dest.to_ax25_bytes());

        // Source address (7 bytes) — set end-of-address bit
        let mut src_bytes = src.to_ax25_bytes();
        src_bytes[6] |= 0x01; // End of address field
        frame.extend_from_slice(&src_bytes);

        // Control: UI frame (0x03)
        frame.push(0x03);

        // PID: No Layer 3 (0xF0)
        frame.push(0xF0);

        // Information field
        frame.extend_from_slice(info);

        Ok(frame)
    }
}

impl Default for Ax25Driver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for Ax25Driver {
    fn name(&self) -> &str {
        "ax25"
    }

    fn available(&self) -> bool {
        !self.tnc_addr.is_empty()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let dest_call = config
            .get("callsign")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("ax25://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("AX.25 destination callsign not specified".to_string())
            })?;

        Callsign::parse(&dest_call)?;

        let stream = TcpStream::connect(&self.tnc_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to KISS TNC at {}: {e}",
                self.tnc_addr
            ))
        })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = TcpStream::connect(&self.tnc_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to KISS TNC at {} for listen: {e}",
                self.tnc_addr
            ))
        })?;

        Ok(Box::new(Ax25Listener {
            tnc_addr: self.tnc_addr.clone(),
        }))
    }
}

/// Listener for incoming AX.25 packets.
struct Ax25Listener {
    tnc_addr: String,
}

#[async_trait::async_trait]
impl Listener for Ax25Listener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.tnc_addr)
            .await
            .map_err(|e| TransportError::Connection(format!("AX.25 accept failed: {e}")))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = Ax25Driver::new();
        assert_eq!(driver.name(), "ax25");
    }

    #[test]
    fn test_callsign_parse_simple() {
        let cs = Callsign::parse("N0CALL").unwrap();
        assert_eq!(cs.call, "N0CALL");
        assert_eq!(cs.ssid, 0);
    }

    #[test]
    fn test_callsign_parse_with_ssid() {
        let cs = Callsign::parse("W1AW-5").unwrap();
        assert_eq!(cs.call, "W1AW");
        assert_eq!(cs.ssid, 5);
    }

    #[test]
    fn test_callsign_parse_lowercase() {
        let cs = Callsign::parse("n0call-2").unwrap();
        assert_eq!(cs.call, "N0CALL"); // uppercased
    }

    #[test]
    fn test_callsign_invalid_ssid() {
        let err = Callsign::parse("N0CALL-16").unwrap_err();
        assert!(err.to_string().contains("SSID must be 0-15"));
    }

    #[test]
    fn test_callsign_too_long() {
        let err = Callsign::parse("TOOLONGCALL").unwrap_err();
        assert!(err.to_string().contains("1-6 characters"));
    }

    #[test]
    fn test_callsign_invalid_chars() {
        let err = Callsign::parse("N0!@#").unwrap_err();
        assert!(err.to_string().contains("alphanumeric"));
    }

    #[test]
    fn test_callsign_to_ax25_bytes() {
        let cs = Callsign::parse("N0CALL").unwrap();
        let bytes = cs.to_ax25_bytes();
        assert_eq!(bytes.len(), 7);
        // First char 'N' = 0x4E, shifted left = 0x9C
        assert_eq!(bytes[0], b'N' << 1);
    }

    #[test]
    fn test_callsign_display() {
        let cs = Callsign::parse("W1AW-5").unwrap();
        assert_eq!(cs.to_string(), "W1AW-5");
        let cs2 = Callsign::parse("N0CALL").unwrap();
        assert_eq!(cs2.to_string(), "N0CALL");
    }

    #[test]
    fn test_kiss_encode_simple() {
        let data = b"hello";
        let frame = Ax25Driver::kiss_encode(data);
        assert_eq!(frame[0], KISS_FEND);
        assert_eq!(frame[1], KISS_CMD_DATA);
        assert_eq!(&frame[2..7], b"hello");
        assert_eq!(frame[7], KISS_FEND);
    }

    #[test]
    fn test_kiss_encode_escape() {
        let data = &[0xC0, 0xDB]; // FEND and FESC
        let frame = Ax25Driver::kiss_encode(data);
        // Should escape: FEND -> FESC TFEND, FESC -> FESC TFESC
        assert_eq!(frame, vec![0xC0, 0x00, 0xDB, 0xDC, 0xDB, 0xDD, 0xC0]);
    }

    #[test]
    fn test_kiss_decode_simple() {
        let frame = vec![0xC0, 0x00, b'a', b'b', b'c', 0xC0];
        let data = Ax25Driver::kiss_decode(&frame).unwrap();
        assert_eq!(data, b"abc");
    }

    #[test]
    fn test_kiss_encode_decode_roundtrip() {
        let original = vec![0xC0, 0xDB, 0x42, 0x00, 0xFF];
        let encoded = Ax25Driver::kiss_encode(&original);
        let decoded = Ax25Driver::kiss_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_kiss_decode_missing_delimiters() {
        let frame = vec![0x00, 0x01, 0x02];
        let result = Ax25Driver::kiss_decode(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_ui_frame() {
        let dest = Callsign::parse("CQ").unwrap();
        let src = Callsign::parse("N0CALL-5").unwrap();
        let frame = Ax25Driver::build_ui_frame(&dest, &src, b"test").unwrap();
        // 7 (dest) + 7 (src) + 1 (ctrl) + 1 (pid) + 4 (info) = 20
        assert_eq!(frame.len(), 20);
        assert_eq!(frame[14], 0x03); // UI control
        assert_eq!(frame[15], 0xF0); // No L3 PID
    }

    #[test]
    fn test_build_ui_frame_too_large() {
        let dest = Callsign::parse("CQ").unwrap();
        let src = Callsign::parse("N0CALL").unwrap();
        let info = vec![0u8; MAX_INFO_SIZE + 1];
        let result = Ax25Driver::build_ui_frame(&dest, &src, &info);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_dial_missing_callsign() {
        let driver = Ax25Driver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("callsign not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_tnc_unavailable() {
        let driver = Ax25Driver::with_tnc("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("callsign".to_string(), "N0CALL".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to KISS TNC")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_tnc_unavailable() {
        let driver = Ax25Driver::with_tnc("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to KISS TNC")),
            Ok(_) => panic!("expected error"),
        }
    }
}
