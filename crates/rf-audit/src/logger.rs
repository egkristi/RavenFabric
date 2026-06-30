use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

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
    #[error("HMAC verification failed at entry {index}: expected {expected}, got {computed}")]
    HmacMismatch {
        index: usize,
        expected: String,
        computed: String,
    },
    #[error("HMAC chain broken at entry {index}: prev_hash mismatch")]
    ChainBroken { index: usize },
    #[error("invalid HMAC key: {0}")]
    InvalidKey(String),
    #[error("deletion filter matched no entries")]
    NoEntriesMatched,
}

/// Filter criteria for deleting audit entries.
///
/// All specified fields are ANDed together — an entry must match every
/// non-None criterion to be selected for deletion.
#[derive(Debug, Clone, Default)]
pub struct DeletionFilter {
    /// Delete entries older than this timestamp.
    pub older_than: Option<DateTime<Utc>>,
    /// Delete entries with this exact action name.
    pub action: Option<String>,
    /// Delete entries with this caller key.
    pub caller_key: Option<String>,
    /// Delete entries with this decision (e.g. "allowed", "denied").
    pub decision: Option<String>,
    /// Delete entries whose `request_id` contains this substring.
    pub request_id_contains: Option<String>,
}

/// Trait for audit loggers.
pub trait AuditLogger: Send + Sync {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError>;
}

/// File-based JSON-lines audit logger with HMAC chain integrity.
///
/// Each entry is linked to the previous entry via `prev_hash` (SHA-256 of the
/// previous entry's canonical JSON) and signed with `hmac` (HMAC-SHA256 over
/// all fields). This provides tamper-evident audit logging.
///
/// Supports data retention operations: `purge_entries_before()` and
/// `delete_entries_by_filter()` for GDPR right-to-erasure and PCI-DSS/SOC 2
/// retention policy enforcement.
pub struct FileAuditLogger {
    file: Mutex<std::fs::File>,
    path: PathBuf,
    hmac_key: Vec<u8>,
    prev_hash: Mutex<String>,
}

impl FileAuditLogger {
    /// Create a new HMAC-chained file audit logger.
    ///
    /// If the file already exists and is non-empty, the last entry's content hash
    /// is loaded as the `prev_hash` for the next entry. If the file is empty or
    /// does not exist, the chain starts fresh (genesis entry).
    pub fn new(path: PathBuf, hmac_key: Vec<u8>) -> std::io::Result<Self> {
        let prev_hash = if path.exists() {
            let file = std::fs::read_to_string(&path)?;
            if let Some(last_line) = file.lines().last() {
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(last_line) {
                    entry.content_hash()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            file: Mutex::new(file),
            path,
            hmac_key,
            prev_hash: Mutex::new(prev_hash),
        })
    }

    /// Purge all entries older than the given timestamp.
    ///
    /// Rewrites the audit log file in place, keeping only entries whose
    /// timestamp is at or after `cutoff`. The HMAC chain is preserved
    /// across the surviving entries.
    ///
    /// Returns the number of entries removed.
    pub fn purge_entries_before(&self, cutoff: DateTime<Utc>) -> Result<usize, AuditError> {
        let entries = self.read_all_entries()?;
        let (kept, removed): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|e| e.timestamp >= cutoff);

        if removed.is_empty() {
            return Ok(0);
        }

        self.rewrite_chain(&kept)?;
        Ok(removed.len())
    }

    /// Delete entries matching the given filter criteria.
    ///
    /// All specified filter fields are ANDed together — an entry must match
    /// every non-None criterion to be selected for deletion. Rewrites the
    /// audit log file preserving the HMAC chain across surviving entries.
    ///
    /// Returns the number of entries removed.
    pub fn delete_entries_by_filter(&self, filter: &DeletionFilter) -> Result<usize, AuditError> {
        let entries = self.read_all_entries()?;

        let (kept, removed): (Vec<_>, Vec<_>) = entries.into_iter().partition(|e| {
            // An entry is KEPT if it does NOT match the filter.
            // If any filter criterion matches, the entry is removed.
            if let Some(ref cutoff) = filter.older_than {
                if e.timestamp < *cutoff {
                    return false;
                }
            }
            if let Some(ref action) = filter.action {
                if e.action == *action {
                    return false;
                }
            }
            if let Some(ref caller_key) = filter.caller_key {
                if e.caller_key == *caller_key {
                    return false;
                }
            }
            if let Some(ref decision) = filter.decision {
                if e.decision == *decision {
                    return false;
                }
            }
            if let Some(ref substr) = filter.request_id_contains {
                if e.request_id.contains(substr) {
                    return false;
                }
            }
            true
        });

        if removed.is_empty() {
            return Err(AuditError::NoEntriesMatched);
        }

        self.rewrite_chain(&kept)?;
        Ok(removed.len())
    }

    /// Read all entries from the audit log file.
    fn read_all_entries(&self) -> Result<Vec<AuditEntry>, AuditError> {
        let file = std::fs::File::open(&self.path)?;
        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(&line)?;
            entries.push(entry);
        }

