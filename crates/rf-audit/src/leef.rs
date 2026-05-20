//! LEEF (Log Event Extended Format) audit log formatter.
//!
//! LEEF is a log format developed by IBM for QRadar SIEM systems. Each event
//! is a single tab-delimited line with a fixed header followed by name=value
//! attribute pairs.
//!
//! LEEF 2.0 format:
//! ```text
//! LEEF:2.0|Vendor|Product|Version|EventID|Label|attr1=val1\tattr2=val2\t...
//! ```
//!
//! LEEF 1.0 format (fallback):
//! ```text
//! LEEF:1.0|Vendor|Product|Version|EventID|attr1=val1\tattr2=val2\t...
//! ```

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

const VENDOR: &str = "RavenFabric";
const PRODUCT: &str = "RavenFabric";

/// LEEF version to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeefVersion {
    /// LEEF 1.0 — supported by QRadar 7.x
    V1,
    /// LEEF 2.0 — supported by QRadar 7.2+ (adds Label field and custom delimiter)
    #[default]
    V2,
}

impl LeefVersion {
    fn header(&self) -> &'static str {
        match self {
            LeefVersion::V1 => "LEEF:1.0",
            LeefVersion::V2 => "LEEF:2.0",
        }
    }
}

/// Map an audit decision to a LEEF severity (0–10, same scale as CEF).
fn leef_severity(decision: &str) -> u8 {
    match decision {
        d if d.contains("denied") => 8,
        d if d.contains("error") => 7,
        d if d.contains("allowed") => 3,
        _ => 5,
    }
}

