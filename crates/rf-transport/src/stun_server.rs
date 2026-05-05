//! Lightweight STUN server (RFC 5389/8489).
//!
//! Responds to STUN Binding Requests with the client's server-reflexive
//! address (XOR-MAPPED-ADDRESS). Used for self-hosted NAT detection
//! without relying on third-party STUN servers.

use std::net::SocketAddr;

use tokio::net::UdpSocket;
use tracing::{debug, warn};

/// STUN constants (shared with stun_client).
const STUN_HEADER_LEN: usize = 20;
const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// STUN server configuration.
#[derive(Debug, Clone)]
pub struct StunServerConfig {
    /// Address to listen on.
    pub listen_addr: SocketAddr,
}

impl Default for StunServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3478".parse().expect("valid default addr"),
        }
    }
}

/// A lightweight STUN server that responds to Binding Requests.
pub struct StunServer {
    socket: UdpSocket,
}

impl StunServer {
    /// Bind the STUN server to the configured address.
    pub async fn bind(config: &StunServerConfig) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(config.listen_addr).await?;
        debug!("STUN server listening on {}", config.listen_addr);
        Ok(Self { socket })
    }

    /// Create from an existing socket (useful for tests).
    pub fn from_socket(socket: UdpSocket) -> Self {
        Self { socket }
    }

    /// Get the local address the server is bound to.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Handle a single incoming STUN request and send a response.
    ///
    /// Returns `Ok(Some(client_addr))` if a valid Binding Request was processed,
    /// `Ok(None)` if the packet was not a valid STUN request, or an IO error.
    pub async fn handle_one(&self) -> std::io::Result<Option<SocketAddr>> {
        let mut buf = [0u8; 576];
        let (len, client_addr) = self.socket.recv_from(&mut buf).await?;

        let data = &buf[..len];

        // Validate minimum STUN header
        if len < STUN_HEADER_LEN {
            debug!("Ignoring non-STUN packet from {}", client_addr);
            return Ok(None);
        }

        // Check message type
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type != BINDING_REQUEST {
            debug!(
                "Ignoring non-binding-request (0x{:04x}) from {}",
                msg_type, client_addr
            );
            return Ok(None);
        }

        // Check magic cookie
        let cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if cookie != STUN_MAGIC_COOKIE {
            warn!("Invalid STUN magic cookie from {}", client_addr);
            return Ok(None);
        }

        // Extract transaction ID
        let mut txn_id = [0u8; 12];
        txn_id.copy_from_slice(&data[8..20]);

        // Build and send response
        let response = build_binding_response(&txn_id, client_addr);
        self.socket.send_to(&response, client_addr).await?;

        debug!(
            "STUN response: {} -> XOR-MAPPED-ADDRESS={}",
            client_addr, client_addr
        );
        Ok(Some(client_addr))
    }
}

/// Build a STUN Binding Response with XOR-MAPPED-ADDRESS.
fn build_binding_response(txn_id: &[u8; 12], mapped_addr: SocketAddr) -> Vec<u8> {
    let attr_data = build_xor_mapped_address(mapped_addr, txn_id);
    let attr_len = attr_data.len();

    // Total attribute TLV: 4 bytes header + attr_data
    let msg_len = 4 + attr_len;

    let mut buf = Vec::with_capacity(STUN_HEADER_LEN + msg_len);

    // Header
    buf.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
    buf.extend_from_slice(&(msg_len as u16).to_be_bytes());
    buf.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    buf.extend_from_slice(txn_id);

    // XOR-MAPPED-ADDRESS attribute
    buf.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    buf.extend_from_slice(&(attr_len as u16).to_be_bytes());
    buf.extend_from_slice(&attr_data);

    buf
}

