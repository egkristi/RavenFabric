//! Metrics collector framework — plugin trait, system metrics, offline buffering.
//!
//! Provides a trait for metric collectors and a built-in system metrics collector
//! that gathers CPU, memory, disk, and network statistics.

use std::collections::HashMap;
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
}
