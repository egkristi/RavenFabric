//! Real-time alert rules evaluated against audit events.
//!
//! An `AlertRule` matches audit entries by pattern (applied to the action, decision,
//! and optional command fields). When a rule fires, it logs a structured warning via
//! `tracing::warn!` and records the timestamp for deduplication.
//!
//! Deduplication: the same rule will not fire more than once within
//! `dedup_window_secs` seconds for the same event key (rule name + action).
//! This prevents alert storms when a policy denial repeats rapidly.
//!
//! ## Alert destinations
//!
//! Rules can optionally deliver alerts to an HTTP webhook endpoint via
//! `AlertRule::with_webhook(url)`. When a non-deduplicated rule fires, an
//! asynchronous HTTP POST is spawned with a JSON payload describing the event.
//! The POST is fire-and-forget; delivery failures are logged at `warn` level.
//!
//! Supported URL scheme: `http://host:port/path` (plain HTTP only).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::types::AuditEntry;

/// A compiled alert rule.
#[derive(Debug)]
pub struct AlertRule {
    /// Human-readable name used in log output and deduplication key.
    pub name: String,
    /// Pattern matched against `entry.action`, `entry.decision`, and `entry.command`
    /// (any field match triggers the rule). Case-insensitive.
    pub pattern: Regex,
    /// Minimum seconds between repeated alerts for the same rule+action pair.
    /// Set to `0` to disable deduplication.
    pub dedup_window_secs: u64,
    /// Optional HTTP webhook URL (`http://host:port/path`).
    /// When set, a JSON POST is sent asynchronously on every non-deduplicated fire.
    pub webhook_url: Option<String>,
}

impl AlertRule {
    /// Create a new alert rule with the given pattern string (compiled as regex).
    pub fn new(
        name: impl Into<String>,
        pattern: &str,
        dedup_window_secs: u64,
    ) -> Result<Self, regex::Error> {
        let re = Regex::new(&format!("(?i){pattern}"))?;
        Ok(Self {
            name: name.into(),
            pattern: re,
            dedup_window_secs,
            webhook_url: None,
        })
    }

    /// Configure a webhook URL for this rule.
    ///
    /// When the rule fires (and is not deduplicated), an async HTTP POST is sent
    /// to `url` with a JSON body containing the alert details. Delivery errors
    /// are logged at `warn` level but never propagate to the caller.
    ///
    /// Only `http://` scheme is supported. The URL format is
    /// `http://host:port/path`.
    #[must_use]
    pub fn with_webhook(mut self, url: impl Into<String>) -> Self {
        self.webhook_url = Some(url.into());
        self
    }

    /// Returns `true` if this rule matches the given audit entry.
    fn matches(&self, entry: &AuditEntry) -> bool {
        if self.pattern.is_match(&entry.action) {
            return true;
        }
        if self.pattern.is_match(&entry.decision) {
            return true;
        }
        if let Some(cmd) = &entry.command {
            if self.pattern.is_match(cmd) {
                return true;
            }
        }
        false
    }
}

// ── webhook delivery ─────────────────────────────────────────────────────────

/// Parse an `http://host:port/path` URL into `(host, port, path)`.
/// Returns `None` if the URL is not a valid plain-HTTP URL.
fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
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

/// Fire-and-forget HTTP POST to the configured webhook URL.
async fn post_webhook(url: String, payload: String) {
    use tokio::io::AsyncWriteExt;

    let Some((host, port, path)) = parse_http_url(&url) else {
        tracing::warn!(
            webhook_url = %url,
            "alert webhook: invalid URL — expected http://host:port/path"
        );
        return;
    };

    let addr = format!("{host}:{port}");
    let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await else {
        tracing::warn!(webhook_url = %url, "alert webhook: connection failed to {addr}");
        return;
    };

    let body_len = payload.len();
    let request = format!(
        "POST {path} HTTP/1.0\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {body_len}\r\n\
         Connection: close\r\n\
         \r\n\
         {payload}"
    );

    if let Err(e) = stream.write_all(request.as_bytes()).await {
        tracing::warn!(webhook_url = %url, error = %e, "alert webhook: send failed");
    }
}

