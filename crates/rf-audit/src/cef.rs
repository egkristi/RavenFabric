//! CEF (Common Event Format) audit log formatter.
//!
//! Wraps any `AuditLogger` and converts each `AuditEntry` to a CEF-formatted
//! string before forwarding to the inner logger.  This makes RavenFabric audit
//! events compatible with standard SIEM systems (ArcSight, Splunk, IBM QRadar,
//! etc.).
//!
//! CEF format:
//! ```text
//! CEF:0|Vendor|Product|Version|DeviceEventClassID|Name|Severity|Extension
//! ```
//!
//! CEF severity maps from audit decision:
//! - `denied` → `8` (High)
//! - `allowed` → `3` (Low)
//! - other    → `5` (Medium)

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

const CEF_VERSION: u8 = 0;
const VENDOR: &str = "RavenFabric";
const PRODUCT: &str = "RavenFabric";

/// Map an audit decision string to a CEF severity (0–10).
fn cef_severity(decision: &str) -> u8 {
    match decision {
        d if d.contains("denied") => 8,  // High — policy enforcement
        d if d.contains("error") => 7,   // High — operational error
        d if d.contains("allowed") => 3, // Low — normal operation
        _ => 5,                          // Medium — unknown
    }
}

/// Escape CEF extension field values (escape `=`, `\`, and `\n`).
fn escape_cef_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Escape CEF header fields (escape `|` and `\`).
fn escape_cef_header(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Format an `AuditEntry` as a CEF log line.
///
/// Returns the full CEF string (without trailing newline).
pub fn format_cef(entry: &AuditEntry, product_version: &str) -> String {
    let severity = cef_severity(&entry.decision);
    let class_id = escape_cef_header(&entry.action);
    let name = escape_cef_header(entry.command.as_deref().unwrap_or(&entry.action));

    let ts_epoch_ms = entry.timestamp.timestamp_millis();

    let mut ext = format!(
        "rt={ts_epoch_ms} requestId={} act={} outcome={} dvcpid={} reason={} duration={} cs1Label=matchedRule cs1={}",
        escape_cef_value(&entry.request_id),
        escape_cef_value(&entry.action),
        escape_cef_value(&entry.decision),
        std::process::id(),
        escape_cef_value(entry.reason.as_deref().unwrap_or("-")),
        entry.duration_ms,
        escape_cef_value(&entry.matched_rule),
    );

    if let Some(code) = entry.exit_code {
        ext.push_str(&format!(" exitCode={code}"));
    }

    // caller_key as source user (suser)
    ext.push_str(&format!(" suser={}", escape_cef_value(&entry.caller_key)));

    format!(
        "CEF:{CEF_VERSION}|{vendor}|{product}|{version}|{class_id}|{name}|{severity}|{ext}",
        vendor = VENDOR,
        product = PRODUCT,
        version = escape_cef_header(product_version),
        class_id = class_id,
        name = name,
        severity = severity,
        ext = ext,
    )
}

// ── CefAuditLogger ────────────────────────────────────────────────────────────

/// Audit logger wrapper that formats entries as CEF before forwarding to an
/// inner logger.
///
/// The inner logger receives a modified `AuditEntry` where `action` is
/// replaced with the CEF-formatted line, making it easy to write CEF to a file
/// via `FileAuditLogger`.
pub struct CefAuditLogger<L: AuditLogger> {
    inner: L,
    product_version: String,
}

impl<L: AuditLogger> CefAuditLogger<L> {
    /// Wrap `inner` with CEF formatting.
    ///
    /// `product_version` is embedded in the CEF header (e.g. `"0.10.0"`).
    pub fn new(inner: L, product_version: impl Into<String>) -> Self {
        Self {
            inner,
            product_version: product_version.into(),
        }
    }
}

impl<L: AuditLogger> AuditLogger for CefAuditLogger<L> {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let cef_line = format_cef(&entry, &self.product_version);
        // Forward a modified entry whose `action` carries the CEF line.
        // All other fields are preserved so the inner logger has full context.
        let mut cef_entry = entry;
        cef_entry.action = cef_line;
        self.inner.log(cef_entry)
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
            request_id: "req-cef-1".into(),
            action: "execute".into(),
            command: Some("systemctl status nginx".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 15,
            caller_key: "aabbccdd".into(),
            reason: Some("health check".into()),
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_cef_format_header_structure() {
        let entry = sample_entry();
        let cef = format_cef(&entry, "0.10.0");
        assert!(
            cef.starts_with("CEF:0|RavenFabric|RavenFabric|0.10.0|"),
            "unexpected header: {cef}"
        );
    }

    #[test]
    fn test_cef_severity_allowed() {
        let entry = sample_entry();
        let cef = format_cef(&entry, "0.10.0");
        // Allowed → severity 3
        assert!(
            cef.contains("|3|"),
            "expected severity 3 for allowed: {cef}"
        );
    }

    #[test]
    fn test_cef_severity_denied() {
        let mut entry = sample_entry();
        entry.decision = "denied".into();
        let cef = format_cef(&entry, "0.10.0");
        // Denied → severity 8
        assert!(cef.contains("|8|"), "expected severity 8 for denied: {cef}");
    }

    #[test]
    fn test_cef_extension_contains_key_fields() {
        let entry = sample_entry();
        let cef = format_cef(&entry, "0.10.0");
        assert!(cef.contains("requestId=req-cef-1"), "missing requestId");
        assert!(cef.contains("outcome=allowed"), "missing outcome");
        assert!(cef.contains("suser=aabbccdd"), "missing suser");
        assert!(cef.contains("duration=15"), "missing duration");
        assert!(cef.contains("reason=health check"), "missing reason");
    }

    #[test]
    fn test_cef_escaping_equals_and_pipe() {
        let mut entry = sample_entry();
        entry.action = "exec|pipe".into();
        entry.command = Some("key=value".into());
        // Inject = into request_id to test extension value escaping.
        entry.request_id = "req=special".into();
        let cef = format_cef(&entry, "0.10.0");
        // Pipe in class_id (header field) must be escaped.
        assert!(
            cef.contains("exec\\|pipe"),
            "pipe not escaped in header: {cef}"
        );
        // = in extension field value must be escaped.
        assert!(
            cef.contains("requestId=req\\=special"),
            "= not escaped in extension value: {cef}"
        );
    }

    #[test]
    fn test_cef_no_command_uses_action_as_name() {
        let mut entry = sample_entry();
        entry.command = None;
        entry.action = "status_check".into();
        let cef = format_cef(&entry, "0.10.0");
        // Both class_id and name should be "status_check"
        assert!(
            cef.contains("|status_check|status_check|"),
            "expected action used as name when command is None: {cef}"
        );
    }

    #[test]
    fn test_cef_audit_logger_delegates_to_inner() {
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

        let logger = CefAuditLogger::new(ArcLogger(inner.clone()), "0.10.0");
        logger.log(sample_entry()).unwrap();

        let entries = inner.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].starts_with("CEF:0|"),
            "inner should receive CEF-formatted string: {}",
            entries[0]
        );
    }

    #[test]
    fn test_cef_audit_logger_null_inner() {
        let logger = CefAuditLogger::new(NullAuditLogger, "0.10.0");
        assert!(logger.log(sample_entry()).is_ok());
    }
}
