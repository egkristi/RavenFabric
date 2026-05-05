//! Controller and management plane types.
//!
//! Defines the controller binary, REST/gRPC API, web UI,
//! OpenTelemetry traces, Prometheus metrics, and Kubernetes CRDs.

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
}
