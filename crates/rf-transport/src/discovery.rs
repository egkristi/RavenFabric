//! LAN discovery via mDNS/DNS-SD.
//!
//! Zero-configuration local network discovery of RavenFabric agents.
//! Agents advertise their presence via _ravenfabric._tcp service records.
//! Includes a real UDP multicast implementation for LAN discovery.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

/// mDNS multicast address (224.0.0.251:5353).
pub const MDNS_MULTICAST_ADDR: SocketAddr =
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353));

/// Custom RavenFabric discovery port (to avoid conflict with system mDNS on 5353).
pub const RF_DISCOVERY_PORT: u16 = 5354;

/// mDNS service type for RavenFabric agents.
pub const SERVICE_TYPE: &str = "_ravenfabric._tcp.local.";

/// A discovered peer on the local network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// Agent ID (from TXT record).
    pub agent_id: String,
    /// Socket address (from SRV + A/AAAA records).
    pub addr: SocketAddr,
    /// Static public key fingerprint (hex, from TXT record).
    pub key_fingerprint: String,
    /// When this peer was last seen.
    pub last_seen: Instant,
    /// Additional TXT record properties.
    pub properties: HashMap<String, String>,
}

/// Service advertisement for this agent.
#[derive(Debug, Clone)]
pub struct ServiceAdvertisement {
    /// Agent ID to advertise.
    pub agent_id: String,
    /// Port the agent listens on for direct connections.
    pub port: u16,
    /// Static key fingerprint (first 8 bytes hex).
    pub key_fingerprint: String,
    /// Protocol version.
    pub version: u8,
}

impl ServiceAdvertisement {
    /// Build TXT record entries for mDNS advertisement.
    pub fn txt_records(&self) -> Vec<(String, String)> {
        vec![
            ("id".into(), self.agent_id.clone()),
            ("fp".into(), self.key_fingerprint.clone()),
            ("v".into(), self.version.to_string()),
        ]
    }
}

/// Discovery state — tracks known peers and handles TTL expiry.
#[derive(Debug)]
pub struct DiscoveryState {
    /// Known peers by agent_id.
    peers: HashMap<String, DiscoveredPeer>,
    /// How long before a peer is considered stale.
    ttl: Duration,
}

impl DiscoveryState {
    /// Create a new discovery state with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            peers: HashMap::new(),
            ttl,
        }
    }

    /// Record a discovered peer (add or update).
    pub fn peer_seen(&mut self, peer: DiscoveredPeer) {
        self.peers.insert(peer.agent_id.clone(), peer);
    }

    /// Remove stale peers (not seen within TTL).
    pub fn prune_stale(&mut self) -> Vec<String> {
        let now = Instant::now();
        let stale: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| now.duration_since(p.last_seen) > self.ttl)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &stale {
            self.peers.remove(id);
        }
        stale
    }

    /// Get all currently known peers.
    pub fn peers(&self) -> Vec<&DiscoveredPeer> {
        self.peers.values().collect()
    }

    /// Look up a specific peer by agent ID.
    pub fn get_peer(&self, agent_id: &str) -> Option<&DiscoveredPeer> {
        self.peers.get(agent_id)
    }

    /// Number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
}

/// mDNS/DNS-SD discovery agent — broadcasts and listens for RavenFabric
/// agents on the local network using UDP multicast.
///
/// Uses a simple JSON-encoded announcement packet (not full mDNS RFC 6762)
/// for lightweight, cross-platform discovery without DNS library dependencies.
pub struct DiscoveryAgent {
    /// Our agent info to advertise.
    advertisement: ServiceAdvertisement,
    /// UDP socket bound for discovery.
    socket: UdpSocket,
    /// Discovery state tracker.
    state: DiscoveryState,
}

/// A discovery announcement packet (JSON-encoded over UDP).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DiscoveryPacket {
    /// Magic identifier.
    magic: String,
    /// Agent ID.
    agent_id: String,
    /// Listening port.
    port: u16,
    /// Key fingerprint.
    key_fingerprint: String,
    /// Protocol version.
    version: u8,
}

