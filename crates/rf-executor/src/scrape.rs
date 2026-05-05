//! Application metrics scraping — pull metrics from localhost endpoints.
//!
//! Scrapes Prometheus-formatted metrics from application endpoints
//! running on agents, governed by the collection policy.

use serde::{Deserialize, Serialize};

/// Configuration for a scrape target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeTarget {
    /// Unique name for this target.
    pub name: String,
    /// URL to scrape (e.g., "http://localhost:9090/metrics").
    pub url: String,
    /// Scrape interval in seconds.
    pub interval_secs: u64,
    /// Scrape timeout in seconds.
    pub timeout_secs: u64,
    /// Optional HTTP headers (e.g., Authorization).
    pub headers: Vec<(String, String)>,
    /// Metric name prefix to add.
    pub metric_prefix: Option<String>,
    /// Labels to add to all scraped metrics.
    pub extra_labels: Vec<(String, String)>,
    /// Include filter: only forward metrics matching these prefixes.
    pub include_prefixes: Vec<String>,
    /// Exclude filter: drop metrics matching these prefixes.
    pub exclude_prefixes: Vec<String>,
}

/// State of a scrape target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrapeState {
    /// Target not yet scraped.
    Idle,
    /// Scrape in progress.
    Scraping,
    /// Last scrape succeeded.
    Healthy,
    /// Last scrape failed.
    Failed,
    /// Target disabled.
    Disabled,
}

/// Statistics for a scrape target.
#[derive(Debug, Clone, Default)]
pub struct ScrapeStats {
    /// Total scrapes attempted.
    pub scrapes_total: u64,
    /// Successful scrapes.
    pub scrapes_success: u64,
    /// Failed scrapes.
    pub scrapes_failed: u64,
    /// Total metrics collected.
    pub metrics_collected: u64,
    /// Last scrape duration in ms.
    pub last_duration_ms: u64,
    /// Last scrape timestamp (Unix ms).
    pub last_scrape_ms: u64,
}

impl ScrapeStats {
    pub fn record_success(&mut self, metrics_count: u64, duration_ms: u64, now_ms: u64) {
        self.scrapes_total += 1;
        self.scrapes_success += 1;
        self.metrics_collected += metrics_count;
        self.last_duration_ms = duration_ms;
        self.last_scrape_ms = now_ms;
    }

    pub fn record_failure(&mut self, now_ms: u64) {
        self.scrapes_total += 1;
        self.scrapes_failed += 1;
        self.last_scrape_ms = now_ms;
    }

    pub fn success_rate(&self) -> f64 {
        if self.scrapes_total == 0 {
            return 0.0;
        }
        self.scrapes_success as f64 / self.scrapes_total as f64
    }
}

/// A parsed Prometheus metric line.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrapedMetric {
    /// Metric name.
    pub name: String,
    /// Labels.
    pub labels: Vec<(String, String)>,
    /// Value.
    pub value: f64,
    /// Optional timestamp (Unix ms).
    pub timestamp_ms: Option<u64>,
}

/// Parse Prometheus exposition format text into metrics.
pub fn parse_prometheus(text: &str) -> Vec<ScrapedMetric> {
    let mut metrics = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(metric) = parse_metric_line(line) {
            metrics.push(metric);
        }
    }

    metrics
}

/// Parse a single Prometheus metric line.
fn parse_metric_line(line: &str) -> Option<ScrapedMetric> {
    // Format: metric_name{label="value",...} value [timestamp]
    let (name_labels, rest) = if let Some(brace_start) = line.find('{') {
        let brace_end = line.find('}')?;
        let name = line[..brace_start].to_string();
        let labels_str = &line[brace_start + 1..brace_end];
        let labels = parse_labels(labels_str);
        let rest = line[brace_end + 1..].trim();
        ((name, labels), rest)
    } else {
        // No labels: metric_name value [timestamp]
        let mut parts = line.splitn(2, ' ');
        let name = parts.next()?.to_string();
        let rest = parts.next().unwrap_or("");
        ((name, Vec::new()), rest)
    };

    let mut value_parts = rest.split_whitespace();
    let value: f64 = value_parts.next()?.parse().ok()?;
    let timestamp_ms: Option<u64> = value_parts.next().and_then(|s| s.parse().ok());

    Some(ScrapedMetric {
        name: name_labels.0,
        labels: name_labels.1,
        value,
        timestamp_ms,
    })
}

