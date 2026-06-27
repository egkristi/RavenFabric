//! OCSF (Open Cybersecurity Schema Framework) audit log formatter.
//!
//! OCSF is a vendor-neutral, open-source schema for security event data,
//! designed for interoperability across security tools and platforms.
//! See <https://schema.ocsf.io/>.
//!
//! This module maps `AuditEntry` to OCSF **Activity** events in the
//! **Application Activity** category (class_uid 6003), serialized as JSON.
//!
//! Key OCSF concepts used here:
//! - **class_uid**: 6003 = Application Activity
//! - **activity_id**: 1 = Create (command execution), 2 = Read (file pull)
//! - **severity_id**: maps from policy decision
//! - **status_id**: 1 = Success (allowed), 2 = Failure (denied), 99 = Other

use serde::Serialize;

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

// ── OCSF type constants ───────────────────────────────────────────────────────

/// OCSF class_uid for Application Activity.
const CLASS_UID_APP_ACTIVITY: u32 = 6003;

/// OCSF metadata version.
const OCSF_SCHEMA_VERSION: &str = "1.1.0";

// ── Serializable OCSF types ───────────────────────────────────────────────────

/// OCSF `metadata` object.
#[derive(Debug, Clone, Serialize)]
pub struct OcsfMetadata {
    pub version: String,
    pub product: OcsfProduct,
    pub log_name: String,
    pub log_provider: String,
}

/// OCSF `product` object inside `metadata`.
#[derive(Debug, Clone, Serialize)]
pub struct OcsfProduct {
    pub name: String,
    pub vendor_name: String,
    pub version: String,
}

/// OCSF `actor` object — who performed the action.
#[derive(Debug, Clone, Serialize)]
pub struct OcsfActor {
    /// Caller public key (hex) used as a stable identity.
    pub user: OcsfUser,
}

/// OCSF `user` object.
#[derive(Debug, Clone, Serialize)]
pub struct OcsfUser {
    /// The caller's public key (used as a stable identifier).
    pub uid: String,
    /// Type: 99 = Other (key-based, not traditional user)
    #[serde(rename = "type_id")]
    pub type_id: u8,
}

