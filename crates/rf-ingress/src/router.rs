//! Routing table: maps inbound requests to registered agent upstreams.
//!
//! An agent registers by sending `IngressRegister { agent_id, upstream_url,
//! subdomain, path_prefix }` over its authenticated RPC channel.  The table
//! keeps the upstream URL and provides matching logic for incoming HTTP
//! requests.
//!
//! ## Load balancing
//!
//! Multiple agents can register with the same subdomain + path_prefix
//! combination.  Requests are distributed among them in round-robin order.
//!
//! ## Sticky sessions
//!
//! When a `caller_identity` is supplied (e.g. the SHA-256 of the API key),
//! the same upstream is preferred for repeated requests within the sticky TTL
//! window (default 1 hour).  The pin is evicted when the agent deregisters.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Sticky session TTL: 1 hour of inactivity evicts the affinity pin.
const STICKY_TTL: Duration = Duration::from_secs(3600);

/// An entry in the routing table representing one registered agent upstream.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub upstream_url: String,
    pub subdomain: Option<String>,
    pub path_prefix: Option<String>,
}

/// Shared routing table, safe for concurrent reads and writes.
///
/// Supports round-robin load balancing across multiple agents registered for
/// the same route, and optional sticky-session affinity by caller identity.
#[derive(Debug, Clone)]
pub struct RoutingTable {
    /// Agent entries keyed by `agent_id`.
    inner: Arc<RwLock<HashMap<String, AgentEntry>>>,
    /// Per-route round-robin counters.
    /// Key = `"<subdomain>/<path_prefix>"` (empty string segments when `None`).
    round_robin: Arc<Mutex<HashMap<String, usize>>>,
    /// Sticky session store: caller_identity → (agent_id, last_seen).
    sticky: Arc<Mutex<HashMap<String, (String, Instant)>>>,
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingTable {
    /// Create an empty routing table.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            round_robin: Arc::new(Mutex::new(HashMap::new())),
            sticky: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register or update an agent entry keyed by `agent_id`.
    pub async fn register(&self, entry: AgentEntry) {
        let mut map = self.inner.write().await;
        map.insert(entry.agent_id.clone(), entry);
    }

    /// Remove a registered agent.
    ///
    /// Also evicts any sticky sessions that were pinned to this agent so
    /// subsequent requests fall back to round-robin selection.
    pub async fn deregister(&self, agent_id: &str) {
        let mut map = self.inner.write().await;
        map.remove(agent_id);
        // Evict sticky pins pointing to the deregistered agent.
        let mut sticky = self.sticky.lock().expect("sticky lock poisoned");
        sticky.retain(|_, (aid, _)| aid.as_str() != agent_id);
    }

    /// Collect all candidate agents matching `host` + `path` using the
    /// three-tier priority:
    ///
    /// 1. Subdomain + optional path prefix  
    /// 2. Path prefix only  
    /// 3. Catch-all when exactly one agent is registered
    async fn collect_candidates(&self, host: &str, path: &str) -> Vec<AgentEntry> {
        let map = self.inner.read().await;
        let entries: Vec<&AgentEntry> = map.values().collect();

        // Tier 1: subdomain + optional path prefix
        let tier1: Vec<AgentEntry> = entries
            .iter()
            .filter(|e| {
                if let Some(sub) = &e.subdomain {
                    let prefix = format!("{sub}.");
                    if host.starts_with(prefix.as_str()) {
                        return match &e.path_prefix {
                            Some(pp) => path.starts_with(pp.as_str()),
                            None => true,
                        };
                    }
                }
                false
            })
            .map(|e| (*e).clone())
            .collect();

        if !tier1.is_empty() {
            return tier1;
        }

        // Tier 2: path prefix only
        let tier2: Vec<AgentEntry> = entries
            .iter()
            .filter(|e| {
                e.path_prefix
                    .as_ref()
                    .map(|pp| path.starts_with(pp.as_str()))
                    .unwrap_or(false)
            })
            .map(|e| (*e).clone())
            .collect();

        if !tier2.is_empty() {
            return tier2;
        }

        // Tier 3: catch-all (only when there is exactly one agent)
        if entries.len() == 1 {
            return entries.into_iter().cloned().collect();
        }

        vec![]
    }

    /// Resolve the best matching agent for a request, applying round-robin
    /// load balancing across all candidates for the matched route.
    ///
    /// When `caller_identity` is supplied (e.g. `"sha256:<hex>"` of the API
    /// key), the same upstream is preferred for repeated requests within the
    /// sticky-session TTL.
    pub async fn resolve_with_affinity(
        &self,
        host: &str,
        path: &str,
        caller_identity: Option<&str>,
    ) -> Option<AgentEntry> {
        let candidates = self.collect_candidates(host, path).await;
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return candidates.into_iter().next();
        }

        // Check sticky session first.
        if let Some(identity) = caller_identity {
            let mut sticky = self.sticky.lock().expect("sticky lock poisoned");
            if let Some((pinned_agent, ts)) = sticky.get_mut(identity) {
                if ts.elapsed() < STICKY_TTL {
                    if let Some(e) = candidates.iter().find(|e| e.agent_id == *pinned_agent) {
                        *ts = Instant::now(); // refresh TTL on each hit
                        return Some(e.clone());
                    }
                }
                // Expired or agent gone — fall through to round-robin.
            }
        }

        // Round-robin selection.
        let route_key = Self::route_key_for(&candidates);
        let idx = {
            let mut rr = self.round_robin.lock().expect("round_robin lock poisoned");
            let counter = rr.entry(route_key).or_insert(0);
            let idx = *counter % candidates.len();
            *counter = counter.wrapping_add(1);
            idx
        };
        let chosen = candidates[idx].clone();

        // Pin caller to chosen agent for future requests.
        if let Some(identity) = caller_identity {
            let mut sticky = self.sticky.lock().expect("sticky lock poisoned");
            sticky.insert(identity.to_string(), (chosen.agent_id.clone(), Instant::now()));
        }

        Some(chosen)
    }