/// Configuration for staleness detection.
///
/// When no audit entries have been received for `max_idle_secs`, the engine
/// fires a synthetic "staleness" alert. This is useful for detecting agent
/// crashes, network partitions, or silent failures.
#[derive(Debug, Clone)]
pub struct StalenessConfig {
    /// Maximum seconds without any audit activity before firing a staleness alert.
    pub max_idle_secs: u64,
    /// Minimum seconds between repeated staleness alerts (dedup).
    pub dedup_window_secs: u64,
    /// Optional webhook URL for staleness alerts.
    pub webhook_url: Option<String>,
}

impl Default for StalenessConfig {
    fn default() -> Self {
        Self {
            max_idle_secs: 300,     // 5 minutes
            dedup_window_secs: 600, // 10 minutes between repeats
            webhook_url: None,
        }
    }
}

/// Engine that evaluates a set of `AlertRule`s against incoming audit entries
/// and fires deduplicated alerts.
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    /// Last fired time per (rule_name, action) key.
    last_fired: Mutex<HashMap<String, Instant>>,
    /// Timestamp of the last audit entry received.
    last_activity: Mutex<Instant>,
    /// Staleness detection configuration.
    staleness: StalenessConfig,
    /// Last time a staleness alert was fired (for dedup).
    last_staleness_alert: Mutex<Option<Instant>>,
}

impl AlertEngine {
    /// Create an engine from a list of rules.
    pub fn new(rules: Vec<AlertRule>) -> Self {
        Self {
            rules,
            last_fired: Mutex::new(HashMap::new()),
            last_activity: Mutex::new(Instant::now()),
            staleness: StalenessConfig::default(),
            last_staleness_alert: Mutex::new(None),
        }
    }

    /// Configure staleness detection.
    #[must_use]
    pub fn with_staleness(mut self, config: StalenessConfig) -> Self {
        self.staleness = config;
        self
    }

    /// Record that an audit entry was received (updates the last-activity timestamp).
    pub fn record_activity(&self) {
        if let Ok(mut last) = self.last_activity.lock() {
            *last = Instant::now();
        }
    }