/// Parse label pairs from "key=\"value\",key2=\"value2\"".
fn parse_labels(s: &str) -> Vec<(String, String)> {
    let mut labels = Vec::new();

    for pair in s.split(',') {
        let pair = pair.trim();
        if let Some(eq_pos) = pair.find('=') {
            let key = pair[..eq_pos].trim().to_string();
            let value = pair[eq_pos + 1..].trim().trim_matches('"').to_string();
            if !key.is_empty() {
                labels.push((key, value));
            }
        }
    }

    labels
}

/// Check if a metric name should be included based on prefix filters.
pub fn should_include(
    metric_name: &str,
    include_prefixes: &[String],
    exclude_prefixes: &[String],
) -> bool {
    // Check exclusions first
    for prefix in exclude_prefixes {
        if metric_name.starts_with(prefix.as_str()) {
            return false;
        }
    }

    // If no include list, include all (that aren't excluded)
    if include_prefixes.is_empty() {
        return true;
    }

    // Check inclusions
    include_prefixes
        .iter()
        .any(|prefix| metric_name.starts_with(prefix.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_metric() {
        let text = "http_requests_total 1234\n";
        let metrics = parse_prometheus(text);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "http_requests_total");
        assert_eq!(metrics[0].value, 1234.0);
        assert!(metrics[0].labels.is_empty());
    }

    #[test]
    fn test_parse_metric_with_labels() {
        let text = r#"http_requests_total{method="GET",status="200"} 42"#;
        let metrics = parse_prometheus(text);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "http_requests_total");
        assert_eq!(metrics[0].value, 42.0);
        assert_eq!(metrics[0].labels.len(), 2);
        assert_eq!(metrics[0].labels[0], ("method".into(), "GET".into()));
        assert_eq!(metrics[0].labels[1], ("status".into(), "200".into()));
    }

    #[test]
    fn test_parse_with_timestamp() {
        let text = "process_cpu_seconds_total 123.45 1700000000000\n";
        let metrics = parse_prometheus(text);
        assert_eq!(metrics[0].value, 123.45);
        assert_eq!(metrics[0].timestamp_ms, Some(1700000000000));
    }

    #[test]
    fn test_skip_comments_and_empty() {
        let text = "# HELP http_requests Total requests\n# TYPE http_requests counter\nhttp_requests 100\n\n";
        let metrics = parse_prometheus(text);
        assert_eq!(metrics.len(), 1);
    }

    #[test]
    fn test_scrape_stats() {
        let mut stats = ScrapeStats::default();
        stats.record_success(50, 100, 1000);
        stats.record_success(30, 80, 2000);
        stats.record_failure(3000);

        assert_eq!(stats.scrapes_total, 3);
        assert_eq!(stats.scrapes_success, 2);
        assert_eq!(stats.scrapes_failed, 1);
        assert_eq!(stats.metrics_collected, 80);
        assert!((stats.success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_should_include() {
        let include = vec!["http_".into(), "process_".into()];
        let exclude = vec!["http_internal_".into()];

        assert!(should_include("http_requests_total", &include, &exclude));
        assert!(should_include("process_cpu", &include, &exclude));
        assert!(!should_include("http_internal_debug", &include, &exclude));
        assert!(!should_include("go_gc_duration", &include, &exclude));
    }

    #[test]
    fn test_include_all_when_empty() {
        let include: Vec<String> = vec![];
        let exclude: Vec<String> = vec![];

        assert!(should_include("anything", &include, &exclude));
    }
}
