//! Metrics collector framework — plugin trait, system metrics, offline buffering,
//! and DTN metrics propagation.
//!
//! Provides a trait for metric collectors and a built-in system metrics collector
//! that gathers CPU, memory, disk, and network statistics.
//! Metrics can be propagated over DTN (store-carry-forward) paths for
//! disconnected or mesh-networked nodes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// A single metric data point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricPoint {
    /// Metric name (e.g., "cpu_usage_percent", "memory_used_bytes").
    pub name: String,
    /// Metric value.
    pub value: MetricValue,
    /// Labels/tags for dimensional metrics.
    pub labels: HashMap<String, String>,
    /// Timestamp in milliseconds since epoch.
    pub timestamp_ms: u64,
}

/// Value type for a metric.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MetricValue {
    /// Gauge (current value, can go up or down).
    Gauge(f64),
    /// Counter (monotonically increasing).
    Counter(u64),
    /// Histogram bucket.
    Histogram { sum: f64, count: u64 },
}

/// Trait for metric collectors.
pub trait MetricCollector: Send + Sync {
    /// Collector name.
    fn name(&self) -> &str;

    /// Collect metrics. Returns collected data points.
    fn collect(&mut self) -> Vec<MetricPoint>;

    /// How often this collector should be polled.
    fn interval(&self) -> Duration;
}

/// Built-in system metrics collector.
#[derive(Debug)]
pub struct SystemMetricsCollector {
    interval: Duration,
    #[cfg(feature = "sysinfo")]
    sys: sysinfo::System,
}

impl SystemMetricsCollector {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            #[cfg(feature = "sysinfo")]
            sys: sysinfo::System::new_all(),
        }
    }

    /// Collect basic system metrics (CPU, memory, load).
    #[cfg(feature = "sysinfo")]
    pub fn collect_system_metrics(&mut self, timestamp_ms: u64) -> Vec<MetricPoint> {
        use sysinfo::System;

        self.sys.refresh_all();

        let cpu_usage = self.sys.global_cpu_usage() as f64;
        let mem_used = self.sys.used_memory() as f64;
        let mem_total = self.sys.total_memory() as f64;
        let load_avg = System::load_average();

        let mut points = vec![
            MetricPoint {
                name: "system_cpu_usage_percent".into(),
                value: MetricValue::Gauge(cpu_usage),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_memory_used_bytes".into(),
                value: MetricValue::Gauge(mem_used),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_memory_total_bytes".into(),
                value: MetricValue::Gauge(mem_total),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_load_1m".into(),
                value: MetricValue::Gauge(load_avg.one),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_load_5m".into(),
                value: MetricValue::Gauge(load_avg.five),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_load_15m".into(),
                value: MetricValue::Gauge(load_avg.fifteen),
                labels: HashMap::new(),
                timestamp_ms,
            },
        ];

        // Per-disk metrics
        let disks = sysinfo::Disks::new_with_refreshed_list();
        for disk in disks.list() {
            let mount = disk.mount_point().to_string_lossy().to_string();
            let mut labels = HashMap::new();
            labels.insert("mount".into(), mount);

            points.push(MetricPoint {
                name: "system_disk_total_bytes".into(),
                value: MetricValue::Gauge(disk.total_space() as f64),
                labels: labels.clone(),
                timestamp_ms,
            });
            points.push(MetricPoint {
                name: "system_disk_available_bytes".into(),
                value: MetricValue::Gauge(disk.available_space() as f64),
                labels,
                timestamp_ms,
            });
        }

        points
    }

    /// Collect basic system metrics (stub when sysinfo feature is disabled).
    #[cfg(not(feature = "sysinfo"))]
    pub fn collect_system_metrics(&mut self, timestamp_ms: u64) -> Vec<MetricPoint> {
        vec![
            MetricPoint {
                name: "system_cpu_usage_percent".into(),
                value: MetricValue::Gauge(0.0),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_memory_used_bytes".into(),
                value: MetricValue::Gauge(0.0),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_memory_total_bytes".into(),
                value: MetricValue::Gauge(0.0),
                labels: HashMap::new(),
                timestamp_ms,
            },
            MetricPoint {
                name: "system_load_1m".into(),
                value: MetricValue::Gauge(0.0),
                labels: HashMap::new(),
                timestamp_ms,
            },
        ]
    }
}

impl MetricCollector for SystemMetricsCollector {
    fn name(&self) -> &str {
        "system"
    }