    /// Check if the audit stream has gone silent and fire a staleness alert if so.
    ///
    /// Returns the staleness alert name if one was fired, or `None` if the stream
    /// is still active or the staleness alert is suppressed by dedup.
    ///
    /// This should be called periodically (e.g., every 30 seconds) by a watchdog task.
    pub fn check_staleness(&self) -> Option<String> {
        let now = Instant::now();

        // Check if we've exceeded the max idle window
        let idle = match self.last_activity.lock() {
            Ok(last) => now.duration_since(*last).as_secs(),
            Err(_) => return None,
        };

        if idle < self.staleness.max_idle_secs {
            return None; // Still within acceptable window
        }

        // Dedup check
        let should_fire = match self.last_staleness_alert.lock() {
            Ok(guard) => match *guard {
                Some(last) => {
                    now.duration_since(last)
                        >= Duration::from_secs(self.staleness.dedup_window_secs)
                }
                None => true,
            },
            Err(_) => return None,
        };

        if !should_fire {
            return None;
        }

        // Fire the staleness alert
        if let Ok(mut last) = self.last_staleness_alert.lock() {
            *last = Some(now);
        }

        let alert_name = format!("staleness-{idle}s");
        tracing::warn!(
            alert_rule = "staleness",
            idle_seconds = idle,
            max_idle_seconds = self.staleness.max_idle_secs,
            "ALERT: no audit entries received — possible agent crash or network partition"
        );

        // Dispatch webhook if configured
        if let Some(ref url) = self.staleness.webhook_url {
            let payload = serde_json::json!({
                "rule": "staleness",
                "action": "audit_staleness",
                "decision": "warning",
                "idle_seconds": idle,
                "max_idle_seconds": self.staleness.max_idle_secs,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
            .to_string();
            let url = url.clone();
            tokio::spawn(post_webhook(url, payload));
        }

        Some(alert_name)
    }

    /// Evaluate all rules against `entry`. For each matching, non-deduplicated rule,
    /// emit a `tracing::warn!` alert and return the rule name.
    ///
    /// Returns the names of all rules that fired.
    pub fn evaluate(&self, entry: &AuditEntry) -> Vec<String> {
        // Record activity for staleness detection
        self.record_activity();

        let now = Instant::now();
        let mut fired = Vec::new();

        let mut last = self.last_fired.lock().unwrap_or_else(|p| p.into_inner());

        for rule in &self.rules {
            if !rule.matches(entry) {
                continue;
            }

            let key = format!("{}:{}", rule.name, entry.action);
            let should_fire = if rule.dedup_window_secs == 0 {
                true
            } else {
                match last.get(&key) {
                    None => true,
                    Some(prev) => {
                        now.duration_since(*prev) >= Duration::from_secs(rule.dedup_window_secs)
                    }
                }
            };

            if should_fire {
                last.insert(key, now);
                tracing::warn!(
                    alert_rule = %rule.name,
                    action = %entry.action,
                    decision = %entry.decision,
                    request_id = %entry.request_id,
                    matched_rule = %entry.matched_rule,
                    command = ?entry.command,
                    "ALERT: audit event matched rule"
                );

                // Dispatch webhook asynchronously if configured.
                if let Some(url) = &rule.webhook_url {
                    let payload = serde_json::json!({
                        "rule": rule.name,
                        "action": entry.action,
                        "decision": entry.decision,
                        "request_id": entry.request_id,
                        "matched_rule": entry.matched_rule,
                        "command": entry.command,
                        "timestamp": entry.timestamp.to_rfc3339(),
                    })
                    .to_string();
                    let url = url.clone();
                    tokio::spawn(post_webhook(url, payload));
                }

                fired.push(rule.name.clone());
            }
        }

        fired
    }

    /// Return the number of configured rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(action: &str, decision: &str, command: Option<&str>) -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: "test-req".to_string(),
            action: action.to_string(),
            command: command.map(|s| s.to_string()),
            decision: decision.to_string(),
            matched_rule: "test-rule".to_string(),
            exit_code: None,
            duration_ms: 0,
            caller_key: "test-key".to_string(),
            reason: None,
        }
    }

    #[test]
    fn test_alert_fires_on_matching_decision() {
        let rule = AlertRule::new("deny-alert", "denied", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);
        let fired = engine.evaluate(&e);
        assert_eq!(fired, vec!["deny-alert"]);
    }

    #[test]
    fn test_alert_fires_on_matching_action() {
        let rule = AlertRule::new("exec-alert", "exec", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "allowed", None);
        let fired = engine.evaluate(&e);
        assert_eq!(fired, vec!["exec-alert"]);
    }

    #[test]
    fn test_alert_fires_on_matching_command() {
        let rule = AlertRule::new("rm-alert", "rm -rf", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", Some("rm -rf /tmp/foo"));
        let fired = engine.evaluate(&e);
        assert_eq!(fired, vec!["rm-alert"]);
    }

    #[test]
    fn test_alert_no_match_returns_empty() {
        let rule = AlertRule::new("http-alert", "http_forward", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "allowed", Some("echo hi"));
        let fired = engine.evaluate(&e);
        assert!(fired.is_empty());
    }

    #[test]
    fn test_alert_dedup_suppresses_repeated() {
        // dedup_window_secs = 60 — second evaluation within window must be suppressed
        let rule = AlertRule::new("deny-alert", "denied", 60).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);
        let first = engine.evaluate(&e);
        let second = engine.evaluate(&e);
        assert_eq!(first.len(), 1, "first should fire");
        assert!(second.is_empty(), "second should be suppressed by dedup");
    }

