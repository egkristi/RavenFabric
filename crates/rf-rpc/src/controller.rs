//! Controller and management plane types.
//!
//! Defines the controller binary, REST/gRPC API, web UI,
//! OpenTelemetry traces, Prometheus metrics, and Kubernetes CRDs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Controller configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerConfig {
    /// Listen address for the controller API.
    pub listen_addr: String,
    /// Web UI enabled.
    pub web_ui: bool,
    /// Web UI static assets path.
    pub web_ui_path: Option<String>,
    /// API authentication method.
    pub auth: ControllerAuth,
    /// Connected agents.
    pub max_agents: u32,
    /// Database path for state.
    pub db_path: String,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".into(),
            web_ui: true,
            web_ui_path: None,
            auth: ControllerAuth::BearerToken {
                token_hash: String::new(),
            },
            max_agents: 10000,
            db_path: "/var/lib/ravenfabric/controller.db".into(),
        }
    }
}

/// Controller authentication methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControllerAuth {
    /// Bearer token.
    BearerToken { token_hash: String },
    /// Mutual TLS (client certificates).
    MtlsCert { ca_path: String },
    /// OIDC integration.
    Oidc { issuer: String, client_id: String },
    /// Agent key-based (Noise XX mutual auth).
    AgentKey,
}

/// REST API endpoint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    /// HTTP method.
    pub method: HttpMethod,
    /// Path pattern (e.g., "/api/v1/agents/{id}").
    pub path: String,
    /// Description.
    pub description: String,
    /// Required authentication role.
    pub required_role: String,
}

/// HTTP method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

/// gRPC service definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcService {
    /// Service name.
    pub name: String,
    /// Proto file path.
    pub proto: String,
    /// RPC methods.
    pub methods: Vec<GrpcMethod>,
}

/// gRPC method definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethod {
    /// Method name.
    pub name: String,
    /// Request type.
    pub request: String,
    /// Response type.
    pub response: String,
    /// Streaming mode.
    pub streaming: StreamingMode,
}

/// gRPC streaming modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingMode {
    Unary,
    ServerStream,
    ClientStream,
    Bidirectional,
}

/// OpenTelemetry trace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelTraceConfig {
    /// OTLP endpoint.
    pub endpoint: String,
    /// Protocol.
    pub protocol: OtelProtocol,
    /// Service name for traces.
    pub service_name: String,
    /// Sampling rate (0.0 - 1.0).
    pub sampling_rate: f64,
    /// Propagation format.
    pub propagation: TracePropagation,
    /// Resource attributes.
    pub resource_attributes: Vec<(String, String)>,
}

impl Default for OtelTraceConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".into(),
            protocol: OtelProtocol::Grpc,
            service_name: "ravenfabric".into(),
            sampling_rate: 1.0,
            propagation: TracePropagation::W3c,
            resource_attributes: Vec::new(),
        }
    }
}

/// OTLP protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OtelProtocol {
    Grpc,
    Http,
}

/// Trace propagation format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TracePropagation {
    W3c,
    B3,
    Jaeger,
}

/// Prometheus metrics endpoint configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusConfig {
    /// Listen address for /metrics endpoint.
    pub listen_addr: String,
    /// Path (default: /metrics).
    pub path: String,
    /// Metric prefix.
    pub prefix: String,
    /// Include process metrics.
    pub process_metrics: bool,
    /// Custom labels added to all metrics.
    pub global_labels: Vec<(String, String)>,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:9090".into(),
            path: "/metrics".into(),
            prefix: "ravenfabric".into(),
            process_metrics: true,
            global_labels: Vec::new(),
        }
    }
}

/// Kubernetes CRD definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KubernetesCrd {
    /// API group (e.g., "ravenfabric.io").
    pub group: String,
    /// API version (e.g., "v1alpha1").
    pub version: String,
    /// Kind (e.g., "RavenAgent", "RavenPolicy").
    pub kind: String,
    /// Plural name.
    pub plural: String,
    /// Scope.
    pub scope: CrdScope,
}

/// CRD scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrdScope {
    Namespaced,
    Cluster,
}

/// Kubernetes operator reconciliation state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileState {
    /// Resource in desired state.
    Synced,
    /// Reconciliation in progress.
    Reconciling,
    /// Error during reconciliation.
    Error { message: String },
    /// Waiting for dependencies.
    Pending { dependency: String },
}

