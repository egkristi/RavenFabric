//! Veilid transport driver via Veilid API.
//!
//! Connects to Veilid destinations through the local Veilid daemon's
//! JSON API. Veilid provides DHT-based, onion-routed networking with
//! privacy-first design (no IP addresses exposed).
//!
//! # Requirements
//!
//! - Veilid daemon (`veilid-server`) running locally with API enabled
//! - API endpoint accessible (default: `127.0.0.1:5959`)
//!
//! # Configuration
//!
//! Driver config keys:
//! - `api_endpoint`: Veilid API address (default: `127.0.0.1:5959`)
//! - `route_id`: Target route ID (DHTKey, 64 hex chars)
//! - `privacy_route`: Use privacy routing (default: `true`)
//! - `app_call_id`: Application identifier for routing

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default Veilid API endpoint.
const DEFAULT_API_ENDPOINT: &str = "127.0.0.1:5959";

/// Veilid DHT key length in hex characters.
const DHT_KEY_HEX_LEN: usize = 64;

/// Transport driver that routes connections through the Veilid network.
pub struct VeilidDriver {
    /// Veilid API endpoint (e.g., "127.0.0.1:5959").
    api_endpoint: String,
}

impl VeilidDriver {
    /// Create a new Veilid driver using the default API endpoint.
    pub fn new() -> Self {
        Self {
            api_endpoint: DEFAULT_API_ENDPOINT.to_string(),
        }
    }

    /// Create a new Veilid driver with a custom API endpoint.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            api_endpoint: endpoint.into(),
        }
    }

    /// Validate a Veilid route ID (DHTKey format: 64 hex characters).
    fn validate_route_id(route_id: &str) -> Result<(), TransportError> {
        if route_id.len() != DHT_KEY_HEX_LEN {
            return Err(TransportError::Connection(format!(
                "Veilid route ID must be {} hex characters, got {}",
                DHT_KEY_HEX_LEN,
                route_id.len()
            )));
        }

        if !route_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TransportError::Connection(
                "Veilid route ID must be hexadecimal characters only".to_string(),
            ));
        }

        Ok(())
    }

    /// Send a JSON-RPC request to the Veilid API and read the response.
    async fn api_call(
        stream: &mut BufReader<TcpStream>,
        method: &str,
        params: &str,
    ) -> Result<String, TransportError> {
        let request = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"{method}\",\"params\":{params}}}\n"
        );

        stream
            .get_mut()
            .write_all(request.as_bytes())
            .await
            .map_err(|e| TransportError::Connection(format!("Veilid API write failed: {e}")))?;

        let mut response = String::new();
        stream
            .read_line(&mut response)
            .await
            .map_err(|e| TransportError::Connection(format!("Veilid API read failed: {e}")))?;

        // Check for JSON-RPC error
        if response.contains("\"error\"") && !response.contains("\"error\":null") {
            return Err(TransportError::Connection(format!(
                "Veilid API error: {}",
                response.trim()
            )));
        }

        Ok(response)
    }

    /// Attach to the Veilid network.
    async fn attach(stream: &mut BufReader<TcpStream>) -> Result<(), TransportError> {
        let response = Self::api_call(stream, "attach", "[]").await?;

        if response.contains("\"error\"") && !response.contains("\"error\":null") {
            return Err(TransportError::Connection(format!(
                "Veilid attach failed: {}",
                response.trim()
            )));
        }

        Ok(())
    }

    /// Create a private route for receiving connections.
    async fn create_private_route(
        stream: &mut BufReader<TcpStream>,
    ) -> Result<String, TransportError> {
        let response = Self::api_call(stream, "new_private_route", "[]").await?;

        // Extract route_id from response
        // Response format: {"jsonrpc":"2.0","id":1,"result":{"route_id":"<hex>",...}}
        if let Some(start) = response.find("\"route_id\":\"") {
            let after = &response[start + 12..];
            if let Some(end) = after.find('"') {
                return Ok(after[..end].to_string());
            }
        }

        Err(TransportError::Connection(
            "failed to parse route_id from Veilid API response".to_string(),
        ))
    }

    /// Initiate an app-call to a remote route (establishes a data channel).
    async fn app_call(
        stream: &mut BufReader<TcpStream>,
        route_id: &str,
        message: &[u8],
    ) -> Result<Vec<u8>, TransportError> {
        let encoded = base64_encode(message);
        let params = format!("[\"{route_id}\",\"{encoded}\"]");
        let response = Self::api_call(stream, "app_call", &params).await?;

        // Extract result (base64-encoded response bytes)
        if let Some(start) = response.find("\"result\":\"") {
            let after = &response[start + 10..];
            if let Some(end) = after.find('"') {
                let decoded = base64_decode(&after[..end])?;
                return Ok(decoded);
            }
        }

        Err(TransportError::Connection(
            "failed to parse app_call response from Veilid API".to_string(),
        ))
    }
}

impl Default for VeilidDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple base64 encoding (no external dependency).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Simple base64 decoding.
fn base64_decode(input: &str) -> Result<Vec<u8>, TransportError> {
    fn char_to_val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);

    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let vals: Vec<u8> = chunk.iter().map(|&b| char_to_val(b).unwrap_or(0)).collect();

        let triple = ((vals[0] as u32) << 18)
            | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6)
            | (vals[3] as u32);

        result.push((triple >> 16) as u8);
        if chunk[2] != b'=' {
            result.push((triple >> 8) as u8);
        }
        if chunk[3] != b'=' {
            result.push(triple as u8);
        }
    }

    Ok(result)
}

#[async_trait::async_trait]
impl Driver for VeilidDriver {
    fn name(&self) -> &str {
        "veilid"
    }

