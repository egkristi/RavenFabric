//! Behavioral anomaly detection for AI agent sessions.
//!
//! Maintains per-identity baselines and detects deviations that may indicate
//! compromised agents, prompt injection, or runaway automation loops.
//!
//! # Anomaly Types
//!
//! - **Velocity** — too many commands in a time window
//! - **Novelty** — accessing paths/commands never seen before in baseline
//! - **Timing** — activity at unusual hours for this identity
//! - **Escalation** — repeated denied-then-reformulated attempts (probing)
//!
//! # Response Actions
//!
//! High anomaly scores trigger automatic capability reduction or session termination.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Configuration for anomaly detection thresholds.
#[derive(Debug, Clone)]
pub struct AnomalyConfig {
    /// Rolling window size for baseline collection.
    pub window_duration: Duration,
    /// Maximum commands per minute before velocity alert.
    pub max_commands_per_minute: u32,
    /// Z-score threshold for statistical deviation alerting.
    pub z_score_threshold: f64,
    /// Anomaly score threshold that triggers capability reduction.
    pub reduction_threshold: f64,
    /// Anomaly score threshold that triggers session termination.
    pub termination_threshold: f64,
    /// Number of consecutive denials that constitute escalation probing.
    pub escalation_denial_count: u32,
    /// Hours considered "normal" for this identity (0-23).
    pub normal_hours: (u8, u8),
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            window_duration: Duration::from_secs(3600), // 1 hour rolling window
            max_commands_per_minute: 60,
            z_score_threshold: 3.0,
            reduction_threshold: 7.0,
            termination_threshold: 15.0,
            escalation_denial_count: 5,
            normal_hours: (6, 22), // 06:00–22:00
        }
    }
}

/// Types of anomalies that can be detected.
#[derive(Debug, Clone, PartialEq)]
pub enum AnomalyType {
    /// Too many commands in the time window.
    Velocity {
        commands_per_minute: f64,
        threshold: u32,
    },
    /// Accessing paths or commands never seen in baseline.
    Novelty {
        resource: String,
        baseline_size: usize,
    },
    /// Activity at unusual hours for this identity.
    Timing { hour: u8, normal_range: (u8, u8) },
    /// Repeated denied-then-reformulated attempts.
    Escalation {
        denial_count: u32,
        window_seconds: u64,
    },
}

/// An anomaly event with score and metadata.
#[derive(Debug, Clone)]
pub struct AnomalyEvent {
    pub anomaly_type: AnomalyType,
    pub score: f64,
    pub identity: String,
    pub timestamp: Instant,
    pub description: String,
}

/// Recommended response action based on cumulative anomaly score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyResponse {
    /// No action needed — within normal parameters.
    None,
    /// Log the anomaly for review.
    Log,
    /// Reduce capabilities (tighten policy).
    ReduceCapabilities,
    /// Terminate the session immediately.
    TerminateSession,
}

/// A recorded command for baseline tracking.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CommandRecord {
    command: String,
    timestamp: Instant,
    denied: bool,
}

/// Per-identity behavioral baseline and anomaly tracker.
#[derive(Debug)]
pub struct IdentityBaseline {
    identity: String,
    config: AnomalyConfig,
    /// Rolling window of recent commands.
    recent_commands: VecDeque<CommandRecord>,
    /// Known command patterns seen in baseline.
    known_commands: HashMap<String, u64>,
    /// Known paths accessed in baseline.
    known_paths: HashMap<String, u64>,
    /// Cumulative anomaly score for this session.
    cumulative_score: f64,
    /// Recent anomaly events.
    events: Vec<AnomalyEvent>,
    /// Count of recent denials (for escalation detection).
    recent_denials: VecDeque<Instant>,
}

impl IdentityBaseline {
    /// Create a new baseline tracker for the given identity.
    pub fn new(identity: impl Into<String>, config: AnomalyConfig) -> Self {
        Self {
            identity: identity.into(),
            config,
            recent_commands: VecDeque::new(),
            known_commands: HashMap::new(),
            known_paths: HashMap::new(),
            cumulative_score: 0.0,
            events: Vec::new(),
            recent_denials: VecDeque::new(),
        }
    }

