use chrono::{DateTime, Utc};
use serde::Serialize;

/// A single audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub request_id: String,
    pub action: String,
    pub command: Option<String>,
    pub decision: String,
    pub matched_rule: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub caller_key: String,
}
