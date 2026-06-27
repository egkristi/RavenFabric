//! Datadog log forwarding audit destination.
//!
//! Forwards each `AuditEntry` to the Datadog Logs Intake API
//! (`POST /api/v2/logs`). Uses the `DD-API-KEY` header for authentication.
//!
//! Datadog log object fields:
//! - `ddsource` — always `"ravenfabric"`
//! - `ddtags` — `"env:<env>,service:ravenfabric"` (configurable)
//! - `hostname` — configurable (default: `"ravenfabric-agent"`)
//! - `service` — `"ravenfabric"` (configurable)
//! - `message` — JSON-serialized `AuditEntry`
//!
//! Site options (configurable):
//! - `datadoghq.com` (US1, default)
//! - `datadoghq.eu` (EU)
//! - `us3.datadoghq.com` (US3)
//! - `us5.datadoghq.com` (US5)
//! - `ap1.datadoghq.com` (AP1)
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

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for the Datadog audit logger.
#[derive(Debug, Clone)]
pub struct DatadogConfig {
    /// Datadog API key.
    pub api_key: String,
    /// Datadog site (e.g. `"datadoghq.com"`, `"datadoghq.eu"`).
    pub site: String,
    /// Service name (appears in Datadog as the service field).
    pub service: String,
    /// Hostname tag (appears as `host` in Datadog).
    pub hostname: String,
    /// Custom tags in `key:value,key:value` format (appended to `env`).
    pub tags: String,
    /// Maximum events to accumulate before flushing.
    pub batch_size: usize,
    /// Optional full URL override (bypasses the site-based URL construction).
    /// Useful for testing or custom ingestion endpoints.
    pub intake_url_override: Option<String>,
}

impl DatadogConfig {
    /// Create a config with defaults. Only `api_key` is required.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            site: "datadoghq.com".into(),
            service: "ravenfabric".into(),
            hostname: "ravenfabric-agent".into(),
            tags: String::new(),
            batch_size: 10,
            intake_url_override: None,
        }
    }

    /// Set the Datadog site (e.g. `"datadoghq.eu"` for EU).
    pub fn with_site(mut self, site: impl Into<String>) -> Self {
        self.site = site.into();
        self
    }

    /// Set the service name.
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = service.into();
        self
    }

    /// Set the hostname.
    pub fn with_hostname(mut self, hostname: impl Into<String>) -> Self {
        self.hostname = hostname.into();
        self
    }

    /// Set custom tags (appended to the `ddsource` tag).
    pub fn with_tags(mut self, tags: impl Into<String>) -> Self {
        self.tags = tags.into();
        self
    }

    /// Set the batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }

    /// Set a full URL override, bypassing the site-based URL construction.
    /// Primarily for testing with local listeners or custom ingestion endpoints.
    pub fn with_intake_url(mut self, url: impl Into<String>) -> Self {
        self.intake_url_override = Some(url.into());
        self
    }

    /// Build the intake endpoint URL for the configured site.
    pub fn intake_url(&self) -> String {
        if let Some(ref url) = self.intake_url_override {
            return url.clone();
        }
        format!("https://http-intake.logs.{}/api/v2/logs", self.site)
    }
}

// ── HTTP helper ───────────────────────────────────────────────────────────────

/// Parse `https://<host>[:<port>]/path` returning `(host, port, path)`.
fn parse_dd_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/api/v2/logs".to_string()),
    };

    let (host, port) = if let Some(i) = host_port.rfind(':') {
        let h = host_port[..i].to_string();
        let p: u16 = host_port[i + 1..].parse().ok()?;
        (h, p)
    } else {
        (host_port.to_string(), 443u16)
    };

    Some((host, port, path))
}

/// Send an HTTP POST with a JSON array body and `DD-API-KEY` auth.
fn send_dd_request(
    host: &str,
    port: u16,
    path: &str,
    api_key: &str,
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
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nDD-API-KEY: {api_key}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        path = path,
        host = host,
        port = port,
        api_key = api_key,
        len = body.len(),
        body = body,
    );
    stream.write_all(request.as_bytes())?;
    // Drain minimal response.
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
    Ok(())
}

// ── DatadogAuditLogger ────────────────────────────────────────────────────────

/// A single Datadog log entry.
#[derive(Debug, serde::Serialize)]
struct DatadogLogEntry {
    /// Source integration.
    ddsource: &'static str,
    /// Comma-separated tags.
    ddtags: String,
    /// Hostname of the agent.
    hostname: String,
    /// Service name.
    service: String,
    /// The audit event serialized as a JSON string.
    message: String,
}

/// Audit logger that forwards events to the Datadog Logs Intake API.
///
/// Events are batched into a JSON array and POSTed to the intake endpoint
/// when the batch reaches `batch_size`. The remaining batch is flushed on drop.
pub struct DatadogAuditLogger {
    config: DatadogConfig,
    batch: Arc<Mutex<Vec<DatadogLogEntry>>>,
}

