use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single audit log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// AI agent reasoning (if provided). Records why the agent performed this action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
