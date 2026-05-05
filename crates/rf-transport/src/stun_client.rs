//! Real STUN client implementation (RFC 5389/8489).
//!
//! Sends actual UDP binding requests to discover the server-reflexive address
//! and detect NAT type.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::nat::{NatType, StunBinding, StunServer};

/// Default STUN timeout per request.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// STUN message header length.
const STUN_HEADER_LEN: usize = 20;

/// STUN magic cookie (RFC 5389).
const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;

/// STUN message types.
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;

/// STUN attribute types.
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Error types for STUN operations.
#[derive(Debug, thiserror::Error)]
pub enum StunError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timeout waiting for STUN response")]
    Timeout,
    #[error("invalid STUN response: {0}")]
    InvalidResponse(String),
    #[error("no mapped address in response")]
    NoMappedAddress,
}

/// Configuration for the STUN client.
#[derive(Debug, Clone)]
pub struct StunClientConfig {
    /// Timeout for each individual request.
    pub timeout: Duration,
    /// Number of retries on timeout.
    pub retries: u8,
    /// STUN servers to query.
    pub servers: Vec<StunServer>,
}

impl Default for StunClientConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            retries: 2,
            servers: vec![
                StunServer {
                    addr: "stun.l.google.com:19302".parse().unwrap_or_else(|_| {
                        SocketAddr::new(std::net::Ipv4Addr::new(74, 125, 250, 129).into(), 19302)
                    }),
                    alt_addr: None,
                },
                StunServer {
                    addr: SocketAddr::new(
                        std::net::Ipv4Addr::new(64, 233, 163, 127).into(),
                        19302,
                    ),
                    alt_addr: None,
                },
            ],
        }
    }
}

/// Build a STUN Binding Request packet.
fn build_binding_request(transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(STUN_HEADER_LEN);

    // Message type: Binding Request
    buf.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    // Message length: 0 (no attributes in basic request)
    buf.extend_from_slice(&0u16.to_be_bytes());
    // Magic cookie
    buf.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    // Transaction ID (12 bytes)
    buf.extend_from_slice(transaction_id);

    buf
}

/// Parse a STUN Binding Response and extract the mapped address.
fn parse_binding_response(
    data: &[u8],
    expected_txn_id: &[u8; 12],
) -> Result<SocketAddr, StunError> {
    if data.len() < STUN_HEADER_LEN {
        return Err(StunError::InvalidResponse("too short".to_string()));
    }

    // Check message type
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    if msg_type != BINDING_RESPONSE {
        return Err(StunError::InvalidResponse(format!(
            "expected binding response (0x0101), got 0x{:04x}",
            msg_type
        )));
    }

    // Check magic cookie
    let cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if cookie != STUN_MAGIC_COOKIE {
        return Err(StunError::InvalidResponse("invalid magic cookie".to_string()));
    }

    // Verify transaction ID
    if &data[8..20] != expected_txn_id {
        return Err(StunError::InvalidResponse(
            "transaction ID mismatch".to_string(),
        ));
    }

    // Parse message length
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    if data.len() < STUN_HEADER_LEN + msg_len {
        return Err(StunError::InvalidResponse("truncated message".to_string()));
    }

    // Parse attributes looking for XOR-MAPPED-ADDRESS or MAPPED-ADDRESS
    let mut offset = STUN_HEADER_LEN;
    let end = STUN_HEADER_LEN + msg_len;
    let mut mapped_addr: Option<SocketAddr> = None;

    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let attr_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        offset += 4;

        if offset + attr_len > end {
            break;
        }

        match attr_type {
            ATTR_XOR_MAPPED_ADDRESS => {
                if let Some(addr) = parse_xor_mapped_address(&data[offset..offset + attr_len], expected_txn_id) {
                    return Ok(addr); // XOR-MAPPED-ADDRESS takes priority
                }
            }
            ATTR_MAPPED_ADDRESS => {
                if mapped_addr.is_none() {
                    mapped_addr = parse_mapped_address(&data[offset..offset + attr_len]);
                }
            }
            _ => {} // Skip unknown attributes
        }

        // Attributes are padded to 4-byte boundary
        let padded_len = (attr_len + 3) & !3;
        offset += padded_len;
    }

    mapped_addr.ok_or(StunError::NoMappedAddress)
}

