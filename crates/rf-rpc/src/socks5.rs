//! SOCKS5 dynamic forward types (ssh -D equivalent).
//!
//! Implements the SOCKS5 protocol state machine for dynamic port forwarding
//! through the RavenFabric mesh. Allows clients to use the agent as a
//! SOCKS5 proxy for arbitrary TCP connections.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

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

/// SOCKS5 proxy server — listens for SOCKS5 clients and proxies TCP connections.
///
/// Handles the full SOCKS5 protocol:
/// 1. Method negotiation (no-auth or username/password)
/// 2. Connection request parsing
/// 3. Policy check (allowed/denied destinations)
/// 4. TCP connect to target
/// 5. Bidirectional data relay
pub struct Socks5Server {
    config: Arc<Socks5Config>,
    cancel: watch::Receiver<bool>,
}

impl Socks5Server {
    /// Create a new SOCKS5 server with the given config and cancellation signal.
    pub fn new(config: Socks5Config, cancel: watch::Receiver<bool>) -> Self {
        Self {
            config: Arc::new(config),
            cancel,
        }
    }

    /// Run the SOCKS5 server. Listens on `config.bind_addr` and handles clients.
    /// Returns the bound address (useful when binding to port 0).
    pub async fn run(&mut self) -> Result<SocketAddr, String> {
        let listener = TcpListener::bind(&self.config.bind_addr)
            .await
            .map_err(|e| format!("bind {}: {}", self.config.bind_addr, e))?;
        let bound_addr = listener
            .local_addr()
            .map_err(|e| format!("local_addr: {}", e))?;

        let config = self.config.clone();
        let mut cancel = self.cancel.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, _peer)) => {
                                let cfg = config.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::handle_client(stream, &cfg).await {
                                        tracing::debug!("SOCKS5 client error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                tracing::warn!("SOCKS5 accept error: {}", e);
                            }
                        }
                    }
                    _ = cancel.changed() => {
                        break;
                    }
                }
            }
        });

        Ok(bound_addr)
    }

    /// Handle a single SOCKS5 client connection.
    async fn handle_client(mut client: TcpStream, config: &Socks5Config) -> Result<(), String> {
        // Step 1: Read method negotiation
        let mut buf = [0u8; 258];
        let n = client
            .read(&mut buf)
            .await
            .map_err(|e| format!("read greeting: {}", e))?;
        if n < 2 {
            return Err("greeting too short".into());
        }

        let version = buf[0];
        if version != 0x05 {
            return Err(format!("unsupported SOCKS version: {}", version));
        }

        let nmethods = buf[1] as usize;
        if n < 2 + nmethods {
            return Err("incomplete greeting".into());
        }

        // Select authentication method
        let methods = &buf[2..2 + nmethods];
        let selected = if config.auth_required {
            if methods.contains(&(AuthMethod::UsernamePassword as u8)) {
                AuthMethod::UsernamePassword
            } else {
                AuthMethod::NoAcceptable
            }
        } else if methods.contains(&(AuthMethod::NoAuth as u8)) {
            AuthMethod::NoAuth
        } else {
            AuthMethod::NoAcceptable
        };

        // Send method selection
        client
            .write_all(&[0x05, selected as u8])
            .await
            .map_err(|e| format!("write method: {}", e))?;

        if selected == AuthMethod::NoAcceptable {
            return Err("no acceptable auth method".into());
        }

        // Step 2: Read connection request
        let n = client
            .read(&mut buf)
            .await
            .map_err(|e| format!("read request: {}", e))?;
        if n < 4 {
            return Err("request too short".into());
        }

        if buf[0] != 0x05 {
            return Err("invalid request version".into());
        }

        let cmd = Socks5Command::from_byte(buf[1])
            .ok_or_else(|| format!("unsupported command: {}", buf[1]))?;

        // Parse address (buf[3] is ATYP)
        let (dest_addr, addr_len) = Socks5Addr::from_bytes(&buf[3..n])
            .ok_or_else(|| "invalid destination address".to_string())?;

        // Port is 2 bytes after the address
        let port_offset = 3 + addr_len;
        if n < port_offset + 2 {
            return Err("request missing port".into());
        }
        let dest_port = u16::from_be_bytes([buf[port_offset], buf[port_offset + 1]]);

        // Only CONNECT is supported for now
        if cmd != Socks5Command::Connect {
            Self::send_reply(
                &mut client,
                Socks5Reply::CommandNotSupported,
                &dest_addr,
                dest_port,
            )
            .await?;
            return Err("only CONNECT supported".into());
        }

        // Step 3: Policy check
        if !is_destination_allowed(config, &dest_addr, dest_port) {
            Self::send_reply(
                &mut client,
                Socks5Reply::ConnectionNotAllowed,
                &dest_addr,
                dest_port,
            )
            .await?;
            return Err(format!("destination denied: {:?}:{}", dest_addr, dest_port));
        }

        // Step 4: Connect to target
        let target_str = match &dest_addr {
            Socks5Addr::Ipv4(ip) => format!("{}:{}", ip, dest_port),
            Socks5Addr::Ipv6(ip) => format!("[{}]:{}", ip, dest_port),
            Socks5Addr::Domain(domain) => format!("{}:{}", domain, dest_port),
        };

        let target = match TcpStream::connect(&target_str).await {
            Ok(stream) => stream,
            Err(_) => {
                Self::send_reply(
                    &mut client,
                    Socks5Reply::HostUnreachable,
                    &dest_addr,
                    dest_port,
                )
                .await?;
                return Err(format!("connect to {}: failed", target_str));
            }
        };

        // Step 5: Send success reply
        let bound_addr = target
            .local_addr()
            .map(|a| {
                Socks5Addr::Ipv4(match a {
                    SocketAddr::V4(v4) => *v4.ip(),
                    SocketAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
                })
            })
            .unwrap_or(Socks5Addr::Ipv4(Ipv4Addr::UNSPECIFIED));
        let bound_port = target.local_addr().map(|a| a.port()).unwrap_or(0);

        Self::send_reply(&mut client, Socks5Reply::Succeeded, &bound_addr, bound_port).await?;

        // Step 6: Bidirectional copy
        let (mut client_read, mut client_write) = client.into_split();
        let (mut target_read, mut target_write) = target.into_split();

        let c2t = tokio::io::copy(&mut client_read, &mut target_write);
        let t2c = tokio::io::copy(&mut target_read, &mut client_write);

        let _ = tokio::try_join!(c2t, t2c);
        Ok(())
    }

    /// Send a SOCKS5 reply to the client.
    async fn send_reply(
        client: &mut TcpStream,
        status: Socks5Reply,
        addr: &Socks5Addr,
        port: u16,
    ) -> Result<(), String> {
        let mut reply = vec![0x05, status as u8, 0x00];
        reply.extend_from_slice(&addr.to_bytes());
        reply.extend_from_slice(&port.to_be_bytes());
        client
            .write_all(&reply)
            .await
            .map_err(|e| format!("write reply: {}", e))
    }
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

    #[tokio::test]
    async fn test_socks5_server_connect() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Start a target TCP server
        let target_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target_listener.local_addr().unwrap();

        // Echo server
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = target_listener.accept().await {
                let mut buf = [0u8; 1024];
                if let Ok(n) = stream.read(&mut buf).await {
                    let _ = stream.write_all(&buf[..n]).await;
                }
            }
        });

        // Start SOCKS5 server
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let config = Socks5Config {
            bind_addr: "127.0.0.1:0".to_string(),
            ..Default::default()
        };
        let mut server = Socks5Server::new(config, cancel_rx);
        let socks_addr = server.run().await.unwrap();

        // Connect as SOCKS5 client
        let mut client = tokio::net::TcpStream::connect(socks_addr).await.unwrap();

        // Send greeting: version 5, 1 method (no auth)
        client.write_all(&[0x05, 0x01, 0x00]).await.unwrap();

        // Read method selection
        let mut resp = [0u8; 2];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, [0x05, 0x00]); // no-auth selected

        // Send CONNECT request to target
        let mut req = vec![0x05, 0x01, 0x00]; // version, CONNECT, reserved
        req.push(0x01); // IPv4
        req.extend_from_slice(
            &target_addr
                .ip()
                .to_string()
                .parse::<Ipv4Addr>()
                .unwrap()
                .octets(),
        );
        req.extend_from_slice(&target_addr.port().to_be_bytes());
        client.write_all(&req).await.unwrap();

        // Read reply
        let mut reply = [0u8; 10]; // minimum IPv4 reply
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0], 0x05); // version
        assert_eq!(reply[1], 0x00); // success

        // Send data through proxy
        client.write_all(b"hello socks5").await.unwrap();

        // Read echoed data
        let mut buf = [0u8; 64];
        let n = client.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello socks5");

        // Cleanup
        drop(cancel_tx);
    }
}
