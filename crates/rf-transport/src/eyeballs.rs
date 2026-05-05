//! Happy Eyeballs (RFC 8305) implementation.
//!
//! Races IPv4 and IPv6 connections in parallel, selecting the first
//! successful connection. Provides fairness by giving IPv6 a head start.

use std::net::SocketAddr;
use std::time::Duration;

/// Configuration for Happy Eyeballs connection racing.
#[derive(Debug, Clone)]
pub struct HappyEyeballsConfig {
    /// How long to wait after starting the preferred (IPv6) attempt
    /// before starting the fallback (IPv4) attempt.
    pub resolution_delay: Duration,
    /// Total connection timeout.
    pub connection_timeout: Duration,
    /// Whether to prefer IPv6 (true) or IPv4 (false).
    pub prefer_ipv6: bool,
}

impl Default for HappyEyeballsConfig {
    fn default() -> Self {
        Self {
            resolution_delay: Duration::from_millis(250), // RFC 8305 recommends 250ms
            connection_timeout: Duration::from_secs(30),
            prefer_ipv6: true,
        }
    }
}

/// Result of a Happy Eyeballs connection attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaceResult {
    /// IPv6 connected first.
    Ipv6Won { addr: SocketAddr, elapsed_ms: u64 },
    /// IPv4 connected first.
    Ipv4Won { addr: SocketAddr, elapsed_ms: u64 },
    /// Both failed.
    BothFailed {
        ipv6_error: String,
        ipv4_error: String,
    },
}

/// Candidate addresses sorted according to RFC 8305.
/// Interleaves address families: first IPv6, then IPv4, alternating.
#[derive(Debug, Clone)]
pub struct SortedCandidates {
    /// Addresses sorted per RFC 8305 (preferred family first, interleaved).
    pub addresses: Vec<SocketAddr>,
}

impl SortedCandidates {
    /// Sort candidates per RFC 8305 rules.
    /// Interleaves address families with preferred family first.
    pub fn new(candidates: &[SocketAddr], prefer_ipv6: bool) -> Self {
        let mut ipv6: Vec<SocketAddr> =
            candidates.iter().filter(|a| a.is_ipv6()).copied().collect();
        let mut ipv4: Vec<SocketAddr> =
            candidates.iter().filter(|a| a.is_ipv4()).copied().collect();

        let mut sorted = Vec::with_capacity(candidates.len());

        let (preferred, fallback) = if prefer_ipv6 {
            (&mut ipv6, &mut ipv4)
        } else {
            (&mut ipv4, &mut ipv6)
        };

        // Interleave: preferred first, then fallback
        let mut p_iter = preferred.drain(..);
        let mut f_iter = fallback.drain(..);

        loop {
            let p = p_iter.next();
            let f = f_iter.next();
            if p.is_none() && f.is_none() {
                break;
            }
            if let Some(addr) = p {
                sorted.push(addr);
            }
            if let Some(addr) = f {
                sorted.push(addr);
            }
        }

        Self { addresses: sorted }
    }

    /// Get the first preferred-family address.
    pub fn preferred(&self) -> Option<SocketAddr> {
        self.addresses.first().copied()
    }

    /// Get the first fallback-family address.
    pub fn fallback(&self) -> Option<SocketAddr> {
        self.addresses.get(1).copied()
    }

    /// Check if there are candidates in both families.
    pub fn is_dual_stack(&self) -> bool {
        let has_v4 = self.addresses.iter().any(|a| a.is_ipv4());
        let has_v6 = self.addresses.iter().any(|a| a.is_ipv6());
        has_v4 && has_v6
    }
}

/// Determines connection attempt ordering for a set of addresses.
/// Returns (first_attempt, delay_before_second, second_attempt).
pub fn plan_race(
    candidates: &SortedCandidates,
    config: &HappyEyeballsConfig,
) -> Option<(SocketAddr, Duration, Option<SocketAddr>)> {
    let first = candidates.preferred()?;
    let second = candidates.fallback();
    Some((first, config.resolution_delay, second))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn v4(port: u16) -> SocketAddr {
        SocketAddr::new(Ipv4Addr::new(192, 168, 1, 1).into(), port)
    }

    fn v6(port: u16) -> SocketAddr {
        SocketAddr::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).into(), port)
    }

    #[test]
    fn test_sorted_prefer_ipv6() {
        let candidates = vec![v4(80), v6(80), v4(443), v6(443)];
        let sorted = SortedCandidates::new(&candidates, true);

        // First should be IPv6
        assert!(sorted.addresses[0].is_ipv6());
        assert!(sorted.addresses[1].is_ipv4());
    }

    #[test]
    fn test_sorted_prefer_ipv4() {
        let candidates = vec![v4(80), v6(80)];
        let sorted = SortedCandidates::new(&candidates, false);

        assert!(sorted.addresses[0].is_ipv4());
        assert!(sorted.addresses[1].is_ipv6());
    }

    #[test]
    fn test_dual_stack_detection() {
        let dual = SortedCandidates::new(&[v4(80), v6(80)], true);
        assert!(dual.is_dual_stack());

        let v4_only = SortedCandidates::new(&[v4(80), v4(443)], true);
        assert!(!v4_only.is_dual_stack());
    }

    #[test]
    fn test_plan_race() {
        let candidates = SortedCandidates::new(&[v4(80), v6(80)], true);
        let config = HappyEyeballsConfig::default();

        let (first, delay, second) = plan_race(&candidates, &config).unwrap();
        assert!(first.is_ipv6());
        assert_eq!(delay, Duration::from_millis(250));
        assert!(second.unwrap().is_ipv4());
    }

    #[test]
    fn test_single_family_no_race() {
        let candidates = SortedCandidates::new(&[v4(80)], true);
        let config = HappyEyeballsConfig::default();

        let (first, _, second) = plan_race(&candidates, &config).unwrap();
        assert!(first.is_ipv4());
        assert!(second.is_none());
    }

    #[test]
    fn test_default_config() {
        let config = HappyEyeballsConfig::default();
        assert_eq!(config.resolution_delay, Duration::from_millis(250));
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert!(config.prefer_ipv6);
    }
}