    /// Record a command execution and check for anomalies.
    pub fn record_command(
        &mut self,
        command: &str,
        denied: bool,
        current_hour: u8,
    ) -> Vec<AnomalyEvent> {
        let now = Instant::now();
        let mut anomalies = Vec::new();

        // Record the command
        self.recent_commands.push_back(CommandRecord {
            command: command.to_string(),
            timestamp: now,
            denied,
        });

        // Trim window
        self.trim_window(now);

        // Track denials for escalation detection
        if denied {
            self.recent_denials.push_back(now);
        }
        self.trim_denials(now);

        // Check velocity
        if let Some(event) = self.check_velocity(now) {
            anomalies.push(event);
        }

        // Check novelty
        if let Some(event) = self.check_novelty(command) {
            anomalies.push(event);
        }

        // Check timing
        if let Some(event) = self.check_timing(current_hour) {
            anomalies.push(event);
        }

        // Check escalation
        if denied {
            if let Some(event) = self.check_escalation() {
                anomalies.push(event);
            }
        }

        // Update baseline (only non-denied commands are "normal")
        if !denied {
            *self
                .known_commands
                .entry(normalize_command(command))
                .or_insert(0) += 1;
        }

        // Update cumulative score
        for event in &anomalies {
            self.cumulative_score += event.score;
        }
        self.events.extend(anomalies.clone());

        anomalies
    }

    /// Record a path access for baseline tracking.
    pub fn record_path_access(&mut self, path: &str) {
        *self.known_paths.entry(path.to_string()).or_insert(0) += 1;
    }

    /// Get the recommended response based on cumulative anomaly score.
    pub fn recommended_response(&self) -> AnomalyResponse {
        if self.cumulative_score >= self.config.termination_threshold {
            AnomalyResponse::TerminateSession
        } else if self.cumulative_score >= self.config.reduction_threshold {
            AnomalyResponse::ReduceCapabilities
        } else if self.cumulative_score > 0.0 {
            AnomalyResponse::Log
        } else {
            AnomalyResponse::None
        }
    }

    /// Get the current cumulative anomaly score.
    pub fn score(&self) -> f64 {
        self.cumulative_score
    }

    /// Get all recorded anomaly events.
    pub fn events(&self) -> &[AnomalyEvent] {
        &self.events
    }

    /// Get the identity this baseline tracks.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Reset the cumulative score (e.g., after operator review).
    pub fn reset_score(&mut self) {
        self.cumulative_score = 0.0;
    }

    // --- Internal checks ---

    fn trim_window(&mut self, now: Instant) {
        while let Some(front) = self.recent_commands.front() {
            if now.duration_since(front.timestamp) > self.config.window_duration {
                self.recent_commands.pop_front();
            } else {
                break;
            }
        }
    }

    fn trim_denials(&mut self, now: Instant) {
        let window = Duration::from_secs(60);
        while let Some(front) = self.recent_denials.front() {
            if now.duration_since(*front) > window {
                self.recent_denials.pop_front();
            } else {
                break;
            }
        }
    }

    fn check_velocity(&self, now: Instant) -> Option<AnomalyEvent> {
        // Count commands in the last minute
        let one_minute_ago = now - Duration::from_secs(60);
        let count = self
            .recent_commands
            .iter()
            .filter(|r| r.timestamp > one_minute_ago)
            .count() as f64;

        let threshold = f64::from(self.config.max_commands_per_minute);
        if count > threshold {
            Some(AnomalyEvent {
                anomaly_type: AnomalyType::Velocity {
                    commands_per_minute: count,
                    threshold: self.config.max_commands_per_minute,
                },
                score: (count - threshold) / threshold * 3.0,
                identity: self.identity.clone(),
                timestamp: now,
                description: format!(
                    "Velocity anomaly: {count:.0} commands/min (threshold: {})",
                    self.config.max_commands_per_minute
                ),
            })
        } else {
            None
        }
    }