        Ok(entries)
    }

    /// Rewrite the audit log file with the given entries, preserving the HMAC chain.
    fn rewrite_chain(&self, entries: &[AuditEntry]) -> Result<(), AuditError> {
        // Rebuild the chain from scratch with correct prev_hash and HMAC
        let mut prev_hash = String::new();
        let mut serialized = Vec::new();

        for entry in entries {
            let mut e = entry.clone();
            e.prev_hash = Some(prev_hash.clone());
            e.hmac = Some(e.compute_hmac(&self.hmac_key));
            let json = serde_json::to_string(&e)?;
            serialized.push(json);
            prev_hash = e.content_hash();
        }

        // Atomically rewrite the file
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&self.path)?;

        for line in &serialized {
            writeln!(file, "{line}")?;
        }

        // Update the in-memory prev_hash for subsequent appends
        let mut prev = self
            .prev_hash
            .lock()
            .map_err(|_| AuditError::LockPoisoned)?;
        *prev = prev_hash;

        Ok(())
    }
}

impl AuditLogger for FileAuditLogger {
    fn log(&self, mut entry: AuditEntry) -> Result<(), AuditError> {
        // Set the prev_hash from the chain
        let prev = self
            .prev_hash
            .lock()
            .map_err(|_| AuditError::LockPoisoned)?;
        entry.prev_hash = Some(prev.clone());
        drop(prev);

        // Compute HMAC over all fields (excluding the hmac field itself)
        entry.hmac = Some(entry.compute_hmac(&self.hmac_key));

        // Serialize and write
        let json = serde_json::to_string(&entry)?;
        let mut file = self.file.lock().map_err(|_| AuditError::LockPoisoned)?;
        writeln!(file, "{json}")?;

        // Update prev_hash for next entry
        let content_hash = entry.content_hash();
        let mut prev = self
            .prev_hash
            .lock()
            .map_err(|_| AuditError::LockPoisoned)?;
        *prev = content_hash;

        Ok(())
    }
}

