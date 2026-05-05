//! Per-relay latency measurement and scoring.
//!
//! Maintains RTT history for each relay endpoint and selects the
//! lowest-latency relay for new connections. Includes an active
//! TCP prober for measuring real RTT.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

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
/// Active latency prober — measures real TCP connect RTT to relay endpoints.
pub struct LatencyProber {
    /// Probe timeout (default: 5s).
    pub timeout: Duration,
    /// Interval between probe rounds (default: 30s).
    pub interval: Duration,
}

impl Default for LatencyProber {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            interval: Duration::from_secs(30),
        }
    }
}

impl LatencyProber {
    /// Create a prober with custom timeout and interval.
    pub fn new(timeout: Duration, interval: Duration) -> Self {
        Self { timeout, interval }
    }

    /// Probe a single relay endpoint via TCP connect. Returns RTT on success.
    pub async fn probe_tcp(&self, addr: SocketAddr) -> Result<Duration, std::io::Error> {
        let start = Instant::now();
        match timeout(self.timeout, TcpStream::connect(addr)).await {
            Ok(Ok(_stream)) => Ok(start.elapsed()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "probe timed out",
            )),
        }
    }

    /// Probe multiple relays and update the tracker with results.
    pub async fn probe_all(
        &self,
        tracker: &mut LatencyTracker,
        endpoints: &[(String, SocketAddr)],
    ) {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for (relay_id, addr) in endpoints {
            match self.probe_tcp(*addr).await {
                Ok(rtt) => {
                    tracker.record(relay_id, rtt, timestamp_ms);
                }
                Err(_) => {
                    tracker.record_failure(relay_id);
                }
            }
        }
    }

    /// Run continuous probing loop until cancellation token fires.
    /// Probes all endpoints every `self.interval`.
    pub async fn run_loop(
        &self,
        tracker: &mut LatencyTracker,
        endpoints: &[(String, SocketAddr)],
        cancel: tokio::sync::watch::Receiver<bool>,
    ) {
        loop {
            self.probe_all(tracker, endpoints).await;

            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {}
                _ = async {
                    let mut rx = cancel.clone();
                    while !*rx.borrow_and_update() {
                        if rx.changed().await.is_err() {
                            return;
                        }
                    }
                } => {
                    break;
                }
            }
        }
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

    #[tokio::test]
    async fn test_prober_tcp_success() {
        // Start a TCP listener to probe
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept in background (just accept and drop)
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let prober = LatencyProber::default();
        let rtt = prober.probe_tcp(addr).await.unwrap();
        // Local connection should be fast
        assert!(rtt < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_prober_tcp_timeout() {
        // Use a non-routable address to trigger timeout
        let prober = LatencyProber::new(Duration::from_millis(100), Duration::from_secs(30));
        let addr: SocketAddr = "192.0.2.1:9999".parse().unwrap(); // TEST-NET, non-routable
        let result = prober.probe_tcp(addr).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prober_probe_all() {
        // Start two TCP listeners
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();
        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = listener1.accept().await;
        });
        tokio::spawn(async move {
            let _ = listener2.accept().await;
        });

        let prober = LatencyProber::default();
        let mut tracker = LatencyTracker::new(50);
        let endpoints = vec![
            ("relay-1".to_string(), addr1),
            ("relay-2".to_string(), addr2),
        ];

        prober.probe_all(&mut tracker, &endpoints).await;

        assert_eq!(tracker.relay_count(), 2);
        assert!(tracker.stats("relay-1").unwrap().mean_rtt().is_some());
        assert!(tracker.stats("relay-2").unwrap().mean_rtt().is_some());
    }
}
