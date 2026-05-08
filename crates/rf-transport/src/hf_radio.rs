//! HF radio / Winlink bridge transport driver.
//!
//! Bridges RavenFabric over HF (High Frequency) radio using the Winlink
//! email system or VARA/ARDOP modems. Provides global reach without any
//! internet infrastructure.
//!
//! # Requirements
//!
//! - HF radio transceiver with digital mode capability
//! - VARA HF modem or Pat (Winlink client)
//! - Valid amateur radio license
//!
//! # Protocol
//!
//! - VARA HF modem: TCP command/data interface
//! - Pat Winlink client: HTTP API for message submission
//! - Messages encoded as Winlink P2P mail with binary attachments
//! - Store-and-forward: messages held until recipient connects

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default VARA HF modem command port.
const DEFAULT_VARA_CMD_PORT: &str = "127.0.0.1:8300";

/// Default VARA HF modem data port.
const DEFAULT_VARA_DATA_PORT: &str = "127.0.0.1:8301";

/// Default Pat Winlink HTTP API.
#[allow(dead_code)]
const DEFAULT_PAT_API: &str = "127.0.0.1:8080";

/// Maximum message size for HF (with encoding overhead).
const MAX_MESSAGE_SIZE: usize = 65536;

/// VARA modem states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VaraState {
    Disconnected,
    Connecting,
    Connected,
    Busy,
}

/// Transport driver that communicates over HF radio.
pub struct HfRadioDriver {
    /// VARA command port address.
    vara_cmd_addr: String,
    /// VARA data port address.
    vara_data_addr: String,
    /// Local station callsign.
    mycall: String,
}

impl HfRadioDriver {
    /// Create a new HF radio driver with default VARA modem settings.
    pub fn new() -> Self {
        Self {
            vara_cmd_addr: DEFAULT_VARA_CMD_PORT.to_string(),
            vara_data_addr: DEFAULT_VARA_DATA_PORT.to_string(),
            mycall: String::new(),
        }
    }

    /// Create with custom VARA modem addresses.
    pub fn with_vara(cmd_addr: impl Into<String>, data_addr: impl Into<String>) -> Self {
        Self {
            vara_cmd_addr: cmd_addr.into(),
            vara_data_addr: data_addr.into(),
            mycall: String::new(),
        }
    }

    /// Create with local callsign.
    pub fn with_callsign(call: impl Into<String>) -> Self {
        Self {
            vara_cmd_addr: DEFAULT_VARA_CMD_PORT.to_string(),
            vara_data_addr: DEFAULT_VARA_DATA_PORT.to_string(),
            mycall: call.into().to_ascii_uppercase(),
        }
    }

    /// Validate a callsign for HF operation (1-10 chars, alphanumeric + /).
    pub fn validate_callsign(call: &str) -> Result<(), TransportError> {
        if call.is_empty() || call.len() > 10 {
            return Err(TransportError::Connection(format!(
                "HF callsign must be 1-10 characters, got {}",
                call.len()
            )));
        }
        if !call.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-') {
            return Err(TransportError::Connection(format!(
                "HF callsign contains invalid characters: '{call}'"
            )));
        }
        Ok(())
    }

    /// Format a VARA CONNECT command.
    pub fn format_vara_connect(mycall: &str, theircall: &str) -> String {
        format!("CONNECT {mycall} {theircall}\r")
    }

    /// Format a VARA DISCONNECT command.
    pub fn format_vara_disconnect() -> &'static str {
        "DISCONNECT\r"
    }

    /// Format a VARA MYCALL command.
    pub fn format_vara_mycall(call: &str) -> String {
        format!("MYCALL {call}\r")
    }

    /// Parse a VARA status response.
    pub fn parse_vara_status(response: &str) -> VaraState {
        let resp = response.trim().to_uppercase();
        if resp.starts_with("CONNECTED") {
            VaraState::Connected
        } else if resp.starts_with("CONNECTING") {
            VaraState::Connecting
        } else if resp.starts_with("BUSY") {
            VaraState::Busy
        } else {
            VaraState::Disconnected
        }
    }

    /// Encode a message for HF transport (with size header).
    pub fn encode_message(payload: &[u8]) -> Result<Vec<u8>, TransportError> {
        if payload.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::Connection(format!(
                "HF message too large: {} > {} bytes",
                payload.len(),
                MAX_MESSAGE_SIZE
            )));
        }
        let mut msg = Vec::with_capacity(4 + payload.len());
        msg.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        msg.extend_from_slice(payload);
        Ok(msg)
    }

    /// Decode a message from HF transport.
    pub fn decode_message(data: &[u8]) -> Result<Vec<u8>, TransportError> {
        if data.len() < 4 {
            return Err(TransportError::Connection(
                "HF message too short: need 4-byte length header".to_string(),
            ));
        }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(TransportError::Connection(format!(
                "HF message truncated: expected {} bytes, got {}",
                len,
                data.len() - 4
            )));
        }
        Ok(data[4..4 + len].to_vec())
    }
}

