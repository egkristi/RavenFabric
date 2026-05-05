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
}

impl SystemMetricsCollector {
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }

    /// Collect basic system metrics (CPU, memory, load).
    /// In production this uses sysinfo crate; here we define the interface.
    pub fn collect_system_metrics(&self, timestamp_ms: u64) -> Vec<MetricPoint> {
        // These would be populated by sysinfo in the actual agent
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
        assert_eq!(points.len(), 4);
        assert_eq!(collector.name(), "system");
        assert_eq!(collector.interval(), Duration::from_secs(10));
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
        let gauge = MetricValue::Gauge(3.14);
        let json = serde_json::to_string(&gauge).unwrap();
        let parsed: MetricValue = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, gauge);
    }
}
