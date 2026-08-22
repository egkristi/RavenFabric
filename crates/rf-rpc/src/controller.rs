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
    /// Geographic region code (e.g. `"eu-west"`, `"us-east"`, `"ap-south"`).
    /// Set from the agent's `raven.toml` `[agent] region` field.
    pub region: Option<String>,
    /// The relay URL this agent is currently connected through.
    /// Used for relay HA — clients can discover which relay an agent is on.
    /// `None` if the agent is in direct-listen mode.
    pub relay_url: Option<String>,
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

    /// List agents in a specific region.
    ///
    /// Matches agents whose `region` field equals `region` (case-insensitive).
    /// Returns all agents if `region` is `None`.
    pub fn select_by_region<'a>(&'a self, region: Option<&str>) -> Vec<&'a AgentInfo> {
        match region {
            None => self.agents.values().collect(),
            Some(r) => self
                .agents
                .values()
                .filter(|a| {
                    a.region
                        .as_deref()
                        .map(|ar| ar.eq_ignore_ascii_case(r))
                        .unwrap_or(false)
                })
                .collect(),
        }
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
                    method: HttpMethod::Post,
                    path: "/api/v1/agents/heartbeat".into(),
                    handler: "register_agent".into(),
                    required_role: "".into(),
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

// --- OpenTelemetry Trace Context ---

/// W3C Trace Context for distributed tracing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID (128-bit, hex-encoded).
    pub trace_id: String,
    /// Span ID (64-bit, hex-encoded).
    pub span_id: String,
    /// Parent span ID (if any).
    pub parent_span_id: Option<String>,
    /// Trace flags (sampled = 0x01).
    pub trace_flags: u8,
}

impl TraceContext {
    /// Create a new root trace context with cryptographically random IDs.
    pub fn new_root() -> Self {
        use rand::Rng;
        let mut rng = rand::rng();
        let trace_id = format!("{:032x}", rng.random::<u128>());
        let span_id = format!("{:016x}", rng.random::<u64>());
        Self {
            trace_id,
            span_id,
            parent_span_id: None,
            trace_flags: 0x01, // Sampled.
        }
    }

    /// Create a child span context.
    pub fn child(&self) -> Self {
        let span_id = format!(
            "{:016x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                & 0xFFFFFFFFFFFFFFFF
        );
        Self {
            trace_id: self.trace_id.clone(),
            span_id,
            parent_span_id: Some(self.span_id.clone()),
            trace_flags: self.trace_flags,
        }
    }

    /// Parse W3C traceparent header (version-trace_id-span_id-flags).
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }
        if parts[0] != "00" {
            return None; // Only version 00 supported.
        }
        if parts[1].len() != 32 || parts[2].len() != 16 || parts[3].len() != 2 {
            return None;
        }
        let flags = u8::from_str_radix(parts[3], 16).ok()?;
        Some(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            parent_span_id: None,
            trace_flags: flags,
        })
    }

    /// Serialize to W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id, self.span_id, self.trace_flags
        )
    }

    /// Whether this trace is sampled.
    pub fn is_sampled(&self) -> bool {
        self.trace_flags & 0x01 != 0
    }
}

/// A span in a distributed trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Span name (operation).
    pub name: String,
    /// Trace context.
    pub context: TraceContext,
    /// Start time (Unix ms).
    pub start_ms: u64,
    /// End time (Unix ms, 0 if still running).
    pub end_ms: u64,
    /// Span kind.
    pub kind: SpanKind,
    /// Attributes (key-value pairs).
    pub attributes: HashMap<String, String>,
    /// Events (timestamped annotations).
    pub events: Vec<SpanEvent>,
    /// Status.
    pub status: SpanStatus,
}

/// Span kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

/// Span event (annotation at a point in time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Timestamp (Unix ms).
    pub timestamp_ms: u64,
    /// Attributes.
    pub attributes: HashMap<String, String>,
}

/// Span completion status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Unset,
    Ok,
    Error { message: String },
}

