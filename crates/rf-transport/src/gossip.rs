//! Relay-reported metrics and mesh neighbor health gossip.
//!
//! Enables partial observability without direct controller paths:
//! each node shares health state with its immediate neighbors,
//! and relays report forwarding metrics. Includes a real UDP gossip
//! agent for SWIM-style failure detection.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

/// Metrics reported by a relay node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMetrics {
    /// Relay node ID.
    pub relay_id: String,
    /// Number of active connections through this relay.
    pub active_connections: u32,
    /// Total bytes forwarded since start.
    pub bytes_forwarded: u64,
    /// Average forwarding latency (added by relay processing).
    pub avg_forwarding_latency: Duration,
    /// Current queue depth (pending forwards).
    pub queue_depth: u32,
    /// Total messages relayed since start.
    pub messages_relayed: u64,
    /// Hop count from reporter to relay.
    pub hop_count: u8,
    /// Timestamp of this report (Unix ms).
    pub timestamp_ms: u64,
}

/// Health state shared via gossip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GossipHealth {
    /// Node is healthy and responsive.
    Healthy,
    /// Node is degraded (high latency, some failures).
    Degraded,
    /// Node is unreachable or unresponsive.
    Unreachable,
    /// Node status unknown (no gossip received yet).
    Unknown,
}

/// A gossip message exchanged between mesh neighbors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipMessage {
    /// Agent ID of the reporting neighbor.
    pub from: String,
    /// Health state of the reporter.
    pub health: GossipHealth,
    /// Sequence number (monotonically increasing).
    pub sequence: u64,
    /// Timestamp of this gossip (Unix ms).
    pub timestamp_ms: u64,
    /// Transitive reports: what this neighbor knows about other nodes.
    pub neighbor_reports: Vec<NeighborReport>,
}

/// A transitive health report about a non-adjacent node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborReport {
    /// Agent ID of the reported node.
    pub agent_id: String,
    /// Health as observed by the gossip source.
    pub health: GossipHealth,
    /// Hops from gossip source to reported node.
    pub hops: u8,
    /// Freshness: how recently this was observed (ms ago).
    pub age_ms: u64,
}

/// Gossip state tracker — maintains a view of mesh health.
#[derive(Debug)]
pub struct GossipTracker {
    /// Known node states.
    states: HashMap<String, NodeState>,
    /// Maximum age before a node is considered unknown (ms).
    max_age_ms: u64,
}

/// Internal state for a tracked node.
#[derive(Debug, Clone)]
struct NodeState {
    health: GossipHealth,
    last_sequence: u64,
    last_seen_ms: u64,
    hops: u8,
}

impl GossipTracker {
    pub fn new(max_age_ms: u64) -> Self {
        Self {
            states: HashMap::new(),
            max_age_ms,
        }
    }

    /// Process an incoming gossip message.
    pub fn receive(&mut self, msg: &GossipMessage, now_ms: u64) {
        // Update direct neighbor state
        let entry = self.states.entry(msg.from.clone()).or_insert(NodeState {
            health: GossipHealth::Unknown,
            last_sequence: 0,
            last_seen_ms: 0,
            hops: 1,
        });

        // Only accept if sequence is newer
        if msg.sequence > entry.last_sequence {
            entry.health = msg.health;
            entry.last_sequence = msg.sequence;
            entry.last_seen_ms = now_ms;
            entry.hops = 1;
        }

        // Process transitive reports
        for report in &msg.neighbor_reports {
            let existing = self.states.get(&report.agent_id);
            let should_update = match existing {
                None => true,
                Some(e) => {
                    // Only update if this report is fresher or has fewer hops
                    report.hops + 1 < e.hops
                        || (report.hops + 1 == e.hops
                            && now_ms.saturating_sub(report.age_ms) > e.last_seen_ms)
                }
            };

            if should_update {
                self.states.insert(
                    report.agent_id.clone(),
                    NodeState {
                        health: report.health,
                        last_sequence: 0,
                        last_seen_ms: now_ms.saturating_sub(report.age_ms),
                        hops: report.hops + 1,
                    },
                );
            }
        }
    }

    /// Get the health of a specific node.
    pub fn health_of(&self, agent_id: &str, now_ms: u64) -> GossipHealth {
        match self.states.get(agent_id) {
            None => GossipHealth::Unknown,
            Some(state) => {
                if now_ms.saturating_sub(state.last_seen_ms) > self.max_age_ms {
                    GossipHealth::Unknown
                } else {
                    state.health
                }
            }
        }
    }

    /// Get all nodes and their health states.
    pub fn all_nodes(&self, now_ms: u64) -> Vec<(&str, GossipHealth)> {
        self.states
            .iter()
            .map(|(id, state)| {
                let health = if now_ms.saturating_sub(state.last_seen_ms) > self.max_age_ms {
                    GossipHealth::Unknown
                } else {
                    state.health
                };
                (id.as_str(), health)
            })
            .collect()
    }

