//! I2P transport driver via SAM (Simple Anonymous Messaging) bridge.
//!
//! Connects to I2P destinations through the local SAM bridge API
//! (typically `127.0.0.1:7656`). Provides anonymous, garlic-routed
//! connectivity where both endpoints are hidden behind I2P tunnels.
//!
//! # Requirements
//!
//! - I2P router running locally with SAM bridge enabled
//! - SAM bridge listening on configured address (default: `127.0.0.1:7656`)
//!
//! # Configuration
//!
//! Driver config keys:
//! - `sam_addr`: SAM bridge address (default: `127.0.0.1:7656`)
//! - `destination`: Target I2P destination (base64-encoded, 516+ chars)
//! - `tunnel_length`: Number of hops per tunnel (default: `3`)
//! - `session_name`: SAM session name (default: `ravenfabric`)

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default SAM bridge address.
const DEFAULT_SAM_ADDR: &str = "127.0.0.1:7656";

/// Default target port for RavenFabric over I2P.
const DEFAULT_TUNNEL_LENGTH: u8 = 3;

/// Default session name.
const DEFAULT_SESSION_NAME: &str = "ravenfabric";

/// SAM protocol version we speak.
const SAM_VERSION: &str = "3.1";

/// Transport driver that routes connections through I2P's SAM bridge.
pub struct I2pDriver {
    /// SAM bridge address (e.g., "127.0.0.1:7656").
    sam_addr: String,
}

impl I2pDriver {
    /// Create a new I2P driver using the default SAM bridge address.
    pub fn new() -> Self {
        Self {
            sam_addr: DEFAULT_SAM_ADDR.to_string(),
        }
    }

    /// Create a new I2P driver with a custom SAM bridge address.
    pub fn with_sam_addr(sam_addr: impl Into<String>) -> Self {
        Self {
            sam_addr: sam_addr.into(),
        }
    }

