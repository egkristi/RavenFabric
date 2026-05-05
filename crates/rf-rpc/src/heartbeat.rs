//! Heartbeat and RTT tracking for connection liveness detection.
//!
//! Tracks round-trip times from Ping/Pong exchanges and detects:
//! - Peer death (3 consecutive missed pongs)
//! - Latency anomalies (> 2x baseline RTT, potential MITM)

use std::time::{Duration, Instant};

/// Configuration for heartbeat behavior.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between pings.
    pub interval: Duration,
    /// Number of consecutive misses before declaring peer dead.
    pub miss_threshold: u32,
    /// RTT multiplier to detect anomalies (e.g., 2.0 = flag if > 2x baseline).
    pub anomaly_multiplier: f64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(15),
            miss_threshold: 3,
            anomaly_multiplier: 2.0,
        }
    }
}

/// Tracks RTT statistics and liveness.
#[derive(Debug)]
pub struct RttTracker {
    config: HeartbeatConfig,
    /// Exponentially weighted moving average of RTT in milliseconds.
    ewma_ms: f64,
    /// Number of samples collected.
    sample_count: u64,
    /// Consecutive missed pongs.
    consecutive_misses: u32,
    /// Last time a ping was sent.
    last_ping_sent: Option<Instant>,
    /// Last time a pong was received.
    last_pong_received: Option<Instant>,
}

/// Result of processing a heartbeat event.
#[derive(Debug, Clone, PartialEq)]
pub enum HeartbeatStatus {
    /// Everything is healthy.
    Healthy { rtt_ms: f64 },
    /// RTT is anomalously high — possible interception.
    LatencyAnomaly { rtt_ms: f64, baseline_ms: f64 },
    /// Peer has not responded — connection may be dead.
    PeerUnresponsive { consecutive_misses: u32 },
    /// Peer is confirmed dead (exceeded miss threshold).
    PeerDead,
}

impl RttTracker {
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            ewma_ms: 0.0,
            sample_count: 0,
            consecutive_misses: 0,
            last_ping_sent: None,
            last_pong_received: None,
        }
    }

    /// Record that a ping was sent at this instant.
    pub fn ping_sent(&mut self) {
        self.last_ping_sent = Some(Instant::now());
    }

    /// Record that a pong was received. Returns the health status.
    pub fn pong_received(&mut self) -> HeartbeatStatus {
        let now = Instant::now();
        self.last_pong_received = Some(now);
        self.consecutive_misses = 0;

        let rtt_ms = if let Some(sent) = self.last_ping_sent {
            sent.elapsed().as_secs_f64() * 1000.0
        } else {
            return HeartbeatStatus::Healthy { rtt_ms: 0.0 };
        };

        // Update EWMA (alpha = 0.2 for smoothing)
        if self.sample_count == 0 {
            self.ewma_ms = rtt_ms;
        } else {
            let alpha = 0.2;
            self.ewma_ms = alpha * rtt_ms + (1.0 - alpha) * self.ewma_ms;
        }
        self.sample_count += 1;

        // Check for latency anomaly (only after baseline is established)
        if self.sample_count > 5 && rtt_ms > self.ewma_ms * self.config.anomaly_multiplier {
            HeartbeatStatus::LatencyAnomaly {
                rtt_ms,
                baseline_ms: self.ewma_ms,
            }
        } else {
            HeartbeatStatus::Healthy { rtt_ms }
        }
    }

    /// Record that a pong was NOT received within the expected window.
    pub fn pong_missed(&mut self) -> HeartbeatStatus {
        self.consecutive_misses += 1;
        if self.consecutive_misses >= self.config.miss_threshold {
            HeartbeatStatus::PeerDead
        } else {
            HeartbeatStatus::PeerUnresponsive {
                consecutive_misses: self.consecutive_misses,
            }
        }
    }

    /// Current baseline RTT in milliseconds.
    pub fn baseline_rtt_ms(&self) -> f64 {
        self.ewma_ms
    }

    /// Number of RTT samples collected.
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Whether the peer is considered alive.
    pub fn is_alive(&self) -> bool {
        self.consecutive_misses < self.config.miss_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let tracker = RttTracker::new(HeartbeatConfig::default());
        assert_eq!(tracker.sample_count(), 0);
        assert!(tracker.is_alive());
        assert_eq!(tracker.baseline_rtt_ms(), 0.0);
    }

    #[test]
    fn test_pong_received_updates_ewma() {
        let mut tracker = RttTracker::new(HeartbeatConfig::default());
        tracker.ping_sent();
        std::thread::sleep(Duration::from_millis(1));
        let status = tracker.pong_received();
        assert!(matches!(status, HeartbeatStatus::Healthy { .. }));
        assert_eq!(tracker.sample_count(), 1);
        assert!(tracker.baseline_rtt_ms() > 0.0);
    }

    #[test]
    fn test_miss_threshold() {
        let config = HeartbeatConfig {
            miss_threshold: 3,
            ..Default::default()
        };
        let mut tracker = RttTracker::new(config);

        assert!(matches!(
            tracker.pong_missed(),
            HeartbeatStatus::PeerUnresponsive {
                consecutive_misses: 1
            }
        ));
        assert!(tracker.is_alive());

        assert!(matches!(
            tracker.pong_missed(),
            HeartbeatStatus::PeerUnresponsive {
                consecutive_misses: 2
            }
        ));
        assert!(tracker.is_alive());

        assert!(matches!(tracker.pong_missed(), HeartbeatStatus::PeerDead));
        assert!(!tracker.is_alive());
    }

    #[test]
    fn test_miss_resets_on_pong() {
        let mut tracker = RttTracker::new(HeartbeatConfig::default());
        tracker.pong_missed();
        tracker.pong_missed();
        assert!(tracker.is_alive());

        tracker.ping_sent();
        tracker.pong_received();
        assert!(tracker.is_alive());
        assert_eq!(tracker.consecutive_misses, 0);
    }

    #[test]
    fn test_latency_anomaly_detection() {
        let config = HeartbeatConfig {
            anomaly_multiplier: 2.0,
            ..Default::default()
        };
        let mut tracker = RttTracker::new(config);

        // Build baseline (6 samples needed for anomaly detection)
        for _ in 0..6 {
            tracker.ewma_ms = 10.0; // Simulate 10ms baseline
            tracker.sample_count += 1;
        }

        // Simulate a normal pong
        tracker.last_ping_sent = Some(Instant::now() - Duration::from_millis(10));
        let status = tracker.pong_received();
        assert!(matches!(status, HeartbeatStatus::Healthy { .. }));

        // Simulate a high-latency pong (> 2x baseline)
        tracker.last_ping_sent = Some(Instant::now() - Duration::from_millis(50));
        let status = tracker.pong_received();
        assert!(matches!(status, HeartbeatStatus::LatencyAnomaly { .. }));
    }
}