impl Default for HfRadioDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for HfRadioDriver {
    fn name(&self) -> &str {
        "hf-radio"
    }

    fn available(&self) -> bool {
        !self.vara_cmd_addr.is_empty()
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
                    .and_then(|u| u.strip_prefix("hf://"))
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                TransportError::Connection("HF destination callsign not specified".to_string())
            })?;

        Self::validate_callsign(&dest_call)?;

        // Connect to VARA command port
        let mut cmd_stream =
            TcpStream::connect(&self.vara_cmd_addr)
                .await
                .map_err(|e| TransportError::Connection(format!(
                    "failed to connect to VARA modem at {}: {e}",
                    self.vara_cmd_addr
                )))?;

        // Set MYCALL
        if !self.mycall.is_empty() {
            let mycall_cmd = Self::format_vara_mycall(&self.mycall);
            cmd_stream.write_all(mycall_cmd.as_bytes()).await.map_err(|e| {
                TransportError::Connection(format!("failed to set MYCALL: {e}"))
            })?;
        }

        // Send CONNECT command
        let connect_cmd = Self::format_vara_connect(
            if self.mycall.is_empty() { "NOCALL" } else { &self.mycall },
            &dest_call,
        );
        cmd_stream.write_all(connect_cmd.as_bytes()).await.map_err(|e| {
            TransportError::Connection(format!("failed to send VARA CONNECT: {e}"))
        })?;

        // Connect to VARA data port for actual data transfer
        let data_stream =
            TcpStream::connect(&self.vara_data_addr)
                .await
                .map_err(|e| TransportError::Connection(format!(
                    "failed to connect to VARA data port at {}: {e}",
                    self.vara_data_addr
                )))?;

        Ok(Box::new(data_stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Connect to VARA command port and set to listen mode
        let mut cmd_stream =
            TcpStream::connect(&self.vara_cmd_addr)
                .await
                .map_err(|e| TransportError::Connection(format!(
                    "failed to connect to VARA modem at {} for listen: {e}",
                    self.vara_cmd_addr
                )))?;

        // Set MYCALL and LISTEN ON
        if !self.mycall.is_empty() {
            let cmd = format!("MYCALL {}\rLISTEN ON\r", self.mycall);
            cmd_stream.write_all(cmd.as_bytes()).await.map_err(|e| {
                TransportError::Connection(format!("failed to enable VARA listen: {e}"))
            })?;
        }

        Ok(Box::new(HfRadioListener {
            vara_data_addr: self.vara_data_addr.clone(),
        }))
    }
}

/// Listener for incoming HF radio connections.
struct HfRadioListener {
    vara_data_addr: String,
}