/// Parse XOR-MAPPED-ADDRESS attribute (RFC 5389 section 15.2).
fn parse_xor_mapped_address(data: &[u8], txn_id: &[u8; 12]) -> Option<SocketAddr> {
    if data.len() < 8 {
        return None;
    }

    let family = data[1];
    let xor_port = u16::from_be_bytes([data[2], data[3]]);
    let port = xor_port ^ (STUN_MAGIC_COOKIE >> 16) as u16;

    match family {
        0x01 => {
            // IPv4
            if data.len() < 8 {
                return None;
            }
            let xor_ip = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            let ip = xor_ip ^ STUN_MAGIC_COOKIE;
            let addr = std::net::Ipv4Addr::from(ip);
            Some(SocketAddr::new(addr.into(), port))
        }
        0x02 => {
            // IPv6
            if data.len() < 20 {
                return None;
            }
            let mut ip_bytes = [0u8; 16];
            ip_bytes.copy_from_slice(&data[4..20]);
            // XOR with magic cookie (first 4 bytes) + transaction ID (next 12 bytes)
            let cookie_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                ip_bytes[i] ^= cookie_bytes[i];
            }
            for i in 0..12 {
                ip_bytes[4 + i] ^= txn_id[i];
            }
            let addr = std::net::Ipv6Addr::from(ip_bytes);
            Some(SocketAddr::new(addr.into(), port))
        }
        _ => None,
    }
}

/// Parse MAPPED-ADDRESS attribute (RFC 5389 section 15.1).
fn parse_mapped_address(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 8 {
        return None;
    }

    let family = data[1];
    let port = u16::from_be_bytes([data[2], data[3]]);

    match family {
        0x01 => {
            // IPv4
            let addr = std::net::Ipv4Addr::new(data[4], data[5], data[6], data[7]);
            Some(SocketAddr::new(addr.into(), port))
        }
        0x02 => {
            // IPv6
            if data.len() < 20 {
                return None;
            }
            let mut ip_bytes = [0u8; 16];
            ip_bytes.copy_from_slice(&data[4..20]);
            let addr = std::net::Ipv6Addr::from(ip_bytes);
            Some(SocketAddr::new(addr.into(), port))
        }
        _ => None,
    }
}

/// Generate a random 12-byte transaction ID.
fn generate_transaction_id() -> [u8; 12] {
    let mut id = [0u8; 12];
    use rand::RngCore;
    rand::rng().fill_bytes(&mut id);
    id
}

/// Perform a single STUN binding request and return the result.
pub async fn stun_binding_request(
    socket: &UdpSocket,
    server: SocketAddr,
    request_timeout: Duration,
) -> Result<StunBinding, StunError> {
    let txn_id = generate_transaction_id();
    let request = build_binding_request(&txn_id);

    socket.send_to(&request, server).await?;
    let start = tokio::time::Instant::now();

    let mut buf = [0u8; 576]; // Minimum recommended STUN response buffer
    let (len, _from) = timeout(request_timeout, socket.recv_from(&mut buf))
        .await
        .map_err(|_| StunError::Timeout)?
        .map_err(StunError::Io)?;

    let rtt = start.elapsed();
    let mapped_addr = parse_binding_response(&buf[..len], &txn_id)?;

    let local_addr = socket.local_addr()?;

    Ok(StunBinding {
        local_addr,
        mapped_addr,
        server,
        rtt,
    })
}

/// Discover external address using STUN.
///
/// Tries multiple servers, returns the first successful binding.
pub async fn discover_external_address(
    config: &StunClientConfig,
) -> Result<StunBinding, StunError> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    for server in &config.servers {
        for attempt in 0..=config.retries {
            if attempt > 0 {
                debug!("STUN retry {} to {}", attempt, server.addr);
            }

            match stun_binding_request(&socket, server.addr, config.timeout).await {
                Ok(binding) => {
                    debug!(
                        "STUN binding: local={} mapped={} server={} rtt={:?}",
                        binding.local_addr, binding.mapped_addr, binding.server, binding.rtt
                    );
                    return Ok(binding);
                }
                Err(StunError::Timeout) => {
                    warn!("STUN timeout to {} (attempt {})", server.addr, attempt + 1);
                    continue;
                }
                Err(e) => {
                    warn!("STUN error from {}: {}", server.addr, e);
                    break; // Try next server
                }
            }
        }
    }

    Err(StunError::Timeout)
}

/// Detect NAT type by comparing bindings from multiple servers.
///
/// Requires at least two STUN servers. Sends binding requests from the same
/// local socket to different servers and compares the reflexive addresses.
pub async fn detect_nat_type(config: &StunClientConfig) -> Result<NatType, StunError> {
    if config.servers.len() < 2 {
        return Err(StunError::InvalidResponse(
            "need at least 2 STUN servers for NAT detection".to_string(),
        ));
    }

    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let local_addr = socket.local_addr()?;

    // Send binding request to first server
    let binding1 = stun_binding_request(&socket, config.servers[0].addr, config.timeout).await?;

    // Check if we're on a public IP (no NAT)
    if binding1.mapped_addr.ip() == local_addr.ip() {
        return Ok(NatType::Open);
    }

    // Send binding request to second server from same socket
    let binding2 = stun_binding_request(&socket, config.servers[1].addr, config.timeout).await?;

    // Compare mapped addresses
    if binding1.mapped_addr == binding2.mapped_addr {
        // Same mapped address from different servers = cone NAT
        // (Distinguishing full/restricted/port-restricted requires server-side tests
        // with CHANGE-REQUEST which most public STUN servers don't support)
        Ok(NatType::FullCone)
    } else if binding1.mapped_addr.ip() == binding2.mapped_addr.ip() {
        // Same IP but different port = port-restricted cone
        Ok(NatType::PortRestrictedCone)
    } else {
        // Different IP = symmetric NAT
        Ok(NatType::Symmetric)
    }
}