    fn collect(&mut self) -> Vec<MetricPoint> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.collect_system_metrics(ts)
    }

    fn interval(&self) -> Duration {
        self.interval
    }
}

/// RavenFabric-specific metrics collector.
///
/// Tracks agent-level metrics: active connections, commands allowed/denied,
/// audit entries, and handshake latency. These are collected from shared
/// atomic counters that the executor updates during operation.
#[derive(Debug)]
pub struct RavenFabricMetricsCollector {
    interval: Duration,
    /// Shared counter: total commands allowed.
    commands_allowed: Arc<std::sync::atomic::AtomicU64>,
    /// Shared counter: total commands denied.
    commands_denied: Arc<std::sync::atomic::AtomicU64>,
    /// Shared counter: total audit entries written.
    audit_entries: Arc<std::sync::atomic::AtomicU64>,
    /// Shared counter: active connections.
    active_connections: Arc<std::sync::atomic::AtomicI64>,
    /// Shared counter: total handshakes completed.
    handshakes_completed: Arc<std::sync::atomic::AtomicU64>,
    /// Shared counter: cumulative handshake latency in microseconds.
    handshake_latency_us: Arc<std::sync::atomic::AtomicU64>,
}

impl RavenFabricMetricsCollector {
    /// Create a new RavenFabric metrics collector with shared counters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        interval: Duration,
        commands_allowed: Arc<std::sync::atomic::AtomicU64>,
        commands_denied: Arc<std::sync::atomic::AtomicU64>,
        audit_entries: Arc<std::sync::atomic::AtomicU64>,
        active_connections: Arc<std::sync::atomic::AtomicI64>,
        handshakes_completed: Arc<std::sync::atomic::AtomicU64>,
        handshake_latency_us: Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            interval,
            commands_allowed,
            commands_denied,
            audit_entries,
            active_connections,
            handshakes_completed,
            handshake_latency_us,
        }
    }

    /// Create a new RavenFabric metrics collector with fresh counters.
    pub fn new_with_counters(interval: Duration) -> Self {
        Self {
            interval,
            commands_allowed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            commands_denied: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            audit_entries: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            active_connections: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            handshakes_completed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            handshake_latency_us: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Get references to the shared counters for wiring into the executor.
    #[allow(clippy::too_many_arguments)]
    pub fn counters(
        &self,
    ) -> (
        &Arc<std::sync::atomic::AtomicU64>,
        &Arc<std::sync::atomic::AtomicU64>,
        &Arc<std::sync::atomic::AtomicU64>,
        &Arc<std::sync::atomic::AtomicI64>,
        &Arc<std::sync::atomic::AtomicU64>,
        &Arc<std::sync::atomic::AtomicU64>,
    ) {
        (
            &self.commands_allowed,
            &self.commands_denied,
            &self.audit_entries,
            &self.active_connections,
            &self.handshakes_completed,
            &self.handshake_latency_us,
        )
    }
}

impl MetricCollector for RavenFabricMetricsCollector {
    fn name(&self) -> &str {
        "ravenfabric"
    }

    fn collect(&mut self) -> Vec<MetricPoint> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let allowed = self.commands_allowed.load(std::sync::atomic::Ordering::Relaxed);
        let denied = self.commands_denied.load(std::sync::atomic::Ordering::Relaxed);
        let audit = self.audit_entries.load(std::sync::atomic::Ordering::Relaxed);
        let connections = self.active_connections.load(std::sync::atomic::Ordering::Relaxed);
        let handshakes = self.handshakes_completed.load(std::sync::atomic::Ordering::Relaxed);
        let latency_us = self.handshake_latency_us.load(std::sync::atomic::Ordering::Relaxed);

        let avg_latency_ms = if handshakes > 0 {
            latency_us as f64 / handshakes as f64 / 1000.0
        } else {
            0.0
        };

        vec![
            MetricPoint {
                name: "ravenfabric_commands_allowed_total".into(),
                value: MetricValue::Counter(allowed),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
            MetricPoint {
                name: "ravenfabric_commands_denied_total".into(),
                value: MetricValue::Counter(denied),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
            MetricPoint {
                name: "ravenfabric_audit_entries_total".into(),
                value: MetricValue::Counter(audit),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
            MetricPoint {
                name: "ravenfabric_active_connections".into(),
                value: MetricValue::Gauge(connections as f64),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
            MetricPoint {
                name: "ravenfabric_handshakes_completed_total".into(),
                value: MetricValue::Counter(handshakes),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
            MetricPoint {
                name: "ravenfabric_handshake_latency_avg_ms".into(),
                value: MetricValue::Gauge(avg_latency_ms),
                labels: HashMap::new(),
                timestamp_ms: ts,
            },
        ]
    }

    fn interval(&self) -> Duration {
        self.interval
    }
}

/// Offline metric buffer — stores metrics when disconnected, flushes on reconnect.
#[derive(Debug)]
pub struct MetricBuffer {
    /// Buffered metrics waiting to be sent.
    buffer: Vec<MetricPoint>,
    /// Maximum buffer size (prevents memory exhaustion).
    max_size: usize,
    /// Total points dropped due to buffer overflow.
    dropped: u64,
    /// When the buffer was last flushed.
    last_flush: Option<Instant>,
}

impl MetricBuffer {
    /// Create a new metric buffer with max capacity.
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_size,
            dropped: 0,
            last_flush: None,
        }
    }

    /// Add a metric point to the buffer.
    pub fn push(&mut self, point: MetricPoint) {
        if self.buffer.len() >= self.max_size {
            // Drop oldest when full
            self.buffer.remove(0);
            self.dropped += 1;
        }
        self.buffer.push(point);
    }

    /// Add multiple points.
    pub fn push_batch(&mut self, points: Vec<MetricPoint>) {
        for point in points {
            self.push(point);
        }
    }

    /// Flush (drain) all buffered metrics for sending.
    pub fn flush(&mut self) -> Vec<MetricPoint> {
        self.last_flush = Some(Instant::now());
        std::mem::take(&mut self.buffer)
    }

    /// Number of buffered points.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Total dropped points.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Time since last flush.
    pub fn time_since_flush(&self) -> Option<Duration> {
        self.last_flush.map(|t| t.elapsed())
    }
}

/// Format metric points as Prometheus exposition format.
pub fn to_prometheus(points: &[MetricPoint]) -> String {
    let mut output = String::new();
    for point in points {
        let labels_str = if point.labels.is_empty() {
            String::new()
        } else {
            let pairs: Vec<String> = point
                .labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{v}\""))
                .collect();
            format!("{{{}}}", pairs.join(","))
        };

        let value_str = match &point.value {
            MetricValue::Gauge(v) | MetricValue::Histogram { sum: v, .. } => format!("{v}"),
            MetricValue::Counter(v) => format!("{v}"),
        };

        output.push_str(&format!(
            "{}{} {} {}\n",
            point.name, labels_str, value_str, point.timestamp_ms
        ));
    }
    output
}

/// Collection policy — defines which metrics to collect, sampling rates,
/// and filtering rules for the data collection agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionPolicy {
    /// Metric name patterns to include (regex-style glob).
    pub include_patterns: Vec<String>,
    /// Metric name patterns to exclude (overrides include).
    pub exclude_patterns: Vec<String>,
    /// Sampling rate (0.0-1.0). 1.0 = collect all, 0.5 = collect half.
    pub sample_rate: f64,
    /// Minimum collection interval (prevents over-collection).
    pub min_interval: Duration,
    /// Maximum batch size per flush.
    pub max_batch_size: usize,
    /// Label filters: only collect metrics with matching labels.
    pub label_filters: HashMap<String, String>,
    /// Whether to collect histogram metrics.
    pub collect_histograms: bool,
}