impl Span {
    /// Create a new span.
    pub fn new(name: String, context: TraceContext, kind: SpanKind, start_ms: u64) -> Self {
        Self {
            name,
            context,
            start_ms,
            end_ms: 0,
            kind,
            attributes: HashMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
        }
    }

    /// Set an attribute.
    pub fn set_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }

    /// Add an event.
    pub fn add_event(&mut self, name: String, timestamp_ms: u64) {
        self.events.push(SpanEvent {
            name,
            timestamp_ms,
            attributes: HashMap::new(),
        });
    }

    /// End the span.
    pub fn end(&mut self, end_ms: u64) {
        self.end_ms = end_ms;
    }

    /// Set status to OK.
    pub fn set_ok(&mut self) {
        self.status = SpanStatus::Ok;
    }

    /// Set status to Error.
    pub fn set_error(&mut self, message: String) {
        self.status = SpanStatus::Error { message };
    }

    /// Duration in ms (0 if not ended).
    pub fn duration_ms(&self) -> u64 {
        if self.end_ms > 0 {
            self.end_ms.saturating_sub(self.start_ms)
        } else {
            0
        }
    }

    /// Export span as OTLP-compatible JSON.
    pub fn to_otlp_json(&self) -> serde_json::Value {
        serde_json::json!({
            "traceId": self.context.trace_id,
            "spanId": self.context.span_id,
            "parentSpanId": self.context.parent_span_id,
            "name": self.name,
            "kind": format!("{:?}", self.kind),
            "startTimeUnixNano": self.start_ms * 1_000_000,
            "endTimeUnixNano": self.end_ms * 1_000_000,
            "attributes": self.attributes.iter().map(|(k, v)| {
                serde_json::json!({"key": k, "value": {"stringValue": v}})
            }).collect::<Vec<_>>(),
            "events": self.events.iter().map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "timeUnixNano": e.timestamp_ms * 1_000_000,
                })
            }).collect::<Vec<_>>(),
            "status": match &self.status {
                SpanStatus::Unset => serde_json::json!({"code": 0}),
                SpanStatus::Ok => serde_json::json!({"code": 1}),
                SpanStatus::Error { message } => serde_json::json!({"code": 2, "message": message}),
            },
        })
    }
}

// --- REST API Request/Response ---

/// API request representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    /// HTTP method.
    pub method: HttpMethod,
    /// Request path.
    pub path: String,
    /// Request body (JSON).
    pub body: Option<serde_json::Value>,
    /// Authentication token.
    pub auth_token: Option<String>,
    /// Trace context (from traceparent header).
    pub trace_context: Option<TraceContext>,
}

/// API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    /// HTTP status code.
    pub status_code: u16,
    /// Response body.
    pub body: serde_json::Value,
    /// Trace context (for traceparent response header).
    pub trace_context: Option<TraceContext>,
}

impl ApiResponse {
    /// Create a 200 OK response.
    pub fn ok(body: serde_json::Value) -> Self {
        Self {
            status_code: 200,
            body,
            trace_context: None,
        }
    }

    /// Create a 201 Created response.
    pub fn created(body: serde_json::Value) -> Self {
        Self {
            status_code: 201,
            body,
            trace_context: None,
        }
    }

    /// Create a 400 Bad Request response.
    pub fn bad_request(message: &str) -> Self {
        Self {
            status_code: 400,
            body: serde_json::json!({"error": message}),
            trace_context: None,
        }
    }

    /// Create a 401 Unauthorized response.
    pub fn unauthorized() -> Self {
        Self {
            status_code: 401,
            body: serde_json::json!({"error": "unauthorized"}),
            trace_context: None,
        }
    }

    /// Create a 403 Forbidden response.
    pub fn forbidden() -> Self {
        Self {
            status_code: 403,
            body: serde_json::json!({"error": "forbidden"}),
            trace_context: None,
        }
    }

    /// Create a 404 Not Found response.
    pub fn not_found() -> Self {
        Self {
            status_code: 404,
            body: serde_json::json!({"error": "not found"}),
            trace_context: None,
        }
    }