    /// Parse a SAM reply line into key-value pairs.
    /// Format: `COMMAND RESULT KEY=VALUE KEY=VALUE ...`
    fn parse_sam_reply(line: &str) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        for part in line.split_whitespace() {
            if let Some((key, value)) = part.split_once('=') {
                map.insert(key.to_string(), value.to_string());
            }
        }
        map
    }

    /// Perform SAM HELLO handshake to establish protocol version.
    async fn sam_hello(stream: &mut BufReader<TcpStream>) -> Result<(), TransportError> {
        let hello_cmd = format!("HELLO VERSION MIN={SAM_VERSION} MAX={SAM_VERSION}\n");
        stream
            .get_mut()
            .write_all(hello_cmd.as_bytes())
            .await
            .map_err(|e| TransportError::Connection(format!("SAM hello write failed: {e}")))?;

        let mut response = String::new();
        stream
            .read_line(&mut response)
            .await
            .map_err(|e| TransportError::Connection(format!("SAM hello read failed: {e}")))?;

        // Expected: "HELLO REPLY RESULT=OK VERSION=3.1\n"
        if !response.contains("RESULT=OK") {
            return Err(TransportError::Connection(format!(
                "SAM hello rejected: {response}"
            )));
        }

        Ok(())
    }

    /// Create a SAM session for stream connections.
    async fn sam_session_create(
        stream: &mut BufReader<TcpStream>,
        session_name: &str,
        tunnel_length: u8,
    ) -> Result<String, TransportError> {
        let cmd = format!(
            "SESSION CREATE STYLE=STREAM ID={session_name} DESTINATION=TRANSIENT \
             inbound.length={tunnel_length} outbound.length={tunnel_length}\n"
        );
        stream
            .get_mut()
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| TransportError::Connection(format!("SAM session write failed: {e}")))?;

        let mut response = String::new();
        stream
            .read_line(&mut response)
            .await
            .map_err(|e| TransportError::Connection(format!("SAM session read failed: {e}")))?;

        // Expected: "SESSION STATUS RESULT=OK DESTINATION=<base64>\n"
        if !response.contains("RESULT=OK") {
            let reply = Self::parse_sam_reply(&response);
            let message = reply
                .get("MESSAGE")
                .cloned()
                .unwrap_or_else(|| response.trim().to_string());
            return Err(TransportError::Connection(format!(
                "SAM session creation failed: {message}"
            )));
        }

        let reply = Self::parse_sam_reply(&response);
        let destination = reply.get("DESTINATION").cloned().unwrap_or_default();

        Ok(destination)
    }

    /// Connect to an I2P destination via SAM STREAM CONNECT.
    async fn sam_stream_connect(
        stream: &mut BufReader<TcpStream>,
        session_name: &str,
        destination: &str,
    ) -> Result<(), TransportError> {
        let cmd =
            format!("STREAM CONNECT ID={session_name} DESTINATION={destination} SILENT=false\n");
        stream
            .get_mut()
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| {
                TransportError::Connection(format!("SAM stream connect write failed: {e}"))
            })?;

        let mut response = String::new();
        stream.read_line(&mut response).await.map_err(|e| {
            TransportError::Connection(format!("SAM stream connect read failed: {e}"))
        })?;

        // Expected: "STREAM STATUS RESULT=OK\n"
        if !response.contains("RESULT=OK") {
            let reply = Self::parse_sam_reply(&response);
            let message = reply
                .get("MESSAGE")
                .cloned()
                .unwrap_or_else(|| response.trim().to_string());
            return Err(TransportError::Connection(format!(
                "SAM stream connect failed: {message}"
            )));
        }

        Ok(())
    }

    /// Accept incoming connections via SAM STREAM ACCEPT.
    async fn sam_stream_accept(
        stream: &mut BufReader<TcpStream>,
        session_name: &str,
    ) -> Result<(), TransportError> {
        let cmd = format!("STREAM ACCEPT ID={session_name} SILENT=false\n");
        stream
            .get_mut()
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| {
                TransportError::Connection(format!("SAM stream accept write failed: {e}"))
            })?;

        let mut response = String::new();
        stream.read_line(&mut response).await.map_err(|e| {
            TransportError::Connection(format!("SAM stream accept read failed: {e}"))
        })?;

        // Expected: "STREAM STATUS RESULT=OK\n"
        if !response.contains("RESULT=OK") {
            let reply = Self::parse_sam_reply(&response);
            let message = reply
                .get("MESSAGE")
                .cloned()
                .unwrap_or_else(|| response.trim().to_string());
            return Err(TransportError::Connection(format!(
                "SAM stream accept failed: {message}"
            )));
        }

        Ok(())
    }

    /// Validate an I2P destination string.
    fn validate_destination(dest: &str) -> Result<(), TransportError> {
        // I2P destinations are base64-encoded, typically 516+ characters
        if dest.len() < 384 {
            return Err(TransportError::Connection(format!(
                "I2P destination too short ({} chars, minimum 384): possibly truncated",
                dest.len()
            )));
        }

        // Basic base64 character validation (I2P uses a modified base64)
        let valid = dest.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '-' || c == '~'
        });
        if !valid {
            return Err(TransportError::Connection(
                "I2P destination contains invalid characters (expected base64)".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for I2pDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for I2pDriver {
    fn name(&self) -> &str {
        "i2p"
    }

    fn available(&self) -> bool {
        // SAM bridge availability is checked at dial time
        // (avoid blocking in available())
        self.sam_addr.parse::<std::net::SocketAddr>().is_ok()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Determine SAM address (config overrides default)
        let sam_addr = config
            .get("sam_addr")
            .cloned()
            .unwrap_or_else(|| self.sam_addr.clone());

        // Get I2P destination from config or target
        let destination = config
            .get("destination")
            .or(target.relay_url.as_ref())
            .ok_or_else(|| {
                TransportError::Connection(
                    "I2P destination not specified (set 'destination' in config or relay_url)"
                        .to_string(),
                )
            })?
            .clone();

        // Strip i2p:// prefix if present
        let destination = destination
            .strip_prefix("i2p://")
            .unwrap_or(&destination)
            .to_string();

        // Validate destination format
        Self::validate_destination(&destination)?;

        // Get tunnel length
        let tunnel_length: u8 = config
            .get("tunnel_length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_TUNNEL_LENGTH);

        let session_name = config
            .get("session_name")
            .cloned()
            .unwrap_or_else(|| DEFAULT_SESSION_NAME.to_string());

        // Connect to SAM bridge
        let sam_socket_addr: std::net::SocketAddr = sam_addr
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid SAM address: {e}")))?;

        let tcp = TcpStream::connect(sam_socket_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to I2P SAM bridge at {sam_addr}: {e}"
            ))
        })?;

        let mut stream = BufReader::new(tcp);

        // SAM protocol handshake
        Self::sam_hello(&mut stream).await?;

        // Create transient session
        Self::sam_session_create(&mut stream, &session_name, tunnel_length).await?;

        // Need a new connection for the stream (SAM requires separate socket per stream)
        let tcp2 = TcpStream::connect(sam_socket_addr).await.map_err(|e| {
            TransportError::Connection(format!("failed to connect for stream socket: {e}"))
        })?;

        let mut stream2 = BufReader::new(tcp2);
        Self::sam_hello(&mut stream2).await?;

        // Connect to destination
        Self::sam_stream_connect(&mut stream2, &session_name, &destination).await?;

        // After STREAM CONNECT succeeds, the socket becomes a raw TCP stream to the destination
        Ok(Box::new(stream2.into_inner()))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        // Parse addr as SAM bridge address (listener creates a destination)
        let sam_addr = if addr.is_empty() {
            self.sam_addr.clone()
        } else {
            addr.to_string()
        };

        let sam_socket_addr: std::net::SocketAddr = sam_addr
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid SAM address: {e}")))?;

        // Connect to SAM bridge and create session
        let tcp = TcpStream::connect(sam_socket_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to I2P SAM bridge at {sam_addr}: {e}"
            ))
        })?;

        let mut stream = BufReader::new(tcp);
        Self::sam_hello(&mut stream).await?;

        let local_dest =
            Self::sam_session_create(&mut stream, DEFAULT_SESSION_NAME, DEFAULT_TUNNEL_LENGTH)
                .await?;

        Ok(Box::new(I2pListener {
            sam_addr,
            session_name: DEFAULT_SESSION_NAME.to_string(),
            local_destination: local_dest,
        }))
    }
}