/// Standard CRDs for the RavenFabric operator.
pub fn standard_crds() -> Vec<KubernetesCrd> {
    vec![
        KubernetesCrd {
            group: "ravenfabric.io".into(),
            version: "v1alpha1".into(),
            kind: "RavenAgent".into(),
            plural: "ravenagents".into(),
            scope: CrdScope::Namespaced,
        },
        KubernetesCrd {
            group: "ravenfabric.io".into(),
            version: "v1alpha1".into(),
            kind: "RavenPolicy".into(),
            plural: "ravenpolicies".into(),
            scope: CrdScope::Namespaced,
        },
        KubernetesCrd {
            group: "ravenfabric.io".into(),
            version: "v1alpha1".into(),
            kind: "RavenRelay".into(),
            plural: "ravenrelays".into(),
            scope: CrdScope::Cluster,
        },
        KubernetesCrd {
            group: "ravenfabric.io".into(),
            version: "v1alpha1".into(),
            kind: "RavenMesh".into(),
            plural: "ravenmeshes".into(),
            scope: CrdScope::Cluster,
        },
    ]
}

// --- Controller Agent Registry ---

/// Connected agent info tracked by the controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent identifier.
    pub id: String,
    /// Agent public key hash (for identity).
    pub key_hash: String,
    /// Last heartbeat timestamp (Unix ms).
    pub last_heartbeat_ms: u64,
    /// Agent status.
    pub status: AgentStatus,
    /// Agent version.
    pub version: String,
    /// Labels for targeting.
    pub labels: HashMap<String, String>,
}

/// Agent connection status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Connected and healthy.
    Online,
    /// Connected but degraded.
    Degraded { reason: String },
    /// Not heard from within heartbeat timeout.
    Offline,
    /// Recently enrolled, awaiting first heartbeat.
    Pending,
}

/// Agent registry — tracks connected agents and their status.
pub struct AgentRegistry {
    /// Agents by ID.
    agents: HashMap<String, AgentInfo>,
    /// Max agents.
    max_agents: u32,
    /// Heartbeat timeout (ms).
    heartbeat_timeout_ms: u64,
}

impl AgentRegistry {
    /// Create a new agent registry.
    pub fn new(max_agents: u32, heartbeat_timeout_ms: u64) -> Self {
        Self {
            agents: HashMap::new(),
            max_agents,
            heartbeat_timeout_ms,
        }
    }

    /// Register or update an agent.
    pub fn upsert(&mut self, info: AgentInfo) -> bool {
        if !self.agents.contains_key(&info.id) && self.agents.len() >= self.max_agents as usize {
            return false;
        }
        self.agents.insert(info.id.clone(), info);
        true
    }

    /// Record a heartbeat from an agent.
    pub fn heartbeat(&mut self, agent_id: &str, now_ms: u64) -> bool {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.last_heartbeat_ms = now_ms;
            agent.status = AgentStatus::Online;
            true
        } else {
            false
        }
    }

    /// Check for stale agents (no heartbeat within timeout).
    pub fn check_stale(&mut self, now_ms: u64) -> Vec<String> {
        let mut stale = Vec::new();
        for (id, agent) in &mut self.agents {
            if agent.status == AgentStatus::Online
                && now_ms.saturating_sub(agent.last_heartbeat_ms) > self.heartbeat_timeout_ms
            {
                agent.status = AgentStatus::Offline;
                stale.push(id.clone());
            }
        }
        stale
    }

    /// Get an agent by ID.
    pub fn get(&self, id: &str) -> Option<&AgentInfo> {
        self.agents.get(id)
    }

    /// List all agents.
    pub fn list(&self) -> Vec<&AgentInfo> {
        self.agents.values().collect()
    }

    /// List agents matching a label selector.
    pub fn select(&self, labels: &HashMap<String, String>) -> Vec<&AgentInfo> {
        self.agents
            .values()
            .filter(|a| labels.iter().all(|(k, v)| a.labels.get(k) == Some(v)))
            .collect()
    }

    /// Remove an agent.
    pub fn remove(&mut self, id: &str) -> bool {
        self.agents.remove(id).is_some()
    }

    /// Number of agents.
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Number of online agents.
    pub fn online_count(&self) -> usize {
        self.agents
            .values()
            .filter(|a| a.status == AgentStatus::Online)
            .count()
    }
}

// --- API Router ---

/// API route definition for the controller HTTP server.
#[derive(Debug, Clone)]
pub struct ApiRoute {
    /// HTTP method.
    pub method: HttpMethod,
    /// Path pattern.
    pub path: String,
    /// Handler name (for dispatch).
    pub handler: String,
    /// Required role.
    pub required_role: String,
}

