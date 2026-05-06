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

/// Scrape metrics from a target URL via plain HTTP GET.
///
/// Performs a minimal HTTP/1.1 GET request (no external HTTP library needed).
/// Parses the Prometheus exposition format response and applies filters.
pub async fn scrape_target(target: &ScrapeTarget) -> Result<Vec<ScrapedMetric>, String> {
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    let start = Instant::now();
    let timeout_dur = Duration::from_secs(target.timeout_secs);

    // Parse URL to get host, port, and path
    let url = &target.url;
    let (host, port, path) = parse_http_url(url)?;

    // Connect via TCP
    let addr = format!("{host}:{port}");
    let mut stream = timeout(timeout_dur, tokio::net::TcpStream::connect(&addr))
        .await
        .map_err(|_| format!("connect timeout to {addr}"))?
        .map_err(|e| format!("connect to {addr}: {e}"))?;

    // Build HTTP GET request
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n"
    );
    for (key, value) in &target.headers {
        request.push_str(&format!("{key}: {value}\r\n"));
    }
    request.push_str("\r\n");

    // Send request
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("write request: {e}"))?;

    // Read response (with timeout)
    let mut response = Vec::new();
    timeout(timeout_dur - start.elapsed(), async {
        stream.read_to_end(&mut response).await
    })
    .await
    .map_err(|_| "read timeout".to_string())?
    .map_err(|e| format!("read response: {e}"))?;

    // Parse HTTP response
    let response_str = String::from_utf8_lossy(&response);
    let body = extract_http_body(&response_str)?;

    // Parse Prometheus metrics
    let mut metrics = parse_prometheus(body);

    // Apply prefix to metric names
    if let Some(prefix) = &target.metric_prefix {
        for m in &mut metrics {
            m.name = format!("{}_{}", prefix, m.name);
        }
    }

    // Apply extra labels
    if !target.extra_labels.is_empty() {
        for m in &mut metrics {
            for label in &target.extra_labels {
                m.labels.push(label.clone());
            }
        }
    }

    // Apply include/exclude filters
    metrics.retain(|m| should_include(&m.name, &target.include_prefixes, &target.exclude_prefixes));

    Ok(metrics)
}

/// Parse a simple HTTP URL into (host, port, path).
fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url
        .strip_prefix("http://")
        .ok_or_else(|| "only http:// URLs supported (no TLS in core)".to_string())?;

    let (host_port, path) = if let Some(slash_pos) = url.find('/') {
        (&url[..slash_pos], &url[slash_pos..])
    } else {
        (url, "/")
    };

    let (host, port) = if let Some(colon_pos) = host_port.rfind(':') {
        let h = &host_port[..colon_pos];
        let p: u16 = host_port[colon_pos + 1..]
            .parse()
            .map_err(|_| "invalid port".to_string())?;
        (h.to_string(), p)
    } else {
        (host_port.to_string(), 80)
    };

    Ok((host, port, path.to_string()))
}

/// Extract the body from an HTTP response (after the empty line).
fn extract_http_body(response: &str) -> Result<&str, String> {
    // HTTP response: headers\r\n\r\nbody
    if let Some(pos) = response.find("\r\n\r\n") {
        Ok(&response[pos + 4..])
    } else if let Some(pos) = response.find("\n\n") {
        Ok(&response[pos + 2..])
    } else {
        Err("malformed HTTP response (no body separator)".into())
    }
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

    #[test]
    fn test_parse_http_url() {
        let (host, port, path) = parse_http_url("http://localhost:9090/metrics").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 9090);
        assert_eq!(path, "/metrics");

        let (host, port, path) = parse_http_url("http://app.local/prom").unwrap();
        assert_eq!(host, "app.local");
        assert_eq!(port, 80);
        assert_eq!(path, "/prom");
    }

    #[test]
    fn test_extract_http_body() {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nmetric_a 42\n";
        let body = extract_http_body(resp).unwrap();
        assert_eq!(body, "metric_a 42\n");
    }

    #[tokio::test]
    async fn test_scrape_target_real_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Start a fake Prometheus metrics server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let body =
                    "# HELP up Service liveness\nup 1\nhttp_requests_total{method=\"GET\"} 42\n";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let target = ScrapeTarget {
            name: "test-app".into(),
            url: format!("http://127.0.0.1:{port}/metrics"),
            interval_secs: 15,
            timeout_secs: 5,
            headers: Vec::new(),
            metric_prefix: None,
            extra_labels: vec![("agent".into(), "test-01".into())],
            include_prefixes: Vec::new(),
            exclude_prefixes: Vec::new(),
        };

        let metrics = scrape_target(&target).await.unwrap();
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].name, "up");
        assert_eq!(metrics[0].value, 1.0);
        // Extra labels added
        assert!(
            metrics[0]
                .labels
                .contains(&("agent".into(), "test-01".into()))
        );
        assert_eq!(metrics[1].name, "http_requests_total");
        assert_eq!(metrics[1].value, 42.0);
    }
}