    /// Create a 500 Internal Server Error.
    pub fn internal_error(message: &str) -> Self {
        Self {
            status_code: 500,
            body: serde_json::json!({"error": message}),
            trace_context: None,
        }
    }

    /// Attach trace context.
    pub fn with_trace(mut self, ctx: TraceContext) -> Self {
        self.trace_context = Some(ctx);
        self
    }
}

/// API request dispatcher — routes requests to handlers.
pub struct ApiDispatcher {
    router: ApiRouter,
    registry: AgentRegistry,
}

impl ApiDispatcher {
    /// Create a new dispatcher.
    pub fn new(registry: AgentRegistry) -> Self {
        Self {
            router: ApiRouter::new(),
            registry,
        }
    }

    /// Dispatch a request and return a response.
    pub fn dispatch(&mut self, request: &ApiRequest) -> ApiResponse {
        let route = match self.router.match_route(&request.method, &request.path) {
            Some(r) => r,
            None => return ApiResponse::not_found(),
        };

        // Check authentication.
        if !route.required_role.is_empty() && request.auth_token.is_none() {
            return ApiResponse::unauthorized();
        }

        // Dispatch to handler.
        match route.handler.as_str() {
            "list_agents" => {
                let agents: Vec<_> = self
                    .registry
                    .list()
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "id": a.id,
                            "status": a.status,
                            "version": a.version,
                            "labels": a.labels,
                        })
                    })
                    .collect();
                ApiResponse::ok(serde_json::json!({"agents": agents, "total": agents.len()}))
            }
            "get_agent" => {
                let id = request.path.strip_prefix("/api/v1/agents/").unwrap_or("");
                match self.registry.get(id) {
                    Some(agent) => ApiResponse::ok(serde_json::to_value(agent).unwrap_or_default()),
                    None => ApiResponse::not_found(),
                }
            }
            "register_agent" => {
                // POST /api/v1/agents/heartbeat — agent registers or refreshes
                // its heartbeat in the registry.
                let body = match request.body.as_ref() {
                    Some(b) => b.clone(),
                    None => serde_json::Value::Null,
                };

                let id = body
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    return ApiResponse::bad_request("missing required field: id");
                }

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let info = AgentInfo {
                    id: id.clone(),
                    key_hash: body
                        .get("key_hash")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    last_heartbeat_ms: now_ms,
                    status: AgentStatus::Online,
                    version: body
                        .get("version")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    labels: body
                        .get("labels")
                        .and_then(serde_json::Value::as_object)
                        .map(|m| {
                            m.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                .collect()
                        })
                        .unwrap_or_default(),
                    region: body
                        .get("region")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    relay_url: body
                        .get("relay_url")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                };

                self.registry.upsert(info);
                ApiResponse::ok(serde_json::json!({
                    "status": "registered",
                    "agent_id": id,
                    "heartbeat_ms": now_ms,
                }))
            }
            "health_check" => ApiResponse::ok(serde_json::json!({
                "status": "healthy",
                "agents_online": self.registry.online_count(),
                "agents_total": self.registry.count(),
            })),
            _ => ApiResponse::ok(serde_json::json!({"handler": route.handler})),
        }
    }
}

// --- Kubernetes Reconciler ---

/// Action to take during reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileAction {
    /// Create a new resource.
    Create { kind: String, name: String },
    /// Update an existing resource.
    Update {
        kind: String,
        name: String,
        diff: String,
    },
    /// Delete a resource.
    Delete { kind: String, name: String },
    /// No action needed.
    Skip {
        kind: String,
        name: String,
        reason: String,
    },
}

/// Desired state for a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesiredState {
    /// Resource kind.
    pub kind: String,
    /// Resource name.
    pub name: String,
    /// Resource spec (JSON).
    pub spec: serde_json::Value,
}

/// Current observed state of a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedState {
    /// Resource kind.
    pub kind: String,
    /// Resource name.
    pub name: String,
    /// Resource spec (JSON).
    pub spec: serde_json::Value,
    /// Reconcile state.
    pub state: ReconcileState,
}