/// API router — dispatches requests to handlers.
pub struct ApiRouter {
    /// Registered routes.
    routes: Vec<ApiRoute>,
}

impl ApiRouter {
    /// Create a new router with standard RavenFabric controller routes.
    pub fn new() -> Self {
        Self {
            routes: vec![
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/api/v1/agents".into(),
                    handler: "list_agents".into(),
                    required_role: "viewer".into(),
                },
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/api/v1/agents/{id}".into(),
                    handler: "get_agent".into(),
                    required_role: "viewer".into(),
                },
                ApiRoute {
                    method: HttpMethod::Post,
                    path: "/api/v1/agents/{id}/exec".into(),
                    handler: "exec_command".into(),
                    required_role: "operator".into(),
                },
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/api/v1/policies".into(),
                    handler: "list_policies".into(),
                    required_role: "viewer".into(),
                },
                ApiRoute {
                    method: HttpMethod::Put,
                    path: "/api/v1/policies/{name}".into(),
                    handler: "update_policy".into(),
                    required_role: "admin".into(),
                },
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/api/v1/metrics".into(),
                    handler: "get_metrics".into(),
                    required_role: "viewer".into(),
                },
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/api/v1/audit".into(),
                    handler: "get_audit_log".into(),
                    required_role: "auditor".into(),
                },
                ApiRoute {
                    method: HttpMethod::Get,
                    path: "/healthz".into(),
                    handler: "health_check".into(),
                    required_role: "".into(),
                },
            ],
        }
    }

    /// Match a request to a route.
    pub fn match_route(&self, method: &HttpMethod, path: &str) -> Option<&ApiRoute> {
        self.routes
            .iter()
            .find(|r| r.method == *method && Self::path_matches(&r.path, path))
    }

    /// Simple path matching (supports {param} placeholders).
    fn path_matches(pattern: &str, path: &str) -> bool {
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let path_parts: Vec<&str> = path.split('/').collect();

        if pattern_parts.len() != path_parts.len() {
            return false;
        }

        pattern_parts
            .iter()
            .zip(path_parts.iter())
            .all(|(p, a)| p.starts_with('{') || *p == *a)
    }

    /// Number of registered routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

