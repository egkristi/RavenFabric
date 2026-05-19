//! Real-time alert rules evaluated against audit events.
//!
//! An `AlertRule` matches audit entries by pattern (applied to the action, decision,
//! and optional command fields). When a rule fires, it logs a structured warning via
//! `tracing::warn!` and records the timestamp for deduplication.
//!
//! Deduplication: the same rule will not fire more than once within
//! `dedup_window_secs` seconds for the same event key (rule name + action).
//! This prevents alert storms when a policy denial repeats rapidly.

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
        })
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

/// Engine that evaluates a set of `AlertRule`s against incoming audit entries
/// and fires deduplicated alerts.
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    /// Last fired time per (rule_name, action) key.
    last_fired: Mutex<HashMap<String, Instant>>,
}

impl AlertEngine {
    /// Create an engine from a list of rules.
    pub fn new(rules: Vec<AlertRule>) -> Self {
        Self {
            rules,
            last_fired: Mutex::new(HashMap::new()),
        }
    }

    /// Evaluate all rules against `entry`. For each matching, non-deduplicated rule,
    /// emit a `tracing::warn!` alert and return the rule name.
    ///
    /// Returns the names of all rules that fired.
    pub fn evaluate(&self, entry: &AuditEntry) -> Vec<String> {
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
}