/// Listener that accepts incoming I2P connections via SAM STREAM ACCEPT.
struct I2pListener {
    sam_addr: String,
    session_name: String,
    #[allow(dead_code)]
    local_destination: String,
}

#[async_trait::async_trait]
impl Listener for I2pListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let sam_socket_addr: std::net::SocketAddr = self
            .sam_addr
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid SAM address: {e}")))?;

        // Each accept needs a new TCP connection to SAM bridge
        let tcp = TcpStream::connect(sam_socket_addr).await.map_err(|e| {
            TransportError::Connection(format!("failed to connect to SAM for accept: {e}"))
        })?;

        let mut stream = BufReader::new(tcp);
        I2pDriver::sam_hello(&mut stream).await?;
        I2pDriver::sam_stream_accept(&mut stream, &self.session_name).await?;

        // After STREAM ACCEPT succeeds, the socket is connected to the remote peer
        Ok(Box::new(stream.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = I2pDriver::new();
        assert_eq!(driver.name(), "i2p");
    }

    #[test]
    fn test_default_sam_addr() {
        let driver = I2pDriver::new();
        assert_eq!(driver.sam_addr, "127.0.0.1:7656");
    }

    #[test]
    fn test_custom_sam_addr() {
        let driver = I2pDriver::with_sam_addr("192.168.1.1:7656");
        assert_eq!(driver.sam_addr, "192.168.1.1:7656");
    }

    #[test]
    fn test_available_valid_addr() {
        let driver = I2pDriver::new();
        assert!(driver.available());
    }

    #[test]
    fn test_available_invalid_addr() {
        let driver = I2pDriver::with_sam_addr("not-an-address");
        assert!(!driver.available());
    }

    #[test]
    fn test_validate_destination_too_short() {
        let result = I2pDriver::validate_destination("abc123");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too short"));
    }

    #[test]
    fn test_validate_destination_invalid_chars() {
        let dest = "!@#$%^&*()".repeat(50);
        let result = I2pDriver::validate_destination(&dest);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid characters"));
    }

    #[test]
    fn test_validate_destination_valid() {
        // Generate a valid-looking base64 destination (516+ chars)
        let dest = "A".repeat(516);
        let result = I2pDriver::validate_destination(&dest);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_sam_reply_ok() {
        let reply = "SESSION STATUS RESULT=OK DESTINATION=AAAA";
        let parsed = I2pDriver::parse_sam_reply(reply);
        assert_eq!(parsed.get("RESULT"), Some(&"OK".to_string()));
        assert_eq!(parsed.get("DESTINATION"), Some(&"AAAA".to_string()));
    }

    #[test]
    fn test_parse_sam_reply_error() {
        let reply = "SESSION STATUS RESULT=DUPLICATED_ID MESSAGE=already exists";
        let parsed = I2pDriver::parse_sam_reply(reply);
        assert_eq!(parsed.get("RESULT"), Some(&"DUPLICATED_ID".to_string()));
        assert_eq!(parsed.get("MESSAGE"), Some(&"already".to_string()));
    }

    #[tokio::test]
    async fn test_dial_missing_destination() {
        let driver = I2pDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("destination not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_destination() {
        let driver = I2pDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("i2p://short".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("too short")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_sam_bridge_unavailable() {
        let driver = I2pDriver::with_sam_addr("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("destination".to_string(), "A".repeat(520));
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(
                e.to_string()
                    .contains("failed to connect to I2P SAM bridge")
            ),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_invalid_sam_addr() {
        let driver = I2pDriver::with_sam_addr("127.0.0.1:19999");
        let result = driver.listen("127.0.0.1:19999").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_strip_i2p_prefix() {
        let dest_with_prefix = "i2p://AAAA";
        let stripped = dest_with_prefix
            .strip_prefix("i2p://")
            .unwrap_or(dest_with_prefix);
        assert_eq!(stripped, "AAAA");
    }
}
