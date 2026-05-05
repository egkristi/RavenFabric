//! Lightweight TURN relay (RFC 5766) for NAT traversal.
//!
//! Provides UDP relay allocations when direct peer-to-peer connections
//! fail. Each allocation binds a relay port and forwards data between
//! the client and the peer.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tracing::{debug, warn};

/// Allocation state for a TURN client.
#[derive(Debug)]
pub struct Allocation {
    /// Client's transport address.
    pub client_addr: SocketAddr,
    /// Relayed transport address (our side).
    pub relay_addr: SocketAddr,
    /// Peer permissions (addresses allowed to send/receive).
    pub permissions: Vec<SocketAddr>,
    /// When the allocation was created.
    pub created_at: Instant,
    /// Allocation lifetime.
    pub lifetime: Duration,
    /// Bytes relayed client → peer.
    pub bytes_relayed_out: u64,
    /// Bytes relayed peer → client.
    pub bytes_relayed_in: u64,
}

impl Allocation {
    /// Check if this allocation has expired.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.lifetime
    }

    /// Check if a peer address has permission.
    pub fn has_permission(&self, peer: &SocketAddr) -> bool {
        self.permissions.iter().any(|p| p.ip() == peer.ip())
    }
}

/// TURN relay server that manages UDP allocations.
pub struct TurnRelay {
    /// Control socket for receiving TURN requests.
    socket: UdpSocket,
    /// Active allocations keyed by client address.
    allocations: HashMap<SocketAddr, Allocation>,
    /// Relay sockets keyed by client address.
    relay_sockets: HashMap<SocketAddr, UdpSocket>,
    /// Default allocation lifetime.
    default_lifetime: Duration,
    /// Maximum allocations.
    max_allocations: usize,
}

impl TurnRelay {
    /// Create a new TURN relay bound to the given socket.
    pub fn from_socket(socket: UdpSocket, max_allocations: usize) -> Self {
        Self {
            socket,
            allocations: HashMap::new(),
            relay_sockets: HashMap::new(),
            default_lifetime: Duration::from_secs(600),
            max_allocations,
        }
    }

    /// Get the control socket's local address.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Create an allocation for a client.
    /// Returns the relay address assigned, or None if at capacity.
    pub async fn allocate(
        &mut self,
        client_addr: SocketAddr,
    ) -> std::io::Result<Option<SocketAddr>> {
        // Check capacity
        if self.allocations.len() >= self.max_allocations {
            warn!("TURN allocation denied: at capacity");
            return Ok(None);
        }

        // Check for existing allocation
        if self.allocations.contains_key(&client_addr) {
            let existing = &self.allocations[&client_addr];
            return Ok(Some(existing.relay_addr));
        }

        // Bind a relay socket on a random port
        let relay_sock = UdpSocket::bind("0.0.0.0:0").await?;
        let relay_addr = relay_sock.local_addr()?;

        debug!(
            "TURN allocation: client={} relay={}",
            client_addr, relay_addr
        );

        self.allocations.insert(
            client_addr,
            Allocation {
                client_addr,
                relay_addr,
                permissions: Vec::new(),
                created_at: Instant::now(),
                lifetime: self.default_lifetime,
                bytes_relayed_out: 0,
                bytes_relayed_in: 0,
            },
        );
        self.relay_sockets.insert(client_addr, relay_sock);

        Ok(Some(relay_addr))
    }

    /// Add a permission for a peer to an allocation.
    pub fn add_permission(&mut self, client_addr: &SocketAddr, peer_addr: SocketAddr) -> bool {
        if let Some(alloc) = self.allocations.get_mut(client_addr) {
            if !alloc.has_permission(&peer_addr) {
                alloc.permissions.push(peer_addr);
            }
            true
        } else {
            false
        }
    }