impl Default for CollectionPolicy {
    fn default() -> Self {
        Self {
            include_patterns: vec!["*".to_string()],
            exclude_patterns: Vec::new(),
            sample_rate: 1.0,
            min_interval: Duration::from_secs(10),
            max_batch_size: 1000,
            label_filters: HashMap::new(),
            collect_histograms: true,
        }
    }
}

impl CollectionPolicy {
    /// Check if a metric should be collected based on the policy.
    pub fn should_collect(&self, point: &MetricPoint) -> bool {
        // Check histograms
        if !self.collect_histograms && matches!(point.value, MetricValue::Histogram { .. }) {
            return false;
        }

        // Check exclude patterns first (they override includes)
        for pattern in &self.exclude_patterns {
            if Self::matches_glob(pattern, &point.name) {
                return false;
            }
        }

        // Check include patterns
        let included = self.include_patterns.is_empty()
            || self
                .include_patterns
                .iter()
                .any(|p| Self::matches_glob(p, &point.name));

        if !included {
            return false;
        }

        // Check label filters
        for (key, expected_value) in &self.label_filters {
            match point.labels.get(key) {
                Some(actual) if actual == expected_value => {}
                _ => return false,
            }
        }

        // Sampling (deterministic based on metric name for consistency)
        if self.sample_rate < 1.0 {
            let hash = Self::simple_hash(&point.name);
            let threshold = (self.sample_rate * u32::MAX as f64) as u32;
            if hash > threshold {
                return false;
            }
        }

        true
    }

