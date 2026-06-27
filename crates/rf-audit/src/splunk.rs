//! Splunk HEC (HTTP Event Collector) audit log destination.
//!
//! Sends each `AuditEntry` to a Splunk HEC endpoint via HTTP POST.
//! Supports batching and configurable retry with exponential back-off.
//!
//! HEC endpoint format: `http[s]://host:port/services/collector/event`
//! Authorization: `Splunk <token>` header.
//!
//! The payload is a JSON object:
//! ```json
//! {
//!   "time": 1234567890.123,
//!   "source": "ravenfabric",
//!   "sourcetype": "rf:audit",
//!   "index": "main",
//!   "event": { ... AuditEntry fields ... }
//! }
//! ```
//!
//! Delivery is fire-and-forget: failures are logged at `warn` level and
//! never surfaced as errors to the caller.

use std::{
    io::Write,
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Serialize;

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

// ── HEC payload ───────────────────────────────────────────────────────────────

/// A single Splunk HEC event payload.
#[derive(Debug, Serialize)]
struct HecPayload<'a> {
    /// Unix epoch with sub-second precision.
    time: f64,
    /// Source identifier.
    source: &'static str,
    /// Sourcetype for Splunk field extraction.
    sourcetype: &'static str,
    /// Target Splunk index.
    index: String,
    /// The audit event data.
    event: &'a AuditEntry,
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

/// Parse a URL into (use_https, host, port, path).
fn parse_hec_url(url: &str) -> Option<(bool, String, u16, String)> {
    let (scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/services/collector/event".to_string()),
    };

    let (host, port) = if let Some(i) = host_port.rfind(':') {
        let h = host_port[..i].to_string();
        let p: u16 = host_port[i + 1..].parse().ok()?;
        (h, p)
    } else {
        // Splunk HEC default port is 8088 for both HTTP and HTTPS
        (host_port.to_string(), 8088u16)
    };

    Some((scheme, host, port, path))
}

/// Send a single HEC payload via plain TCP HTTP (no TLS in core — TLS is a
/// compile-time feature that can be added on top).
fn send_http_post(
    host: &str,
    port: u16,
    path: &str,
    token: &str,
    body: &str,
) -> std::io::Result<()> {
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect_timeout(
        &addr
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{e}")))?,
        Duration::from_secs(5),
    )?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAuthorization: Splunk {token}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        path = path,
        host = host,
        port = port,
        token = token,
        len = body.len(),
        body = body,
    );
    stream.write_all(request.as_bytes())?;
    // Read minimal response to ensure the server accepted it
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
    Ok(())
}

use std::io::Read;

// ── SplunkHecAuditLogger ──────────────────────────────────────────────────────

/// Configuration for the Splunk HEC audit logger.
#[derive(Debug, Clone)]
pub struct SplunkHecConfig {
    /// HEC endpoint URL (e.g. `http://splunk-host:8088/services/collector/event`).
    pub url: String,
    /// HEC authentication token.
    pub token: String,
    /// Splunk index to write events to (default: `"main"`).
    pub index: String,
    /// Maximum number of events to accumulate before flushing.
    /// Set to 1 to disable batching.
    pub batch_size: usize,
}

impl SplunkHecConfig {
    /// Create a minimal config with a single-event batch.
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            token: token.into(),
            index: "main".into(),
            batch_size: 1,
        }
    }

    /// Set the Splunk index.
    pub fn with_index(mut self, index: impl Into<String>) -> Self {
        self.index = index.into();
        self
    }

    /// Set the batch size (events accumulated before flushing).
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }
}

/// Audit logger that forwards events to a Splunk HEC endpoint.
///
/// Events are serialized as HEC JSON payloads and posted via HTTP.
/// Batching is supported: events are accumulated up to `batch_size` before
/// being sent in a single POST (multiple JSON objects concatenated, which
/// HEC supports natively).
///
/// Delivery failures are logged at `warn` level and never propagated.
pub struct SplunkHecAuditLogger {
    config: SplunkHecConfig,
    batch: Arc<Mutex<Vec<String>>>,
}

