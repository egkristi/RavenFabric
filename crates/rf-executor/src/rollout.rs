//! Staged rollout coordinator for multi-agent fleet updates.
//!
//! The `RolloutCoordinator` manages a single active rollout campaign:
//! canary → percentage → fleet, with health-check gates between stages.
//! Pause and abort are supported at any point.

use std::{collections::HashMap, sync::Arc, time::Instant};

use rf_rpc::types::{RolloutStage, RolloutStrategy};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Configuration for a new rollout campaign.
#[derive(Debug, Clone)]
pub struct RolloutConfig {
    /// Target version string.
    pub version: String,
    /// HTTPS download URL for the new binary.
    pub url: String,
    /// Expected SHA-256 hex digest.
    pub sha256: String,
    /// Rollout strategy (canary / percentage / fleet).
    pub strategy: RolloutStrategy,
    /// For `Percentage` strategy: batch size. Defaults to 10.
    pub batch_percent: u8,
    /// Seconds to wait for each agent's health-check before advancing.
    pub health_check_timeout_secs: u64,
}

/// Status snapshot returned by `RolloutCoordinator::status()`.
#[derive(Debug, Clone)]
pub struct RolloutStatus {
    pub rollout_id: String,
    pub stage: RolloutStage,
    pub total_agents: usize,
    pub updated_agents: usize,
    pub healthy_agents: usize,
    pub failed_agents: usize,
    pub elapsed_secs: u64,
}

struct RolloutState {
    rollout_id: String,
    config: RolloutConfig,
    stage: RolloutStage,
    /// All agent IDs targeted by this rollout.
    all_agents: Vec<String>,
    /// `true` = update was delivered and `UpdateApplied` was received.
    sent: HashMap<String, bool>,
    /// `true` = agent passed post-update health check.
    health: HashMap<String, bool>,
    started_at: Instant,
}

/// Manages a staged rollout campaign across a fleet of agents.
///
/// Tracks which agents have been updated and their health-check outcomes.
/// The coordinator does not directly send RPCs — the caller (CLI or controller)
/// uses `agents_to_update()` and `can_advance()` to drive the campaign.
#[derive(Clone)]
pub struct RolloutCoordinator {
    state: Arc<RwLock<Option<RolloutState>>>,
}