    /// Apply the policy to a batch of metrics.
    pub fn filter_batch(&self, points: Vec<MetricPoint>) -> Vec<MetricPoint> {
        let filtered: Vec<MetricPoint> = points
            .into_iter()
            .filter(|p| self.should_collect(p))
            .collect();

        if filtered.len() > self.max_batch_size {
            filtered.into_iter().take(self.max_batch_size).collect()
        } else {
            filtered
        }
    }

    /// Simple glob matching (supports * and ? wildcards).
    fn matches_glob(pattern: &str, text: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        // Simple prefix/suffix matching for common patterns
        if let Some(prefix) = pattern.strip_suffix('*') {
            return text.starts_with(prefix);
        }
        if let Some(suffix) = pattern.strip_prefix('*') {
            return text.ends_with(suffix);
        }

        // Exact match
        pattern == text
    }

    /// Simple deterministic hash for sampling.
    fn simple_hash(s: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(u32::from(byte));
        }
        hash
    }
}

// --- DTN Metrics Propagation ---

/// DTN metrics propagator — wraps collected metrics into DTN bundles for
/// store-carry-forward delivery across disconnected mesh paths.
///
/// Ensures monitoring data reaches the controller even when nodes are
/// intermittently connected, via the DTN queue infrastructure.
pub struct MetricsPropagator {
    /// This node's agent ID.
    agent_id: String,
    /// Destination for metrics (typically "controller" or a group label).
    destination: String,
    /// Buffer for metrics awaiting bundling.
    pending: Vec<MetricPoint>,
    /// Maximum metrics per bundle (controls bundle size).
    max_per_bundle: usize,
    /// TTL for metric bundles (seconds).
    bundle_ttl_secs: u64,
    /// Monotonic bundle sequence number.
    sequence: u64,
}

impl MetricsPropagator {
    /// Create a new propagator.
    pub fn new(agent_id: String, destination: String) -> Self {
        Self {
            agent_id,
            destination,
            pending: Vec::new(),
            max_per_bundle: 100,
            bundle_ttl_secs: 3600, // 1 hour default
            sequence: 0,
        }
    }

    /// Set max metrics per bundle.
    pub fn with_max_per_bundle(mut self, max: usize) -> Self {
        self.max_per_bundle = max;
        self
    }

    /// Set bundle TTL.
    pub fn with_ttl_secs(mut self, ttl: u64) -> Self {
        self.bundle_ttl_secs = ttl;
        self
    }

    /// Add metrics for propagation.
    pub fn add_metrics(&mut self, points: Vec<MetricPoint>) {
        self.pending.extend(points);
    }

    /// Create DTN bundles from pending metrics.
    /// Returns bundles ready to be enqueued into a `DtnQueue`.
    pub fn create_bundles(&mut self, now_ms: u64) -> Vec<rf_rpc::dtn::Bundle> {
        if self.pending.is_empty() {
            return Vec::new();
        }

        let mut bundles = Vec::new();

        // Drain pending into chunks
        while !self.pending.is_empty() {
            let chunk_end = self.pending.len().min(self.max_per_bundle);
            let chunk: Vec<MetricPoint> = self.pending.drain(..chunk_end).collect();

            // Serialize the chunk as JSON payload
            let payload = match serde_json::to_vec(&chunk) {
                Ok(data) => data,
                Err(_) => continue,
            };

            self.sequence += 1;

            bundles.push(rf_rpc::dtn::Bundle {
                id: format!("{}-metrics-{}", self.agent_id, self.sequence),
                source: self.agent_id.clone(),
                destination: self.destination.clone(),
                priority: rf_rpc::dtn::Priority::Low,
                ttl_secs: self.bundle_ttl_secs,
                created_at_ms: now_ms,
                payload,
                custody_requested: false,
                idempotency_key: Some(format!("{}-metrics-{}", self.agent_id, self.sequence)),
                hop_count: 0,
                max_hops: 10,
            });
        }

        bundles
    }

