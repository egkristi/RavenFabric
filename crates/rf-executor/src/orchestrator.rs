//! Multi-agent orchestration — rolling, canary, parallel execution strategies.
//!
//! Defines the types and logic for executing commands across multiple agents
//! with different rollout strategies and automatic rollback on failure.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Strategy for executing commands across multiple agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RolloutStrategy {
    /// Execute on all agents simultaneously.
    Parallel,
    /// Execute on agents one at a time, stopping on first failure.
    Sequential,
    /// Execute in batches (e.g., 25% at a time), wait for success before next batch.
    Rolling { batch_percent: u8 },
    /// Execute on a small canary group first, then proceed if successful.
    Canary { canary_count: usize },
}

/// Targeting rules for selecting which agents to execute on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetGrain {
    /// Target specific agents by ID.
    Agents(Vec<String>),
    /// Target agents matching a label/tag.
    Label { key: String, value: String },
    /// Target all agents in a group/cluster.
    Group(String),
    /// Target agents matching a glob pattern on their ID.
    Pattern(String),
    /// Target all connected agents.
    All,
}

/// Rollback behavior when a batch/canary fails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackPolicy {
    /// Stop execution, do not rollback already-executed agents.
    StopOnly,
    /// Execute a rollback command on agents that already succeeded.
    Rollback { command: String },
    /// Continue execution despite failures (best-effort).
    Continue,
}

/// A multi-agent execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationPlan {
    /// The command to execute on target agents.
    pub command: String,
    /// Which agents to target.
    pub target: TargetGrain,
    /// Execution strategy.
    pub strategy: RolloutStrategy,
    /// What to do on failure.
    pub on_failure: RollbackPolicy,
    /// Maximum time (seconds) to wait for each agent's execution.
    pub timeout_secs: u64,
}

/// Result of executing on a single agent within an orchestration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentResult {
    /// Agent ID.
    pub agent_id: String,
    /// Whether execution succeeded.
    pub success: bool,
    /// Exit code (if completed).
    pub exit_code: Option<i32>,
    /// Stdout output.
    pub stdout: String,
    /// Stderr output.
    pub stderr: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

/// Overall result of a multi-agent orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationResult {
    /// Results per agent.
    pub results: Vec<AgentResult>,
    /// Whether the overall orchestration succeeded (all agents/batches passed).
    pub success: bool,
    /// Whether a rollback was triggered.
    pub rollback_triggered: bool,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Orchestration state machine — tracks progress of a multi-agent rollout.
#[derive(Debug)]
pub struct Orchestrator {
    plan: OrchestrationPlan,
    resolved_agents: Vec<String>,
    results: Vec<AgentResult>,
    current_batch: usize,
    rollback_triggered: bool,
}

impl Orchestrator {
    /// Create a new orchestrator from a plan and resolved agent list.
    pub fn new(plan: OrchestrationPlan, resolved_agents: Vec<String>) -> Self {
        Self {
            plan,
            resolved_agents,
            results: Vec::new(),
            current_batch: 0,
            rollback_triggered: false,
        }
    }

    /// Get the next batch of agents to execute on (based on strategy).
    pub fn next_batch(&mut self) -> Option<Vec<String>> {
        let total = self.resolved_agents.len();
        if total == 0 {
            return None;
        }

        let already_executed: usize = self.results.len();
        if already_executed >= total {
            return None;
        }

        let batch_size = match &self.plan.strategy {
            RolloutStrategy::Parallel => total,
            RolloutStrategy::Sequential => 1,
            RolloutStrategy::Rolling { batch_percent } => {
                let pct = (*batch_percent).clamp(1, 100) as usize;
                (total * pct / 100).max(1)
            }
            RolloutStrategy::Canary { canary_count } => {
                if self.current_batch == 0 {
                    (*canary_count).min(total)
                } else {
                    total - already_executed
                }
            }
        };

        let end = (already_executed + batch_size).min(total);
        let batch = self.resolved_agents[already_executed..end].to_vec();
        self.current_batch += 1;
        Some(batch)
    }

    /// Record results from a batch execution. Returns whether to continue.
    pub fn record_batch(&mut self, batch_results: Vec<AgentResult>) -> bool {
        let all_success = batch_results.iter().all(|r| r.success);
        self.results.extend(batch_results);

        if !all_success {
            match &self.plan.on_failure {
                RollbackPolicy::Continue => true,
                RollbackPolicy::StopOnly | RollbackPolicy::Rollback { .. } => {
                    if matches!(self.plan.on_failure, RollbackPolicy::Rollback { .. }) {
                        self.rollback_triggered = true;
                    }
                    false
                }
            }
        } else {
            true
        }
    }

