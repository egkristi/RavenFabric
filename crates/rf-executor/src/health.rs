//! Health check probes — HTTP/TCP/UDP endpoint monitoring.
//!
//! Provides types and logic for checking the health of endpoints,
//! services, and the agent itself.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Type of health check to perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    /// TCP connection test (connect + optional banner read).
    Tcp { host: String, port: u16 },
    /// HTTP(S) GET with expected status code.
    Http { url: String, expected_status: u16 },
    /// Process alive check (by name or PID).
    Process { name: String },
    /// Command execution (exit code 0 = healthy).
    Command { cmd: String },
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheck {
    /// Unique name for this check.
    pub name: String,
    /// Type of probe.
    pub probe: ProbeType,
    /// How often to run this check.
    pub interval: Duration,
    /// Timeout for each probe attempt.
    pub timeout: Duration,
    /// How many consecutive failures before marking unhealthy.
    pub failure_threshold: u32,
    /// How many consecutive successes to recover from unhealthy.
    pub success_threshold: u32,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            name: String::new(),
            probe: ProbeType::Tcp {
                host: "localhost".into(),
                port: 80,
            },
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 1,
        }
    }
}

/// Current health status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Check has not run yet.
    Unknown,
    /// Endpoint is healthy.
    Healthy,
    /// Endpoint is degraded (some failures but below threshold).
    Degraded,
    /// Endpoint is unhealthy (failure threshold exceeded).
    Unhealthy,
}

/// Result of a single probe execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeResult {
    /// Whether the probe succeeded.
    pub success: bool,
    /// Response time in milliseconds.
    pub latency_ms: u64,
    /// Error message if failed.
    pub error: Option<String>,
    /// Timestamp (ms since epoch).
    pub timestamp_ms: u64,
}

/// State tracker for a health check (tracks consecutive successes/failures).
#[derive(Debug, Clone)]
pub struct HealthTracker {
    check: HealthCheck,
    status: HealthStatus,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_result: Option<ProbeResult>,
    total_checks: u64,
    total_failures: u64,
}

impl HealthTracker {
    /// Create a new health tracker.
    pub fn new(check: HealthCheck) -> Self {
        Self {
            check,
            status: HealthStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_result: None,
            total_checks: 0,
            total_failures: 0,
        }
    }

    /// Record a probe result and update health status.
    pub fn record(&mut self, result: ProbeResult) {
        self.total_checks += 1;

        if result.success {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;

            if self.consecutive_successes >= self.check.success_threshold {
                self.status = HealthStatus::Healthy;
            }
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
            self.total_failures += 1;

            if self.consecutive_failures >= self.check.failure_threshold {
                self.status = HealthStatus::Unhealthy;
            } else if self.consecutive_failures > 0 {
                self.status = HealthStatus::Degraded;
            }
        }

        self.last_result = Some(result);
    }

    /// Current health status.
    pub fn status(&self) -> HealthStatus {
        self.status
    }

    /// The health check configuration.
    pub fn check(&self) -> &HealthCheck {
        &self.check
    }

    /// Last probe result.
    pub fn last_result(&self) -> Option<&ProbeResult> {
        self.last_result.as_ref()
    }

    /// Availability percentage (0.0 - 1.0).
    pub fn availability(&self) -> f64 {
        if self.total_checks == 0 {
            return 1.0;
        }
        1.0 - (self.total_failures as f64 / self.total_checks as f64)
    }

    /// Whether the check is currently healthy.
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy || self.status == HealthStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_check() -> HealthCheck {
        HealthCheck {
            name: "web-server".into(),
            probe: ProbeType::Http {
                url: "http://localhost:8080/health".into(),
                expected_status: 200,
            },
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(3),
            failure_threshold: 3,
            success_threshold: 2,
            ..Default::default()
        }
    }

    fn success_result() -> ProbeResult {
        ProbeResult {
            success: true,
            latency_ms: 15,
            error: None,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    fn failure_result() -> ProbeResult {
        ProbeResult {
            success: false,
            latency_ms: 3000,
            error: Some("connection refused".into()),
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn test_initial_state() {
        let tracker = HealthTracker::new(test_check());
        assert_eq!(tracker.status(), HealthStatus::Unknown);
        assert!(tracker.is_healthy()); // Unknown counts as healthy
    }

    #[test]
    fn test_becomes_healthy_after_threshold() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Unknown); // Need 2 successes

        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_becomes_unhealthy_after_threshold() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_recovery_resets_failures() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(failure_result());
        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        // One success resets failure counter
        tracker.record(success_result());
        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_availability() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(success_result());
        tracker.record(success_result());
        tracker.record(success_result());
        tracker.record(failure_result());

        assert!((tracker.availability() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_probe_types_serialize() {
        let tcp = ProbeType::Tcp {
            host: "db.local".into(),
            port: 5432,
        };
        let json = serde_json::to_string(&tcp).unwrap();
        let parsed: ProbeType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tcp);
    }
}
