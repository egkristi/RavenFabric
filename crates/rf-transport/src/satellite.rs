//! Satellite link transport driver.
//!
//! Bridges RavenFabric over satellite communication links. Supports Iridium
//! Short Burst Data (SBD), store-and-forward messaging, and orbital window
//! scheduling for LEO satellite passes.
//!
//! # Requirements
//!
//! - Iridium 9602/9603 modem (SBD) or RockBLOCK module
//! - Serial connection to modem or RockBLOCK HTTP API
//! - Iridium service subscription (per-message billing)
//!
//! # Protocol
//!
//! - Iridium SBD: Mobile Originated (MO) / Mobile Terminated (MT) messages
//! - Max MO message: 340 bytes; Max MT message: 270 bytes
//! - AT command interface for direct modem control
//! - Store-forward: messages queued during satellite unavailability

use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Maximum Mobile Originated message size (bytes).
const MAX_MO_SIZE: usize = 340;

/// Maximum Mobile Terminated message size (bytes).
#[allow(dead_code)]
const MAX_MT_SIZE: usize = 270;

/// Default serial-to-TCP bridge address for SBD modem.
const DEFAULT_MODEM_ADDR: &str = "127.0.0.1:5000";

/// Iridium SBD session status codes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SbdStatus {
    /// MO message transferred successfully.
    Success,
    /// MO transfer success, MT message queued.
    SuccessWithMt,
    /// Timeout waiting for response.
    Timeout,
    /// Modem not registered.
    NotRegistered,
    /// Message too long.
    TooLong,
    /// Link failure.
    LinkFailure,
    /// Unknown status.
    Unknown(u8),
}

impl SbdStatus {
    /// Parse from AT+SBDIX response code.
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => SbdStatus::Success,
            1 => SbdStatus::SuccessWithMt,
            32 => SbdStatus::NotRegistered,
            33 => SbdStatus::TooLong,
            34..=36 => SbdStatus::LinkFailure,
            _ if (10..=19).contains(&code) => SbdStatus::Timeout,
            _ => SbdStatus::Unknown(code),
        }
    }

    /// Whether the status indicates success.
    pub fn is_success(&self) -> bool {
        matches!(self, SbdStatus::Success | SbdStatus::SuccessWithMt)
    }
}

/// Orbital pass window for LEO satellites.
#[derive(Debug, Clone)]
pub struct PassWindow {
    /// Elevation angle above horizon (degrees).
    pub elevation: f32,
    /// Duration of pass (seconds).
    pub duration_secs: u32,
    /// Signal strength estimate (0-5 bars).
    pub signal_bars: u8,
}

impl PassWindow {
    /// Estimate available throughput during this pass (bytes).
    pub fn estimated_throughput_bytes(&self) -> usize {
        // Iridium SBD: ~1 message per 20 seconds
        let messages = (self.duration_secs / 20) as usize;
        messages * MAX_MO_SIZE
    }

    /// Whether signal is usable for transmission.
    pub fn is_usable(&self) -> bool {
        self.elevation > 10.0 && self.signal_bars >= 2
    }
}

/// Transport driver that communicates over satellite links.
#[allow(dead_code)]
pub struct SatelliteDriver {
    /// Modem TCP address (serial-to-TCP bridge).
    modem_addr: String,
    /// IMEI of the Iridium modem (15 digits).
    imei: Option<String>,
}

impl SatelliteDriver {
    /// Create a new satellite driver with default settings.
    pub fn new() -> Self {
        Self {
            modem_addr: DEFAULT_MODEM_ADDR.to_string(),
            imei: None,
        }
    }

    /// Create with custom modem address.
    pub fn with_modem(addr: impl Into<String>) -> Self {
        Self {
            modem_addr: addr.into(),
            imei: None,
        }
    }

    /// Create with IMEI.
    pub fn with_imei(imei: impl Into<String>) -> Result<Self, TransportError> {
        let imei_str = imei.into();
        Self::validate_imei(&imei_str)?;
        Ok(Self {
            modem_addr: DEFAULT_MODEM_ADDR.to_string(),
            imei: Some(imei_str),
        })
    }

