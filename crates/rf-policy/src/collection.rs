//! Collection policy — governs what telemetry data can be collected.
//!
//! Follows the same deny-by-default philosophy as the execution policy:
//! only explicitly allowed metrics, logs, and traces are collected.

use serde::{Deserialize, Serialize};

/// What type of telemetry is governed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryType {
    /// System metrics (CPU, memory, disk, etc.).
    SystemMetrics,
    /// Application metrics (scraped from endpoints).
    AppMetrics,
    /// Log tailing.
    Logs,
    /// Distributed traces.
    Traces,
    /// Network metrics (bandwidth, latency, connections).
    NetworkMetrics,
    /// Process metrics (per-process CPU, memory).
    ProcessMetrics,
}

/// A rule governing collection of a specific telemetry type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRule {
    /// What type of telemetry this rule applies to.
    pub telemetry_type: TelemetryType,
    /// Whether collection is allowed.
    pub allowed: bool,
    /// Collection interval in seconds (for metrics).
    pub interval_secs: Option<u64>,
    /// Maximum retention before flush/drop (seconds).
    pub retention_secs: Option<u64>,
    /// Patterns to include (regex for metric names, log sources).
    pub include_patterns: Vec<String>,
    /// Patterns to exclude.
    pub exclude_patterns: Vec<String>,
}

/// Full collection policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionPolicy {
    /// Policy name/version.
    pub name: String,
    /// Default: deny all collection unless explicitly allowed.
    pub default_allow: bool,
    /// Individual rules.
    pub rules: Vec<CollectionRule>,
    /// Global export destinations.
    pub exporters: Vec<ExporterConfig>,
}

impl Default for CollectionPolicy {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            default_allow: false, // deny-by-default
            rules: Vec::new(),
            exporters: Vec::new(),
        }
    }
}

impl CollectionPolicy {
    /// Check if collection of a specific telemetry type is allowed.
    pub fn is_allowed(&self, telemetry_type: &TelemetryType) -> bool {
        // Find the most specific rule
        for rule in &self.rules {
            if &rule.telemetry_type == telemetry_type {
                return rule.allowed;
            }
        }
        self.default_allow
    }

    /// Get the collection interval for a telemetry type.
    pub fn interval(&self, telemetry_type: &TelemetryType) -> Option<u64> {
        self.rules
            .iter()
            .find(|r| &r.telemetry_type == telemetry_type)
            .and_then(|r| r.interval_secs)
    }
}

/// Exporter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExporterConfig {
    /// Prometheus remote-write endpoint.
    PrometheusRemoteWrite {
        endpoint: String,
        /// Bearer token for auth.
        auth_token: Option<String>,
        /// Batch size (number of samples per request).
        batch_size: u32,
        /// Flush interval in seconds.
        flush_interval_secs: u64,
    },
    /// OpenTelemetry Protocol (OTLP) exporter.
    Otlp {
        endpoint: String,
        /// Protocol: grpc or http.
        protocol: OtlpProtocol,
        /// Headers for authentication.
        headers: Vec<(String, String)>,
        /// Export timeout in seconds.
        timeout_secs: u64,
    },
    /// InfluxDB line protocol.
    InfluxDb {
        endpoint: String,
        /// Database/bucket name.
        database: String,
        /// Auth token.
        token: Option<String>,
        /// Precision (s, ms, us, ns).
        precision: String,
    },
    /// Local file (for offline/air-gapped environments).
    File {
        path: String,
        /// Format: json, prometheus, influx.
        format: String,
        /// Max file size before rotation (bytes).
        max_size_bytes: u64,
    },
}

/// OTLP protocol variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OtlpProtocol {
    Grpc,
    Http,
}

/// Export result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportResult {
    /// Successfully exported N items.
    Success(u32),
    /// Partial failure — some items exported.
    Partial { exported: u32, failed: u32 },
    /// Complete failure.
    Failed(String),
    /// Exporter not configured/disabled.
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_denies_all() {
        let policy = CollectionPolicy::default();
        assert!(!policy.is_allowed(&TelemetryType::SystemMetrics));
        assert!(!policy.is_allowed(&TelemetryType::Logs));
        assert!(!policy.is_allowed(&TelemetryType::Traces));
    }

    #[test]
    fn test_policy_allows_specific() {
        let policy = CollectionPolicy {
            name: "custom".into(),
            default_allow: false,
            rules: vec![
                CollectionRule {
                    telemetry_type: TelemetryType::SystemMetrics,
                    allowed: true,
                    interval_secs: Some(15),
                    retention_secs: Some(300),
                    include_patterns: vec![],
                    exclude_patterns: vec![],
                },
                CollectionRule {
                    telemetry_type: TelemetryType::Logs,
                    allowed: true,
                    interval_secs: None,
                    retention_secs: Some(3600),
                    include_patterns: vec!["error".into(), "warn".into()],
                    exclude_patterns: vec!["healthcheck".into()],
                },
            ],
            exporters: vec![],
        };

        assert!(policy.is_allowed(&TelemetryType::SystemMetrics));
        assert!(policy.is_allowed(&TelemetryType::Logs));
        assert!(!policy.is_allowed(&TelemetryType::Traces));
    }

    #[test]
    fn test_policy_interval() {
        let policy = CollectionPolicy {
            rules: vec![CollectionRule {
                telemetry_type: TelemetryType::SystemMetrics,
                allowed: true,
                interval_secs: Some(30),
                retention_secs: None,
                include_patterns: vec![],
                exclude_patterns: vec![],
            }],
            ..Default::default()
        };

        assert_eq!(policy.interval(&TelemetryType::SystemMetrics), Some(30));
        assert_eq!(policy.interval(&TelemetryType::Logs), None);
    }

    #[test]
    fn test_exporter_config_serde() {
        let config = ExporterConfig::Otlp {
            endpoint: "https://otel.example.com:4317".into(),
            protocol: OtlpProtocol::Grpc,
            headers: vec![("Authorization".into(), "Bearer token123".into())],
            timeout_secs: 30,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("otlp"));
        assert!(json.contains("grpc"));

        let parsed: ExporterConfig = serde_json::from_str(&json).unwrap();
        match parsed {
            ExporterConfig::Otlp { protocol, .. } => assert_eq!(protocol, OtlpProtocol::Grpc),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_influxdb_exporter_config() {
        let config = ExporterConfig::InfluxDb {
            endpoint: "http://influx.local:8086".into(),
            database: "ravenfabric".into(),
            token: Some("my-token".into()),
            precision: "ms".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("influx_db"));
        assert!(json.contains("ravenfabric"));
    }
}
