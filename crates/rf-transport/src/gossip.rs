//! Relay-reported metrics and mesh neighbor health gossip.
//!
//! Enables partial observability without direct controller paths:
//! each node shares health state with its immediate neighbors,
//! and relays report forwarding metrics.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

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
}