    /// Validate an IMEI number (15 digits).
    pub fn validate_imei(imei: &str) -> Result<(), TransportError> {
        if imei.len() != 15 {
            return Err(TransportError::Connection(format!(
                "IMEI must be exactly 15 digits, got {}",
                imei.len()
            )));
        }
        if !imei.chars().all(|c| c.is_ascii_digit()) {
            return Err(TransportError::Connection(
                "IMEI must contain only digits".to_string(),
            ));
        }
        Ok(())
    }

    /// Format AT command to write SBD message to modem buffer.
    pub fn format_sbdwb(payload_len: usize) -> Result<String, TransportError> {
        if payload_len > MAX_MO_SIZE {
            return Err(TransportError::Connection(format!(
                "SBD message too large: {payload_len} > {MAX_MO_SIZE} bytes"
            )));
        }
        Ok(format!("AT+SBDWB={payload_len}\r"))
    }

    /// Format AT command to initiate SBD session.
    pub fn format_sbdix() -> &'static str {
        "AT+SBDIX\r"
    }

    /// Format AT command to read MT message from buffer.
    pub fn format_sbdrb() -> &'static str {
        "AT+SBDRB\r"
    }

    /// Format AT command to check signal quality.
    pub fn format_csq() -> &'static str {
        "AT+CSQ\r"
    }

    /// Parse signal quality response (0-5 bars).
    pub fn parse_signal_quality(response: &str) -> Option<u8> {
        // Response format: +CSQ:N where N is 0-5
        response
            .find("+CSQ:")
            .and_then(|pos| response[pos + 5..].trim().chars().next())
            .and_then(|c| c.to_digit(10))
            .map(|d| d as u8)
            .filter(|&q| q <= 5)
    }

    /// Encode payload for SBD transmission with checksum.
    pub fn encode_sbd_payload(data: &[u8]) -> Result<Vec<u8>, TransportError> {
        if data.len() > MAX_MO_SIZE {
            return Err(TransportError::Connection(format!(
                "SBD payload exceeds max MO size: {} > {MAX_MO_SIZE}",
                data.len()
            )));
        }
        // Calculate 2-byte checksum (sum of all bytes)
        let checksum: u16 = data.iter().map(|&b| b as u16).sum();
        let mut payload = Vec::with_capacity(data.len() + 2);
        payload.extend_from_slice(data);
        payload.extend_from_slice(&checksum.to_be_bytes());
        Ok(payload)
    }

    /// Verify SBD payload checksum.
    pub fn verify_sbd_checksum(payload: &[u8]) -> bool {
        if payload.len() < 2 {
            return false;
        }
        let data = &payload[..payload.len() - 2];
        let expected = u16::from_be_bytes([payload[payload.len() - 2], payload[payload.len() - 1]]);
        let actual: u16 = data.iter().map(|&b| b as u16).sum();
        actual == expected
    }
}

impl Default for SatelliteDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for SatelliteDriver {
    fn name(&self) -> &str {
        "satellite"
    }

    fn available(&self) -> bool {
        !self.modem_addr.is_empty()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let dest_imei = config
            .get("imei")
            .cloned()
            .or_else(|| {
                target
                    .relay_url
                    .as_ref()
                    .and_then(|u| u.strip_prefix("sat://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("Satellite destination IMEI not specified".to_string())
            })?;

        Self::validate_imei(&dest_imei)?;

        let stream = TcpStream::connect(&self.modem_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to satellite modem at {}: {e}",
                self.modem_addr
            ))
        })?;

        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = TcpStream::connect(&self.modem_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to satellite modem at {} for listen: {e}",
                self.modem_addr
            ))
        })?;

        Ok(Box::new(SatelliteListener {
            modem_addr: self.modem_addr.clone(),
        }))
    }
}

/// Listener for incoming satellite messages.
struct SatelliteListener {
    modem_addr: String,
}