/// OCSF event object for a RavenFabric audit entry (Application Activity).
#[derive(Debug, Clone, Serialize)]
pub struct OcsfEvent {
    /// OCSF class identifier (6003 = Application Activity).
    pub class_uid: u32,
    /// Human-readable class name.
    pub class_name: String,
    /// Activity identifier (1 = Create/Execute, 2 = Read, 3 = Update, 4 = Delete, 99 = Other).
    pub activity_id: u8,
    /// Human-readable activity name.
    pub activity_name: String,
    /// Epoch milliseconds timestamp.
    pub time: i64,
    /// Severity (0=Unknown, 1=Informational, 2=Low, 3=Medium, 4=High, 5=Critical).
    pub severity_id: u8,
    /// Severity name.
    pub severity: String,
    /// Status (0=Unknown, 1=Success, 2=Failure, 99=Other).
    pub status_id: u8,
    /// Status string.
    pub status: String,
    /// OCSF metadata.
    pub metadata: OcsfMetadata,
    /// Actor (caller identity).
    pub actor: OcsfActor,
    /// The command or RPC action performed.
    pub message: String,
    /// Correlation / request identifier.
    pub correlation_uid: String,
    /// Matched policy rule.
    pub policy_rule: String,
    /// Execution duration in milliseconds.
    pub duration: u64,
    /// Exit code of the executed command (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// AI reasoning (if provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Mapping helpers ───────────────────────────────────────────────────────────

fn ocsf_severity(decision: &str) -> (u8, &'static str) {
    match decision {
        d if d.contains("denied") => (4, "High"),
        d if d.contains("error") => (3, "Medium"),
        d if d.contains("allowed") => (1, "Informational"),
        _ => (0, "Unknown"),
    }
}

fn ocsf_status(decision: &str) -> (u8, &'static str) {
    match decision {
        d if d.contains("allowed") => (1, "Success"),
        d if d.contains("denied") => (2, "Failure"),
        _ => (99, "Other"),
    }
}

fn ocsf_activity(action: &str) -> (u8, &'static str) {
    match action {
        a if a.contains("Execute") || a.contains("execute") || a.contains("Command") => {
            (1, "Execute")
        }
        a if a.contains("Read") || a.contains("FilePull") || a.contains("file_pull") => (2, "Read"),
        a if a.contains("Write") || a.contains("FilePush") || a.contains("file_push") => {
            (3, "Write")
        }
        a if a.contains("Delete") || a.contains("delete") => (4, "Delete"),
        _ => (99, "Other"),
    }
}

// ── Public format function ────────────────────────────────────────────────────

/// Convert an `AuditEntry` to an `OcsfEvent`.
pub fn to_ocsf_event(entry: &AuditEntry, product_version: &str) -> OcsfEvent {
    let (severity_id, severity) = ocsf_severity(&entry.decision);
    let (status_id, status) = ocsf_status(&entry.decision);
    let (activity_id, activity_name) = ocsf_activity(&entry.action);
    let message = match &entry.command {
        Some(cmd) => format!("{} {}", entry.action, cmd),
        None => entry.action.clone(),
    };

    OcsfEvent {
        class_uid: CLASS_UID_APP_ACTIVITY,
        class_name: "Application Activity".into(),
        activity_id,
        activity_name: activity_name.into(),
        time: entry.timestamp.timestamp_millis(),
        severity_id,
        severity: severity.into(),
        status_id,
        status: status.into(),
        metadata: OcsfMetadata {
            version: OCSF_SCHEMA_VERSION.into(),
            product: OcsfProduct {
                name: "RavenFabric".into(),
                vendor_name: "RavenFabric".into(),
                version: product_version.into(),
            },
            log_name: "rf-audit".into(),
            log_provider: "RavenFabric".into(),
        },
        actor: OcsfActor {
            user: OcsfUser {
                uid: entry.caller_key.clone(),
                type_id: 99, // Other — key-based identity
            },
        },
        message,
        correlation_uid: entry.request_id.clone(),
        policy_rule: entry.matched_rule.clone(),
        duration: entry.duration_ms,
        exit_code: entry.exit_code,
        reason: entry.reason.clone(),
    }
}

/// Format an `AuditEntry` as a JSON-serialized OCSF event (single line).
pub fn format_ocsf(entry: &AuditEntry, product_version: &str) -> Result<String, serde_json::Error> {
    let event = to_ocsf_event(entry, product_version);
    serde_json::to_string(&event)
}

// ── OcsfAuditLogger ───────────────────────────────────────────────────────────

/// Audit logger wrapper that formats entries as OCSF JSON before forwarding
/// to an inner logger.
pub struct OcsfAuditLogger<L: AuditLogger> {
    inner: L,
    product_version: String,
}

impl<L: AuditLogger> OcsfAuditLogger<L> {
    /// Wrap `inner` with OCSF JSON formatting.
    ///
    /// `product_version` is embedded in the OCSF `metadata.product.version` field.
    pub fn new(inner: L, product_version: impl Into<String>) -> Self {
        Self {
            inner,
            product_version: product_version.into(),
        }
    }
}

impl<L: AuditLogger> AuditLogger for OcsfAuditLogger<L> {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        match format_ocsf(&entry, &self.product_version) {
            Ok(json) => {
                let mut ocsf_entry = entry;
                ocsf_entry.action = json;
                self.inner.log(ocsf_entry)
            }
            Err(e) => {
                tracing::warn!("OCSF serialization failed: {e}");
                Ok(())
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::{logger::NullAuditLogger, types::AuditEntry};

    fn sample_entry() -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: "req-ocsf-1".into(),
            action: "Execute".into(),
            command: Some("systemctl status nginx".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 30,
            caller_key: "0011aabb".into(),
            reason: Some("health monitor".into()),
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_ocsf_event_class_uid() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.class_uid, 6003);
        assert_eq!(event.class_name, "Application Activity");
    }

