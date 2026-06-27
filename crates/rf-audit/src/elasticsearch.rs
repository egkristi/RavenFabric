//! Elasticsearch / OpenSearch audit log destination.
//!
//! Indexes each `AuditEntry` into an Elasticsearch or OpenSearch cluster via
//! the [Bulk API](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html).
//!
//! The bulk API accepts newline-delimited JSON (NDJSON): each event is two lines:
//! 1. Action metadata: `{"index":{"_index":"<index>","_id":"<request_id>"}}`
//! 2. Document body: the serialized `AuditEntry`
//!
//! Authentication:
//! - `ElasticAuth::None` — unauthenticated (dev/testing)
//! - `ElasticAuth::Basic { username, password }` — HTTP Basic auth
//! - `ElasticAuth::ApiKey(key)` — `Authorization: ApiKey <key>` header
//!
//! Delivery is fire-and-forget: failures are logged at `warn` level and never
//! surfaced as errors to the caller.

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

// ── Auth ──────────────────────────────────────────────────────────────────────

/// Authentication method for the Elasticsearch/OpenSearch cluster.
#[derive(Debug, Clone)]
pub enum ElasticAuth {
    /// No authentication (development / unauthenticated clusters).
    None,
    /// HTTP Basic authentication.
    Basic {
        /// Username.
        username: String,
        /// Password.
        password: String,
    },
    /// API key authentication.
    /// The `key` value is the `<id>:<api_key>` string Base64-encoded,
    /// as returned by the Create API Key API.
    ApiKey(String),
}

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for the Elasticsearch audit logger.
#[derive(Debug, Clone)]
pub struct ElasticsearchConfig {
    /// Base URL of the cluster (e.g. `http://localhost:9200`).
    pub url: String,
    /// Index name prefix. Entries are written to `<prefix>-ravenfabric`.
    pub index: String,
    /// Authentication.
    pub auth: ElasticAuth,
    /// Maximum events to accumulate before flushing.
    pub batch_size: usize,
}

impl ElasticsearchConfig {
    /// Create a config with no authentication and batch_size = 1.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            index: "ravenfabric".into(),
            auth: ElasticAuth::None,
            batch_size: 1,
        }
    }

    /// Set the index name.
    pub fn with_index(mut self, index: impl Into<String>) -> Self {
        self.index = index.into();
        self
    }

    /// Set Basic authentication.
    pub fn with_basic_auth(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.auth = ElasticAuth::Basic {
            username: username.into(),
            password: password.into(),
        };
        self
    }

    /// Set API key authentication.
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.auth = ElasticAuth::ApiKey(key.into());
        self
    }

    /// Set the batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

/// Parse a URL into `(use_https, host, port, path_prefix)`.
fn parse_elastic_url(url: &str) -> Option<(bool, String, u16, String)> {
    let (tls, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, String::new()),
    };

    let (host, port) = if let Some(i) = host_port.rfind(':') {
        let h = host_port[..i].to_string();
        let p: u16 = host_port[i + 1..].parse().ok()?;
        (h, p)
    } else {
        let default_port = if tls { 9243u16 } else { 9200u16 };
        (host_port.to_string(), default_port)
    };

    Some((tls, host, port, path))
}

/// Build the `Authorization` header value for the given auth config.
fn auth_header(auth: &ElasticAuth) -> Option<String> {
    match auth {
        ElasticAuth::None => None,
        ElasticAuth::Basic { username, password } => {
            use std::fmt::Write as FmtWrite;
            let mut raw = String::new();
            let _ = write!(raw, "{username}:{password}");
            Some(format!("Basic {}", base64_encode(raw.as_bytes())))
        }
        ElasticAuth::ApiKey(key) => Some(format!("ApiKey {key}")),
    }
}

/// Minimal Base64 encoding (RFC 4648, no padding issues).
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Send an HTTP POST request with NDJSON body to the bulk endpoint.
fn send_bulk_request(
    host: &str,
    port: u16,
    path_prefix: &str,
    auth: &ElasticAuth,
    body: &str,
) -> std::io::Result<()> {
    let addr = format!("{host}:{port}");
    let path = format!("{path_prefix}/_bulk");
    let mut stream = TcpStream::connect_timeout(
        &addr
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{e}")))?,
        Duration::from_secs(5),
    )?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let auth_line = if let Some(v) = auth_header(auth) {
        format!("Authorization: {v}\r\n")
    } else {
        String::new()
    };

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\n{auth_line}Content-Type: application/x-ndjson\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        path = path,
        host = host,
        port = port,
        auth_line = auth_line,
        len = body.len(),
        body = body,
    );
    stream.write_all(request.as_bytes())?;
    // Drain response to ensure server processes the request.
    let mut buf = [0u8; 512];
    let _ = stream.read(&mut buf);
    Ok(())
}

// ── ElasticsearchAuditLogger ──────────────────────────────────────────────────

/// Audit logger that indexes events into Elasticsearch or OpenSearch.
///
/// Events are accumulated in a batch and flushed to the `/_bulk` endpoint
/// when the batch reaches `batch_size`. The remaining batch is flushed on drop.
pub struct ElasticsearchAuditLogger {
    config: ElasticsearchConfig,
    batch: Arc<Mutex<Vec<String>>>,
}