impl Default for ApiRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_config_default() {
        let config = ControllerConfig::default();
        assert!(config.web_ui);
        assert_eq!(config.max_agents, 10000);
    }

    #[test]
    fn test_api_endpoint() {
        let endpoint = ApiEndpoint {
            method: HttpMethod::Get,
            path: "/api/v1/agents/{id}".into(),
            description: "Get agent details".into(),
            required_role: "viewer".into(),
        };
        let json = serde_json::to_string(&endpoint).unwrap();
        assert!(json.contains("GET"));
    }

    #[test]
    fn test_grpc_service() {
        let service = GrpcService {
            name: "AgentService".into(),
            proto: "proto/agent.proto".into(),
            methods: vec![GrpcMethod {
                name: "Execute".into(),
                request: "ExecuteRequest".into(),
                response: "ExecuteResponse".into(),
                streaming: StreamingMode::ServerStream,
            }],
        };
        let json = serde_json::to_string(&service).unwrap();
        assert!(json.contains("server_stream"));
    }

    #[test]
    fn test_otel_config_default() {
        let config = OtelTraceConfig::default();
        assert_eq!(config.service_name, "ravenfabric");
        assert_eq!(config.protocol, OtelProtocol::Grpc);
    }

    #[test]
    fn test_prometheus_config() {
        let config = PrometheusConfig::default();
        assert_eq!(config.path, "/metrics");
        assert_eq!(config.prefix, "ravenfabric");
    }

    #[test]
    fn test_standard_crds() {
        let crds = standard_crds();
        assert_eq!(crds.len(), 4);
        assert!(crds.iter().all(|c| c.group == "ravenfabric.io"));
        assert!(crds.iter().any(|c| c.kind == "RavenAgent"));
        assert!(crds.iter().any(|c| c.kind == "RavenPolicy"));
    }

    #[test]
    fn test_reconcile_state() {
        let states = [
            ReconcileState::Synced,
            ReconcileState::Reconciling,
            ReconcileState::Error {
                message: "timeout".into(),
            },
            ReconcileState::Pending {
                dependency: "relay".into(),
            },
        ];
        for s in &states {
            let json = serde_json::to_string(s).unwrap();
            let parsed: ReconcileState = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn test_controller_auth_variants() {
        let auths = [
            ControllerAuth::BearerToken {
                token_hash: "hash".into(),
            },
            ControllerAuth::MtlsCert {
                ca_path: "/etc/ca.pem".into(),
            },
            ControllerAuth::Oidc {
                issuer: "https://auth.example.com".into(),
                client_id: "client".into(),
            },
            ControllerAuth::AgentKey,
        ];
        for a in &auths {
            let json = serde_json::to_string(a).unwrap();
            let parsed: ControllerAuth = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, a);
        }
    }

    #[test]
    fn test_agent_registry_upsert() {
        let mut reg = AgentRegistry::new(100, 30_000);
        let agent = AgentInfo {
            id: "web-01".into(),
            key_hash: "abc123".into(),
            last_heartbeat_ms: 1000,
            status: AgentStatus::Pending,
            version: "0.1.0".into(),
            labels: {
                let mut m = HashMap::new();
                m.insert("role".into(), "web".into());
                m
            },
        };
        assert!(reg.upsert(agent));
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn test_agent_registry_heartbeat() {
        let mut reg = AgentRegistry::new(100, 30_000);
        reg.upsert(AgentInfo {
            id: "agent-1".into(),
            key_hash: "hash".into(),
            last_heartbeat_ms: 1000,
            status: AgentStatus::Pending,
            version: "0.1.0".into(),
            labels: HashMap::new(),
        });

        assert!(reg.heartbeat("agent-1", 5000));
        assert_eq!(reg.get("agent-1").unwrap().status, AgentStatus::Online);
        assert_eq!(reg.online_count(), 1);
    }

    #[test]
    fn test_agent_registry_stale() {
        let mut reg = AgentRegistry::new(100, 10_000);
        reg.upsert(AgentInfo {
            id: "agent-1".into(),
            key_hash: "hash".into(),
            last_heartbeat_ms: 1000,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
        });

        let stale = reg.check_stale(15_000); // 14s since heartbeat, > 10s timeout
        assert_eq!(stale.len(), 1);
        assert_eq!(reg.get("agent-1").unwrap().status, AgentStatus::Offline);
    }

    #[test]
    fn test_agent_registry_select_labels() {
        let mut reg = AgentRegistry::new(100, 30_000);
        reg.upsert(AgentInfo {
            id: "web-01".into(),
            key_hash: "h1".into(),
            last_heartbeat_ms: 1000,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: {
                let mut m = HashMap::new();
                m.insert("role".into(), "web".into());
                m
            },
        });
        reg.upsert(AgentInfo {
            id: "db-01".into(),
            key_hash: "h2".into(),
            last_heartbeat_ms: 1000,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: {
                let mut m = HashMap::new();
                m.insert("role".into(), "database".into());
                m
            },
        });

        let web_agents = reg.select(&{
            let mut m = HashMap::new();
            m.insert("role".into(), "web".into());
            m
        });
        assert_eq!(web_agents.len(), 1);
        assert_eq!(web_agents[0].id, "web-01");
    }

    #[test]
    fn test_agent_registry_capacity() {
        let mut reg = AgentRegistry::new(1, 30_000);
        reg.upsert(AgentInfo {
            id: "agent-1".into(),
            key_hash: "h".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
        });
        assert!(!reg.upsert(AgentInfo {
            id: "agent-2".into(),
            key_hash: "h".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
        }));
    }

    #[test]
    fn test_api_router_match() {
        let router = ApiRouter::new();
        assert_eq!(router.route_count(), 8);

        let route = router.match_route(&HttpMethod::Get, "/api/v1/agents");
        assert!(route.is_some());
        assert_eq!(route.unwrap().handler, "list_agents");

        let route = router.match_route(&HttpMethod::Get, "/api/v1/agents/web-01");
        assert!(route.is_some());
        assert_eq!(route.unwrap().handler, "get_agent");

        let route = router.match_route(&HttpMethod::Post, "/api/v1/agents/web-01/exec");
        assert!(route.is_some());
        assert_eq!(route.unwrap().handler, "exec_command");

        // Health check (no auth required)
        let route = router.match_route(&HttpMethod::Get, "/healthz");
        assert!(route.is_some());
        assert_eq!(route.unwrap().required_role, "");
    }

    #[test]
    fn test_api_router_no_match() {
        let router = ApiRouter::new();
        assert!(
            router
                .match_route(&HttpMethod::Delete, "/api/v1/nonexistent")
                .is_none()
        );
    }
}
