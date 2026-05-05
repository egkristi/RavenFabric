//! LAN discovery via mDNS/DNS-SD.
//!
//! Zero-configuration local network discovery of RavenFabric agents.
//! Agents advertise their presence via _ravenfabric._tcp service records.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

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
}