impl ElasticsearchAuditLogger {
    /// Create a new Elasticsearch audit logger.
    pub fn new(config: ElasticsearchConfig) -> Self {
        Self {
            config,
            batch: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Serialize one audit entry as an NDJSON bulk action pair (two lines).
    fn serialize_entry(&self, entry: &AuditEntry) -> Result<String, serde_json::Error> {
        let meta = serde_json::json!({
            "index": {
                "_index": &self.config.index,
                "_id": &entry.request_id
            }
        });
        let doc = serde_json::to_value(entry)?;
        Ok(format!("{meta}\n{doc}\n"))
    }

    /// Flush the accumulated batch to the Elasticsearch bulk endpoint.
    fn flush(&self, batch: &[String]) {
        // The bulk body is all NDJSON pairs concatenated.
        let body = batch.join("");

        if let Some((_, host, port, path_prefix)) = parse_elastic_url(&self.config.url) {
            if let Err(e) = send_bulk_request(&host, port, &path_prefix, &self.config.auth, &body) {
                tracing::warn!(
                    "Elasticsearch bulk delivery failed ({} events): {e}",
                    batch.len()
                );
            }
        } else {
            tracing::warn!("Elasticsearch: invalid URL '{}'", self.config.url);
        }
    }
}

impl AuditLogger for ElasticsearchAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        match self.serialize_entry(&entry) {
            Ok(ndjson) => {
                let mut batch = self.batch.lock().unwrap_or_else(|p| p.into_inner());
                batch.push(ndjson);
                if batch.len() >= self.config.batch_size {
                    let to_send: Vec<String> = batch.drain(..).collect();
                    drop(batch);
                    self.flush(&to_send);
                }
            }
            Err(e) => {
                tracing::warn!("Elasticsearch serialization failed: {e}");
            }
        }
        Ok(())
    }
}

impl Drop for ElasticsearchAuditLogger {
    fn drop(&mut self) {
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
            request_id: "req-es-1".into(),
            action: "Execute".into(),
            command: Some("ls -la".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 5,
            caller_key: "eskeyaabb".into(),
            reason: None,
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_ndjson_bulk_format() {
        let config = ElasticsearchConfig::new("http://localhost:9200");
        let logger = ElasticsearchAuditLogger::new(config);
        let ndjson = logger.serialize_entry(&sample_entry()).unwrap();
        let lines: Vec<&str> = ndjson.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2, "bulk action must be exactly two lines");

        let meta: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(meta["index"]["_index"], "ravenfabric");
        assert_eq!(meta["index"]["_id"], "req-es-1");

        let doc: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(doc["request_id"], "req-es-1");
        assert_eq!(doc["decision"], "allowed");
    }

    #[test]
    fn test_custom_index() {
        let config = ElasticsearchConfig::new("http://localhost:9200").with_index("audit-logs");
        let logger = ElasticsearchAuditLogger::new(config);
        let ndjson = logger.serialize_entry(&sample_entry()).unwrap();
        let lines: Vec<&str> = ndjson.trim_end_matches('\n').split('\n').collect();
        let meta: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(meta["index"]["_index"], "audit-logs");
    }

    #[test]
    fn test_parse_elastic_url_http() {
        let (tls, host, port, _path) = parse_elastic_url("http://elastic.internal:9200").unwrap();
        assert!(!tls);
        assert_eq!(host, "elastic.internal");
        assert_eq!(port, 9200);
    }

    #[test]
    fn test_parse_elastic_url_https_default_port() {
        let (tls, _, port, _) = parse_elastic_url("https://cloud.es.io").unwrap();
        assert!(tls);
        assert_eq!(port, 9243, "HTTPS default for Elastic Cloud is 9243");
    }

    #[test]
    fn test_parse_elastic_url_http_default_port() {
        let (tls, _, port, _) = parse_elastic_url("http://localhost").unwrap();
        assert!(!tls);
        assert_eq!(port, 9200, "HTTP default for Elasticsearch is 9200");
    }

    #[test]
    fn test_parse_elastic_url_invalid() {
        assert!(parse_elastic_url("ftp://localhost:9200").is_none());
        assert!(parse_elastic_url("not-a-url").is_none());
    }

    #[test]
    fn test_auth_header_none() {
        assert!(auth_header(&ElasticAuth::None).is_none());
    }

    #[test]
    fn test_auth_header_basic() {
        let auth = ElasticAuth::Basic {
            username: "elastic".into(),
            password: "changeme".into(),
        };
        let hdr = auth_header(&auth).unwrap();
        assert!(hdr.starts_with("Basic "), "should be Basic auth");
        // "elastic:changeme" base64 = "ZWxhc3RpYzpjaGFuZ2VtZQ=="
        assert!(hdr.contains("ZWxhc3RpYzpjaGFuZ2VtZQ=="));
    }

    #[test]
    fn test_auth_header_api_key() {
        let auth = ElasticAuth::ApiKey("mykey123".into());
        let hdr = auth_header(&auth).unwrap();
        assert_eq!(hdr, "ApiKey mykey123");
    }

    #[test]
    fn test_log_no_server_is_silent() {
        let config = ElasticsearchConfig::new("http://127.0.0.1:19998");
        let logger = ElasticsearchAuditLogger::new(config);
        assert!(logger.log(sample_entry()).is_ok());
    }

    #[test]
    fn test_batching_accumulates_before_flush() {
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recv_clone = received.clone();

        let handle = std::thread::spawn(move || {
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
            recv_clone
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).to_string());
        });

        let url = format!("http://127.0.0.1:{}", addr.port());
        let config = ElasticsearchConfig::new(url).with_batch_size(2);
        let logger = ElasticsearchAuditLogger::new(config);

        // First event: should not flush yet
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert_eq!(b.len(), 1);
        }
        // Second event: should flush
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert!(b.is_empty(), "batch should be empty after flush");
        }

        handle.join().unwrap();
        let r = received.lock().unwrap();
        assert_eq!(r.len(), 1, "expected exactly one bulk POST");
        // Each event is 2 NDJSON lines → 2 events = 4 lines
        let body_start = r[0].find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        let body = &r[0][body_start..];
        let non_empty: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(non_empty.len(), 4, "2 events × 2 NDJSON lines = 4 lines");
    }
}
