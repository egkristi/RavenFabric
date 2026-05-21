//! Routing table: maps inbound requests to registered agent upstreams.
//!
//! An agent registers by sending `IngressRegister { agent_id, upstream_url,
//! subdomain, path_prefix }` over its authenticated RPC channel.  The table
//! keeps the upstream URL and provides matching logic for incoming HTTP
//! requests.

use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// An entry in the routing table representing one registered agent upstream.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub upstream_url: String,
    pub subdomain: Option<String>,
    pub path_prefix: Option<String>,
}

/// Shared routing table, safe for concurrent reads and writes.
#[derive(Debug, Clone, Default)]
pub struct RoutingTable {
    inner: Arc<RwLock<HashMap<String, AgentEntry>>>,
}

impl RoutingTable {
    /// Create an empty routing table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update an agent entry keyed by `agent_id`.
    pub async fn register(&self, entry: AgentEntry) {
        let mut map = self.inner.write().await;
        map.insert(entry.agent_id.clone(), entry);
    }

    /// Remove a registered agent.
    pub async fn deregister(&self, agent_id: &str) {
        let mut map = self.inner.write().await;
        map.remove(agent_id);
    }

    /// Resolve the best matching agent for a request with the given `host`
    /// header and `path`.
    ///
    /// Matching priority:
    /// 1. Subdomain match (host starts with `<subdomain>.`) and optional path
    ///    prefix match.
    /// 2. Path-prefix-only match (ignoring host).
    /// 3. First registered entry as catch-all (single-tenant mode).
    pub async fn resolve(&self, host: &str, path: &str) -> Option<AgentEntry> {
        let map = self.inner.read().await;
        let entries: Vec<&AgentEntry> = map.values().collect();

        // 1. Subdomain + optional path prefix
        for entry in &entries {
            if let Some(sub) = &entry.subdomain {
                let prefix = format!("{sub}.");
                if host.starts_with(prefix.as_str()) {
                    match &entry.path_prefix {
                        Some(pp) if !path.starts_with(pp.as_str()) => continue,
                        _ => return Some((*entry).clone()),
                    }
                }
            }
        }

        // 2. Path prefix only
        for entry in &entries {
            if let Some(pp) = &entry.path_prefix {
                if path.starts_with(pp.as_str()) {
                    return Some((*entry).clone());
                }
            }
        }

        // 3. Catch-all: single entry
        if entries.len() == 1 {
            return entries.first().map(|e| (*e).clone());
        }

        None
    }

    /// Number of registered agents.
    pub async fn len(&self) -> usize {
        self.inner.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
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
}