    /// Decode metric points from a received DTN bundle payload.
    pub fn decode_bundle(payload: &[u8]) -> Result<Vec<MetricPoint>, serde_json::Error> {
        serde_json::from_slice(payload)
    }

    /// Number of pending (unbundled) metrics.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

// --- OTLP/Prometheus/InfluxDB Exporters ---

/// Metric export destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    /// Prometheus exposition format (text/plain).
    Prometheus,
    /// OTLP JSON (OpenTelemetry Protocol, HTTP/JSON encoding).
    OtlpJson,
    /// InfluxDB line protocol.
    InfluxLineProtocol,
}

/// Metric exporter — formats and exports metric points to various backends.
pub struct MetricExporter {
    /// Export format.
    format: ExportFormat,
    /// Metric name prefix.
    prefix: String,
    /// Global labels added to all metrics.
    global_labels: HashMap<String, String>,
}

impl MetricExporter {
    /// Create a new exporter.
    pub fn new(format: ExportFormat) -> Self {
        Self {
            format,
            prefix: String::new(),
            global_labels: HashMap::new(),
        }
    }

    /// Set metric name prefix.
    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.prefix = prefix;
        self
    }

    /// Add a global label.
    pub fn with_label(mut self, key: String, value: String) -> Self {
        self.global_labels.insert(key, value);
        self
    }

    /// Export metrics to the configured format.
    pub fn export(&self, points: &[MetricPoint]) -> String {
        match self.format {
            ExportFormat::Prometheus => self.to_prometheus(points),
            ExportFormat::OtlpJson => self.to_otlp_json(points),
            ExportFormat::InfluxLineProtocol => self.to_influx_line(points),
        }
    }

    /// Prometheus exposition format.
    fn to_prometheus(&self, points: &[MetricPoint]) -> String {
        let mut output = String::new();
        for point in points {
            let name = if self.prefix.is_empty() {
                point.name.clone()
            } else {
                format!("{}_{}", self.prefix, point.name)
            };

            let mut all_labels = self.global_labels.clone();
            for (k, v) in &point.labels {
                all_labels.insert(k.clone(), v.clone());
            }

            let labels_str = if all_labels.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> = all_labels
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{v}\""))
                    .collect();
                format!("{{{}}}", pairs.join(","))
            };

            let value_str = match &point.value {
                MetricValue::Gauge(v) => format!("{v}"),
                MetricValue::Counter(v) => format!("{v}"),
                MetricValue::Histogram { sum, count } => {
                    // Emit _sum and _count
                    output.push_str(&format!(
                        "{name}_sum{labels_str} {sum} {}\n",
                        point.timestamp_ms
                    ));
                    format!("{count}")
                }
            };

            let suffix = if matches!(point.value, MetricValue::Histogram { .. }) {
                "_count"
            } else {
                ""
            };
            output.push_str(&format!(
                "{name}{suffix}{labels_str} {value_str} {}\n",
                point.timestamp_ms
            ));
        }
        output
    }

    /// OTLP JSON format (simplified — real OTLP uses protobuf, this is JSON variant).
    fn to_otlp_json(&self, points: &[MetricPoint]) -> String {
        let metrics: Vec<serde_json::Value> = points
            .iter()
            .map(|p| {
                let mut all_labels = self.global_labels.clone();
                for (k, v) in &p.labels {
                    all_labels.insert(k.clone(), v.clone());
                }
                let name = if self.prefix.is_empty() {
                    p.name.clone()
                } else {
                    format!("{}_{}", self.prefix, p.name)
                };

                let (data_type, value) = match &p.value {
                    MetricValue::Gauge(v) => ("gauge", serde_json::json!({"asDouble": v})),
                    MetricValue::Counter(v) => ("sum", serde_json::json!({"asInt": v})),
                    MetricValue::Histogram { sum, count } => {
                        ("histogram", serde_json::json!({"sum": sum, "count": count}))
                    }
                };

                let attributes: Vec<serde_json::Value> = all_labels
                    .iter()
                    .map(|(k, v)| {
                        serde_json::json!({
                            "key": k,
                            "value": {"stringValue": v}
                        })
                    })
                    .collect();

                serde_json::json!({
                    "name": name,
                    "unit": "",
                    data_type: {
                        "dataPoints": [{
                            "timeUnixNano": p.timestamp_ms * 1_000_000,
                            "attributes": attributes,
                            "value": value
                        }]
                    }
                })
            })
            .collect();

        let envelope = serde_json::json!({
            "resourceMetrics": [{
                "scopeMetrics": [{
                    "scope": {"name": "ravenfabric"},
                    "metrics": metrics
                }]
            }]
        });

        serde_json::to_string(&envelope).unwrap_or_default()
    }

    /// InfluxDB line protocol.
    fn to_influx_line(&self, points: &[MetricPoint]) -> String {
        let mut output = String::new();
        for point in points {
            let name = if self.prefix.is_empty() {
                point.name.clone()
            } else {
                format!("{}_{}", self.prefix, point.name)
            };

            // Tags (labels + global labels)
            let mut all_labels = self.global_labels.clone();
            for (k, v) in &point.labels {
                all_labels.insert(k.clone(), v.clone());
            }
            let tags_str = if all_labels.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> =
                    all_labels.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!(",{}", pairs.join(","))
            };

            // Field
            let field = match &point.value {
                MetricValue::Gauge(v) => format!("value={v}"),
                MetricValue::Counter(v) => format!("value={v}i"),
                MetricValue::Histogram { sum, count } => {
                    format!("sum={sum},count={count}i")
                }
            };

            // Timestamp in nanoseconds
            let ts_ns = point.timestamp_ms * 1_000_000;

            output.push_str(&format!("{name}{tags_str} {field} {ts_ns}\n"));
        }
        output
    }

    /// Export format.
    pub fn format(&self) -> &ExportFormat {
        &self.format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_buffer_push_and_flush() {
        let mut buffer = MetricBuffer::new(100);
        buffer.push(MetricPoint {
            name: "test_metric".into(),
            value: MetricValue::Gauge(42.0),
            labels: HashMap::new(),
            timestamp_ms: 1_000_000,
        });

        assert_eq!(buffer.len(), 1);
        let flushed = buffer.flush();
        assert_eq!(flushed.len(), 1);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_overflow_drops_oldest() {
        let mut buffer = MetricBuffer::new(3);
        for i in 0..5 {
            buffer.push(MetricPoint {
                name: format!("metric_{i}"),
                value: MetricValue::Counter(i),
                labels: HashMap::new(),
                timestamp_ms: i,
            });
        }

        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.dropped(), 2);
        let flushed = buffer.flush();
        // Should have the 3 most recent
        assert_eq!(flushed[0].name, "metric_2");
        assert_eq!(flushed[2].name, "metric_4");
    }

    #[test]
    fn test_system_metrics_collector() {
        let mut collector = SystemMetricsCollector::new(Duration::from_secs(10));
        let points = collector.collect();
        // With sysinfo feature, we get at least 6 system metrics + disk metrics
        // Without sysinfo feature, we get 4 stub metrics
        assert!(points.len() >= 4);
        assert_eq!(collector.name(), "system");
        assert_eq!(collector.interval(), Duration::from_secs(10));

        // Verify metric names
        let names: Vec<&str> = points.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"system_cpu_usage_percent"));
        assert!(names.contains(&"system_memory_used_bytes"));
        assert!(names.contains(&"system_memory_total_bytes"));
        assert!(names.contains(&"system_load_1m"));
    }

    #[test]
    fn test_prometheus_format() {
        let points = vec![
            MetricPoint {
                name: "http_requests_total".into(),
                value: MetricValue::Counter(1024),
                labels: {
                    let mut m = HashMap::new();
                    m.insert("method".into(), "GET".into());
                    m
                },
                timestamp_ms: 1_700_000,
            },
            MetricPoint {
                name: "cpu_temp".into(),
                value: MetricValue::Gauge(65.5),
                labels: HashMap::new(),
                timestamp_ms: 1_700_000,
            },
        ];

        let output = to_prometheus(&points);
        assert!(output.contains("http_requests_total{method=\"GET\"} 1024 1700000"));
        assert!(output.contains("cpu_temp 65.5 1700000"));
    }

    #[test]
    fn test_metric_value_serialization() {
        let gauge = MetricValue::Gauge(std::f64::consts::PI);
        let json = serde_json::to_string(&gauge).unwrap();
        let parsed: MetricValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, gauge);
    }

    #[test]
    fn test_collection_policy_default_allows_all() {
        let policy = CollectionPolicy::default();
        let point = MetricPoint {
            name: "anything".into(),
            value: MetricValue::Gauge(1.0),
            labels: HashMap::new(),
            timestamp_ms: 0,
        };
        assert!(policy.should_collect(&point));
    }

    #[test]
    fn test_collection_policy_exclude() {
        let policy = CollectionPolicy {
            include_patterns: vec!["system_*".to_string()],
            exclude_patterns: vec!["system_debug_*".to_string()],
            ..CollectionPolicy::default()
        };

        let included = MetricPoint {
            name: "system_cpu_usage".into(),
            value: MetricValue::Gauge(50.0),
            labels: HashMap::new(),
            timestamp_ms: 0,
        };
        let excluded = MetricPoint {
            name: "system_debug_internal".into(),
            value: MetricValue::Gauge(1.0),
            labels: HashMap::new(),
            timestamp_ms: 0,
        };
        let not_matched = MetricPoint {
            name: "app_requests".into(),
            value: MetricValue::Counter(100),
            labels: HashMap::new(),
            timestamp_ms: 0,
        };

        assert!(policy.should_collect(&included));
        assert!(!policy.should_collect(&excluded));
        assert!(!policy.should_collect(&not_matched));
    }

    #[test]
    fn test_collection_policy_label_filter() {
        let mut label_filters = HashMap::new();
        label_filters.insert("env".to_string(), "prod".to_string());

        let policy = CollectionPolicy {
            label_filters,
            ..CollectionPolicy::default()
        };

        let prod = MetricPoint {
            name: "requests".into(),
            value: MetricValue::Counter(1),
            labels: {
                let mut m = HashMap::new();
                m.insert("env".into(), "prod".into());
                m
            },
            timestamp_ms: 0,
        };
        let dev = MetricPoint {
            name: "requests".into(),
            value: MetricValue::Counter(1),
            labels: {
                let mut m = HashMap::new();
                m.insert("env".into(), "dev".into());
                m
            },
            timestamp_ms: 0,
        };

        assert!(policy.should_collect(&prod));
        assert!(!policy.should_collect(&dev));
    }

    #[test]
    fn test_collection_policy_no_histograms() {
        let policy = CollectionPolicy {
            collect_histograms: false,
            ..CollectionPolicy::default()
        };

        let gauge = MetricPoint {
            name: "cpu".into(),
            value: MetricValue::Gauge(50.0),
            labels: HashMap::new(),
            timestamp_ms: 0,
        };
        let histogram = MetricPoint {
            name: "latency".into(),
            value: MetricValue::Histogram {
                sum: 100.0,
                count: 10,
            },
            labels: HashMap::new(),
            timestamp_ms: 0,
        };

        assert!(policy.should_collect(&gauge));
        assert!(!policy.should_collect(&histogram));
    }

    #[test]
    fn test_collection_policy_filter_batch() {
        let policy = CollectionPolicy {
            include_patterns: vec!["system_*".to_string()],
            max_batch_size: 2,
            ..CollectionPolicy::default()
        };

        let points = vec![
            MetricPoint {
                name: "system_cpu".into(),
                value: MetricValue::Gauge(1.0),
                labels: HashMap::new(),
                timestamp_ms: 0,
            },
            MetricPoint {
                name: "app_req".into(),
                value: MetricValue::Counter(1),
                labels: HashMap::new(),
                timestamp_ms: 0,
            },
            MetricPoint {
                name: "system_mem".into(),
                value: MetricValue::Gauge(2.0),
                labels: HashMap::new(),
                timestamp_ms: 0,
            },
            MetricPoint {
                name: "system_disk".into(),
                value: MetricValue::Gauge(3.0),
                labels: HashMap::new(),
                timestamp_ms: 0,
            },
        ];

        let filtered = policy.filter_batch(points);
        assert_eq!(filtered.len(), 2); // max_batch_size caps it
        assert_eq!(filtered[0].name, "system_cpu");
        assert_eq!(filtered[1].name, "system_mem");
    }

    #[test]
    fn test_metrics_propagator_create_bundles() {
        let mut prop = MetricsPropagator::new("node-1".into(), "controller".into());
        prop.add_metrics(vec![
            MetricPoint {
                name: "cpu".into(),
                value: MetricValue::Gauge(75.0),
                labels: HashMap::new(),
                timestamp_ms: 1000,
            },
            MetricPoint {
                name: "mem".into(),
                value: MetricValue::Gauge(1024.0),
                labels: HashMap::new(),
                timestamp_ms: 1000,
            },
        ]);

        let bundles = prop.create_bundles(2000);
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].source, "node-1");
        assert_eq!(bundles[0].destination, "controller");
        assert_eq!(bundles[0].priority, rf_rpc::dtn::Priority::Low);
        assert!(bundles[0].idempotency_key.is_some());
        assert_eq!(prop.pending_count(), 0);
    }

    #[test]
    fn test_metrics_propagator_chunking() {
        let mut prop =
            MetricsPropagator::new("node-1".into(), "controller".into()).with_max_per_bundle(2);

        let points: Vec<MetricPoint> = (0..5)
            .map(|i| MetricPoint {
                name: format!("metric_{i}"),
                value: MetricValue::Counter(i),
                labels: HashMap::new(),
                timestamp_ms: 1000,
            })
            .collect();
        prop.add_metrics(points);

        let bundles = prop.create_bundles(2000);
        assert_eq!(bundles.len(), 3); // 5 metrics / 2 per bundle = 3 bundles
        assert_eq!(prop.pending_count(), 0);
    }

    #[test]
    fn test_metrics_propagator_decode_bundle() {
        let mut prop = MetricsPropagator::new("node-1".into(), "ctrl".into());
        prop.add_metrics(vec![MetricPoint {
            name: "test".into(),
            value: MetricValue::Gauge(42.0),
            labels: HashMap::new(),
            timestamp_ms: 1000,
        }]);

        let bundles = prop.create_bundles(2000);
        let decoded = MetricsPropagator::decode_bundle(&bundles[0].payload).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "test");
        assert_eq!(decoded[0].value, MetricValue::Gauge(42.0));
    }

    #[test]
    fn test_metrics_propagator_empty() {
        let mut prop = MetricsPropagator::new("node-1".into(), "ctrl".into());
        let bundles = prop.create_bundles(1000);
        assert!(bundles.is_empty());
    }

    #[test]
    fn test_exporter_prometheus_format() {
        let exporter = MetricExporter::new(ExportFormat::Prometheus)
            .with_prefix("rf".into())
            .with_label("cluster".into(), "prod".into());

        let points = vec![MetricPoint {
            name: "cpu_usage".into(),
            value: MetricValue::Gauge(75.5),
            labels: {
                let mut m = HashMap::new();
                m.insert("host".into(), "web-01".into());
                m
            },
            timestamp_ms: 1_000_000,
        }];

        let output = exporter.export(&points);
        assert!(output.contains("rf_cpu_usage"));
        assert!(output.contains("75.5"));
        assert!(output.contains("cluster=\"prod\""));
        assert!(output.contains("host=\"web-01\""));
    }

    #[test]
    fn test_exporter_otlp_json() {
        let exporter = MetricExporter::new(ExportFormat::OtlpJson);

        let points = vec![MetricPoint {
            name: "requests_total".into(),
            value: MetricValue::Counter(42),
            labels: HashMap::new(),
            timestamp_ms: 1_000_000,
        }];

        let output = exporter.export(&points);
        assert!(output.contains("resourceMetrics"));
        assert!(output.contains("requests_total"));
        assert!(output.contains("\"asInt\":42"));
    }

    #[test]
    fn test_exporter_influx_line_protocol() {
        let exporter =
            MetricExporter::new(ExportFormat::InfluxLineProtocol).with_prefix("rf".into());

        let points = vec![
            MetricPoint {
                name: "cpu".into(),
                value: MetricValue::Gauge(65.5),
                labels: {
                    let mut m = HashMap::new();
                    m.insert("host".into(), "web-01".into());
                    m
                },
                timestamp_ms: 1_000_000,
            },
            MetricPoint {
                name: "requests".into(),
                value: MetricValue::Counter(100),
                labels: HashMap::new(),
                timestamp_ms: 1_000_000,
            },
        ];

        let output = exporter.export(&points);
        assert!(output.contains("rf_cpu,host=web-01 value=65.5 1000000000000"));
        assert!(output.contains("rf_requests value=100i 1000000000000"));
    }

    #[test]
    fn test_exporter_histogram() {
        let exporter = MetricExporter::new(ExportFormat::Prometheus);

        let points = vec![MetricPoint {
            name: "latency".into(),
            value: MetricValue::Histogram {
                sum: 123.45,
                count: 100,
            },
            labels: HashMap::new(),
            timestamp_ms: 5000,
        }];

        let output = exporter.export(&points);
        assert!(output.contains("latency_sum 123.45 5000"));
        assert!(output.contains("latency_count 100 5000"));
    }
}