    #[test]
    fn test_ocsf_event_severity_allowed() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.severity_id, 1, "allowed → Informational (1)");
        assert_eq!(event.severity, "Informational");
    }

    #[test]
    fn test_ocsf_event_severity_denied() {
        let mut entry = sample_entry();
        entry.decision = "denied".into();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.severity_id, 4, "denied → High (4)");
        assert_eq!(event.status_id, 2, "denied → Failure (2)");
    }

    #[test]
    fn test_ocsf_event_status_allowed() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.status_id, 1);
        assert_eq!(event.status, "Success");
    }

    #[test]
    fn test_ocsf_activity_execute() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.activity_id, 1);
        assert_eq!(event.activity_name, "Execute");
    }

    #[test]
    fn test_ocsf_activity_file_pull() {
        let mut entry = sample_entry();
        entry.action = "FilePull".into();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.activity_id, 2, "FilePull → Read (2)");
    }

    #[test]
    fn test_ocsf_metadata_fields() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.metadata.version, "1.1.0");
        assert_eq!(event.metadata.product.name, "RavenFabric");
        assert_eq!(event.metadata.product.version, "0.11.0");
        assert_eq!(event.metadata.log_provider, "RavenFabric");
    }

    #[test]
    fn test_ocsf_actor_identity() {
        let entry = sample_entry();
        let event = to_ocsf_event(&entry, "0.11.0");
        assert_eq!(event.actor.user.uid, "0011aabb");
        assert_eq!(event.actor.user.type_id, 99); // key-based identity
    }

    #[test]
    fn test_ocsf_format_json_roundtrip() {
        let entry = sample_entry();
        let json = format_ocsf(&entry, "0.11.0").expect("OCSF format failed");
        // Must be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("invalid JSON");
        assert_eq!(parsed["class_uid"], 6003);
        assert_eq!(parsed["correlation_uid"], "req-ocsf-1");
        assert_eq!(parsed["actor"]["user"]["uid"], "0011aabb");
        assert_eq!(parsed["metadata"]["product"]["version"], "0.11.0");
        // reason should be present
        assert_eq!(parsed["reason"], "health monitor");
    }

    #[test]
    fn test_ocsf_format_no_exit_code_no_reason() {
        let mut entry = sample_entry();
        entry.exit_code = None;
        entry.reason = None;
        let json = format_ocsf(&entry, "0.11.0").expect("OCSF format failed");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Fields marked skip_serializing_if = None should be absent
        assert!(
            parsed.get("exit_code").is_none(),
            "exit_code should be absent"
        );
        assert!(parsed.get("reason").is_none(), "reason should be absent");
    }

    #[test]
    fn test_ocsf_audit_logger_delegates_to_inner() {
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct CapturingLogger {
            entries: Mutex<Vec<String>>,
        }
        impl AuditLogger for CapturingLogger {
            fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
                self.entries.lock().unwrap().push(entry.action.clone());
                Ok(())
            }
        }

        let inner = Arc::new(CapturingLogger::default());

        struct ArcLogger(Arc<CapturingLogger>);
        impl AuditLogger for ArcLogger {
            fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
                self.0.log(entry)
            }
        }

        let logger = OcsfAuditLogger::new(ArcLogger(inner.clone()), "0.11.0");
        logger.log(sample_entry()).unwrap();

        let entries = inner.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        // The inner logger receives JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&entries[0]).expect("inner should receive valid OCSF JSON");
        assert_eq!(parsed["class_uid"], 6003);
    }

    #[test]
    fn test_ocsf_audit_logger_null_inner() {
        let logger = OcsfAuditLogger::new(NullAuditLogger, "0.11.0");
        assert!(logger.log(sample_entry()).is_ok());
    }
}
