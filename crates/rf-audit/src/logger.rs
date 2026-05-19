use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::types::AuditEntry;

/// Errors from audit logging operations.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("lock poisoned")]
    LockPoisoned,
}

/// Trait for audit loggers.
pub trait AuditLogger: Send + Sync {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError>;
}

/// File-based JSON-lines audit logger.
pub struct FileAuditLogger {
    file: Mutex<std::fs::File>,
}

impl FileAuditLogger {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl AuditLogger for FileAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let json = serde_json::to_string(&entry)?;
        let mut file = self.file.lock().map_err(|_| AuditError::LockPoisoned)?;
        writeln!(file, "{json}")?;
        Ok(())
    }
}

/// No-op audit logger for testing.
pub struct NullAuditLogger;

impl AuditLogger for NullAuditLogger {
    fn log(&self, _entry: AuditEntry) -> Result<(), AuditError> {
        Ok(())
    }
}

// ── webhook log forwarding ────────────────────────────────────────────────────

/// Audit logger that wraps an inner logger and also forwards each entry to a
/// remote HTTP webhook via an asynchronous HTTP POST (fire-and-forget).
///
/// Each audit entry is serialized to JSON (identical to the JSON-lines format
/// written by `FileAuditLogger`) and POSTed to the configured URL as the request
/// body with `Content-Type: application/json`.
///
/// Delivery is best-effort. Connection failures and HTTP errors are logged at
/// `warn` level and never surface as errors to the caller.
///
/// Supported URL scheme: `http://host:port/path` (plain HTTP only).
pub struct WebhookAuditLogger {
    inner: Box<dyn AuditLogger>,
    webhook_url: String,
}

impl WebhookAuditLogger {
    /// Create a new webhook logger.
    ///
    /// Each call to `log()` delegates to `inner` and then spawns an async task
    /// that POSTs the JSON-serialized entry to `webhook_url`.
    pub fn new(inner: Box<dyn AuditLogger>, webhook_url: impl Into<String>) -> Self {
        Self {
            inner,
            webhook_url: webhook_url.into(),
        }
    }
}

impl AuditLogger for WebhookAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        // Delegate to the inner logger first.
        self.inner.log(entry.clone())?;

        // Serialize and dispatch asynchronously.
        let json = serde_json::to_string(&entry)?;
        let url = self.webhook_url.clone();
        tokio::spawn(post_audit_webhook(url, json));

        Ok(())
    }
}

/// Parse `http://host:port/path` into `(host, port, path)`.
fn parse_webhook_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = if let Some(idx) = rest.find('/') {
        (&rest[..idx], rest[idx..].to_string())
    } else {
        (rest, "/".to_string())
    };
    let (host, port) = if let Some(idx) = host_port.rfind(':') {
        let port: u16 = host_port[idx + 1..].parse().ok()?;
        (host_port[..idx].to_string(), port)
    } else {
        (host_port.to_string(), 80u16)
    };
    Some((host, port, path))
}