    #[test]
    fn test_alert_dedup_disabled_fires_every_time() {
        let rule = AlertRule::new("deny-alert", "denied", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);
        let first = engine.evaluate(&e);
        let second = engine.evaluate(&e);
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1, "dedup=0 fires every time");
    }

    #[test]
    fn test_alert_case_insensitive() {
        let rule = AlertRule::new("deny-alert", "DENIED", 0).unwrap();
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);
        let fired = engine.evaluate(&e);
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn test_multiple_rules_multiple_fires() {
        let rules = vec![
            AlertRule::new("deny-alert", "denied", 0).unwrap(),
            AlertRule::new("exec-alert", "exec", 0).unwrap(),
        ];
        let engine = AlertEngine::new(rules);
        let e = entry("exec", "denied", None);
        let mut fired = engine.evaluate(&e);
        fired.sort();
        assert_eq!(fired, vec!["deny-alert", "exec-alert"]);
    }

    #[test]
    fn test_engine_rule_count() {
        let rules = vec![
            AlertRule::new("r1", "exec", 0).unwrap(),
            AlertRule::new("r2", "denied", 0).unwrap(),
        ];
        let engine = AlertEngine::new(rules);
        assert_eq!(engine.rule_count(), 2);
    }

    // ── webhook tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_webhook_url_configured() {
        let rule = AlertRule::new("hook-rule", "denied", 0)
            .unwrap()
            .with_webhook("http://localhost:9999/hook");
        assert_eq!(
            rule.webhook_url.as_deref(),
            Some("http://localhost:9999/hook")
        );
    }

    #[test]
    fn test_no_webhook_by_default() {
        let rule = AlertRule::new("plain-rule", "denied", 0).unwrap();
        assert!(rule.webhook_url.is_none());
    }

    #[tokio::test]
    async fn test_webhook_delivered_on_alert() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let rule = AlertRule::new("webhook-rule", "denied", 0)
            .unwrap()
            .with_webhook(format!("http://127.0.0.1:{port}/alert"));
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);
        let fired = engine.evaluate(&e);
        assert_eq!(fired, vec!["webhook-rule"]);

        // The POST is dispatched asynchronously — accept the connection.
        let (mut conn, _) =
            tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                .await
                .expect("webhook timed out")
                .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = conn.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);

        assert!(
            request.contains("POST /alert HTTP/1.0"),
            "expected POST line in {request}"
        );
        assert!(
            request.contains("application/json"),
            "expected content-type in {request}"
        );
        assert!(
            request.contains("webhook-rule"),
            "expected rule name in payload: {request}"
        );
        assert!(
            request.contains("\"action\":\"exec\""),
            "expected action in payload: {request}"
        );
    }

    #[tokio::test]
    async fn test_webhook_not_fired_when_deduped() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        // dedup_window_secs = 60 — second fire within window must be suppressed
        let rule = AlertRule::new("dedup-hook", "denied", 60)
            .unwrap()
            .with_webhook(format!("http://127.0.0.1:{port}/alert"));
        let engine = AlertEngine::new(vec![rule]);
        let e = entry("exec", "denied", None);

        let first = engine.evaluate(&e);
        let second = engine.evaluate(&e); // should be suppressed

        assert_eq!(first.len(), 1, "first evaluation must fire");
        assert!(
            second.is_empty(),
            "second evaluation must be suppressed by dedup"
        );

        // Exactly one connection arrives (from the first fire).
        let (mut conn, _) =
            tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                .await
                .expect("first webhook timed out")
                .unwrap();
        let mut consumed = Vec::new();
        conn.read_to_end(&mut consumed).await.unwrap();

        // No second connection should arrive.
        let second_conn =
            tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept()).await;
        assert!(
            second_conn.is_err(),
            "no second webhook expected after dedup"
        );
    }
}