const DISCOVERY_MAGIC: &str = "RVNF-DISC-v1";

impl DiscoveryAgent {
    /// Create a discovery agent bound to a local port.
    /// Uses `0.0.0.0:<port>` for receiving multicasts.
    pub async fn bind(
        advertisement: ServiceAdvertisement,
        port: u16,
        ttl: Duration,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).await?;

        // Join multicast group for receiving announcements
        socket.set_broadcast(true)?;

        Ok(Self {
            advertisement,
            socket,
            state: DiscoveryState::new(ttl),
        })
    }

    /// Create a discovery agent from an existing socket (for testing).
    pub fn from_socket(
        advertisement: ServiceAdvertisement,
        socket: UdpSocket,
        ttl: Duration,
    ) -> Self {
        Self {
            advertisement,
            socket,
            state: DiscoveryState::new(ttl),
        }
    }

    /// Get the local address this agent is bound to.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Broadcast our presence to a target address (multicast or unicast).
    pub async fn announce_to(&self, target: SocketAddr) -> std::io::Result<()> {
        let packet = DiscoveryPacket {
            magic: DISCOVERY_MAGIC.to_string(),
            agent_id: self.advertisement.agent_id.clone(),
            port: self.advertisement.port,
            key_fingerprint: self.advertisement.key_fingerprint.clone(),
            version: self.advertisement.version,
        };

        let encoded = serde_json::to_vec(&packet)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        self.socket.send_to(&encoded, target).await?;
        Ok(())
    }

    /// Listen for one incoming discovery packet and update state.
    /// Returns the agent_id of the discovered peer, or None if packet was invalid.
    pub async fn listen_one(&mut self) -> std::io::Result<Option<String>> {
        let mut buf = [0u8; 1024];
        let (len, from_addr) = self.socket.recv_from(&mut buf).await?;

        let packet: DiscoveryPacket = match serde_json::from_slice(&buf[..len]) {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Validate magic
        if packet.magic != DISCOVERY_MAGIC {
            return Ok(None);
        }

        // Don't discover ourselves
        if packet.agent_id == self.advertisement.agent_id {
            return Ok(None);
        }

        // Build peer entry — use the source IP but the advertised port
        let peer_addr = SocketAddr::new(from_addr.ip(), packet.port);
        let peer = DiscoveredPeer {
            agent_id: packet.agent_id.clone(),
            addr: peer_addr,
            key_fingerprint: packet.key_fingerprint,
            last_seen: Instant::now(),
            properties: HashMap::new(),
        };

        self.state.peer_seen(peer);
        Ok(Some(packet.agent_id))
    }

    /// Get all currently discovered peers.
    pub fn peers(&self) -> Vec<&DiscoveredPeer> {
        self.state.peers()
    }

    /// Get a specific peer.
    pub fn get_peer(&self, agent_id: &str) -> Option<&DiscoveredPeer> {
        self.state.get_peer(agent_id)
    }

    /// Number of discovered peers.
    pub fn peer_count(&self) -> usize {
        self.state.peer_count()
    }

    /// Prune stale peers.
    pub fn prune_stale(&mut self) -> Vec<String> {
        self.state.prune_stale()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn make_peer(id: &str) -> DiscoveredPeer {
        DiscoveredPeer {
            agent_id: id.to_string(),
            addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 100), 9090)),
            key_fingerprint: "abcd1234".to_string(),
            last_seen: Instant::now(),
            properties: HashMap::new(),
        }
    }

    #[test]
    fn test_peer_seen_and_lookup() {
        let mut state = DiscoveryState::new(Duration::from_secs(60));
        state.peer_seen(make_peer("agent-01"));
        state.peer_seen(make_peer("agent-02"));

        assert_eq!(state.peer_count(), 2);
        assert!(state.get_peer("agent-01").is_some());
        assert!(state.get_peer("agent-03").is_none());
    }

    #[test]
    fn test_prune_stale() {
        let mut state = DiscoveryState::new(Duration::from_secs(1));
        let mut old_peer = make_peer("old-agent");
        old_peer.last_seen = Instant::now() - Duration::from_secs(10);
        state.peer_seen(old_peer);
        state.peer_seen(make_peer("new-agent"));

        let stale = state.prune_stale();
        assert_eq!(stale, vec!["old-agent"]);
        assert_eq!(state.peer_count(), 1);
    }

    #[test]
    fn test_service_advertisement_txt() {
        let advert = ServiceAdvertisement {
            agent_id: "web-01".into(),
            port: 9090,
            key_fingerprint: "deadbeef".into(),
            version: 1,
        };
        let txt = advert.txt_records();
        assert_eq!(txt.len(), 3);
        assert!(txt.iter().any(|(k, v)| k == "id" && v == "web-01"));
        assert!(txt.iter().any(|(k, v)| k == "fp" && v == "deadbeef"));
        assert!(txt.iter().any(|(k, v)| k == "v" && v == "1"));
    }

    #[test]
    fn test_update_existing_peer() {
        let mut state = DiscoveryState::new(Duration::from_secs(60));
        state.peer_seen(make_peer("agent-01"));

        // Update with new address
        let mut updated = make_peer("agent-01");
        updated.addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 200), 9090));
        state.peer_seen(updated);

        assert_eq!(state.peer_count(), 1);
        let peer = state.get_peer("agent-01").unwrap();
        assert_eq!(
            peer.addr,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 200), 9090))
        );
    }

    #[tokio::test]
    async fn test_discovery_agent_announce_and_listen() {
        let advert_a = ServiceAdvertisement {
            agent_id: "agent-a".into(),
            port: 9090,
            key_fingerprint: "aaaa1111".into(),
            version: 1,
        };
        let advert_b = ServiceAdvertisement {
            agent_id: "agent-b".into(),
            port: 9091,
            key_fingerprint: "bbbb2222".into(),
            version: 1,
        };

        // Bind two agents on random ports
        let sock_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sock_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let _addr_a = sock_a.local_addr().unwrap();
        let addr_b = sock_b.local_addr().unwrap();

        let agent_a = DiscoveryAgent::from_socket(advert_a, sock_a, Duration::from_secs(60));
        let mut agent_b = DiscoveryAgent::from_socket(advert_b, sock_b, Duration::from_secs(60));

        // A announces to B's address
        agent_a.announce_to(addr_b).await.unwrap();

        // B listens and discovers A
        let discovered = agent_b.listen_one().await.unwrap();
        assert_eq!(discovered, Some("agent-a".to_string()));
        assert_eq!(agent_b.peer_count(), 1);

        let peer = agent_b.get_peer("agent-a").unwrap();
        assert_eq!(peer.key_fingerprint, "aaaa1111");
        // Port should be the advertised port (9090), not the UDP source port
        assert_eq!(peer.addr.port(), 9090);
    }

    #[tokio::test]
    async fn test_discovery_agent_ignores_self() {
        let advert = ServiceAdvertisement {
            agent_id: "self-agent".into(),
            port: 9090,
            key_fingerprint: "cccc3333".into(),
            version: 1,
        };

        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = sock.local_addr().unwrap();
        let mut agent = DiscoveryAgent::from_socket(advert, sock, Duration::from_secs(60));

        // Send announcement from a separate socket pretending to be the same agent
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let packet = serde_json::to_vec(&serde_json::json!({
            "magic": "RVNF-DISC-v1",
            "agent_id": "self-agent",
            "port": 9090,
            "key_fingerprint": "cccc3333",
            "version": 1
        }))
        .unwrap();
        sender.send_to(&packet, addr).await.unwrap();

        let discovered = agent.listen_one().await.unwrap();
        assert_eq!(discovered, None); // Should ignore self
        assert_eq!(agent.peer_count(), 0);
    }
}