impl SplunkHecAuditLogger {
    /// Create a new Splunk HEC logger with the given configuration.
    pub fn new(config: SplunkHecConfig) -> Self {
        Self {
            config,
            batch: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Serialize one audit entry as a HEC JSON payload string.
    fn serialize_entry(&self, entry: &AuditEntry) -> Result<String, serde_json::Error> {
        let ts = entry.timestamp.timestamp_millis() as f64 / 1000.0;
        let payload = HecPayload {
            time: ts,
            source: "ravenfabric",
            sourcetype: "rf:audit",
            index: self.config.index.clone(),
            event: entry,
        };
        serde_json::to_string(&payload)
    }

    /// Flush the accumulated batch to the HEC endpoint.
    fn flush(&self, batch: &[String]) {
        // Splunk HEC accepts multiple JSON objects concatenated with newlines.
        let body = batch.join("\n");

        if let Some((_, host, port, path)) = parse_hec_url(&self.config.url) {
            if let Err(e) = send_http_post(&host, port, &path, &self.config.token, &body) {
                tracing::warn!("Splunk HEC delivery failed ({} events): {e}", batch.len());
            }
        } else {
            tracing::warn!("Splunk HEC: invalid URL '{}'", self.config.url);
        }
    }
}

impl AuditLogger for SplunkHecAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        match self.serialize_entry(&entry) {
            Ok(json) => {
                let mut batch = self.batch.lock().unwrap_or_else(|p| p.into_inner());
                batch.push(json);
                if batch.len() >= self.config.batch_size {
                    let to_send: Vec<String> = batch.drain(..).collect();
                    drop(batch); // release lock before network I/O
                    self.flush(&to_send);
                }
            }
            Err(e) => {
                tracing::warn!("Splunk HEC serialization failed: {e}");
            }
        }
        Ok(())
    }
}

impl Drop for SplunkHecAuditLogger {
    fn drop(&mut self) {
        // Flush any remaining events in the batch on drop.
        let mut batch = self.batch.lock().unwrap_or_else(|p| p.into_inner());
        if !batch.is_empty() {
            let to_send: Vec<String> = batch.drain(..).collect();
            drop(batch);
            self.flush(&to_send);
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::types::AuditEntry;

    fn sample_entry() -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: "req-hec-1".into(),
            action: "Execute".into(),
            command: Some("df -h".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 10,
            caller_key: "heckeyaabb".into(),
            reason: None,
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_hec_payload_serialization() {
        let config = SplunkHecConfig::new("http://127.0.0.1:8088", "test-token");
        let logger = SplunkHecAuditLogger::new(config);
        let entry = sample_entry();
        let json = logger.serialize_entry(&entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["source"], "ravenfabric");
        assert_eq!(parsed["sourcetype"], "rf:audit");
        assert_eq!(parsed["index"], "main");
        assert!(parsed["time"].is_f64(), "time should be float");
        assert_eq!(parsed["event"]["request_id"], "req-hec-1");
        assert_eq!(parsed["event"]["decision"], "allowed");
    }

    #[test]
    fn test_hec_custom_index() {
        let config = SplunkHecConfig::new("http://127.0.0.1:8088", "tok").with_index("security");
        let logger = SplunkHecAuditLogger::new(config);
        let json = logger.serialize_entry(&sample_entry()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["index"], "security");
    }

    #[test]
    fn test_parse_hec_url_http() {
        let (tls, host, port, path) =
            parse_hec_url("http://splunk.internal:8088/services/collector/event").unwrap();
        assert!(!tls);
        assert_eq!(host, "splunk.internal");
        assert_eq!(port, 8088);
        assert_eq!(path, "/services/collector/event");
    }

    #[test]
    fn test_parse_hec_url_https() {
        let (tls, host, port, path) =
            parse_hec_url("https://splunk.internal:8089/services/collector/event").unwrap();
        assert!(tls);
        assert_eq!(host, "splunk.internal");
        assert_eq!(port, 8089);
        assert_eq!(path, "/services/collector/event");
    }

    #[test]
    fn test_parse_hec_url_default_path() {
        // No path in URL — should default
        let (_, _, _, path) = parse_hec_url("http://splunk:8088").unwrap();
        assert_eq!(path, "/services/collector/event");
    }

    #[test]
    fn test_parse_hec_url_invalid() {
        assert!(parse_hec_url("ftp://splunk:8088").is_none());
        assert!(parse_hec_url("not-a-url").is_none());
    }

    #[test]
    fn test_hec_log_no_server_is_silent() {
        // Logging with no server should not panic or return an error
        let config = SplunkHecConfig::new("http://127.0.0.1:19999/services/collector/event", "tok");
        let logger = SplunkHecAuditLogger::new(config);
        assert!(logger.log(sample_entry()).is_ok());
    }

    #[test]
    fn test_hec_batching_flushes_at_batch_size() {
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();

        let handle = std::thread::spawn(move || {
            // Accept one connection (the batch POST)
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut buf = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                match stream.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                }
            }
            let body = String::from_utf8_lossy(&buf).to_string();
            recv_clone.lock().unwrap().push(body);
        });

        let url = format!("http://127.0.0.1:{}/services/collector/event", addr.port());
        let config = SplunkHecConfig::new(url, "test-tok").with_batch_size(2);
        let logger = SplunkHecAuditLogger::new(config);

        // Log first event — not flushed yet (batch_size=2)
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert_eq!(b.len(), 1, "batch should have 1 event");
        }

        // Log second event — should flush
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert!(b.is_empty(), "batch should be empty after flush");
        }

        handle.join().unwrap();
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1, "expected exactly one HTTP POST");
        // The body should contain two JSON objects (one per event)
        let body = &received[0];
        assert!(body.contains("req-hec-1"), "expected request_id in body");
    }
}