/// Reconciler — compares desired and observed state, produces actions.
pub struct Reconciler {
    /// Desired state.
    desired: Vec<DesiredState>,
    /// Observed state.
    observed: Vec<ObservedState>,
}

impl Reconciler {
    /// Create a new reconciler.
    pub fn new() -> Self {
        Self {
            desired: Vec::new(),
            observed: Vec::new(),
        }
    }

    /// Set the desired state.
    pub fn set_desired(&mut self, states: Vec<DesiredState>) {
        self.desired = states;
    }

    /// Set the observed state.
    pub fn set_observed(&mut self, states: Vec<ObservedState>) {
        self.observed = states;
    }

    /// Compute the diff and produce reconciliation actions.
    pub fn plan(&self) -> Vec<ReconcileAction> {
        let mut actions = Vec::new();

        // Build lookup of observed resources.
        let observed_map: HashMap<(&str, &str), &ObservedState> = self
            .observed
            .iter()
            .map(|o| ((o.kind.as_str(), o.name.as_str()), o))
            .collect();

        // Check each desired resource.
        for d in &self.desired {
            match observed_map.get(&(d.kind.as_str(), d.name.as_str())) {
                Some(obs) => {
                    if obs.spec != d.spec {
                        actions.push(ReconcileAction::Update {
                            kind: d.kind.clone(),
                            name: d.name.clone(),
                            diff: "spec changed".to_string(),
                        });
                    } else {
                        actions.push(ReconcileAction::Skip {
                            kind: d.kind.clone(),
                            name: d.name.clone(),
                            reason: "already in desired state".into(),
                        });
                    }
                }
                None => {
                    actions.push(ReconcileAction::Create {
                        kind: d.kind.clone(),
                        name: d.name.clone(),
                    });
                }
            }
        }

        // Check for resources that exist but aren't desired (orphans).
        let desired_map: HashMap<(&str, &str), &DesiredState> = self
            .desired
            .iter()
            .map(|d| ((d.kind.as_str(), d.name.as_str()), d))
            .collect();

        for o in &self.observed {
            if !desired_map.contains_key(&(o.kind.as_str(), o.name.as_str())) {
                actions.push(ReconcileAction::Delete {
                    kind: o.kind.clone(),
                    name: o.name.clone(),
                });
            }
        }

        actions
    }

    /// Number of desired resources.
    pub fn desired_count(&self) -> usize {
        self.desired.len()
    }

    /// Number of observed resources.
    pub fn observed_count(&self) -> usize {
        self.observed.len()
    }
}