impl Default for RolloutCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl RolloutCoordinator {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
        }
    }

    /// Start a new rollout campaign.
    ///
    /// Returns the generated rollout ID, or an error if a campaign is already active.
    pub async fn start(
        &self,
        config: RolloutConfig,
        agents: Vec<String>,
    ) -> Result<String, String> {
        let mut guard = self.state.write().await;
        if let Some(ref existing) = *guard {
            if !matches!(
                existing.stage,
                RolloutStage::Complete | RolloutStage::Aborted | RolloutStage::Failed
            ) {
                return Err(format!(
                    "rollout {} is already active (stage: {:?})",
                    existing.rollout_id, existing.stage
                ));
            }
        }
        let rollout_id = Uuid::new_v4().to_string();
        let initial_stage = match &config.strategy {
            RolloutStrategy::Fleet => RolloutStage::Fleet,
            RolloutStrategy::Canary => RolloutStage::Canary,
            RolloutStrategy::Percentage { .. } => RolloutStage::Percentage,
        };
        *guard = Some(RolloutState {
            rollout_id: rollout_id.clone(),
            config,
            stage: initial_stage,
            all_agents: agents,
            sent: HashMap::new(),
            health: HashMap::new(),
            started_at: Instant::now(),
        });
        Ok(rollout_id)
    }

    /// Pause the active rollout.
    pub async fn pause(&self) -> Result<(), String> {
        let mut guard = self.state.write().await;
        match guard.as_mut() {
            None => Err("no active rollout".into()),
            Some(state) => {
                if matches!(
                    state.stage,
                    RolloutStage::Paused
                        | RolloutStage::Complete
                        | RolloutStage::Aborted
                        | RolloutStage::Failed
                ) {
                    Err(format!("cannot pause rollout in stage {:?}", state.stage))
                } else {
                    state.stage = RolloutStage::Paused;
                    Ok(())
                }
            }
        }
    }

    /// Abort the active rollout.
    pub async fn abort(&self) -> Result<(), String> {
        let mut guard = self.state.write().await;
        match guard.as_mut() {
            None => Err("no active rollout".into()),
            Some(state) => {
                if matches!(state.stage, RolloutStage::Complete | RolloutStage::Aborted) {
                    Err(format!(
                        "rollout already in terminal state {:?}",
                        state.stage
                    ))
                } else {
                    state.stage = RolloutStage::Aborted;
                    Ok(())
                }
            }
        }
    }

    /// Resume a paused rollout.
    pub async fn resume(&self) -> Result<(), String> {
        let mut guard = self.state.write().await;
        match guard.as_mut() {
            None => Err("no active rollout".into()),
            Some(state) => {
                if state.stage != RolloutStage::Paused {
                    return Err(format!("rollout is not paused (stage: {:?})", state.stage));
                }
                let all_sent = state
                    .all_agents
                    .iter()
                    .all(|a| *state.sent.get(a.as_str()).unwrap_or(&false));
                state.stage = if all_sent {
                    RolloutStage::Fleet
                } else {
                    match &state.config.strategy {
                        RolloutStrategy::Canary => RolloutStage::Canary,
                        RolloutStrategy::Percentage { .. } => RolloutStage::Percentage,
                        RolloutStrategy::Fleet => RolloutStage::Fleet,
                    }
                };
                Ok(())
            }
        }
    }

    /// Record that an update was sent to `agent_id` with outcome `success`.
    ///
    /// `success = true` means `UpdateApplied` was received from that agent.
    pub async fn record_update(&self, agent_id: &str, success: bool) {
        let mut guard = self.state.write().await;
        if let Some(state) = guard.as_mut() {
            state.sent.insert(agent_id.to_string(), success);
        }
    }

    /// Record the result of a health-check for `agent_id`.
    ///
    /// If any agent fails its health-check, the rollout is automatically
    /// moved to `Failed` stage.
    pub async fn record_health_check(&self, agent_id: &str, passed: bool) {
        let mut guard = self.state.write().await;
        if let Some(state) = guard.as_mut() {
            state.health.insert(agent_id.to_string(), passed);
            if !passed {
                state.stage = RolloutStage::Failed;
            }
        }
    }

    /// Returns the next batch of agent IDs that should receive the update command.
    ///
    /// Returns an empty vec if the rollout is paused/aborted/complete/failed,
    /// or if we are waiting for health-checks before the next batch.
    pub async fn agents_to_update(&self) -> Vec<String> {
        let guard = self.state.read().await;
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return vec![],
        };
        if matches!(
            state.stage,
            RolloutStage::Paused
                | RolloutStage::Aborted
                | RolloutStage::Failed
                | RolloutStage::Complete
                | RolloutStage::Idle
        ) {
            return vec![];
        }
        let not_sent: Vec<String> = state
            .all_agents
            .iter()
            .filter(|a| !state.sent.contains_key(a.as_str()))
            .cloned()
            .collect();
        match &state.config.strategy {
            RolloutStrategy::Canary => not_sent.into_iter().take(1).collect(),
            RolloutStrategy::Percentage { percent } => {
                let n = ((state.all_agents.len() as f64 * *percent as f64 / 100.0).ceil() as usize)
                    .max(1);
                not_sent.into_iter().take(n).collect()
            }
            RolloutStrategy::Fleet => not_sent,
        }
    }

    /// Returns `true` when all agents updated so far have passed health-checks.
    ///
    /// The caller may then call `advance()` to move to the next stage.
    pub async fn can_advance(&self) -> bool {
        let guard = self.state.read().await;
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return false,
        };
        let sent_ok: Vec<&str> = state
            .sent
            .iter()
            .filter_map(|(a, &ok)| if ok { Some(a.as_str()) } else { None })
            .collect();
        if sent_ok.is_empty() {
            return false;
        }
        sent_ok
            .iter()
            .all(|a| *state.health.get(*a).unwrap_or(&false))
    }

    /// Advance to the next rollout stage.
    ///
    /// Requires all agents updated in the current stage to have passed
    /// health-checks (`can_advance()` must return `true`).
    ///
    /// Returns the new `RolloutStage` on success.
    pub async fn advance(&self) -> Result<RolloutStage, String> {
        if !self.can_advance().await {
            return Err("not all updated agents have passed health checks".into());
        }
        let mut guard = self.state.write().await;
        let state = match guard.as_mut() {
            Some(s) => s,
            None => return Err("no active rollout".into()),
        };
        let all_done = state
            .all_agents
            .iter()
            .all(|a| *state.sent.get(a.as_str()).unwrap_or(&false));
        if all_done {
            state.stage = RolloutStage::Complete;
        } else {
            state.stage = match state.stage {
                RolloutStage::Canary => match &state.config.strategy {
                    RolloutStrategy::Percentage { .. } => RolloutStage::Percentage,
                    _ => RolloutStage::Fleet,
                },
                RolloutStage::Percentage => RolloutStage::Fleet,
                RolloutStage::Fleet => RolloutStage::Complete,
                _ => return Err(format!("cannot advance from stage {:?}", state.stage)),
            };
        }
        Ok(state.stage.clone())
    }

    /// Get a status snapshot of the active rollout.
    pub async fn status(&self) -> Option<RolloutStatus> {
        let guard = self.state.read().await;
        let state = guard.as_ref()?;
        Some(RolloutStatus {
            rollout_id: state.rollout_id.clone(),
            stage: state.stage.clone(),
            total_agents: state.all_agents.len(),
            updated_agents: state.sent.values().filter(|&&ok| ok).count(),
            healthy_agents: state.health.values().filter(|&&ok| ok).count(),
            failed_agents: state.health.values().filter(|&&ok| !ok).count(),
            elapsed_secs: state.started_at.elapsed().as_secs(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_rpc::types::RolloutStrategy;

    fn config(strategy: RolloutStrategy) -> RolloutConfig {
        RolloutConfig {
            version: "1.0.0".into(),
            url: "https://example.com/agent".into(),
            sha256: "abc123".into(),
            strategy,
            batch_percent: 25,
            health_check_timeout_secs: 60,
        }
    }

    #[tokio::test]
    async fn canary_start_returns_one_agent() {
        let coord = RolloutCoordinator::new();
        let agents = vec!["a1".into(), "a2".into(), "a3".into()];
        coord
            .start(config(RolloutStrategy::Canary), agents)
            .await
            .unwrap();
        let batch = coord.agents_to_update().await;
        assert_eq!(batch.len(), 1);
    }

    #[tokio::test]
    async fn fleet_start_returns_all_agents() {
        let coord = RolloutCoordinator::new();
        let agents = vec!["a1".into(), "a2".into(), "a3".into()];
        coord
            .start(config(RolloutStrategy::Fleet), agents)
            .await
            .unwrap();
        let batch = coord.agents_to_update().await;
        assert_eq!(batch.len(), 3);
    }

    #[tokio::test]
    async fn percentage_batch_size() {
        let coord = RolloutCoordinator::new();
        let agents: Vec<String> = (0..10).map(|i| format!("a{i}")).collect();
        coord
            .start(config(RolloutStrategy::Percentage { percent: 30 }), agents)
            .await
            .unwrap();
        let batch = coord.agents_to_update().await;
        // ceil(10 * 0.30) = 3
        assert_eq!(batch.len(), 3);
    }

    #[tokio::test]
    async fn pause_and_abort() {
        let coord = RolloutCoordinator::new();
        coord
            .start(
                config(RolloutStrategy::Fleet),
                vec!["a1".into(), "a2".into()],
            )
            .await
            .unwrap();
        coord.pause().await.unwrap();
        assert!(coord.agents_to_update().await.is_empty());
        coord.abort().await.unwrap();
        // Aborted → can_advance returns false
        assert!(!coord.can_advance().await);
    }

    #[tokio::test]
    async fn advance_after_healthy_check() {
        let coord = RolloutCoordinator::new();
        let agents = vec!["a1".into(), "a2".into(), "a3".into()];
        coord
            .start(config(RolloutStrategy::Canary), agents)
            .await
            .unwrap();
        let batch = coord.agents_to_update().await;
        let agent = batch[0].clone();
        coord.record_update(&agent, true).await;
        coord.record_health_check(&agent, true).await;
        assert!(coord.can_advance().await);
        let new_stage = coord.advance().await.unwrap();
        assert_eq!(new_stage, RolloutStage::Fleet);
    }

    #[tokio::test]
    async fn health_check_failure_stops_rollout() {
        let coord = RolloutCoordinator::new();
        coord
            .start(
                config(RolloutStrategy::Canary),
                vec!["a1".into(), "a2".into()],
            )
            .await
            .unwrap();
        let batch = coord.agents_to_update().await;
        coord.record_update(&batch[0], true).await;
        coord.record_health_check(&batch[0], false).await;
        let status = coord.status().await.unwrap();
        assert_eq!(status.stage, RolloutStage::Failed);
        assert!(coord.agents_to_update().await.is_empty());
    }

    #[tokio::test]
    async fn second_start_blocked_while_active() {
        let coord = RolloutCoordinator::new();
        coord
            .start(config(RolloutStrategy::Fleet), vec!["a1".into()])
            .await
            .unwrap();
        let result = coord
            .start(config(RolloutStrategy::Fleet), vec!["a2".into()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn status_tracks_progress() {
        let coord = RolloutCoordinator::new();
        coord
            .start(
                config(RolloutStrategy::Fleet),
                vec!["a1".into(), "a2".into()],
            )
            .await
            .unwrap();
        coord.record_update("a1", true).await;
        coord.record_health_check("a1", true).await;
        let s = coord.status().await.unwrap();
        assert_eq!(s.total_agents, 2);
        assert_eq!(s.updated_agents, 1);
        assert_eq!(s.healthy_agents, 1);
        assert_eq!(s.failed_agents, 0);
    }
}
