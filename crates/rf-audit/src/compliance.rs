//! AI Compliance Reporting — generate structured compliance reports from audit data.
//!
//! Supports:
//! - EU AI Act traceability reports
//! - NIST AI Risk Management Framework alignment
//! - Audit report generation (JSON, CSV)
//! - Human-in-loop evidence collection
//! - Incident reconstruction timelines

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Risk classification for AI operations per EU AI Act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Minimal risk — no restrictions.
    Minimal,
    /// Limited risk — transparency obligations.
    Limited,
    /// High risk — conformity assessment, documentation, human oversight required.
    High,
    /// Unacceptable risk — prohibited.
    Unacceptable,
}

/// A compliance report entry — one AI agent action with full context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceEntry {
    pub timestamp: DateTime<Utc>,
    pub agent_id: String,
    pub session_id: String,
    pub action: String,
    pub command: Option<String>,
    pub decision: String,
    pub reasoning: Option<String>,
    pub risk_level: RiskLevel,
    pub human_oversight: Option<HumanOversight>,
    pub policy_applied: String,
    pub matched_rule: String,
    pub duration_ms: u64,
}

/// Evidence of human oversight for high-risk AI operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanOversight {
    pub operator_id: String,
    pub approval_timestamp: DateTime<Utc>,
    pub approval_type: ApprovalType,
    pub notes: Option<String>,
}

/// Type of human approval given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalType {
    /// Explicitly approved by operator.
    Approved,
    /// Denied by operator.
    Denied,
    /// Auto-approved by policy (no human intervention needed).
    PolicyAutoApproved,
    /// Timed out waiting for approval — default action taken.
    TimedOut,
}

/// NIST AI RMF function mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NistAiFunction {
    /// Govern — policies, roles, accountability.
    Govern,
    /// Map — context, risk identification.
    Map,
    /// Measure — metrics, monitoring.
    Measure,
    /// Manage — response, mitigation.
    Manage,
}

/// Maps a RavenFabric control to a NIST AI RMF function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NistMapping {
    pub control: String,
    pub function: NistAiFunction,
    pub subcategory: String,
    pub description: String,
    pub evidence: Vec<String>,
}

/// Configuration for report generation.
#[derive(Debug, Clone)]
pub struct ReportConfig {
    /// Filter by agent ID (None = all agents).
    pub agent_filter: Option<String>,
    /// Filter by time range start.
    pub start_time: Option<DateTime<Utc>>,
    /// Filter by time range end.
    pub end_time: Option<DateTime<Utc>>,
    /// Filter by action type.
    pub action_filter: Option<String>,
    /// Include human oversight evidence.
    pub include_oversight: bool,
    /// Include AI reasoning.
    pub include_reasoning: bool,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            agent_filter: None,
            start_time: None,
            end_time: None,
            action_filter: None,
            include_oversight: true,
            include_reasoning: true,
        }
    }
}

/// Generated compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub title: String,
    pub generated_at: DateTime<Utc>,
    pub report_type: ReportType,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub summary: ReportSummary,
    pub entries: Vec<ComplianceEntry>,
    pub nist_mappings: Vec<NistMapping>,
}

/// Type of compliance report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportType {
    /// EU AI Act traceability report.
    EuAiAct,
    /// NIST AI RMF alignment report.
    NistAiRmf,
    /// General audit report.
    Audit,
    /// Incident reconstruction timeline.
    IncidentReconstruction,
}

/// Summary statistics for a compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_actions: u64,
    pub allowed_actions: u64,
    pub denied_actions: u64,
    pub high_risk_actions: u64,
    pub human_approvals: u64,
    pub unique_agents: u64,
    pub unique_sessions: u64,
    pub anomalies_detected: u64,
}

/// The compliance report generator.
pub struct ReportGenerator {
    entries: Vec<ComplianceEntry>,
}

impl ReportGenerator {
    /// Create a new report generator.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a compliance entry to the report data.
    pub fn add_entry(&mut self, entry: ComplianceEntry) {
        self.entries.push(entry);
    }

    /// Add multiple entries.
    pub fn add_entries(&mut self, entries: impl IntoIterator<Item = ComplianceEntry>) {
        self.entries.extend(entries);
    }