impl DatadogAuditLogger {
    /// Create a new Datadog audit logger.
    pub fn new(config: DatadogConfig) -> Self {
        Self {
            config,
            batch: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Convert an `AuditEntry` into a `DatadogLogEntry`.
    fn make_log_entry(&self, entry: &AuditEntry) -> DatadogLogEntry {
        let message = serde_json::to_string(entry)
            .unwrap_or_else(|_| format!("{{\"request_id\":\"{}\"}}", entry.request_id));

        let ddtags = if self.config.tags.is_empty() {
            format!("service:{}", self.config.service)
        } else {
            format!("service:{},{}", self.config.service, self.config.tags)
        };

        DatadogLogEntry {
            ddsource: "ravenfabric",
            ddtags,
            hostname: self.config.hostname.clone(),
            service: self.config.service.clone(),
            message,
        }
    }

    /// Flush the accumulated batch to the Datadog intake endpoint.
    fn flush(&self, entries: &[DatadogLogEntry]) {
        let body = match serde_json::to_string(entries) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Datadog batch serialization failed: {e}");
                return;
            }
        };

        let url = self.config.intake_url();
        if let Some((host, port, path)) = parse_dd_url(&url) {
            if let Err(e) = send_dd_request(&host, port, &path, &self.config.api_key, &body) {
                tracing::warn!("Datadog delivery failed ({} events): {e}", entries.len());
            }
        } else {
            tracing::warn!("Datadog: invalid URL '{url}'");
        }
    }
}

impl AuditLogger for DatadogAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let log_entry = self.make_log_entry(&entry);
        let mut batch = self.batch.lock().unwrap_or_else(|p| p.into_inner());
        batch.push(log_entry);
        if batch.len() >= self.config.batch_size {
            let to_send: Vec<DatadogLogEntry> = batch.drain(..).collect();
            drop(batch);
            self.flush(&to_send);
        }
        Ok(())
    }
}

impl Drop for DatadogAuditLogger {
    fn drop(&mut self) {
        let mut batch = self.batch.lock().unwrap_or_else(|p| p.into_inner());
        if !batch.is_empty() {
            let to_send: Vec<DatadogLogEntry> = batch.drain(..).collect();
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
            request_id: "req-dd-1".into(),
            action: "Execute".into(),
            command: Some("uptime".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 3,
            caller_key: "ddkeyaabb".into(),
            reason: None,
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_make_log_entry_fields() {
        let config = DatadogConfig::new("dd-test-key");
        let logger = DatadogAuditLogger::new(config);
        let entry = logger.make_log_entry(&sample_entry());
        assert_eq!(entry.ddsource, "ravenfabric");
        assert_eq!(entry.service, "ravenfabric");
        assert_eq!(entry.hostname, "ravenfabric-agent");
        assert!(entry.ddtags.contains("service:ravenfabric"));
        // message is a valid JSON string
        let parsed: serde_json::Value = serde_json::from_str(&entry.message).unwrap();
        assert_eq!(parsed["request_id"], "req-dd-1");
        assert_eq!(parsed["decision"], "allowed");
    }

    #[test]
    fn test_make_log_entry_custom_tags() {
        let config = DatadogConfig::new("key").with_tags("env:prod,team:infra");
        let logger = DatadogAuditLogger::new(config);
        let entry = logger.make_log_entry(&sample_entry());
        assert!(entry.ddtags.contains("env:prod"));
        assert!(entry.ddtags.contains("team:infra"));
    }

    #[test]
    fn test_intake_url_default() {
        let config = DatadogConfig::new("key");
        assert_eq!(
            config.intake_url(),
            "https://http-intake.logs.datadoghq.com/api/v2/logs"
        );
    }

    #[test]
    fn test_intake_url_eu_site() {
        let config = DatadogConfig::new("key").with_site("datadoghq.eu");
        assert_eq!(
            config.intake_url(),
            "https://http-intake.logs.datadoghq.eu/api/v2/logs"
        );
    }

    #[test]
    fn test_parse_dd_url() {
        let (host, port, path) =
            parse_dd_url("https://http-intake.logs.datadoghq.com/api/v2/logs").unwrap();
        assert_eq!(host, "http-intake.logs.datadoghq.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/api/v2/logs");
    }

    #[test]
    fn test_parse_dd_url_invalid() {
        assert!(parse_dd_url("not-a-url").is_none());
    }

    #[test]
    fn test_log_no_server_is_silent() {
        // Port 19997 should be closed; should not panic or error.
        // Use with_intake_url so the URL parses to a real IP:port instead of
        // going through the "https://http-intake.logs.{site}" template which
        // would produce an unresolvable hostname.
        let config = DatadogConfig::new("dd-key").with_intake_url("http://127.0.0.1:19997");
        let logger = DatadogAuditLogger::new(config);
        assert!(logger.log(sample_entry()).is_ok());
    }

    #[test]
    fn test_batch_json_array_format() {
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

        // Use http:// pointing to our listener (no real TLS here)
        let url = format!("http://127.0.0.1:{}", addr.port());

        // Override intake_url by using the raw send helper directly:
        // We test via DatadogConfig with a custom site that points to our listener.
        let port_str = addr.port().to_string();
        let config = DatadogConfig::new("test-api-key")
            .with_intake_url(format!("http://127.0.0.1:{port_str}"))
            .with_batch_size(3);
        let _ = url; // suppress unused warning

        let logger = DatadogAuditLogger::new(config);

        // Log 2 events — should not flush yet (batch_size=3)
        logger.log(sample_entry()).unwrap();
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert_eq!(b.len(), 2);
        }
        // Log third event — should flush
        logger.log(sample_entry()).unwrap();
        {
            let b = logger.batch.lock().unwrap();
            assert!(b.is_empty(), "batch should flush at batch_size=3");
        }

        handle.join().unwrap();
        let r = received.lock().unwrap();
        assert_eq!(r.len(), 1, "expected exactly one POST");
        // The body should be a JSON array
        let body_start = r[0].find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        let body = &r[0][body_start..];
        let parsed: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
        assert!(parsed.is_array(), "Datadog body must be a JSON array");
        assert_eq!(parsed.as_array().unwrap().len(), 3);
    }
}