    /// Get agents that need rollback (only those that succeeded before failure).
    pub fn agents_needing_rollback(&self) -> Vec<&str> {
        if !self.rollback_triggered {
            return Vec::new();
        }
        self.results
            .iter()
            .filter(|r| r.success)
            .map(|r| r.agent_id.as_str())
            .collect()
    }

    /// Get the rollback command (if rollback policy specifies one).
    pub fn rollback_command(&self) -> Option<&str> {
        match &self.plan.on_failure {
            RollbackPolicy::Rollback { command } => Some(command.as_str()),
            _ => None,
        }
    }

    /// Finalize and produce the orchestration result.
    pub fn finalize(self, total_duration_ms: u64) -> OrchestrationResult {
        let success = self.results.iter().all(|r| r.success);
        OrchestrationResult {
            results: self.results,
            success,
            rollback_triggered: self.rollback_triggered,
            total_duration_ms,
        }
    }

    /// Get the execution plan.
    pub fn plan(&self) -> &OrchestrationPlan {
        &self.plan
    }

    /// Get results collected so far.
    pub fn results_so_far(&self) -> &[AgentResult] {
        &self.results
    }
}

/// Resolve a target grain into a list of agent IDs.
/// In a real system this queries agent registry; here we provide the interface.
pub fn resolve_targets(
    grain: &TargetGrain,
    known_agents: &HashMap<String, HashMap<String, String>>,
) -> Vec<String> {
    match grain {
        TargetGrain::All => known_agents.keys().cloned().collect(),
        TargetGrain::Agents(ids) => ids.clone(),
        TargetGrain::Group(group) => known_agents
            .iter()
            .filter(|(_, labels)| labels.get("group").map(|g| g == group).unwrap_or(false))
            .map(|(id, _)| id.clone())
            .collect(),
        TargetGrain::Label { key, value } => known_agents
            .iter()
            .filter(|(_, labels)| labels.get(key).map(|v| v == value).unwrap_or(false))
            .map(|(id, _)| id.clone())
            .collect(),
        TargetGrain::Pattern(pattern) => {
            // Simple glob: only support trailing *
            let prefix = pattern.trim_end_matches('*');
            known_agents
                .keys()
                .filter(|id| id.starts_with(prefix))
                .cloned()
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plan(strategy: RolloutStrategy) -> OrchestrationPlan {
        OrchestrationPlan {
            command: "systemctl restart app".to_string(),
            target: TargetGrain::All,
            strategy,
            on_failure: RollbackPolicy::StopOnly,
            timeout_secs: 60,
        }
    }

    fn agent_result(id: &str, success: bool) -> AgentResult {
        AgentResult {
            agent_id: id.to_string(),
            success,
            exit_code: if success { Some(0) } else { Some(1) },
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 100,
        }
    }

    #[test]
    fn test_parallel_single_batch() {
        let agents = vec!["a".into(), "b".into(), "c".into()];
        let mut orch = Orchestrator::new(test_plan(RolloutStrategy::Parallel), agents);

        let batch = orch.next_batch().unwrap();
        assert_eq!(batch.len(), 3);
        assert!(orch.next_batch().is_none() || orch.results.is_empty());
    }

    #[test]
    fn test_sequential_one_at_a_time() {
        let agents = vec!["a".into(), "b".into(), "c".into()];
        let mut orch = Orchestrator::new(test_plan(RolloutStrategy::Sequential), agents);

        let batch1 = orch.next_batch().unwrap();
        assert_eq!(batch1, vec!["a"]);
        orch.record_batch(vec![agent_result("a", true)]);

        let batch2 = orch.next_batch().unwrap();
        assert_eq!(batch2, vec!["b"]);
        orch.record_batch(vec![agent_result("b", true)]);

        let batch3 = orch.next_batch().unwrap();
        assert_eq!(batch3, vec!["c"]);
        orch.record_batch(vec![agent_result("c", true)]);

        assert!(orch.next_batch().is_none());
    }

    #[test]
    fn test_rolling_batches() {
        let agents: Vec<String> = (0..10).map(|i| format!("agent-{i}")).collect();
        let mut orch = Orchestrator::new(
            test_plan(RolloutStrategy::Rolling { batch_percent: 30 }),
            agents,
        );

        let batch1 = orch.next_batch().unwrap();
        assert_eq!(batch1.len(), 3); // 30% of 10
        orch.record_batch(batch1.iter().map(|id| agent_result(id, true)).collect());

        let batch2 = orch.next_batch().unwrap();
        assert_eq!(batch2.len(), 3);
        orch.record_batch(batch2.iter().map(|id| agent_result(id, true)).collect());

        let batch3 = orch.next_batch().unwrap();
        assert_eq!(batch3.len(), 3);
        orch.record_batch(batch3.iter().map(|id| agent_result(id, true)).collect());

        let batch4 = orch.next_batch().unwrap();
        assert_eq!(batch4.len(), 1); // remaining
    }

    #[test]
    fn test_canary_then_rest() {
        let agents: Vec<String> = (0..5).map(|i| format!("agent-{i}")).collect();
        let mut orch = Orchestrator::new(
            test_plan(RolloutStrategy::Canary { canary_count: 2 }),
            agents,
        );

        let canary = orch.next_batch().unwrap();
        assert_eq!(canary.len(), 2);
        orch.record_batch(canary.iter().map(|id| agent_result(id, true)).collect());

        let rest = orch.next_batch().unwrap();
        assert_eq!(rest.len(), 3);
    }

    #[test]
    fn test_stop_on_failure() {
        let agents = vec!["a".into(), "b".into(), "c".into()];
        let mut orch = Orchestrator::new(test_plan(RolloutStrategy::Sequential), agents);

        let batch1 = orch.next_batch().unwrap();
        let should_continue = orch.record_batch(vec![agent_result(&batch1[0], false)]);
        assert!(!should_continue);
    }

    #[test]
    fn test_rollback_triggered() {
        let plan = OrchestrationPlan {
            command: "deploy v2".to_string(),
            target: TargetGrain::All,
            strategy: RolloutStrategy::Sequential,
            on_failure: RollbackPolicy::Rollback {
                command: "deploy v1".to_string(),
            },
            timeout_secs: 60,
        };
        let agents = vec!["a".into(), "b".into(), "c".into()];
        let mut orch = Orchestrator::new(plan, agents);

        orch.next_batch().unwrap();
        orch.record_batch(vec![agent_result("a", true)]);

        orch.next_batch().unwrap();
        orch.record_batch(vec![agent_result("b", false)]);

        assert!(orch.rollback_triggered);
        assert_eq!(orch.agents_needing_rollback(), vec!["a"]);
        assert_eq!(orch.rollback_command(), Some("deploy v1"));
    }

    #[test]
    fn test_resolve_targets_all() {
        let mut agents: HashMap<String, HashMap<String, String>> = HashMap::new();
        agents.insert("web-01".into(), HashMap::new());
        agents.insert("web-02".into(), HashMap::new());

        let resolved = resolve_targets(&TargetGrain::All, &agents);
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn test_resolve_targets_label() {
        let mut agents: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut labels = HashMap::new();
        labels.insert("env".to_string(), "prod".to_string());
        agents.insert("web-01".into(), labels);
        agents.insert("web-02".into(), HashMap::new());

        let resolved = resolve_targets(
            &TargetGrain::Label {
                key: "env".into(),
                value: "prod".into(),
            },
            &agents,
        );
        assert_eq!(resolved, vec!["web-01"]);
    }

    #[test]
    fn test_resolve_targets_pattern() {
        let mut agents: HashMap<String, HashMap<String, String>> = HashMap::new();
        agents.insert("web-01".into(), HashMap::new());
        agents.insert("web-02".into(), HashMap::new());
        agents.insert("db-01".into(), HashMap::new());

        let resolved = resolve_targets(&TargetGrain::Pattern("web-*".into()), &agents);
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().all(|id| id.starts_with("web-")));
    }

    #[test]
    fn test_finalize() {
        let agents = vec!["a".into(), "b".into()];
        let mut orch = Orchestrator::new(test_plan(RolloutStrategy::Parallel), agents);
        orch.next_batch().unwrap();
        orch.record_batch(vec![agent_result("a", true), agent_result("b", true)]);

        let result = orch.finalize(500);
        assert!(result.success);
        assert!(!result.rollback_triggered);
        assert_eq!(result.results.len(), 2);
        assert_eq!(result.total_duration_ms, 500);
    }
}