    fn available(&self) -> bool {
        self.api_endpoint.parse::<std::net::SocketAddr>().is_ok()
    }

    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let api_endpoint = config
            .get("api_endpoint")
            .cloned()
            .unwrap_or_else(|| self.api_endpoint.clone());

        // Get route ID from config or target
        let route_id = config
            .get("route_id")
            .or(target.relay_url.as_ref())
            .ok_or_else(|| {
                TransportError::Connection(
                    "Veilid route_id not specified (set 'route_id' in config or relay_url)"
                        .to_string(),
                )
            })?
            .clone();

        // Strip veilid:// prefix if present
        let route_id = route_id
            .strip_prefix("veilid://")
            .unwrap_or(&route_id)
            .to_string();

        Self::validate_route_id(&route_id)?;

        let endpoint_addr: std::net::SocketAddr = api_endpoint
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid API endpoint: {e}")))?;

        // Connect to Veilid API
        let tcp = TcpStream::connect(endpoint_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Veilid API at {api_endpoint}: {e}"
            ))
        })?;

        let mut stream = BufReader::new(tcp);

        // Attach to the network
        Self::attach(&mut stream).await?;

        // Initiate connection via app_call (sends a handshake message)
        let handshake_msg = b"RVNF_CONNECT";
        let response = Self::app_call(&mut stream, &route_id, handshake_msg).await?;

        if response != b"RVNF_ACCEPT" {
            return Err(TransportError::Connection(
                "Veilid peer did not accept connection (invalid handshake response)".to_string(),
            ));
        }

        // After successful handshake, the API connection becomes our data channel
        Ok(Box::new(stream.into_inner()))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let api_endpoint = if addr.is_empty() {
            self.api_endpoint.clone()
        } else {
            addr.to_string()
        };

        let endpoint_addr: std::net::SocketAddr = api_endpoint
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid API endpoint: {e}")))?;

        // Connect to Veilid API and create a private route
        let tcp = TcpStream::connect(endpoint_addr).await.map_err(|e| {
            TransportError::Connection(format!(
                "failed to connect to Veilid API at {api_endpoint}: {e}"
            ))
        })?;

        let mut stream = BufReader::new(tcp);
        Self::attach(&mut stream).await?;
        let route_id = Self::create_private_route(&mut stream).await?;

        Ok(Box::new(VeilidListener {
            api_endpoint,
            route_id,
        }))
    }
}

/// Listener that accepts incoming Veilid connections via app_call.
struct VeilidListener {
    api_endpoint: String,
    #[allow(dead_code)]
    route_id: String,
}

#[async_trait::async_trait]
impl Listener for VeilidListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let endpoint_addr: std::net::SocketAddr = self
            .api_endpoint
            .parse()
            .map_err(|e| TransportError::Connection(format!("invalid API endpoint: {e}")))?;

        // Connect to API and wait for incoming app_call
        let tcp = TcpStream::connect(endpoint_addr).await.map_err(|e| {
            TransportError::Connection(format!("failed to connect to Veilid API for accept: {e}"))
        })?;

        let mut stream = BufReader::new(tcp);
        VeilidDriver::attach(&mut stream).await?;

        // Wait for incoming connection (blocking read for app_call event)
        let mut line = String::new();
        stream
            .read_line(&mut line)
            .await
            .map_err(|e| TransportError::Connection(format!("Veilid accept read failed: {e}")))?;

        // Verify it's a RavenFabric connection request
        if !line.contains("RVNF_CONNECT") {
            return Err(TransportError::Connection(
                "received non-RavenFabric connection on Veilid route".to_string(),
            ));
        }

        Ok(Box::new(stream.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = VeilidDriver::new();
        assert_eq!(driver.name(), "veilid");
    }

    #[test]
    fn test_default_endpoint() {
        let driver = VeilidDriver::new();
        assert_eq!(driver.api_endpoint, "127.0.0.1:5959");
    }

    #[test]
    fn test_custom_endpoint() {
        let driver = VeilidDriver::with_endpoint("10.0.0.1:6060");
        assert_eq!(driver.api_endpoint, "10.0.0.1:6060");
    }

    #[test]
    fn test_available_valid() {
        let driver = VeilidDriver::new();
        assert!(driver.available());
    }

    #[test]
    fn test_available_invalid() {
        let driver = VeilidDriver::with_endpoint("not-valid");
        assert!(!driver.available());
    }

    #[test]
    fn test_validate_route_id_valid() {
        let route_id = "a".repeat(64);
        assert!(VeilidDriver::validate_route_id(&route_id).is_ok());
    }

    #[test]
    fn test_validate_route_id_too_short() {
        let result = VeilidDriver::validate_route_id("abc123");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("64 hex characters"));
    }

    #[test]
    fn test_validate_route_id_invalid_chars() {
        let route_id = "g".repeat(64); // 'g' is not hex
        let result = VeilidDriver::validate_route_id(&route_id);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("hexadecimal"));
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Hello, Veilid!";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_empty() {
        let data = b"";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[tokio::test]
    async fn test_dial_missing_route_id() {
        let driver = VeilidDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("route_id not specified")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_invalid_route_id() {
        let driver = VeilidDriver::new();
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: Some("veilid://short".to_string()),
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("64 hex characters")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_dial_api_unavailable() {
        let driver = VeilidDriver::with_endpoint("127.0.0.1:19998");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let mut config = DriverConfig::new();
        config.insert("route_id".to_string(), "a".repeat(64));
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to Veilid API")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_api_unavailable() {
        let driver = VeilidDriver::with_endpoint("127.0.0.1:19998");
        let result = driver.listen("127.0.0.1:19998").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_strip_veilid_prefix() {
        let route = "veilid://abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let stripped = route.strip_prefix("veilid://").unwrap_or(route);
        assert_eq!(stripped.len(), 64);
    }
}