    /// Relay data from client to peer through the allocation.
    pub async fn relay_to_peer(
        &mut self,
        client_addr: &SocketAddr,
        peer_addr: SocketAddr,
        data: &[u8],
    ) -> std::io::Result<bool> {
        // Check allocation exists and peer has permission
        let has_perm = self
            .allocations
            .get(client_addr)
            .map(|a| a.has_permission(&peer_addr))
            .unwrap_or(false);

        if !has_perm {
            return Ok(false);
        }

        if let Some(relay_sock) = self.relay_sockets.get(client_addr) {
            relay_sock.send_to(data, peer_addr).await?;
            if let Some(alloc) = self.allocations.get_mut(client_addr) {
                alloc.bytes_relayed_out += data.len() as u64;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Relay data from peer back to client through the control socket.
    pub async fn relay_to_client(
        &self,
        client_addr: &SocketAddr,
        data: &[u8],
    ) -> std::io::Result<bool> {
        if !self.allocations.contains_key(client_addr) {
            return Ok(false);
        }

        self.socket.send_to(data, client_addr).await?;
        Ok(true)
    }

    /// Remove expired allocations.
    pub fn prune_expired(&mut self) -> usize {
        let expired: Vec<SocketAddr> = self
            .allocations
            .iter()
            .filter(|(_, a)| a.is_expired())
            .map(|(addr, _)| *addr)
            .collect();

        let count = expired.len();
        for addr in expired {
            self.allocations.remove(&addr);
            self.relay_sockets.remove(&addr);
            debug!("TURN allocation expired: client={}", addr);
        }
        count
    }

    /// Deallocate a client's allocation.
    pub fn deallocate(&mut self, client_addr: &SocketAddr) -> bool {
        let removed = self.allocations.remove(client_addr).is_some();
        self.relay_sockets.remove(client_addr);
        removed
    }

    /// Number of active allocations.
    pub fn allocation_count(&self) -> usize {
        self.allocations.len()
    }

    /// Get allocation info for a client.
    pub fn get_allocation(&self, client_addr: &SocketAddr) -> Option<&Allocation> {
        self.allocations.get(client_addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_allocate_and_deallocate() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut relay = TurnRelay::from_socket(sock, 10);

        let client: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let relay_addr = relay.allocate(client).await.unwrap().unwrap();
        assert_eq!(relay.allocation_count(), 1);
        assert!(relay_addr.port() > 0);

        // Duplicate allocation returns same address
        let relay_addr2 = relay.allocate(client).await.unwrap().unwrap();
        assert_eq!(relay_addr, relay_addr2);
        assert_eq!(relay.allocation_count(), 1);

        assert!(relay.deallocate(&client));
        assert_eq!(relay.allocation_count(), 0);
    }

    #[tokio::test]
    async fn test_allocation_capacity() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut relay = TurnRelay::from_socket(sock, 2);

        let c1: SocketAddr = "127.0.0.1:50001".parse().unwrap();
        let c2: SocketAddr = "127.0.0.1:50002".parse().unwrap();
        let c3: SocketAddr = "127.0.0.1:50003".parse().unwrap();

        assert!(relay.allocate(c1).await.unwrap().is_some());
        assert!(relay.allocate(c2).await.unwrap().is_some());
        assert!(relay.allocate(c3).await.unwrap().is_none()); // At capacity
    }

    #[tokio::test]
    async fn test_permissions() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut relay = TurnRelay::from_socket(sock, 10);

        let client: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let peer: SocketAddr = "127.0.0.1:60000".parse().unwrap();

        relay.allocate(client).await.unwrap();

        // No permission initially
        let alloc = relay.get_allocation(&client).unwrap();
        assert!(!alloc.has_permission(&peer));

        // Add permission
        assert!(relay.add_permission(&client, peer));
        let alloc = relay.get_allocation(&client).unwrap();
        assert!(alloc.has_permission(&peer));
    }

    #[tokio::test]
    async fn test_relay_data() {
        let control_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut relay = TurnRelay::from_socket(control_sock, 10);

        let client: SocketAddr = "127.0.0.1:50000".parse().unwrap();

        // Set up a "peer" socket
        let peer_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = peer_sock.local_addr().unwrap();

        relay.allocate(client).await.unwrap();
        relay.add_permission(&client, peer_addr);

        // Relay data to peer
        let sent = relay
            .relay_to_peer(&client, peer_addr, b"hello peer")
            .await
            .unwrap();
        assert!(sent);

        // Peer should receive it
        let mut buf = [0u8; 64];
        let (len, _from) = peer_sock.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..len], b"hello peer");

        // Check bytes tracked
        let alloc = relay.get_allocation(&client).unwrap();
        assert_eq!(alloc.bytes_relayed_out, 10);
    }

    #[tokio::test]
    async fn test_relay_no_permission() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut relay = TurnRelay::from_socket(sock, 10);

        let client: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let peer: SocketAddr = "127.0.0.1:60000".parse().unwrap();

        relay.allocate(client).await.unwrap();
        // No permission added

        let sent = relay
            .relay_to_peer(&client, peer, b"blocked")
            .await
            .unwrap();
        assert!(!sent);
    }
}