/// Gather ICE candidates (host + server-reflexive).
///
/// Returns host candidates from local interfaces and server-reflexive
/// candidates from STUN binding responses.
pub async fn gather_candidates(
    config: &StunClientConfig,
) -> Result<Vec<crate::nat::IceCandidate>, StunError> {
    use crate::nat::{CandidateTransport, CandidateType, IceCandidate};

    let mut candidates = Vec::new();

    // Host candidates from local interfaces
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let local_addr = socket.local_addr()?;

    // Add host candidate
    let host_priority = IceCandidate::compute_priority(
        IceCandidate::type_preference(&CandidateType::Host),
        65535,
        1,
    );
    candidates.push(IceCandidate {
        candidate_type: CandidateType::Host,
        transport: CandidateTransport::Udp,
        addr: local_addr,
        related_addr: None,
        priority: host_priority,
        foundation: "host-udp".to_string(),
    });

    // Server-reflexive candidates via STUN
    for server in &config.servers {
        match stun_binding_request(&socket, server.addr, config.timeout).await {
            Ok(binding) => {
                let srflx_priority = IceCandidate::compute_priority(
                    IceCandidate::type_preference(&CandidateType::ServerReflexive),
                    65535,
                    1,
                );
                candidates.push(IceCandidate {
                    candidate_type: CandidateType::ServerReflexive,
                    transport: CandidateTransport::Udp,
                    addr: binding.mapped_addr,
                    related_addr: Some(local_addr),
                    priority: srflx_priority,
                    foundation: format!("srflx-udp-{}", server.addr),
                });
                break; // One srflx candidate is enough
            }
            Err(e) => {
                debug!("STUN candidate gathering from {} failed: {}", server.addr, e);
                continue;
            }
        }
    }

    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_binding_request() {
        let txn_id = [1u8; 12];
        let request = build_binding_request(&txn_id);

        assert_eq!(request.len(), STUN_HEADER_LEN);
        // Message type: Binding Request
        assert_eq!(request[0], 0x00);
        assert_eq!(request[1], 0x01);
        // Message length: 0
        assert_eq!(request[2], 0x00);
        assert_eq!(request[3], 0x00);
        // Magic cookie
        assert_eq!(request[4], 0x21);
        assert_eq!(request[5], 0x12);
        assert_eq!(request[6], 0xA4);
        assert_eq!(request[7], 0x42);
        // Transaction ID
        assert_eq!(&request[8..20], &txn_id);
    }

    #[test]
    fn test_parse_xor_mapped_address_ipv4() {
        let txn_id = [0u8; 12];
        // XOR-MAPPED-ADDRESS: family=0x01, port=XOR'd, ip=XOR'd
        // Target: 203.0.113.1:5000
        let port: u16 = 5000;
        let ip: u32 = u32::from(std::net::Ipv4Addr::new(203, 0, 113, 1));
        let xor_port = port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
        let xor_ip = ip ^ STUN_MAGIC_COOKIE;

        let mut data = vec![0x00, 0x01]; // reserved + family
        data.extend_from_slice(&xor_port.to_be_bytes());
        data.extend_from_slice(&xor_ip.to_be_bytes());

        let addr = parse_xor_mapped_address(&data, &txn_id).unwrap();
        assert_eq!(addr.ip(), std::net::Ipv4Addr::new(203, 0, 113, 1));
        assert_eq!(addr.port(), 5000);
    }

    #[test]
    fn test_parse_mapped_address_ipv4() {
        // MAPPED-ADDRESS: family=0x01, port, ip
        let mut data = vec![0x00, 0x01]; // reserved + family
        data.extend_from_slice(&1234u16.to_be_bytes()); // port
        data.extend_from_slice(&[192, 168, 1, 100]); // ip

        let addr = parse_mapped_address(&data).unwrap();
        assert_eq!(addr.ip(), std::net::Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(addr.port(), 1234);
    }

    #[test]
    fn test_parse_binding_response_valid() {
        let txn_id = [0x42u8; 12];
        // Build a valid binding response with XOR-MAPPED-ADDRESS
        let mut response = Vec::new();
        // Header
        response.extend_from_slice(&BINDING_RESPONSE.to_be_bytes()); // type
        // Attributes: XOR-MAPPED-ADDRESS (type=0x0020, len=8)
        let attr_type = ATTR_XOR_MAPPED_ADDRESS;
        let attr_len: u16 = 8;
        let port: u16 = 9000;
        let ip: u32 = u32::from(std::net::Ipv4Addr::new(1, 2, 3, 4));
        let xor_port = port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
        let xor_ip = ip ^ STUN_MAGIC_COOKIE;

        let msg_len: u16 = 4 + attr_len; // attr header + attr value
        response.extend_from_slice(&msg_len.to_be_bytes()); // message length
        response.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes()); // cookie
        response.extend_from_slice(&txn_id); // transaction ID

        // XOR-MAPPED-ADDRESS attribute
        response.extend_from_slice(&attr_type.to_be_bytes());
        response.extend_from_slice(&attr_len.to_be_bytes());
        response.push(0x00); // reserved
        response.push(0x01); // family: IPv4
        response.extend_from_slice(&xor_port.to_be_bytes());
        response.extend_from_slice(&xor_ip.to_be_bytes());

        let addr = parse_binding_response(&response, &txn_id).unwrap();
        assert_eq!(addr, SocketAddr::new(std::net::Ipv4Addr::new(1, 2, 3, 4).into(), 9000));
    }

    #[test]
    fn test_parse_binding_response_wrong_txn_id() {
        let txn_id = [0x42u8; 12];
        let wrong_id = [0x99u8; 12];

        let mut response = Vec::new();
        response.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
        response.extend_from_slice(&0u16.to_be_bytes()); // msg len = 0
        response.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        response.extend_from_slice(&wrong_id);

        let err = parse_binding_response(&response, &txn_id).unwrap_err();
        assert!(matches!(err, StunError::InvalidResponse(_)));
    }

    #[test]
    fn test_parse_binding_response_too_short() {
        let txn_id = [0u8; 12];
        let err = parse_binding_response(&[0; 10], &txn_id).unwrap_err();
        assert!(matches!(err, StunError::InvalidResponse(_)));
    }

    #[test]
    fn test_generate_transaction_id_unique() {
        let id1 = generate_transaction_id();
        let id2 = generate_transaction_id();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_stun_binding_request_timeout() {
        // Bind to a random port and try to reach an unreachable address
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        // Use a non-routable address to ensure timeout
        let fake_server: SocketAddr = "192.0.2.1:3478".parse().unwrap();

        let result =
            stun_binding_request(&socket, fake_server, Duration::from_millis(100)).await;
        // May be Timeout or Io error depending on OS network stack
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stun_loopback_echo() {
        // Simulate a STUN response by having a local "server" that echoes back a valid response
        let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_socket.local_addr().unwrap();

        let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_socket.local_addr().unwrap();

        // Spawn a task that reads the request and sends back a valid response
        let handle = tokio::spawn(async move {
            let mut buf = [0u8; 576];
            let (len, from) = server_socket.recv_from(&mut buf).await.unwrap();
            assert!(len >= STUN_HEADER_LEN);

            // Extract transaction ID from request
            let mut txn_id = [0u8; 12];
            txn_id.copy_from_slice(&buf[8..20]);

            // Build a valid binding response
            let port = from.port();
            let xor_port = port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
            let ip: u32 = match from.ip() {
                std::net::IpAddr::V4(v4) => u32::from(v4),
                _ => panic!("expected IPv4"),
            };
            let xor_ip = ip ^ STUN_MAGIC_COOKIE;

            let mut response = Vec::new();
            response.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
            let msg_len: u16 = 12; // attr header (4) + attr value (8)
            response.extend_from_slice(&msg_len.to_be_bytes());
            response.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            response.extend_from_slice(&txn_id);

            // XOR-MAPPED-ADDRESS
            response.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
            response.extend_from_slice(&8u16.to_be_bytes());
            response.push(0x00); // reserved
            response.push(0x01); // family IPv4
            response.extend_from_slice(&xor_port.to_be_bytes());
            response.extend_from_slice(&xor_ip.to_be_bytes());

            server_socket.send_to(&response, from).await.unwrap();
        });

        let binding =
            stun_binding_request(&client_socket, server_addr, Duration::from_secs(2)).await
                .unwrap();

        handle.await.unwrap();

        // The "server" should reflect back the client's address
        assert_eq!(binding.mapped_addr, client_addr);
        assert_eq!(binding.server, server_addr);
        assert!(binding.rtt < Duration::from_secs(1));
    }
}