    /// Resolve the best matching agent without sticky-session affinity.
    ///
    /// Preserves backward compatibility for callers that do not supply a
    /// caller identity.
    pub async fn resolve(&self, host: &str, path: &str) -> Option<AgentEntry> {
        self.resolve_with_affinity(host, path, None).await
    }

    /// Number of registered agents.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Build a stable route key from a set of candidates.
    ///
    /// All candidates within one group share the same subdomain + path_prefix
    /// so the first entry's values are representative.
    fn route_key_for(candidates: &[AgentEntry]) -> String {
        candidates
            .first()
            .map(|e| {
                format!(
                    "{}/{}",
                    e.subdomain.as_deref().unwrap_or(""),
                    e.path_prefix.as_deref().unwrap_or("")
                )
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_resolve_by_subdomain() {
        let table = RoutingTable::new();
        table
            .register(AgentEntry {
                agent_id: "web-01".into(),
                upstream_url: "http://127.0.0.1:8080".into(),
                subdomain: Some("web".into()),
                path_prefix: None,
            })
            .await;

        let entry = table.resolve("web.example.com", "/api/v1").await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().agent_id, "web-01");
    }

    #[tokio::test]
    async fn resolve_by_path_prefix() {
        let table = RoutingTable::new();
        table
            .register(AgentEntry {
                agent_id: "api-01".into(),
                upstream_url: "http://127.0.0.1:9090".into(),
                subdomain: None,
                path_prefix: Some("/api".into()),
            })
            .await;

        let entry = table.resolve("anything.com", "/api/users").await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().upstream_url, "http://127.0.0.1:9090");
    }

    #[tokio::test]
    async fn catch_all_single_entry() {
        let table = RoutingTable::new();
        table
            .register(AgentEntry {
                agent_id: "only".into(),
                upstream_url: "http://127.0.0.1:7070".into(),
                subdomain: None,
                path_prefix: None,
            })
            .await;

        let entry = table.resolve("foo.bar", "/anything").await;
        assert!(entry.is_some());
    }

    #[tokio::test]
    async fn no_match_with_multiple_unmatched_entries() {
        let table = RoutingTable::new();
        table
            .register(AgentEntry {
                agent_id: "a".into(),
                upstream_url: "http://127.0.0.1:1".into(),
                subdomain: Some("a".into()),
                path_prefix: None,
            })
            .await;
        table
            .register(AgentEntry {
                agent_id: "b".into(),
                upstream_url: "http://127.0.0.1:2".into(),
                subdomain: Some("b".into()),
                path_prefix: None,
            })
            .await;

        let entry = table.resolve("c.example.com", "/").await;
        assert!(entry.is_none());
    }

    #[tokio::test]
    async fn deregister_removes_entry() {
        let table = RoutingTable::new();
        table
            .register(AgentEntry {
                agent_id: "gone".into(),
                upstream_url: "http://127.0.0.1:1".into(),
                subdomain: None,
                path_prefix: None,
            })
            .await;
        assert_eq!(table.len().await, 1);
        table.deregister("gone").await;
        assert_eq!(table.len().await, 0);
    }

    /// Two agents with the same subdomain are served in round-robin order.
    #[tokio::test]
    async fn round_robin_across_two_agents() {
        let table = RoutingTable::new();
        for i in 1..=2 {
            table
                .register(AgentEntry {
                    agent_id: format!("web-{i:02}"),
                    upstream_url: format!("http://127.0.0.1:{i}"),
                    subdomain: Some("web".into()),
                    path_prefix: None,
                })
                .await;
        }

        let first = table.resolve("web.example.com", "/").await.unwrap();
        let second = table.resolve("web.example.com", "/").await.unwrap();
        // Must rotate — the two calls return different agents.
        assert_ne!(first.agent_id, second.agent_id);
        // Third call wraps back to the first.
        let third = table.resolve("web.example.com", "/").await.unwrap();
        assert_eq!(first.agent_id, third.agent_id);
    }

    /// A caller with a stable identity is always routed to the same agent
    /// within the sticky-session TTL.
    #[tokio::test]
    async fn sticky_session_affinity() {
        let table = RoutingTable::new();
        for i in 1..=3 {
            table
                .register(AgentEntry {
                    agent_id: format!("svc-{i:02}"),
                    upstream_url: format!("http://127.0.0.1:{i}"),
                    subdomain: Some("svc".into()),
                    path_prefix: None,
                })
                .await;
        }

        let identity = "sha256:abc123";
        let first = table
            .resolve_with_affinity("svc.example.com", "/", Some(identity))
            .await
            .unwrap();

        // Subsequent calls with the same identity must return the same agent.
        for _ in 0..5 {
            let subsequent = table
                .resolve_with_affinity("svc.example.com", "/", Some(identity))
                .await
                .unwrap();
            assert_eq!(first.agent_id, subsequent.agent_id);
        }
    }

    /// A second caller with a different identity may be routed to a different
    /// agent (not pinned to the first caller's choice).
    #[tokio::test]
    async fn different_callers_can_reach_different_agents() {
        let table = RoutingTable::new();
        for i in 1..=2 {
            table
                .register(AgentEntry {
                    agent_id: format!("node-{i:02}"),
                    upstream_url: format!("http://127.0.0.1:{i}"),
                    subdomain: Some("node".into()),
                    path_prefix: None,
                })
                .await;
        }

        let a = table
            .resolve_with_affinity("node.example.com", "/", Some("caller-a"))
            .await
            .unwrap();
        let b = table
            .resolve_with_affinity("node.example.com", "/", Some("caller-b"))
            .await
            .unwrap();
        // With two agents and round-robin seeding, the second distinct caller
        // lands on the opposite agent.
        assert_ne!(a.agent_id, b.agent_id);
    }

    /// Deregistering an agent removes its sticky pins.
    #[tokio::test]
    async fn deregister_evicts_sticky_pins() {
        let table = RoutingTable::new();
        for i in 1..=2 {
            table
                .register(AgentEntry {
                    agent_id: format!("srv-{i:02}"),
                    upstream_url: format!("http://127.0.0.1:{i}"),
                    subdomain: Some("srv".into()),
                    path_prefix: None,
                })
                .await;
        }

        let identity = "sha256:pinme";
        let pinned = table
            .resolve_with_affinity("srv.example.com", "/", Some(identity))
            .await
            .unwrap();

        // Remove the pinned agent.
        table.deregister(&pinned.agent_id).await;

        // Sticky map must no longer hold the evicted agent.
        let sticky = table.sticky.lock().unwrap();
        for (_, (aid, _)) in sticky.iter() {
            assert_ne!(aid, &pinned.agent_id);
        }
    }
}