    /// Get nodes that are healthy (for routing decisions).
    pub fn healthy_nodes(&self, now_ms: u64) -> Vec<&str> {
        self.states
            .iter()
            .filter(|(_, state)| {
                state.health == GossipHealth::Healthy
                    && now_ms.saturating_sub(state.last_seen_ms) <= self.max_age_ms
            })
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Prune stale entries.
    pub fn prune(&mut self, now_ms: u64) {
        self.states
            .retain(|_, state| now_ms.saturating_sub(state.last_seen_ms) <= self.max_age_ms * 3);
    }

    /// Number of tracked nodes.
    pub fn node_count(&self) -> usize {
        self.states.len()
    }

    /// Build neighbor reports for outgoing gossip.
    pub fn build_reports(&self, now_ms: u64) -> Vec<NeighborReport> {
        self.states
            .iter()
            .filter(|(_, state)| now_ms.saturating_sub(state.last_seen_ms) <= self.max_age_ms)
            .map(|(id, state)| NeighborReport {
                agent_id: id.clone(),
                health: state.health,
                hops: state.hops,
                age_ms: now_ms.saturating_sub(state.last_seen_ms),
            })
            .collect()
    }
}

/// UDP gossip agent — implements SWIM-style failure detection and state
/// dissemination over UDP sockets.
pub struct GossipAgent {
    /// Our agent ID.
    agent_id: String,
    /// UDP socket for gossip communication.
    socket: UdpSocket,
    /// Known peers to gossip with.
    peers: Vec<SocketAddr>,
    /// Gossip tracker for maintaining state.
    tracker: GossipTracker,
    /// Monotonically increasing sequence number.
    sequence: u64,
    /// Our own health status.
    self_health: GossipHealth,
}

impl GossipAgent {
    /// Create a new gossip agent bound to a local address.
    pub async fn bind(
        agent_id: String,
        local_addr: &str,
        max_age_ms: u64,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(local_addr).await?;
        Ok(Self {
            agent_id,
            socket,
            peers: Vec::new(),
            tracker: GossipTracker::new(max_age_ms),
            sequence: 0,
            self_health: GossipHealth::Healthy,
        })
    }

    /// Get the local address this agent is bound to.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Add a peer to gossip with.
    pub fn add_peer(&mut self, addr: SocketAddr) {
        if !self.peers.contains(&addr) {
            self.peers.push(addr);
        }
    }

    /// Set our own health status.
    pub fn set_health(&mut self, health: GossipHealth) {
        self.self_health = health;
    }

    /// Send a gossip message to all known peers.
    pub async fn gossip_to_peers(&mut self) -> std::io::Result<usize> {
        self.sequence += 1;
        let now_ms = now_millis();

        let msg = GossipMessage {
            from: self.agent_id.clone(),
            health: self.self_health,
            sequence: self.sequence,
            timestamp_ms: now_ms,
            neighbor_reports: self.tracker.build_reports(now_ms),
        };

        let encoded = serde_json::to_vec(&msg).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;

        let mut sent = 0;
        for peer in &self.peers {
            if self.socket.send_to(&encoded, peer).await.is_ok() {
                sent += 1;
            }
        }
        Ok(sent)
    }

    /// Receive and process one incoming gossip message.
    pub async fn receive_one(&mut self) -> std::io::Result<String> {
        let mut buf = [0u8; 4096];
        let (len, _from) = self.socket.recv_from(&mut buf).await?;

        let msg: GossipMessage = serde_json::from_slice(&buf[..len]).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;

        let from_id = msg.from.clone();
        let now_ms = now_millis();
        self.tracker.receive(&msg, now_ms);
        Ok(from_id)
    }

    /// Get the health of a specific node.
    pub fn health_of(&self, agent_id: &str) -> GossipHealth {
        self.tracker.health_of(agent_id, now_millis())
    }

    /// Get all known healthy nodes.
    pub fn healthy_nodes(&self) -> Vec<&str> {
        self.tracker.healthy_nodes(now_millis())
    }

    /// Number of tracked nodes.
    pub fn node_count(&self) -> usize {
        self.tracker.node_count()
    }
}

/// Get current time in milliseconds since epoch.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gossip_receive_direct() {
        let mut tracker = GossipTracker::new(60_000);
        let msg = GossipMessage {
            from: "node-a".into(),
            health: GossipHealth::Healthy,
            sequence: 1,
            timestamp_ms: 1000,
            neighbor_reports: vec![],
        };