#[async_trait::async_trait]
impl Listener for HfRadioListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = TcpStream::connect(&self.vara_data_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!("HF radio accept failed: {e}"))
            })?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = HfRadioDriver::new();
        assert_eq!(driver.name(), "hf-radio");
    }

    #[test]
    fn test_available() {
        let driver = HfRadioDriver::new();
        assert!(driver.available());
        let empty = HfRadioDriver::with_vara("", "");
        assert!(!empty.available());
    }

    #[test]
    fn test_validate_callsign_valid() {
        assert!(HfRadioDriver::validate_callsign("N0CALL").is_ok());
        assert!(HfRadioDriver::validate_callsign("VE3/W1AW").is_ok());
        assert!(HfRadioDriver::validate_callsign("LA1ABC").is_ok());
    }

    #[test]
    fn test_validate_callsign_empty() {
        let err = HfRadioDriver::validate_callsign("").unwrap_err();
        assert!(err.to_string().contains("1-10 characters"));
    }

    #[test]
    fn test_validate_callsign_too_long() {
        let err = HfRadioDriver::validate_callsign("TOOLONGCALLSIGN").unwrap_err();
        assert!(err.to_string().contains("1-10 characters"));
    }

    #[test]
    fn test_validate_callsign_invalid_chars() {
        let err = HfRadioDriver::validate_callsign("N0!@#").unwrap_err();
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn test_format_vara_connect() {
        let cmd = HfRadioDriver::format_vara_connect("N0CALL", "W1AW");
        assert_eq!(cmd, "CONNECT N0CALL W1AW\r");
    }

    #[test]
    fn test_format_vara_mycall() {
        let cmd = HfRadioDriver::format_vara_mycall("LA1ABC");
        assert_eq!(cmd, "MYCALL LA1ABC\r");
    }

    #[test]
    fn test_parse_vara_status() {
        assert_eq!(HfRadioDriver::parse_vara_status("CONNECTED W1AW"), VaraState::Connected);
        assert_eq!(HfRadioDriver::parse_vara_status("CONNECTING"), VaraState::Connecting);
        assert_eq!(HfRadioDriver::parse_vara_status("BUSY"), VaraState::Busy);
        assert_eq!(HfRadioDriver::parse_vara_status("DISCONNECTED"), VaraState::Disconnected);
        assert_eq!(HfRadioDriver::parse_vara_status("UNKNOWN"), VaraState::Disconnected);
    }

    #[test]
    fn test_encode_message() {
        let payload = b"hello HF";
        let msg = HfRadioDriver::encode_message(payload).unwrap();
        assert_eq!(msg.len(), 4 + 8);
        assert_eq!(&msg[0..4], &[0, 0, 0, 8]);
    }

    #[test]
    fn test_encode_message_too_large() {
        let payload = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let result = HfRadioDriver::encode_message(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_message() {
        let data = vec![0, 0, 0, 3, b'a', b'b', b'c'];
        let payload = HfRadioDriver::decode_message(&data).unwrap();
        assert_eq!(payload, b"abc");
    }

    #[test]
    fn test_decode_message_truncated() {
        let data = vec![0, 0, 0, 10, 0x01]; // claims 10 bytes but only 1
        let result = HfRadioDriver::decode_message(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("truncated"));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let payload = b"HF radio roundtrip test";
        let encoded = HfRadioDriver::encode_message(payload).unwrap();
        let decoded = HfRadioDriver::decode_message(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    #[tokio::test]
    async fn test_dial_missing_callsign() {
        let driver = HfRadioDriver::new();
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
    async fn test_dial_vara_unavailable() {
        let driver = HfRadioDriver::with_vara("127.0.0.1:19999", "127.0.0.1:19998");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("callsign".to_string(), "N0CALL".to_string());
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to VARA")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_vara_unavailable() {
        let driver = HfRadioDriver::with_vara("127.0.0.1:19999", "127.0.0.1:19998");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to VARA")),
            Ok(_) => panic!("expected error"),
        }
    }
}