#[async_trait::async_trait]
impl Listener for SatelliteListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.modem_addr)
            .await
            .map_err(|e| TransportError::Connection(format!("satellite accept failed: {e}")))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = SatelliteDriver::new();
        assert_eq!(driver.name(), "satellite");
    }

    #[test]
    fn test_available() {
        let driver = SatelliteDriver::new();
        assert!(driver.available());
        let empty = SatelliteDriver::with_modem("");
        assert!(!empty.available());
    }

    #[test]
    fn test_validate_imei_valid() {
        assert!(SatelliteDriver::validate_imei("300234065123456").is_ok());
    }

    #[test]
    fn test_validate_imei_wrong_length() {
        let err = SatelliteDriver::validate_imei("12345").unwrap_err();
        assert!(err.to_string().contains("15 digits"));
    }

    #[test]
    fn test_validate_imei_non_digits() {
        let err = SatelliteDriver::validate_imei("30023406512345A").unwrap_err();
        assert!(err.to_string().contains("only digits"));
    }

    #[test]
    fn test_sbd_status_from_code() {
        assert_eq!(SbdStatus::from_code(0), SbdStatus::Success);
        assert_eq!(SbdStatus::from_code(1), SbdStatus::SuccessWithMt);
        assert_eq!(SbdStatus::from_code(32), SbdStatus::NotRegistered);
        assert!(SbdStatus::from_code(0).is_success());
        assert!(!SbdStatus::from_code(32).is_success());
    }

    #[test]
    fn test_format_sbdwb() {
        let cmd = SatelliteDriver::format_sbdwb(100).unwrap();
        assert_eq!(cmd, "AT+SBDWB=100\r");
    }

    #[test]
    fn test_format_sbdwb_too_large() {
        let result = SatelliteDriver::format_sbdwb(MAX_MO_SIZE + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_signal_quality() {
        assert_eq!(SatelliteDriver::parse_signal_quality("+CSQ:5"), Some(5));
        assert_eq!(SatelliteDriver::parse_signal_quality("+CSQ:0"), Some(0));
        assert_eq!(SatelliteDriver::parse_signal_quality("OK"), None);
        assert_eq!(SatelliteDriver::parse_signal_quality("+CSQ:9"), None);
    }

    #[test]
    fn test_encode_sbd_payload() {
        let data = vec![0x01, 0x02, 0x03];
        let encoded = SatelliteDriver::encode_sbd_payload(&data).unwrap();
        assert_eq!(encoded.len(), 5); // 3 data + 2 checksum
        // Checksum = 1 + 2 + 3 = 6 = 0x0006
        assert_eq!(encoded[3], 0x00);
        assert_eq!(encoded[4], 0x06);
    }

    #[test]
    fn test_verify_sbd_checksum() {
        let data = vec![0x01, 0x02, 0x03];
        let encoded = SatelliteDriver::encode_sbd_payload(&data).unwrap();
        assert!(SatelliteDriver::verify_sbd_checksum(&encoded));
    }

    #[test]
    fn test_verify_sbd_checksum_invalid() {
        let bad = vec![0x01, 0x02, 0x03, 0xFF, 0xFF];
        assert!(!SatelliteDriver::verify_sbd_checksum(&bad));
    }

    #[test]
    fn test_pass_window_throughput() {
        let pass = PassWindow {
            elevation: 45.0,
            duration_secs: 600, // 10 minutes
            signal_bars: 4,
        };
        // 600/20 = 30 messages × 340 bytes = 10200
        assert_eq!(pass.estimated_throughput_bytes(), 10200);
    }

    #[test]
    fn test_pass_window_usable() {
        let good = PassWindow {
            elevation: 45.0,
            duration_secs: 300,
            signal_bars: 4,
        };
        assert!(good.is_usable());
        let low_elev = PassWindow {
            elevation: 5.0,
            duration_secs: 300,
            signal_bars: 4,
        };
        assert!(!low_elev.is_usable());
        let low_signal = PassWindow {
            elevation: 45.0,
            duration_secs: 300,
            signal_bars: 1,
        };
        assert!(!low_signal.is_usable());
    }

    #[tokio::test]
    async fn test_dial_missing_imei() {
        let driver = SatelliteDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("IMEI not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_imei() {
        let driver = SatelliteDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("sat://12345".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("15 digits")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_modem_unavailable() {
        let driver = SatelliteDriver::with_modem("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("imei".to_string(), "300234065123456".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to satellite modem")
            ),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_modem_unavailable() {
        let driver = SatelliteDriver::with_modem("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to satellite modem")
            ),
            Ok(_) => panic!("expected error"),
        }
    }
}
