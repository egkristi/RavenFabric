//! Per-path connection metrics collection and reporting.
//!
//! Tracks RTT, packet loss, throughput, and transport type for each connection path.
//! Metrics are accumulated locally and can be flushed to an observer when connectivity allows.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Metrics for a single connection path.
#[derive(Debug, Clone)]
pub struct PathMetrics {
    /// Transport type identifier (e.g., "websocket", "quic", "wireguard").
    pub transport: String,
    /// Remote endpoint description.
    pub endpoint: String,
    /// Number of hop count (1 = direct, >1 = relayed/mesh).
    pub hop_count: u32,
    /// RTT samples (most recent N).
    rtt_samples: VecDeque<Duration>,
    /// Bytes sent on this path.
    pub bytes_sent: u64,
    /// Bytes received on this path.
    pub bytes_received: u64,
    /// Number of send failures (proxy for packet loss).
    pub send_failures: u64,
    /// Number of successful sends.
    pub sends_total: u64,
    /// When this path was established.
    pub established_at: Instant,
    /// Last activity on this path.
    pub last_activity: Instant,
}

/// A snapshot of path metrics for reporting.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub transport: String,
    pub endpoint: String,
    pub hop_count: u32,
    pub avg_rtt_ms: f64,
    pub min_rtt_ms: f64,
    pub max_rtt_ms: f64,
    pub loss_rate: f64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub uptime_secs: u64,
}

impl PathMetrics {
    /// Create a new path metrics tracker.
    pub fn new(transport: String, endpoint: String, hop_count: u32) -> Self {
        let now = Instant::now();
        Self {
            transport,
            endpoint,
            hop_count,
            rtt_samples: VecDeque::with_capacity(100),
            bytes_sent: 0,
            bytes_received: 0,
            send_failures: 0,
            sends_total: 0,
            established_at: now,
            last_activity: now,
        }
    }

    /// Record an RTT measurement.
    pub fn record_rtt(&mut self, rtt: Duration) {
        if self.rtt_samples.len() >= 100 {
            self.rtt_samples.pop_front();
        }
        self.rtt_samples.push_back(rtt);
        self.last_activity = Instant::now();
    }

    /// Record bytes sent.
    pub fn record_send(&mut self, bytes: u64, success: bool) {
        self.sends_total += 1;
        if success {
            self.bytes_sent += bytes;
        } else {
            self.send_failures += 1;
        }
        self.last_activity = Instant::now();
    }

    /// Record bytes received.
    pub fn record_recv(&mut self, bytes: u64) {
        self.bytes_received += bytes;
        self.last_activity = Instant::now();
    }

    /// Calculate packet loss rate (0.0 to 1.0).
    pub fn loss_rate(&self) -> f64 {
        if self.sends_total == 0 {
            return 0.0;
        }
        self.send_failures as f64 / self.sends_total as f64
    }

    /// Average RTT from collected samples.
    pub fn avg_rtt(&self) -> Duration {
        if self.rtt_samples.is_empty() {
            return Duration::ZERO;
        }
        let sum: Duration = self.rtt_samples.iter().sum();
        sum / self.rtt_samples.len() as u32
    }

    /// Produce a snapshot of current metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let avg_rtt_ms = self.avg_rtt().as_secs_f64() * 1000.0;
        let min_rtt_ms = self
            .rtt_samples
            .iter()
            .min()
            .unwrap_or(&Duration::ZERO)
            .as_secs_f64()
            * 1000.0;
        let max_rtt_ms = self
            .rtt_samples
            .iter()
            .max()
            .unwrap_or(&Duration::ZERO)
            .as_secs_f64()
            * 1000.0;

        MetricsSnapshot {
            transport: self.transport.clone(),
            endpoint: self.endpoint.clone(),
            hop_count: self.hop_count,
            avg_rtt_ms,
            min_rtt_ms,
            max_rtt_ms,
            loss_rate: self.loss_rate(),
            bytes_sent: self.bytes_sent,
            bytes_received: self.bytes_received,
            uptime_secs: self.established_at.elapsed().as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_path_metrics() {
        let m = PathMetrics::new("websocket".into(), "relay.example.com".into(), 1);
        assert_eq!(m.transport, "websocket");
        assert_eq!(m.hop_count, 1);
        assert_eq!(m.bytes_sent, 0);
        assert_eq!(m.loss_rate(), 0.0);
    }

    #[test]
    fn test_record_rtt() {
        let mut m = PathMetrics::new("quic".into(), "direct".into(), 1);
        m.record_rtt(Duration::from_millis(10));
        m.record_rtt(Duration::from_millis(20));
        m.record_rtt(Duration::from_millis(30));
        assert_eq!(m.avg_rtt(), Duration::from_millis(20));
    }

    #[test]
    fn test_loss_rate() {
        let mut m = PathMetrics::new("ws".into(), "relay".into(), 2);
        m.record_send(100, true);
        m.record_send(100, true);
        m.record_send(100, false);
        m.record_send(100, true);
        assert!((m.loss_rate() - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_snapshot() {
        let mut m = PathMetrics::new("wireguard".into(), "peer".into(), 1);
        m.record_rtt(Duration::from_millis(5));
        m.record_rtt(Duration::from_millis(15));
        m.record_send(1024, true);
        m.record_recv(2048);

        let snap = m.snapshot();
        assert_eq!(snap.transport, "wireguard");
        assert_eq!(snap.bytes_sent, 1024);
        assert_eq!(snap.bytes_received, 2048);
        assert!((snap.avg_rtt_ms - 10.0).abs() < 0.1);
        assert!((snap.min_rtt_ms - 5.0).abs() < 0.1);
        assert!((snap.max_rtt_ms - 15.0).abs() < 0.1);
    }

    #[test]
    fn test_rtt_buffer_overflow() {
        let mut m = PathMetrics::new("test".into(), "test".into(), 1);
        for i in 0..150 {
            m.record_rtt(Duration::from_millis(i));
        }
        // Buffer should cap at 100 samples
        assert_eq!(m.rtt_samples.len(), 100);
        // Should contain samples 50-149
        assert_eq!(*m.rtt_samples.front().unwrap(), Duration::from_millis(50));
    }
}