        tracker.receive(&msg, 1000);
        assert_eq!(tracker.health_of("node-a", 1000), GossipHealth::Healthy);
    }

    #[test]
    fn test_gossip_transitive() {
        let mut tracker = GossipTracker::new(60_000);
        let msg = GossipMessage {
            from: "node-a".into(),
            health: GossipHealth::Healthy,
            sequence: 1,
            timestamp_ms: 1000,
            neighbor_reports: vec![NeighborReport {
                agent_id: "node-b".into(),
                health: GossipHealth::Degraded,
                hops: 1,
                age_ms: 500,
            }],
        };

        tracker.receive(&msg, 2000);
        assert_eq!(tracker.health_of("node-b", 2000), GossipHealth::Degraded);
    }

    #[test]
    fn test_gossip_stale() {
        let mut tracker = GossipTracker::new(5_000); // 5s max age
        let msg = GossipMessage {
            from: "node-a".into(),
            health: GossipHealth::Healthy,
            sequence: 1,
            timestamp_ms: 1000,
            neighbor_reports: vec![],
        };

        tracker.receive(&msg, 1000);
        assert_eq!(tracker.health_of("node-a", 1000), GossipHealth::Healthy);
        assert_eq!(tracker.health_of("node-a", 10_000), GossipHealth::Unknown); // Stale
    }

    #[test]
    fn test_gossip_sequence_ordering() {
        let mut tracker = GossipTracker::new(60_000);

        // Receive seq 2 first
        tracker.receive(
            &GossipMessage {
                from: "node-a".into(),
                health: GossipHealth::Healthy,
                sequence: 2,
                timestamp_ms: 2000,
                neighbor_reports: vec![],
            },
            2000,
        );

        // Then seq 1 (should be ignored — stale)
        tracker.receive(
            &GossipMessage {
                from: "node-a".into(),
                health: GossipHealth::Unreachable,
                sequence: 1,
                timestamp_ms: 1000,
                neighbor_reports: vec![],
            },
            2500,
        );

        assert_eq!(tracker.health_of("node-a", 2500), GossipHealth::Healthy);
    }

    #[test]
    fn test_healthy_nodes() {
        let mut tracker = GossipTracker::new(60_000);
        tracker.receive(
            &GossipMessage {
                from: "healthy".into(),
                health: GossipHealth::Healthy,
                sequence: 1,
                timestamp_ms: 1000,
                neighbor_reports: vec![],
            },
            1000,
        );
        tracker.receive(
            &GossipMessage {
                from: "degraded".into(),
                health: GossipHealth::Degraded,
                sequence: 1,
                timestamp_ms: 1000,
                neighbor_reports: vec![],
            },
            1000,
        );

        let healthy = tracker.healthy_nodes(1000);
        assert_eq!(healthy.len(), 1);
        assert!(healthy.contains(&"healthy"));
    }

    #[test]
    fn test_build_reports() {
        let mut tracker = GossipTracker::new(60_000);
        tracker.receive(
            &GossipMessage {
                from: "peer-1".into(),
                health: GossipHealth::Healthy,
                sequence: 1,
                timestamp_ms: 1000,
                neighbor_reports: vec![],
            },
            1000,
        );

        let reports = tracker.build_reports(2000);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].agent_id, "peer-1");
        assert_eq!(reports[0].age_ms, 1000);
    }

    #[tokio::test]
    async fn test_gossip_agent_send_receive() {
        // Create two gossip agents
        let mut agent_a = GossipAgent::bind("node-a".into(), "127.0.0.1:0", 60_000)
            .await
            .unwrap();
        let mut agent_b = GossipAgent::bind("node-b".into(), "127.0.0.1:0", 60_000)
            .await
            .unwrap();

        let addr_a = agent_a.local_addr().unwrap();
        let addr_b = agent_b.local_addr().unwrap();

        // A knows about B
        agent_a.add_peer(addr_b);
        // B knows about A
        agent_b.add_peer(addr_a);

        // A sends gossip
        let sent = agent_a.gossip_to_peers().await.unwrap();
        assert_eq!(sent, 1);

        // B receives it
        let from = agent_b.receive_one().await.unwrap();
        assert_eq!(from, "node-a");
        assert_eq!(agent_b.health_of("node-a"), GossipHealth::Healthy);
    }

    #[tokio::test]
    async fn test_gossip_agent_bidirectional() {
        let mut agent_a = GossipAgent::bind("alpha".into(), "127.0.0.1:0", 60_000)
            .await
            .unwrap();
        let mut agent_b = GossipAgent::bind("beta".into(), "127.0.0.1:0", 60_000)
            .await
            .unwrap();

        let addr_a = agent_a.local_addr().unwrap();
        let addr_b = agent_b.local_addr().unwrap();

        agent_a.add_peer(addr_b);
        agent_b.add_peer(addr_a);

        // A gossips (healthy)
        agent_a.gossip_to_peers().await.unwrap();
        agent_b.receive_one().await.unwrap();

        // B gossips (degraded)
        agent_b.set_health(GossipHealth::Degraded);
        agent_b.gossip_to_peers().await.unwrap();
        agent_a.receive_one().await.unwrap();

        assert_eq!(agent_a.health_of("beta"), GossipHealth::Degraded);
        assert_eq!(agent_b.health_of("alpha"), GossipHealth::Healthy);
        // B has 1 tracked node (alpha), A has 2 (beta + alpha via transitive report)
        assert!(agent_a.node_count() >= 1);
        assert_eq!(agent_b.node_count(), 1);
    }
}
