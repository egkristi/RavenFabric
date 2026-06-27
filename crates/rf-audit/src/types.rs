use chrono::{DateTime, Utc};
use hmac::{KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    /// SHA-256 hash of the previous audit entry in the chain.
    /// Empty string for the first entry (genesis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    /// HMAC-SHA256 of this entry's canonical JSON (excluding the hmac field itself).
    /// Computed over: prev_hash || timestamp || request_id || action || command || decision
    /// || matched_rule || exit_code || duration_ms || caller_key || reason
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hmac: Option<String>,
}

impl AuditEntry {
    /// Compute the canonical bytes for HMAC — all fields serialized deterministically.
    fn canonical_bytes(&self) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        // Use write! to format each field with a separator
        let _ = write!(
            buf,
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|",
            self.prev_hash.as_deref().unwrap_or(""),
            self.timestamp.to_rfc3339(),
            self.request_id,
            self.action,
            self.command.as_deref().unwrap_or(""),
            self.decision,
            self.matched_rule,
            self.exit_code.map_or("".to_string(), |c| c.to_string()),
            self.duration_ms,
            self.caller_key,
        );
        if let Some(ref reason) = self.reason {
            let _ = write!(buf, "{reason}|");
        }
        buf
    }

    /// Compute the HMAC-SHA256 for this entry using the given key.
    pub fn compute_hmac(&self, key: &[u8]) -> String {
        use hmac::Hmac;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(&self.canonical_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Verify this entry's HMAC against the given key.
    pub fn verify_hmac(&self, key: &[u8]) -> bool {
        match self.hmac {
            Some(ref expected) => {
                use hmac::Hmac;
                type HmacSha256 = Hmac<Sha256>;
                let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
                mac.update(&self.canonical_bytes());
                mac.verify_slice(&hex::decode(expected).unwrap_or_default())
                    .is_ok()
            }
            None => false,
        }
    }

    /// Compute the SHA-256 hash of this entry's canonical JSON representation.
    pub fn content_hash(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        hex::encode(hasher.finalize())
    }
}
