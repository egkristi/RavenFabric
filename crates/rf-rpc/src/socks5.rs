//! SOCKS5 dynamic forward types (ssh -D equivalent).
//!
//! Implements the SOCKS5 protocol state machine for dynamic port forwarding
//! through the RavenFabric mesh. Allows clients to use the agent as a
//! SOCKS5 proxy for arbitrary TCP connections.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

/// SOCKS5 authentication method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// No authentication required.
    NoAuth = 0x00,
    /// Username/password authentication (RFC 1929).
    UsernamePassword = 0x02,
    /// No acceptable methods.
    NoAcceptable = 0xFF,
}

/// SOCKS5 command type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socks5Command {
    /// TCP connect to target.
    Connect = 0x01,
    /// TCP bind (listen).
    Bind = 0x02,
    /// UDP associate.
    UdpAssociate = 0x03,
}

impl Socks5Command {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(Self::Connect),
            0x02 => Some(Self::Bind),
            0x03 => Some(Self::UdpAssociate),
            _ => None,
        }
    }
}

/// SOCKS5 address type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Socks5Addr {
    /// IPv4 address.
    Ipv4(Ipv4Addr),
    /// Domain name (to be resolved by the proxy).
    Domain(String),
    /// IPv6 address.
    Ipv6(Ipv6Addr),
}

impl Socks5Addr {
    /// Address type byte for the SOCKS5 protocol.
    pub fn atyp(&self) -> u8 {
        match self {
            Self::Ipv4(_) => 0x01,
            Self::Domain(_) => 0x03,
            Self::Ipv6(_) => 0x06,
        }
    }

    /// Serialize to wire format bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Ipv4(addr) => {
                let mut bytes = vec![0x01];
                bytes.extend_from_slice(&addr.octets());
                bytes
            }
            Self::Domain(domain) => {
                let mut bytes = vec![0x03, domain.len() as u8];
                bytes.extend_from_slice(domain.as_bytes());
                bytes
            }
            Self::Ipv6(addr) => {
                let mut bytes = vec![0x04];
                bytes.extend_from_slice(&addr.octets());
                bytes
            }
        }
    }

    /// Parse from wire format bytes. Returns (addr, bytes_consumed).
    pub fn from_bytes(data: &[u8]) -> Option<(Self, usize)> {
        if data.is_empty() {
            return None;
        }
        match data[0] {
            0x01 => {
                if data.len() < 5 {
                    return None;
                }
                let addr = Ipv4Addr::new(data[1], data[2], data[3], data[4]);
                Some((Self::Ipv4(addr), 5))
            }
            0x03 => {
                if data.len() < 2 {
                    return None;
                }
                let len = data[1] as usize;
                if data.len() < 2 + len {
                    return None;
                }
                let domain = String::from_utf8(data[2..2 + len].to_vec()).ok()?;
                Some((Self::Domain(domain), 2 + len))
            }
            0x04 => {
                if data.len() < 17 {
                    return None;
                }
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&data[1..17]);
                let addr = Ipv6Addr::from(octets);
                Some((Self::Ipv6(addr), 17))
            }
            _ => None,
        }
    }
}

/// SOCKS5 reply status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socks5Reply {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    ConnectionNotAllowed = 0x02,
    NetworkUnreachable = 0x03,
    HostUnreachable = 0x04,
    ConnectionRefused = 0x05,
    TtlExpired = 0x06,
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}

/// SOCKS5 connection request (after handshake).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socks5Request {
    /// Command (Connect, Bind, UdpAssociate).
    pub command: Socks5Command,
    /// Destination address.
    pub dest_addr: Socks5Addr,
    /// Destination port.
    pub dest_port: u16,
}

/// State of the SOCKS5 protocol state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socks5State {
    /// Waiting for client greeting (method negotiation).
    AwaitingGreeting,
    /// Waiting for authentication.
    AwaitingAuth,
    /// Waiting for connection request.
    AwaitingRequest,
    /// Connecting to target.
    Connecting,
    /// Proxying data bidirectionally.
    Established,
    /// Connection closed.
    Closed,
    /// Error state.
    Failed,
}

/// SOCKS5 proxy session statistics.
#[derive(Debug, Clone, Default)]
pub struct Socks5Stats {
    /// Total connections handled.
    pub connections_total: u64,
    /// Currently active connections.
    pub connections_active: u32,
    /// Bytes sent to target.
    pub bytes_sent: u64,
    /// Bytes received from target.
    pub bytes_received: u64,
    /// Connection failures.
    pub failures: u64,
    /// Connections denied by policy.
    pub denied: u64,
}