    fn check_novelty(&self, command: &str) -> Option<AnomalyEvent> {
        let normalized = normalize_command(command);

        // Only trigger if we have a meaningful baseline
        if self.known_commands.len() < 10 {
            return None;
        }

        if !self.known_commands.contains_key(&normalized) {
            Some(AnomalyEvent {
                anomaly_type: AnomalyType::Novelty {
                    resource: command.to_string(),
                    baseline_size: self.known_commands.len(),
                },
                score: 1.5,
                identity: self.identity.clone(),
                timestamp: Instant::now(),
                description: format!(
                    "Novelty: command '{}' not seen in baseline ({} known patterns)",
                    truncate_command(command),
                    self.known_commands.len()
                ),
            })
        } else {
            None
        }
    }

    fn check_timing(&self, current_hour: u8) -> Option<AnomalyEvent> {
        let (start, end) = self.config.normal_hours;
        let is_normal = if start <= end {
            current_hour >= start && current_hour < end
        } else {
            // Wraps around midnight (e.g., 22-06)
            current_hour >= start || current_hour < end
        };

        if !is_normal {
            Some(AnomalyEvent {
                anomaly_type: AnomalyType::Timing {
                    hour: current_hour,
                    normal_range: self.config.normal_hours,
                },
                score: 2.0,
                identity: self.identity.clone(),
                timestamp: Instant::now(),
                description: format!(
                    "Timing anomaly: activity at hour {current_hour} (normal: {start:02}:00-{end:02}:00)",
                ),
            })
        } else {
            None
        }
    }

    fn check_escalation(&self) -> Option<AnomalyEvent> {
        let count = self.recent_denials.len() as u32;
        if count >= self.config.escalation_denial_count {
            Some(AnomalyEvent {
                anomaly_type: AnomalyType::Escalation {
                    denial_count: count,
                    window_seconds: 60,
                },
                score: 4.0,
                identity: self.identity.clone(),
                timestamp: Instant::now(),
                description: format!(
                    "Escalation: {count} denials in 60s (threshold: {})",
                    self.config.escalation_denial_count
                ),
            })
        } else {
            None
        }
    }
}

/// Normalize a command to its base pattern (first word + arg count).
fn normalize_command(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }
    format!("{}[{}]", parts[0], parts.len() - 1)
}

