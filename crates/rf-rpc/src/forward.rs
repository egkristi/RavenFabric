//! Port forwarding types — local/remote/dynamic forward definitions.
//!
//! Implements the equivalent of ssh -L, ssh -R, and ssh -D through
//! the RavenFabric mesh.

use serde::{Deserialize, Serialize};

/// Direction of port forwarding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForwardDirection {
    /// Local forward (ssh -L): listen locally, forward to remote agent.
    Local,
    /// Remote forward (ssh -R): listen on remote agent, forward back to client.
    Remote,
}

/// A port forwarding rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortForward {
    /// Direction of the forward.
    pub direction: ForwardDirection,
    /// Local bind address (host:port for local, bind address for remote listener).
    pub bind_addr: String,
    /// Target address to forward to (host:port on the remote/local side).
    pub target_addr: String,
    /// Optional unique ID for this forward rule.
    pub id: Option<String>,
}

/// State of an active port forward.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForwardState {
    /// Listener bound, waiting for connections.
    Listening,
    /// Forward active (at least one connection being forwarded).
    Active,
    /// Forward closed.
    Closed,
    /// Error occurred (listener bind failed, target unreachable).
    Error,
}

/// Statistics for an active port forward.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForwardStats {
    /// Total connections accepted.
    pub connections_total: u64,
    /// Currently active connections.
    pub connections_active: u32,
    /// Total bytes forwarded (both directions).
    pub bytes_forwarded: u64,
}

/// Manages a set of active port forwards.
#[derive(Debug, Default)]
pub struct ForwardManager {
    forwards: Vec<(PortForward, ForwardState, ForwardStats)>,
}

impl ForwardManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new port forward rule.
    pub fn add(&mut self, forward: PortForward) -> usize {
        let idx = self.forwards.len();
        self.forwards
            .push((forward, ForwardState::Listening, ForwardStats::default()));
        idx
    }

    /// Remove a port forward by index.
    pub fn remove(&mut self, idx: usize) -> Option<PortForward> {
        if idx < self.forwards.len() {
            let (fwd, _, _) = self.forwards.remove(idx);
            Some(fwd)
        } else {
            None
        }
    }

    /// Get current state of a forward.
    pub fn state(&self, idx: usize) -> Option<ForwardState> {
        self.forwards.get(idx).map(|(_, state, _)| *state)
    }

    /// Update state of a forward.
    pub fn set_state(&mut self, idx: usize, state: ForwardState) {
        if let Some((_, s, _)) = self.forwards.get_mut(idx) {
            *s = state;
        }
    }

    /// Record a new connection on a forward.
    pub fn record_connection(&mut self, idx: usize) {
        if let Some((_, state, stats)) = self.forwards.get_mut(idx) {
            stats.connections_total += 1;
            stats.connections_active += 1;
            *state = ForwardState::Active;
        }
    }

    /// Record a connection close on a forward.
    pub fn record_disconnect(&mut self, idx: usize) {
        if let Some((_, state, stats)) = self.forwards.get_mut(idx) {
            stats.connections_active = stats.connections_active.saturating_sub(1);
            if stats.connections_active == 0 {
                *state = ForwardState::Listening;
            }
        }
    }

    /// Record bytes forwarded.
    pub fn record_bytes(&mut self, idx: usize, bytes: u64) {
        if let Some((_, _, stats)) = self.forwards.get_mut(idx) {
            stats.bytes_forwarded += bytes;
        }
    }

    /// List all active forwards.
    pub fn list(&self) -> Vec<(&PortForward, ForwardState, &ForwardStats)> {
        self.forwards
            .iter()
            .map(|(fwd, state, stats)| (fwd, *state, stats))
            .collect()
    }

    /// Number of active forwards.
    pub fn len(&self) -> usize {
        self.forwards.len()
    }

    /// Whether the manager has no forwards.
    pub fn is_empty(&self) -> bool {
        self.forwards.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_forward() -> PortForward {
        PortForward {
            direction: ForwardDirection::Local,
            bind_addr: "127.0.0.1:8080".into(),
            target_addr: "db-server:5432".into(),
            id: Some("db-tunnel".into()),
        }
    }

    fn remote_forward() -> PortForward {
        PortForward {
            direction: ForwardDirection::Remote,
            bind_addr: "0.0.0.0:9090".into(),
            target_addr: "localhost:3000".into(),
            id: None,
        }
    }

    #[test]
    fn test_add_and_list() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        mgr.add(remote_forward());

        assert_eq!(mgr.len(), 2);
        let list = mgr.list();
        assert_eq!(list[0].0.bind_addr, "127.0.0.1:8080");
        assert_eq!(list[1].0.bind_addr, "0.0.0.0:9090");
    }

    #[test]
    fn test_initial_state_listening() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        assert_eq!(mgr.state(0), Some(ForwardState::Listening));
    }

    #[test]
    fn test_connection_tracking() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());

        mgr.record_connection(0);
        assert_eq!(mgr.state(0), Some(ForwardState::Active));

        mgr.record_connection(0);
        let list = mgr.list();
        assert_eq!(list[0].2.connections_active, 2);
        assert_eq!(list[0].2.connections_total, 2);

        mgr.record_disconnect(0);
        let list = mgr.list();
        assert_eq!(list[0].2.connections_active, 1);

        mgr.record_disconnect(0);
        assert_eq!(mgr.state(0), Some(ForwardState::Listening));
    }

    #[test]
    fn test_bytes_tracking() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());

        mgr.record_bytes(0, 1024);
        mgr.record_bytes(0, 2048);

        let list = mgr.list();
        assert_eq!(list[0].2.bytes_forwarded, 3072);
    }

    #[test]
    fn test_remove() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        mgr.add(remote_forward());

        let removed = mgr.remove(0).unwrap();
        assert_eq!(removed.bind_addr, "127.0.0.1:8080");
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn test_serialization() {
        let fwd = local_forward();
        let json = serde_json::to_string(&fwd).unwrap();
        let parsed: PortForward = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fwd);
    }
}