/// SOCKS5 proxy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Socks5Config {
    /// Local address to bind SOCKS5 listener.
    pub bind_addr: String,
    /// Authentication required.
    pub auth_required: bool,
    /// Allowed destination patterns (empty = allow all).
    pub allowed_destinations: Vec<String>,
    /// Denied destination patterns.
    pub denied_destinations: Vec<String>,
    /// Maximum concurrent connections.
    pub max_connections: u32,
}

impl Default for Socks5Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:1080".to_string(),
            auth_required: false,
            allowed_destinations: Vec::new(),
            denied_destinations: Vec::new(),
            max_connections: 256,
        }
    }
}

/// Check if a destination is allowed by the SOCKS5 policy.
pub fn is_destination_allowed(config: &Socks5Config, dest: &Socks5Addr, port: u16) -> bool {
    let dest_str = match dest {
        Socks5Addr::Ipv4(ip) => format!("{}:{}", ip, port),
        Socks5Addr::Ipv6(ip) => format!("[{}]:{}", ip, port),
        Socks5Addr::Domain(d) => format!("{}:{}", d, port),
    };

    // Check denied first (deny-by-default principle)
    for pattern in &config.denied_destinations {
        if dest_str.contains(pattern) {
            return false;
        }
    }

    // If allowed list is empty, allow all (that aren't denied)
    if config.allowed_destinations.is_empty() {
        return true;
    }

    // Check allowed
    config
        .allowed_destinations
        .iter()
        .any(|pattern| dest_str.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socks5_addr_ipv4_roundtrip() {
        let addr = Socks5Addr::Ipv4(Ipv4Addr::new(192, 168, 1, 1));
        let bytes = addr.to_bytes();
        let (parsed, consumed) = Socks5Addr::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, addr);
        assert_eq!(consumed, 5);
    }

    #[test]
    fn test_socks5_addr_domain_roundtrip() {
        let addr = Socks5Addr::Domain("example.com".to_string());
        let bytes = addr.to_bytes();
        let (parsed, consumed) = Socks5Addr::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, addr);
        assert_eq!(consumed, 2 + 11); // type + len + "example.com"
    }

    #[test]
    fn test_socks5_addr_ipv6_roundtrip() {
        let addr = Socks5Addr::Ipv6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        let bytes = addr.to_bytes();
        let (parsed, consumed) = Socks5Addr::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, addr);
        assert_eq!(consumed, 17);
    }

    #[test]
    fn test_socks5_command_from_byte() {
        assert_eq!(Socks5Command::from_byte(0x01), Some(Socks5Command::Connect));
        assert_eq!(Socks5Command::from_byte(0x02), Some(Socks5Command::Bind));
        assert_eq!(
            Socks5Command::from_byte(0x03),
            Some(Socks5Command::UdpAssociate)
        );
        assert_eq!(Socks5Command::from_byte(0x04), None);
    }

    #[test]
    fn test_destination_allowed_empty() {
        let config = Socks5Config::default();
        let dest = Socks5Addr::Domain("example.com".into());
        assert!(is_destination_allowed(&config, &dest, 443));
    }

    #[test]
    fn test_destination_denied() {
        let config = Socks5Config {
            denied_destinations: vec!["internal.corp".into()],
            ..Default::default()
        };
        let dest = Socks5Addr::Domain("secret.internal.corp".into());
        assert!(!is_destination_allowed(&config, &dest, 80));
    }

    #[test]
    fn test_destination_allowed_list() {
        let config = Socks5Config {
            allowed_destinations: vec!["example.com".into()],
            ..Default::default()
        };
        let allowed = Socks5Addr::Domain("example.com".into());
        let blocked = Socks5Addr::Domain("other.com".into());
        assert!(is_destination_allowed(&config, &allowed, 443));
        assert!(!is_destination_allowed(&config, &blocked, 443));
    }

    #[test]
    fn test_atyp() {
        assert_eq!(Socks5Addr::Ipv4(Ipv4Addr::LOCALHOST).atyp(), 0x01);
        assert_eq!(Socks5Addr::Domain("x".into()).atyp(), 0x03);
        assert_eq!(Socks5Addr::Ipv6(Ipv6Addr::LOCALHOST).atyp(), 0x06);
    }
}