/// Verify the HMAC chain integrity of an audit log file.
///
/// Reads all entries from the file, verifies that:
/// 1. Each entry's `hmac` matches its content (computed with the given key)
/// 2. Each entry's `prev_hash` matches the previous entry's content hash
///
/// Returns `Ok(())` if the entire chain is valid, or an error describing the
/// first integrity violation found.
pub fn verify_audit_chain(path: &PathBuf, hmac_key: &[u8]) -> Result<(), AuditError> {
    let file = std::fs::File::open(path).map_err(AuditError::Io)?;
    let reader = std::io::BufReader::new(file);
    let mut prev_content_hash: Option<String> = None;

    for (i, line) in reader.lines().enumerate() {
        let line = line.map_err(AuditError::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: AuditEntry = serde_json::from_str(&line).map_err(|e| {
            AuditError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line {}: failed to parse entry: {e}", i + 1),
            ))
        })?;

        // Verify prev_hash chain
        if let Some(ref expected_prev) = prev_content_hash {
            match entry.prev_hash {
                Some(ref actual_prev) => {
                    if actual_prev != expected_prev {
                        return Err(AuditError::ChainBroken { index: i + 1 });
                    }
                }
                None => {
                    return Err(AuditError::ChainBroken { index: i + 1 });
                }
            }
        } else {
            // First entry — prev_hash should be empty or genesis
            if let Some(ref prev) = entry.prev_hash {
                if !prev.is_empty() {
                    return Err(AuditError::ChainBroken { index: i + 1 });
                }
            }
        }

        // Verify HMAC
        let expected_hmac = entry.compute_hmac(hmac_key);
        match entry.hmac {
            Some(ref actual_hmac) => {
                if actual_hmac != &expected_hmac {
                    return Err(AuditError::HmacMismatch {
                        index: i + 1,
                        expected: expected_hmac,
                        computed: actual_hmac.clone(),
                    });
                }
            }
            None => {
                return Err(AuditError::HmacMismatch {
                    index: i + 1,
                    expected: expected_hmac,
                    computed: "(none)".to_string(),
                });
            }
        }

        prev_content_hash = Some(entry.content_hash());
    }

    Ok(())
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
            prev_hash: None,
            hmac: None,
        }
    }

    fn test_hmac_key() -> Vec<u8> {
        b"test-hmac-key-0000000000000000".to_vec()
    }

    #[test]
    fn test_file_audit_logger_writes_jsonl() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        logger.log(sample_entry()).expect("log entry");
        logger.log(sample_entry()).expect("log second entry");

        let content = std::fs::read_to_string(&path).expect("read log");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON that deserializes back
        let parsed: AuditEntry = serde_json::from_str(lines[0]).expect("parse entry");
        assert_eq!(parsed.request_id, "req-001");
        assert_eq!(parsed.action, "execute");
        // Verify HMAC chain integrity
        assert!(parsed.hmac.is_some(), "entry should have HMAC");
        assert!(parsed.prev_hash.is_some(), "entry should have prev_hash");
        assert_eq!(
            parsed.prev_hash.as_deref().unwrap_or(""),
            "",
            "first entry prev_hash should be empty"
        );
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

        let inner = Box::new(FileAuditLogger::new(path.clone(), test_hmac_key()).unwrap());
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

    // ── Data retention / deletion tests ───────────────────────────────────────

    fn entry_with_timestamp(ts: DateTime<Utc>, action: &str, caller: &str) -> AuditEntry {
        AuditEntry {
            timestamp: ts,
            request_id: format!("req-{action}-{caller}"),
            action: action.to_string(),
            command: Some("echo test".to_string()),
            decision: "allowed".to_string(),
            matched_rule: "^echo .*".to_string(),
            exit_code: Some(0),
            duration_ms: 10,
            caller_key: caller.to_string(),
            reason: None,
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_purge_entries_before_removes_old() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        let old = now - chrono::Duration::hours(2);
        let very_old = now - chrono::Duration::hours(48);

        logger
            .log(entry_with_timestamp(very_old, "exec", "alice"))
            .expect("log old");
        logger
            .log(entry_with_timestamp(old, "exec", "bob"))
            .expect("log mid");
        logger
            .log(entry_with_timestamp(now, "exec", "charlie"))
            .expect("log recent");

        // Purge entries older than 24 hours
        let cutoff = now - chrono::Duration::hours(24);
        let removed = logger.purge_entries_before(cutoff).expect("purge");
        assert_eq!(removed, 1, "should remove the very_old entry");

        // Verify remaining entries
        let remaining = logger.read_all_entries().expect("read entries");
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].caller_key, "bob");
        assert_eq!(remaining[1].caller_key, "charlie");

        // Verify HMAC chain is still valid
        verify_audit_chain(&path, &test_hmac_key()).expect("chain should be valid");
    }

    #[test]
    fn test_purge_entries_before_no_match() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log");

        // Purge with cutoff in the past — nothing to remove
        let cutoff = now - chrono::Duration::hours(48);
        let removed = logger.purge_entries_before(cutoff).expect("purge");
        assert_eq!(removed, 0, "no entries should be removed");
    }

    #[test]
    fn test_delete_entries_by_filter_action() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log exec");
        logger
            .log(entry_with_timestamp(now, "read", "bob"))
            .expect("log read");
        logger
            .log(entry_with_timestamp(now, "exec", "charlie"))
            .expect("log exec");

        let filter = DeletionFilter {
            action: Some("exec".to_string()),
            ..Default::default()
        };
        let removed = logger.delete_entries_by_filter(&filter).expect("delete");
        assert_eq!(removed, 2, "should remove both exec entries");

        let remaining = logger.read_all_entries().expect("read entries");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].action, "read");

        verify_audit_chain(&path, &test_hmac_key()).expect("chain should be valid");
    }

    #[test]
    fn test_delete_entries_by_filter_caller() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log");
        logger
            .log(entry_with_timestamp(now, "exec", "bob"))
            .expect("log");
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log");

        let filter = DeletionFilter {
            caller_key: Some("alice".to_string()),
            ..Default::default()
        };
        let removed = logger.delete_entries_by_filter(&filter).expect("delete");
        assert_eq!(removed, 2, "should remove both alice entries");

        let remaining = logger.read_all_entries().expect("read entries");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].caller_key, "bob");

        verify_audit_chain(&path, &test_hmac_key()).expect("chain should be valid");
    }

    #[test]
    fn test_delete_entries_by_filter_no_match_returns_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log");

        let filter = DeletionFilter {
            action: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let result = logger.delete_entries_by_filter(&filter);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuditError::NoEntriesMatched));
    }

    #[test]
    fn test_delete_entries_by_filter_combined() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("audit.jsonl");
        let logger = FileAuditLogger::new(path.clone(), test_hmac_key()).expect("create logger");

        let now = Utc::now();
        logger
            .log(entry_with_timestamp(now, "exec", "alice"))
            .expect("log");
        logger
            .log(entry_with_timestamp(now, "exec", "bob"))
            .expect("log");
        logger
            .log(entry_with_timestamp(now, "read", "alice"))
            .expect("log");

        // Filter: older_than=now (removes nothing since all are at "now")
        // Combined with action=exec — since older_than doesn't match anything,
        // only action=exec is effective. This removes 2 entries (both exec).
        let cutoff = now - chrono::Duration::hours(1);
        let filter = DeletionFilter {
            older_than: Some(cutoff),
            action: Some("exec".to_string()),
            ..Default::default()
        };
        let removed = logger.delete_entries_by_filter(&filter).expect("delete");
        assert_eq!(
            removed, 2,
            "should remove both exec entries (older_than matches none, action matches both)"
        );

        let remaining = logger.read_all_entries().expect("read entries");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].action, "read");

        verify_audit_chain(&path, &test_hmac_key()).expect("chain should be valid");
    }
}
