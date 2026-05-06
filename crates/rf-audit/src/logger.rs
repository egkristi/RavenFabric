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
}