    /// Get count of entries currently stored.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Generate a compliance report with the given configuration.
    pub fn generate(&self, config: &ReportConfig, report_type: ReportType) -> ComplianceReport {
        let filtered = self.filter_entries(config);
        let summary = Self::compute_summary(&filtered);
        let nist_mappings = Self::default_nist_mappings();

        let title = match report_type {
            ReportType::EuAiAct => "EU AI Act Traceability Report".to_string(),
            ReportType::NistAiRmf => "NIST AI Risk Management Framework Alignment".to_string(),
            ReportType::Audit => "AI Agent Audit Report".to_string(),
            ReportType::IncidentReconstruction => "Incident Reconstruction Timeline".to_string(),
        };

        ComplianceReport {
            title,
            generated_at: Utc::now(),
            report_type,
            period_start: config.start_time,
            period_end: config.end_time,
            summary,
            entries: filtered,
            nist_mappings,
        }
    }

    /// Export report as JSON string.
    pub fn export_json(&self, config: &ReportConfig, report_type: ReportType) -> String {
        let report = self.generate(config, report_type);
        serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    }

    /// Export report as CSV string.
    pub fn export_csv(&self, config: &ReportConfig) -> String {
        let filtered = self.filter_entries(config);
        let mut csv = String::from(
            "timestamp,agent_id,session_id,action,command,decision,risk_level,reasoning\n",
        );
        for entry in &filtered {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{:?},{}\n",
                entry.timestamp.to_rfc3339(),
                entry.agent_id,
                entry.session_id,
                entry.action,
                entry.command.as_deref().unwrap_or(""),
                entry.decision,
                entry.risk_level,
                entry.reasoning.as_deref().unwrap_or(""),
            ));
        }
        csv
    }

    fn filter_entries(&self, config: &ReportConfig) -> Vec<ComplianceEntry> {
        self.entries
            .iter()
            .filter(|e| {
                if let Some(ref agent) = config.agent_filter {
                    if &e.agent_id != agent {
                        return false;
                    }
                }
                if let Some(start) = config.start_time {
                    if e.timestamp < start {
                        return false;
                    }
                }
                if let Some(end) = config.end_time {
                    if e.timestamp > end {
                        return false;
                    }
                }
                if let Some(ref action) = config.action_filter {
                    if !e.action.contains(action.as_str()) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    fn compute_summary(entries: &[ComplianceEntry]) -> ReportSummary {
        let mut agents: HashMap<&str, bool> = HashMap::new();
        let mut sessions: HashMap<&str, bool> = HashMap::new();
        let mut allowed = 0u64;
        let mut denied = 0u64;
        let mut high_risk = 0u64;
        let mut approvals = 0u64;

        for entry in entries {
            agents.insert(&entry.agent_id, true);
            sessions.insert(&entry.session_id, true);

            if entry.decision == "allowed" {
                allowed += 1;
            } else {
                denied += 1;
            }

            if entry.risk_level == RiskLevel::High {
                high_risk += 1;
            }

            if entry.human_oversight.is_some() {
                approvals += 1;
            }
        }

        ReportSummary {
            total_actions: entries.len() as u64,
            allowed_actions: allowed,
            denied_actions: denied,
            high_risk_actions: high_risk,
            human_approvals: approvals,
            unique_agents: agents.len() as u64,
            unique_sessions: sessions.len() as u64,
            anomalies_detected: 0,
        }
    }

    /// Default NIST AI RMF control mappings for RavenFabric.
    fn default_nist_mappings() -> Vec<NistMapping> {
        vec![
            NistMapping {
                control: "Deny-by-default policy engine".into(),
                function: NistAiFunction::Govern,
                subcategory: "GV-1.1".into(),
                description: "AI systems operate within organizational policies".into(),
                evidence: vec![
                    "Policy YAML enforcement".into(),
                    "Immutable deny rules".into(),
                ],
            },
            NistMapping {
                control: "Structured audit logging".into(),
                function: NistAiFunction::Measure,
                subcategory: "MS-2.3".into(),
                description: "AI system behavior is monitored and measured".into(),
                evidence: vec!["JSON-lines audit log".into(), "AI reasoning capture".into()],
            },
            NistMapping {
                control: "Behavioral anomaly detection".into(),
                function: NistAiFunction::Manage,
                subcategory: "MG-2.1".into(),
                description: "Identified risks are mitigated through automated response".into(),
                evidence: vec![
                    "Velocity/novelty/timing/escalation detection".into(),
                    "Automatic capability reduction".into(),
                ],
            },
            NistMapping {
                control: "Human-in-loop approval workflow".into(),
                function: NistAiFunction::Govern,
                subcategory: "GV-3.2".into(),
                description: "Human oversight for high-risk AI operations".into(),
                evidence: vec!["Approval records in audit log".into()],
            },
            NistMapping {
                control: "Prompt injection detection".into(),
                function: NistAiFunction::Map,
                subcategory: "MP-4.1".into(),
                description: "Risks from adversarial inputs are identified and mitigated".into(),
                evidence: vec![
                    "Base64/hex/homoglyph detection".into(),
                    "Shell evasion pattern matching".into(),
                ],
            },
        ]
    }
}

impl Default for ReportGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(agent: &str, decision: &str, risk: RiskLevel) -> ComplianceEntry {
        ComplianceEntry {
            timestamp: Utc::now(),
            agent_id: agent.to_string(),
            session_id: "session-1".to_string(),
            action: "execute".to_string(),
            command: Some("ls -la".to_string()),
            decision: decision.to_string(),
            reasoning: Some("checking directory contents".to_string()),
            risk_level: risk,
            human_oversight: None,
            policy_applied: "safe-dev-mode".to_string(),
            matched_rule: "^ls.*".to_string(),
            duration_ms: 42,
        }
    }

    #[test]
    fn test_report_generator_new() {
        let generator = ReportGenerator::new();
        assert_eq!(generator.entry_count(), 0);
    }

    #[test]
    fn test_add_entries() {
        let mut generator = ReportGenerator::new();
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Minimal));
        generator.add_entry(sample_entry("agent-2", "denied", RiskLevel::High));
        assert_eq!(generator.entry_count(), 2);
    }

    #[test]
    fn test_generate_report_summary() {
        let mut generator = ReportGenerator::new();
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Minimal));
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Limited));
        generator.add_entry(sample_entry("agent-2", "denied", RiskLevel::High));

        let report = generator.generate(&ReportConfig::default(), ReportType::Audit);
        assert_eq!(report.summary.total_actions, 3);
        assert_eq!(report.summary.allowed_actions, 2);
        assert_eq!(report.summary.denied_actions, 1);
        assert_eq!(report.summary.high_risk_actions, 1);
        assert_eq!(report.summary.unique_agents, 2);
    }

    #[test]
    fn test_filter_by_agent() {
        let mut generator = ReportGenerator::new();
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Minimal));
        generator.add_entry(sample_entry("agent-2", "allowed", RiskLevel::Minimal));

        let config = ReportConfig {
            agent_filter: Some("agent-1".into()),
            ..ReportConfig::default()
        };
        let report = generator.generate(&config, ReportType::Audit);
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].agent_id, "agent-1");
    }

    #[test]
    fn test_export_json() {
        let mut generator = ReportGenerator::new();
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Minimal));

        let json = generator.export_json(&ReportConfig::default(), ReportType::EuAiAct);
        assert!(json.contains("EU AI Act"));
        assert!(json.contains("agent-1"));
        assert!(json.contains("\"total_actions\": 1"));
    }

    #[test]
    fn test_export_csv() {
        let mut generator = ReportGenerator::new();
        generator.add_entry(sample_entry("agent-1", "allowed", RiskLevel::Minimal));
        generator.add_entry(sample_entry("agent-2", "denied", RiskLevel::High));

        let csv = generator.export_csv(&ReportConfig::default());
        assert!(csv.starts_with("timestamp,"));
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 entries
    }

    #[test]
    fn test_nist_mappings_present() {
        let generator = ReportGenerator::new();
        let report = generator.generate(&ReportConfig::default(), ReportType::NistAiRmf);
        assert!(!report.nist_mappings.is_empty());
        assert!(
            report
                .nist_mappings
                .iter()
                .any(|m| m.function == NistAiFunction::Govern)
        );
        assert!(
            report
                .nist_mappings
                .iter()
                .any(|m| m.function == NistAiFunction::Measure)
        );
    }

    #[test]
    fn test_report_types() {
        let generator = ReportGenerator::new();
        let eu = generator.generate(&ReportConfig::default(), ReportType::EuAiAct);
        assert_eq!(eu.title, "EU AI Act Traceability Report");

        let nist = generator.generate(&ReportConfig::default(), ReportType::NistAiRmf);
        assert_eq!(nist.title, "NIST AI Risk Management Framework Alignment");
    }

    #[test]
    fn test_human_oversight_entry() {
        let mut entry = sample_entry("agent-1", "allowed", RiskLevel::High);
        entry.human_oversight = Some(HumanOversight {
            operator_id: "operator-1".into(),
            approval_timestamp: Utc::now(),
            approval_type: ApprovalType::Approved,
            notes: Some("Reviewed and approved".into()),
        });

        let mut generator = ReportGenerator::new();
        generator.add_entry(entry);
        let report = generator.generate(&ReportConfig::default(), ReportType::EuAiAct);
        assert_eq!(report.summary.human_approvals, 1);
    }
}