impl Default for Reconciler {
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
            region: None,
            relay_url: None,
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
            region: None,
            relay_url: None,
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
            region: None,
            relay_url: None,
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
            region: None,
            relay_url: None,
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
            region: None,
            relay_url: None,
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
            region: None,
            relay_url: None,
        });
        assert!(!reg.upsert(AgentInfo {
            id: "agent-2".into(),
            key_hash: "h".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
            region: None,
            relay_url: None,
        }));
    }

    #[test]
    fn test_agent_registry_select_by_region() {
        let mut reg = AgentRegistry::new(100, 30_000);
        reg.upsert(AgentInfo {
            id: "eu-01".into(),
            key_hash: "h1".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
            region: Some("eu-west".into()),
            relay_url: None,
        });
        reg.upsert(AgentInfo {
            id: "us-01".into(),
            key_hash: "h2".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
            region: Some("us-east".into()),
            relay_url: None,
        });
        reg.upsert(AgentInfo {
            id: "no-region".into(),
            key_hash: "h3".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
            region: None,
            relay_url: None,
        });

        // Filter by eu-west
        let eu = reg.select_by_region(Some("eu-west"));
        assert_eq!(eu.len(), 1);
        assert_eq!(eu[0].id, "eu-01");

        // Case-insensitive match
        let eu_upper = reg.select_by_region(Some("EU-WEST"));
        assert_eq!(eu_upper.len(), 1);

        // Filter by us-east
        let us = reg.select_by_region(Some("us-east"));
        assert_eq!(us.len(), 1);
        assert_eq!(us[0].id, "us-01");

        // None region returns all agents
        let all = reg.select_by_region(None);
        assert_eq!(all.len(), 3);

        // Non-existent region returns empty
        let none = reg.select_by_region(Some("ap-south"));
        assert!(none.is_empty());
    }

    #[test]
    fn test_api_router_match() {
        let router = ApiRouter::new();
        assert_eq!(router.route_count(), 9);

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

    #[test]
    fn test_trace_context_traceparent() {
        let ctx = TraceContext::from_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert!(ctx.is_sampled());

        let header = ctx.to_traceparent();
        assert!(header.starts_with("00-"));
        assert!(header.ends_with("-01"));
    }

    #[test]
    fn test_trace_context_invalid() {
        assert!(TraceContext::from_traceparent("invalid").is_none());
        assert!(TraceContext::from_traceparent("01-abc-def-00").is_none()); // Wrong version.
    }

    #[test]
    fn test_trace_context_child() {
        let root = TraceContext::from_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .unwrap();
        let child = root.child();
        assert_eq!(child.trace_id, root.trace_id); // Same trace.
        assert_ne!(child.span_id, root.span_id); // Different span.
        assert_eq!(child.parent_span_id, Some(root.span_id));
    }

    #[test]
    fn test_span_lifecycle() {
        let ctx = TraceContext::from_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .unwrap();
        let mut span = Span::new("test_op".into(), ctx, SpanKind::Server, 1000);
        span.set_attribute("http.method".into(), "GET".into());
        span.add_event("started".into(), 1000);
        span.set_ok();
        span.end(2000);

        assert_eq!(span.duration_ms(), 1000);
        assert_eq!(span.status, SpanStatus::Ok);
        assert_eq!(span.attributes.get("http.method").unwrap(), "GET");
        assert_eq!(span.events.len(), 1);
    }

    #[test]
    fn test_span_otlp_json() {
        let ctx = TraceContext::from_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .unwrap();
        let mut span = Span::new("rpc.exec".into(), ctx, SpanKind::Server, 1000);
        span.end(2000);
        span.set_ok();

        let json = span.to_otlp_json();
        assert_eq!(json["name"], "rpc.exec");
        assert_eq!(json["traceId"], "4bf92f3577b34da6a3ce929d0e0e4736");
    }

    #[test]
    fn test_api_response_helpers() {
        let ok = ApiResponse::ok(serde_json::json!({"data": 1}));
        assert_eq!(ok.status_code, 200);

        let not_found = ApiResponse::not_found();
        assert_eq!(not_found.status_code, 404);

        let unauth = ApiResponse::unauthorized();
        assert_eq!(unauth.status_code, 401);

        let forbidden = ApiResponse::forbidden();
        assert_eq!(forbidden.status_code, 403);

        let err = ApiResponse::internal_error("boom");
        assert_eq!(err.status_code, 500);
    }

    #[test]
    fn test_api_dispatcher_health() {
        let reg = AgentRegistry::new(100, 30_000);
        let mut dispatcher = ApiDispatcher::new(reg);
        let req = ApiRequest {
            method: HttpMethod::Get,
            path: "/healthz".into(),
            body: None,
            auth_token: None,
            trace_context: None,
        };
        let resp = dispatcher.dispatch(&req);
        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.body["status"], "healthy");
    }

    #[test]
    fn test_api_dispatcher_list_agents() {
        let mut reg = AgentRegistry::new(100, 30_000);
        reg.upsert(AgentInfo {
            id: "web-01".into(),
            key_hash: "h".into(),
            last_heartbeat_ms: 0,
            status: AgentStatus::Online,
            version: "0.1.0".into(),
            labels: HashMap::new(),
            region: None,
            relay_url: None,
        });
        let mut dispatcher = ApiDispatcher::new(reg);
        let req = ApiRequest {
            method: HttpMethod::Get,
            path: "/api/v1/agents".into(),
            body: None,
            auth_token: Some("token".into()),
            trace_context: None,
        };
        let resp = dispatcher.dispatch(&req);
        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.body["total"], 1);
    }

    #[test]
    fn test_api_dispatcher_unauthorized() {
        let reg = AgentRegistry::new(100, 30_000);
        let mut dispatcher = ApiDispatcher::new(reg);
        let req = ApiRequest {
            method: HttpMethod::Get,
            path: "/api/v1/agents".into(),
            body: None,
            auth_token: None, // No auth.
            trace_context: None,
        };
        let resp = dispatcher.dispatch(&req);
        assert_eq!(resp.status_code, 401);
    }

    #[test]
    fn test_api_dispatcher_register_agent() {
        let reg = AgentRegistry::new(100, 30_000);
        let mut dispatcher = ApiDispatcher::new(reg);
        let req = ApiRequest {
            method: HttpMethod::Post,
            path: "/api/v1/agents/heartbeat".into(),
            body: Some(serde_json::json!({
                "id": "web-02",
                "key_hash": "abc",
                "version": "1.0.0-rc.12",
                "region": "eu-west",
                "relay_url": "wss://relay.example.com/meet",
                "labels": {"role": "web"}
            })),
            auth_token: None,
            trace_context: None,
        };
        let resp = dispatcher.dispatch(&req);
        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.body["status"], "registered");
        assert_eq!(resp.body["agent_id"], "web-02");

        // Now list agents should include the registered agent.
        let list_req = ApiRequest {
            method: HttpMethod::Get,
            path: "/api/v1/agents".into(),
            body: None,
            auth_token: Some("token".into()),
            trace_context: None,
        };
        let list_resp = dispatcher.dispatch(&list_req);
        assert_eq!(list_resp.status_code, 200);
        assert_eq!(list_resp.body["total"], 1);
    }

    #[test]
    fn test_reconciler_create() {
        let mut rec = Reconciler::new();
        rec.set_desired(vec![DesiredState {
            kind: "RavenAgent".into(),
            name: "web-01".into(),
            spec: serde_json::json!({"replicas": 1}),
        }]);
        rec.set_observed(vec![]);

        let actions = rec.plan();
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0], ReconcileAction::Create { kind, name } if kind == "RavenAgent" && name == "web-01")
        );
    }

    #[test]
    fn test_reconciler_update() {
        let mut rec = Reconciler::new();
        rec.set_desired(vec![DesiredState {
            kind: "RavenAgent".into(),
            name: "web-01".into(),
            spec: serde_json::json!({"replicas": 2}),
        }]);
        rec.set_observed(vec![ObservedState {
            kind: "RavenAgent".into(),
            name: "web-01".into(),
            spec: serde_json::json!({"replicas": 1}),
            state: ReconcileState::Synced,
        }]);

        let actions = rec.plan();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ReconcileAction::Update { .. }));
    }

    #[test]
    fn test_reconciler_delete_orphan() {
        let mut rec = Reconciler::new();
        rec.set_desired(vec![]);
        rec.set_observed(vec![ObservedState {
            kind: "RavenAgent".into(),
            name: "orphan".into(),
            spec: serde_json::json!({}),
            state: ReconcileState::Synced,
        }]);

        let actions = rec.plan();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ReconcileAction::Delete { name, .. } if name == "orphan"));
    }

    #[test]
    fn test_reconciler_skip_synced() {
        let spec = serde_json::json!({"replicas": 1});
        let mut rec = Reconciler::new();
        rec.set_desired(vec![DesiredState {
            kind: "RavenAgent".into(),
            name: "web-01".into(),
            spec: spec.clone(),
        }]);
        rec.set_observed(vec![ObservedState {
            kind: "RavenAgent".into(),
            name: "web-01".into(),
            spec,
            state: ReconcileState::Synced,
        }]);

        let actions = rec.plan();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ReconcileAction::Skip { .. }));
    }
}
