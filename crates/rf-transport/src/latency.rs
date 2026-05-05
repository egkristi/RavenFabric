//! Per-relay latency measurement and scoring.
//!
//! Maintains RTT history for each relay endpoint and selects the
//! lowest-latency relay for new connections.

use std::collections::HashMap;
use std::time::Duration;

/// A single latency sample.
#[derive(Debug, Clone, Copy)]
pub struct LatencySample {
    /// Round-trip time.
    pub rtt: Duration,
    /// Timestamp (monotonic ms since process start or epoch).
    pub timestamp_ms: u64,
}

/// Latency statistics for one relay.
#[derive(Debug, Clone)]
pub struct RelayStats {
    /// Relay endpoint identifier (URL or name).
    pub relay_id: String,
    /// Recent RTT samples (ring buffer, newest last).
    samples: Vec<LatencySample>,
    /// Maximum samples to retain.
    max_samples: usize,
    /// Number of consecutive failures.
    pub consecutive_failures: u32,
}

impl RelayStats {
    pub fn new(relay_id: String, max_samples: usize) -> Self {
        Self {
            relay_id,
            samples: Vec::with_capacity(max_samples),
            max_samples,
            consecutive_failures: 0,
        }
    }

    /// Record a successful RTT measurement.
    pub fn record_success(&mut self, rtt: Duration, timestamp_ms: u64) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(LatencySample { rtt, timestamp_ms });
        self.consecutive_failures = 0;
    }

    /// Record a failed probe (timeout/error).
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
    }

    /// Mean RTT across stored samples.
    pub fn mean_rtt(&self) -> Option<Duration> {
        if self.samples.is_empty() {
            return None;
        }
        let total: Duration = self.samples.iter().map(|s| s.rtt).sum();
        Some(total / self.samples.len() as u32)
    }

    /// P50 (median) RTT.
    pub fn p50_rtt(&self) -> Option<Duration> {
        percentile_rtt(&self.samples, 50)
    }

    /// P95 RTT.
    pub fn p95_rtt(&self) -> Option<Duration> {
        percentile_rtt(&self.samples, 95)
    }

    /// Number of samples stored.
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Whether this relay is considered reachable.
    pub fn is_reachable(&self) -> bool {
        self.consecutive_failures < 3
    }

    /// Score for ranking (lower is better). Unreachable relays get u64::MAX.
    pub fn score(&self) -> u64 {
        if !self.is_reachable() {
            return u64::MAX;
        }
        self.mean_rtt()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(u64::MAX)
    }
}

fn percentile_rtt(samples: &[LatencySample], pct: u8) -> Option<Duration> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<Duration> = samples.iter().map(|s| s.rtt).collect();
    sorted.sort();
    let idx = ((pct as usize) * sorted.len() / 100).min(sorted.len() - 1);
    Some(sorted[idx])
}

/// Manages latency measurements for multiple relays.
#[derive(Debug)]
pub struct LatencyTracker {
    relays: HashMap<String, RelayStats>,
    max_samples_per_relay: usize,
}

impl LatencyTracker {
    pub fn new(max_samples_per_relay: usize) -> Self {
        Self {
            relays: HashMap::new(),
            max_samples_per_relay,
        }
    }

    /// Record a successful RTT measurement for a relay.
    pub fn record(&mut self, relay_id: &str, rtt: Duration, timestamp_ms: u64) {
        let stats = self
            .relays
            .entry(relay_id.to_string())
            .or_insert_with(|| RelayStats::new(relay_id.to_string(), self.max_samples_per_relay));
        stats.record_success(rtt, timestamp_ms);
    }

    /// Record a probe failure for a relay.
    pub fn record_failure(&mut self, relay_id: &str) {
        let stats = self
            .relays
            .entry(relay_id.to_string())
            .or_insert_with(|| RelayStats::new(relay_id.to_string(), self.max_samples_per_relay));
        stats.record_failure();
    }

    /// Get stats for a specific relay.
    pub fn stats(&self, relay_id: &str) -> Option<&RelayStats> {
        self.relays.get(relay_id)
    }

    /// Select the best relay (lowest score). Returns None if no relays tracked.
    pub fn best_relay(&self) -> Option<&str> {
        self.relays
            .values()
            .filter(|s| s.is_reachable())
            .min_by_key(|s| s.score())
            .map(|s| s.relay_id.as_str())
    }

    /// Get all relays ranked by score (best first).
    pub fn ranked(&self) -> Vec<&str> {
        let mut entries: Vec<&RelayStats> = self.relays.values().collect();
        entries.sort_by_key(|s| s.score());
        entries.iter().map(|s| s.relay_id.as_str()).collect()
    }

    /// Number of tracked relays.
    pub fn relay_count(&self) -> usize {
        self.relays.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_stats_basic() {
        let mut stats = RelayStats::new("relay-a".into(), 100);
        stats.record_success(Duration::from_millis(50), 1000);
        stats.record_success(Duration::from_millis(100), 2000);

        assert_eq!(stats.mean_rtt(), Some(Duration::from_millis(75)));
        assert_eq!(stats.sample_count(), 2);
        assert!(stats.is_reachable());
    }

    #[test]
    fn test_relay_stats_failure() {
        let mut stats = RelayStats::new("relay-b".into(), 100);
        stats.record_failure();
        stats.record_failure();
        assert!(stats.is_reachable());

        stats.record_failure();
        assert!(!stats.is_reachable());
        assert_eq!(stats.score(), u64::MAX);
    }

    #[test]
    fn test_relay_stats_ring_buffer() {
        let mut stats = RelayStats::new("relay-c".into(), 3);
        stats.record_success(Duration::from_millis(10), 1);
        stats.record_success(Duration::from_millis(20), 2);
        stats.record_success(Duration::from_millis(30), 3);
        stats.record_success(Duration::from_millis(100), 4); // evicts 10ms

        assert_eq!(stats.sample_count(), 3);
        assert_eq!(stats.mean_rtt(), Some(Duration::from_millis(50)));
    }

    #[test]
    fn test_tracker_best_relay() {
        let mut tracker = LatencyTracker::new(50);
        tracker.record("fast-relay", Duration::from_millis(20), 1);
        tracker.record("slow-relay", Duration::from_millis(200), 1);
        tracker.record("medium-relay", Duration::from_millis(80), 1);

        assert_eq!(tracker.best_relay(), Some("fast-relay"));
    }

    #[test]
    fn test_tracker_ranked() {
        let mut tracker = LatencyTracker::new(50);
        tracker.record("c", Duration::from_millis(100), 1);
        tracker.record("a", Duration::from_millis(10), 1);
        tracker.record("b", Duration::from_millis(50), 1);

        let ranked = tracker.ranked();
        assert_eq!(ranked, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_tracker_unreachable_excluded() {
        let mut tracker = LatencyTracker::new(50);
        tracker.record("good", Duration::from_millis(50), 1);
        tracker.record("bad", Duration::from_millis(10), 1);

        // Make "bad" unreachable
        tracker.record_failure("bad");
        tracker.record_failure("bad");
        tracker.record_failure("bad");

        assert_eq!(tracker.best_relay(), Some("good"));
    }

    #[test]
    fn test_percentiles() {
        let mut stats = RelayStats::new("relay".into(), 100);
        for i in 1..=100 {
            stats.record_success(Duration::from_millis(i), i);
        }

        let p50 = stats.p50_rtt().unwrap();
        assert!(p50.as_millis() >= 49 && p50.as_millis() <= 51);

        let p95 = stats.p95_rtt().unwrap();
        assert!(p95.as_millis() >= 94 && p95.as_millis() <= 96);
    }
}