/// Truncate a command for display in descriptions.
fn truncate_command(command: &str) -> &str {
    if command.len() > 50 {
        &command[..50]
    } else {
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AnomalyConfig::default();
        assert_eq!(config.max_commands_per_minute, 60);
        assert_eq!(config.z_score_threshold, 3.0);
        assert_eq!(config.normal_hours, (6, 22));
    }

    #[test]
    fn test_new_baseline_empty() {
        let baseline = IdentityBaseline::new("test-agent", AnomalyConfig::default());
        assert_eq!(baseline.identity(), "test-agent");
        assert_eq!(baseline.score(), 0.0);
        assert_eq!(baseline.recommended_response(), AnomalyResponse::None);
    }

    #[test]
    fn test_normal_command_no_anomaly() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());
        let events = baseline.record_command("ls -la", false, 10);
        assert!(events.is_empty());
    }

    #[test]
    fn test_timing_anomaly_late_night() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());
        let events = baseline.record_command("ls -la", false, 3); // 3 AM
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].anomaly_type,
            AnomalyType::Timing { hour: 3, .. }
        ));
        assert!(baseline.score() > 0.0);
    }

    #[test]
    fn test_timing_normal_hours() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());
        let events = baseline.record_command("ls -la", false, 14); // 2 PM
        assert!(events.is_empty());
    }

    #[test]
    fn test_escalation_detection() {
        let config = AnomalyConfig {
            escalation_denial_count: 3,
            ..AnomalyConfig::default()
        };
        let mut baseline = IdentityBaseline::new("agent-1", config);

        // First two denials — no escalation yet
        let events = baseline.record_command("rm -rf /", true, 10);
        assert!(
            events
                .iter()
                .all(|e| !matches!(e.anomaly_type, AnomalyType::Escalation { .. }))
        );
        let events = baseline.record_command("sudo rm -rf /", true, 10);
        assert!(
            events
                .iter()
                .all(|e| !matches!(e.anomaly_type, AnomalyType::Escalation { .. }))
        );

        // Third denial triggers escalation
        let events = baseline.record_command("rm -rf /*", true, 10);
        assert!(
            events
                .iter()
                .any(|e| matches!(e.anomaly_type, AnomalyType::Escalation { .. }))
        );
    }

    #[test]
    fn test_novelty_needs_baseline() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());

        // With fewer than 10 known commands, novelty check is skipped
        for i in 0..5 {
            baseline.record_command(&format!("cmd{i}"), false, 10);
        }
        let events = baseline.record_command("brand-new-cmd", false, 10);
        assert!(
            events
                .iter()
                .all(|e| !matches!(e.anomaly_type, AnomalyType::Novelty { .. }))
        );
    }

    #[test]
    fn test_novelty_triggers_with_baseline() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());

        // Build up baseline with 10+ unique command patterns
        for i in 0..15 {
            baseline.record_command(&format!("known-cmd-{i} arg1"), false, 10);
        }

        // Now a novel command should trigger
        let events = baseline.record_command("never-seen-before arg1 arg2", false, 10);
        assert!(
            events
                .iter()
                .any(|e| matches!(e.anomaly_type, AnomalyType::Novelty { .. }))
        );
    }

    #[test]
    fn test_reduction_threshold() {
        let config = AnomalyConfig {
            reduction_threshold: 5.0,
            termination_threshold: 10.0,
            ..AnomalyConfig::default()
        };
        let mut baseline = IdentityBaseline::new("agent-1", config);

        // Timing anomaly at 3 AM scores 2.0 each
        baseline.record_command("cmd1", false, 3);
        baseline.record_command("cmd2", false, 3);
        baseline.record_command("cmd3", false, 3); // score = 6.0

        assert_eq!(
            baseline.recommended_response(),
            AnomalyResponse::ReduceCapabilities
        );
    }

    #[test]
    fn test_termination_threshold() {
        let config = AnomalyConfig {
            reduction_threshold: 5.0,
            termination_threshold: 10.0,
            ..AnomalyConfig::default()
        };
        let mut baseline = IdentityBaseline::new("agent-1", config);

        // Drive score above termination threshold
        for _ in 0..6 {
            baseline.record_command("cmd", false, 3); // 2.0 each = 12.0 total
        }

        assert_eq!(
            baseline.recommended_response(),
            AnomalyResponse::TerminateSession
        );
    }

    #[test]
    fn test_reset_score() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());
        baseline.record_command("cmd", false, 3); // timing anomaly
        assert!(baseline.score() > 0.0);

        baseline.reset_score();
        assert_eq!(baseline.score(), 0.0);
        assert_eq!(baseline.recommended_response(), AnomalyResponse::None);
    }

    #[test]
    fn test_normalize_command() {
        assert_eq!(normalize_command("ls -la /tmp"), "ls[2]");
        assert_eq!(normalize_command("git commit -m 'msg'"), "git[3]");
        assert_eq!(normalize_command("echo"), "echo[0]");
        assert_eq!(normalize_command(""), "");
    }

    #[test]
    fn test_record_path_access() {
        let mut baseline = IdentityBaseline::new("agent-1", AnomalyConfig::default());
        baseline.record_path_access("/tmp/test.txt");
        baseline.record_path_access("/tmp/test.txt");
        baseline.record_path_access("/var/log/syslog");
        assert_eq!(baseline.known_paths.len(), 2);
        assert_eq!(baseline.known_paths["/tmp/test.txt"], 2);
    }
}