/// Escape a LEEF attribute value.
///
/// Per IBM QRadar LEEF spec, the following characters must be escaped in attribute values:
/// - `\` → `\\`
/// - `=` → `\=`
/// - newline → `\n`
/// - carriage return → `\r`
/// - tab → `\t` (tab is the default attribute delimiter)
fn escape_leef_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Escape a LEEF header field (pipe-delimited fields).
/// Only `|` and `\` need to be escaped in header fields.
fn escape_leef_header(s: &str) -> String {
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Format an `AuditEntry` as a LEEF log line.
///
/// `product_version` is embedded in the LEEF header (e.g. `"0.11.0"`).
/// Returns the formatted line without trailing newline.
pub fn format_leef(entry: &AuditEntry, product_version: &str, version: LeefVersion) -> String {
    let severity = leef_severity(&entry.decision);
    let event_id = escape_leef_header(&entry.action);
    let ts_epoch_ms = entry.timestamp.timestamp_millis();

    // Attribute pairs (tab-separated per LEEF spec)
    let mut attrs = vec![
        format!("devTime={ts_epoch_ms}"),
        format!("requestId={}", escape_leef_value(&entry.request_id)),
        format!("act={}", escape_leef_value(&entry.action)),
        format!("outcome={}", escape_leef_value(&entry.decision)),
        format!("sev={severity}"),
        format!("src={}", escape_leef_value(&entry.caller_key)),
        format!("duration={}", entry.duration_ms),
        format!("matchedRule={}", escape_leef_value(&entry.matched_rule)),
    ];

    if let Some(ref cmd) = entry.command {
        attrs.push(format!("cmd={}", escape_leef_value(cmd)));
    }
    if let Some(code) = entry.exit_code {
        attrs.push(format!("exitCode={code}"));
    }
    if let Some(ref reason) = entry.reason {
        attrs.push(format!("reason={}", escape_leef_value(reason)));
    }

    let attrs_str = attrs.join("\t");

    match version {
        LeefVersion::V1 => {
            format!(
                "{header}|{vendor}|{product}|{ver}|{event_id}|{attrs}",
                header = version.header(),
                vendor = VENDOR,
                product = PRODUCT,
                ver = escape_leef_header(product_version),
                event_id = event_id,
                attrs = attrs_str,
            )
        }
        LeefVersion::V2 => {
            // LEEF 2.0 adds Label field after EventID
            let label = escape_leef_header(entry.command.as_deref().unwrap_or(&entry.action));
            format!(
                "{header}|{vendor}|{product}|{ver}|{event_id}|{label}|{attrs}",
                header = version.header(),
                vendor = VENDOR,
                product = PRODUCT,
                ver = escape_leef_header(product_version),
                event_id = event_id,
                label = label,
                attrs = attrs_str,
            )
        }
    }
}

// ── LeefAuditLogger ───────────────────────────────────────────────────────────

/// Audit logger wrapper that formats entries as LEEF before forwarding to an
/// inner logger.
pub struct LeefAuditLogger<L: AuditLogger> {
    inner: L,
    product_version: String,
    leef_version: LeefVersion,
}

impl<L: AuditLogger> LeefAuditLogger<L> {
    /// Wrap `inner` with LEEF 2.0 formatting.
    pub fn new(inner: L, product_version: impl Into<String>) -> Self {
        Self {
            inner,
            product_version: product_version.into(),
            leef_version: LeefVersion::V2,
        }
    }

    /// Wrap `inner` with the specified LEEF version.
    pub fn with_version(
        inner: L,
        product_version: impl Into<String>,
        leef_version: LeefVersion,
    ) -> Self {
        Self {
            inner,
            product_version: product_version.into(),
            leef_version,
        }
    }
}

impl<L: AuditLogger> AuditLogger for LeefAuditLogger<L> {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let leef_line = format_leef(&entry, &self.product_version, self.leef_version);
        let mut leef_entry = entry;
        leef_entry.action = leef_line;
        self.inner.log(leef_entry)
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
            request_id: "req-leef-1".into(),
            action: "execute".into(),
            command: Some("journalctl -n 100".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 22,
            caller_key: "cafebeef".into(),
            reason: None,
        }
    }

    #[test]
    fn test_leef2_format_header_structure() {
        let entry = sample_entry();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        assert!(
            line.starts_with("LEEF:2.0|RavenFabric|RavenFabric|0.11.0|"),
            "unexpected header: {line}"
        );
    }

    #[test]
    fn test_leef1_format_header_structure() {
        let entry = sample_entry();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V1);
        assert!(
            line.starts_with("LEEF:1.0|RavenFabric|RavenFabric|0.11.0|"),
            "unexpected header: {line}"
        );
        // LEEF 1.0 has no Label field — header has 5 pipe-delimited fields before attrs
        let pipes: Vec<_> = line.splitn(6, '|').collect();
        assert_eq!(pipes.len(), 6, "LEEF 1.0 should have exactly 6 pipe parts");
    }

    #[test]
    fn test_leef2_has_label_field() {
        let entry = sample_entry();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        // LEEF 2.0: LEEF:2.0|Vendor|Product|Version|EventID|Label|attrs
        let pipes: Vec<_> = line.splitn(7, '|').collect();
        assert_eq!(pipes.len(), 7, "LEEF 2.0 should have 7 pipe parts");
        // Label should be the command
        assert!(
            pipes[5].contains("journalctl"),
            "LEEF 2.0 label should contain command: {line}"
        );
    }

    #[test]
    fn test_leef_contains_required_attributes() {
        let entry = sample_entry();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        assert!(line.contains("requestId=req-leef-1"), "missing requestId");
        assert!(line.contains("outcome=allowed"), "missing outcome");
        assert!(line.contains("src=cafebeef"), "missing src (caller_key)");
        assert!(line.contains("duration=22"), "missing duration");
        assert!(line.contains("act=execute"), "missing act");
        assert!(line.contains("sev=3"), "missing severity (allowed=3)");
    }

    #[test]
    fn test_leef_severity_denied() {
        let mut entry = sample_entry();
        entry.decision = "denied".into();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        assert!(
            line.contains("sev=8"),
            "expected severity 8 for denied: {line}"
        );
    }

    #[test]
    fn test_leef_escaping_in_attribute_values() {
        let mut entry = sample_entry();
        entry.request_id = "req=special\\val".into();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        assert!(
            line.contains("requestId=req\\=special\\\\val"),
            "= and \\ not escaped in attribute value: {line}"
        );
    }

    #[test]
    fn test_leef_pipe_escaped_in_header() {
        let mut entry = sample_entry();
        entry.action = "exec|action".into();
        let line = format_leef(&entry, "0.11.0", LeefVersion::V2);
        assert!(
            line.contains("exec\\|action"),
            "pipe not escaped in header: {line}"
        );
    }

    #[test]
    fn test_leef_audit_logger_delegates() {
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

        let logger = LeefAuditLogger::new(ArcLogger(inner.clone()), "0.11.0");
        logger.log(sample_entry()).unwrap();

        let entries = inner.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0].starts_with("LEEF:2.0|"),
            "inner should receive LEEF-formatted string: {}",
            entries[0]
        );
    }

    #[test]
    fn test_leef_audit_logger_null_inner() {
        let logger = LeefAuditLogger::new(NullAuditLogger, "0.11.0");
        assert!(logger.log(sample_entry()).is_ok());
    }
}