/// Build XOR-MAPPED-ADDRESS attribute value.
fn build_xor_mapped_address(addr: SocketAddr, txn_id: &[u8; 12]) -> Vec<u8> {
    match addr {
        SocketAddr::V4(v4) => {
            let mut data = vec![0u8; 8]; // 1 reserved + 1 family + 2 port + 4 ip
            data[1] = 0x01; // IPv4 family
            let xor_port = addr.port() ^ (STUN_MAGIC_COOKIE >> 16) as u16;
            data[2..4].copy_from_slice(&xor_port.to_be_bytes());
            let ip_u32 = u32::from_be_bytes(v4.ip().octets());
            let xor_ip = ip_u32 ^ STUN_MAGIC_COOKIE;
            data[4..8].copy_from_slice(&xor_ip.to_be_bytes());
            data
        }
        SocketAddr::V6(v6) => {
            let mut data = vec![0u8; 20]; // 1 reserved + 1 family + 2 port + 16 ip
            data[1] = 0x02; // IPv6 family
            let xor_port = addr.port() ^ (STUN_MAGIC_COOKIE >> 16) as u16;
            data[2..4].copy_from_slice(&xor_port.to_be_bytes());
            let mut ip_bytes = v6.ip().octets();
            let cookie_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                ip_bytes[i] ^= cookie_bytes[i];
            }
            for i in 0..12 {
                ip_bytes[4 + i] ^= txn_id[i];
            }
            data[4..20].copy_from_slice(&ip_bytes);
            data
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stun_client;

    #[test]
    fn test_default_config() {
        let config = StunServerConfig::default();
        assert_eq!(config.listen_addr.port(), 3478);
    }

    #[test]
    fn test_build_binding_response_ipv4() {
        let txn_id = [1u8; 12];
        let addr: SocketAddr = "192.168.1.1:12345".parse().unwrap();
        let response = build_binding_response(&txn_id, addr);

        // Verify header
        assert!(response.len() >= STUN_HEADER_LEN);
        let msg_type = u16::from_be_bytes([response[0], response[1]]);
        assert_eq!(msg_type, BINDING_RESPONSE);

        let cookie = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);
        assert_eq!(cookie, STUN_MAGIC_COOKIE);

        assert_eq!(&response[8..20], &txn_id);
    }

    #[test]
    fn test_build_binding_response_ipv6() {
        let txn_id = [0xAB; 12];
        let addr: SocketAddr = "[::1]:8080".parse().unwrap();
        let response = build_binding_response(&txn_id, addr);

        assert!(response.len() >= STUN_HEADER_LEN);
        let msg_type = u16::from_be_bytes([response[0], response[1]]);
        assert_eq!(msg_type, BINDING_RESPONSE);
    }

    #[test]
    fn test_xor_mapped_address_roundtrip_ipv4() {
        let txn_id = [0x42; 12];
        let original: SocketAddr = "10.0.0.1:55555".parse().unwrap();
        let encoded = build_xor_mapped_address(original, &txn_id);

        // Decode: XOR back
        let xor_port = u16::from_be_bytes([encoded[2], encoded[3]]);
        let port = xor_port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
        assert_eq!(port, 55555);

        let xor_ip = u32::from_be_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]);
        let ip = xor_ip ^ STUN_MAGIC_COOKIE;
        let addr = std::net::Ipv4Addr::from(ip);
        assert_eq!(addr, std::net::Ipv4Addr::new(10, 0, 0, 1));
    }

    #[tokio::test]
    async fn test_stun_server_client_roundtrip() {
        // Start server on random port
        let server_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_sock.local_addr().unwrap();
        let server = StunServer::from_socket(server_sock);

        // Client sends binding request
        let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_local = client_sock.local_addr().unwrap();

        // Spawn server handler
        let handle = tokio::spawn(async move {
            let result = server.handle_one().await.unwrap();
            assert!(result.is_some());
            result.unwrap()
        });

        // Send a binding request using the client
        let binding = stun_client::stun_binding_request(
            &client_sock,
            server_addr,
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();

        let served_client = handle.await.unwrap();
        assert_eq!(served_client, client_local);
        assert_eq!(binding.mapped_addr, client_local);
        assert_eq!(binding.server, server_addr);
    }

    #[tokio::test]
    async fn test_stun_server_ignores_non_stun() {
        let server_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_sock.local_addr().unwrap();
        let server = StunServer::from_socket(server_sock);

        // Send garbage data
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(b"not stun data", server_addr).await.unwrap();

        let result = server.handle_one().await.unwrap();
        assert!(result.is_none()); // Should be ignored
    }
}