/// Fire-and-forget HTTP POST for audit log forwarding.
async fn post_audit_webhook(url: String, body: String) {
    use tokio::io::AsyncWriteExt;

    let Some((host, port, path)) = parse_webhook_url(&url) else {
        tracing::warn!(
            webhook_url = %url,
            "audit webhook: invalid URL — expected http://host:port/path"
        );
        return;
    };

    let addr = format!("{host}:{port}");
    let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await else {
        tracing::warn!(webhook_url = %url, "audit webhook: connection failed to {addr}");
        return;
    };

    let body_len = body.len();
    let request = format!(
        "POST {path} HTTP/1.0\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {body_len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}"
    );

    if let Err(e) = stream.write_all(request.as_bytes()).await {
        tracing::warn!(webhook_url = %url, error = %e, "audit webhook: send failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AuditEntry;
    use chrono::Utc;

    fn sample_entry() -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: "req-001".to_string(),
            action: "execute".to_string(),
            command: Some("echo hello".to_string()),
            decision: "allowed".to_string(),
            matched_rule: "^echo .*".to_string(),
            exit_code: Some(0),
            duration_ms: 42,
            caller_key: "aabbccdd".to_string(),
            reason: None,
        }
    }

    #[test]
    fn test_file_audit_logger_writes_jsonl() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone()).expect("create logger");

        logger.log(sample_entry()).expect("log entry");
        logger.log(sample_entry()).expect("log second entry");

        let content = std::fs::read_to_string(&path).expect("read log");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON that deserializes back
        let parsed: AuditEntry = serde_json::from_str(lines[0]).expect("parse entry");
        assert_eq!(parsed.request_id, "req-001");
        assert_eq!(parsed.action, "execute");
    }

    #[test]
    fn test_null_audit_logger() {
        let logger = NullAuditLogger;
        assert!(logger.log(sample_entry()).is_ok());
    }

    #[test]
    fn test_audit_entry_roundtrip() {
        let entry = sample_entry();
        let json = serde_json::to_string(&entry).expect("serialize");
        let parsed: AuditEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, parsed);
    }

    #[test]
    fn test_audit_entry_with_reason() {
        let mut entry = sample_entry();
        entry.reason = Some("AI agent needs to check service health".into());

        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("AI agent needs to check service health"));

        let parsed: AuditEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            parsed.reason.unwrap(),
            "AI agent needs to check service health"
        );
    }

    #[test]
    fn test_audit_entry_reason_omitted_when_none() {
        let entry = sample_entry();
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(!json.contains("reason"));
    }

    // ── WebhookAuditLogger tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_webhook_audit_logger_delivers_entry() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let inner = Box::new(NullAuditLogger);
        let webhook_logger =
            WebhookAuditLogger::new(inner, format!("http://127.0.0.1:{port}/audit"));

        webhook_logger.log(sample_entry()).expect("log entry");

        // Accept the incoming webhook connection.
        let (mut conn, _) =
            tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                .await
                .expect("webhook timed out")
                .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);

        assert!(
            request.contains("POST /audit HTTP/1.0"),
            "expected POST line: {request}"
        );
        assert!(
            request.contains("application/json"),
            "expected content-type: {request}"
        );
        assert!(
            request.contains("req-001"),
            "expected request_id in payload: {request}"
        );
        assert!(
            request.contains("execute"),
            "expected action in payload: {request}"
        );
    }

    #[tokio::test]
    async fn test_webhook_audit_logger_delegates_to_inner() {
        use tokio::io::AsyncReadExt;

        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let inner = Box::new(FileAuditLogger::new(path.clone()).unwrap());
        let webhook_logger =
            WebhookAuditLogger::new(inner, format!("http://127.0.0.1:{port}/audit"));

        webhook_logger.log(sample_entry()).expect("log entry");

        // Verify inner logger wrote to file.
        let content = std::fs::read_to_string(&path).expect("read log");
        assert_eq!(
            content.lines().count(),
            1,
            "inner logger must write one line"
        );
        let parsed: AuditEntry =
            serde_json::from_str(content.lines().next().unwrap()).expect("parse json");
        assert_eq!(parsed.request_id, "req-001");

        // Consume the webhook connection so the spawned task can finish.
        let (mut conn, _) =
            tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                .await
                .expect("webhook timed out")
                .unwrap();
        let mut buf = Vec::new();
        conn.read_to_end(&mut buf).await.unwrap();
    }

    #[test]
    fn test_parse_webhook_url_with_port_and_path() {
        let result = parse_webhook_url("http://example.com:8080/audit/v1");
        assert_eq!(
            result,
            Some(("example.com".to_string(), 8080, "/audit/v1".to_string()))
        );
    }

    #[test]
    fn test_parse_webhook_url_default_port() {
        let result = parse_webhook_url("http://example.com/hook");
        assert_eq!(
            result,
            Some(("example.com".to_string(), 80, "/hook".to_string()))
        );
    }

    #[test]
    fn test_parse_webhook_url_invalid() {
        assert!(parse_webhook_url("https://example.com/hook").is_none());
        assert!(parse_webhook_url("not-a-url").is_none());
    }
}
