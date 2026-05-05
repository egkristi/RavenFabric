//! Happy Eyeballs (RFC 8305) implementation.
//!
//! Races IPv4 and IPv6 connections in parallel, selecting the first
//! successful connection. Provides fairness by giving IPv6 a head start.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::time::{Instant, sleep, timeout};

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

/// Race TCP connections per RFC 8305 Happy Eyeballs algorithm.
///
/// Starts a connection to the preferred address (typically IPv6).
/// After `resolution_delay`, starts the fallback (typically IPv4).
/// Returns the first successful `TcpStream` and its address,
/// or an error if both fail.
pub async fn race_connect(
    candidates: &SortedCandidates,
    config: &HappyEyeballsConfig,
) -> Result<(TcpStream, RaceResult), String> {
    let (first_addr, delay, second_addr) =
        plan_race(candidates, config).ok_or_else(|| "no candidate addresses".to_string())?;

    let start = Instant::now();
    let total_timeout = config.connection_timeout;

    match second_addr {
        None => {
            // Single address — no racing needed.
            match timeout(total_timeout, TcpStream::connect(first_addr)).await {
                Ok(Ok(stream)) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let result = if first_addr.is_ipv6() {
                        RaceResult::Ipv6Won {
                            addr: first_addr,
                            elapsed_ms,
                        }
                    } else {
                        RaceResult::Ipv4Won {
                            addr: first_addr,
                            elapsed_ms,
                        }
                    };
                    Ok((stream, result))
                }
                Ok(Err(e)) => Err(format!("connect to {}: {}", first_addr, e)),
                Err(_) => Err(format!("connect to {}: timeout", first_addr)),
            }
        }
        Some(second_addr) => {
            // Race both addresses with the preferred family getting a head start.
            // Use mpsc channel to receive winner — avoids select! borrow issues.
            let race_result = timeout(total_timeout, async {
                let (tx, mut rx) = tokio::sync::mpsc::channel::<(TcpStream, SocketAddr)>(2);

                // Start preferred connection immediately
                let tx1 = tx.clone();
                let h1 = tokio::spawn(async move {
                    if let Ok(stream) = TcpStream::connect(first_addr).await {
                        let _ = tx1.send((stream, first_addr)).await;
                    }
                });

                // Start fallback after resolution_delay
                let tx2 = tx;
                let h2 = tokio::spawn(async move {
                    sleep(delay).await;
                    if let Ok(stream) = TcpStream::connect(second_addr).await {
                        let _ = tx2.send((stream, second_addr)).await;
                    }
                });

                // Wait for first successful connection
                let winner = rx.recv().await;
                h1.abort();
                h2.abort();
                winner
            })
            .await;

            match race_result {
                Ok(Some((stream, addr))) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    let result = if addr.is_ipv6() {
                        RaceResult::Ipv6Won { addr, elapsed_ms }
                    } else {
                        RaceResult::Ipv4Won { addr, elapsed_ms }
                    };
                    Ok((stream, result))
                }
                Ok(None) => Err(format!(
                    "all connections failed: {} and {}",
                    first_addr, second_addr
                )),
                Err(_) => Err("connection timeout".to_string()),
            }
        }
    }
}

/// Race connections to multiple candidate addresses, attempting them in
/// RFC 8305 order with staggered starts.
///
/// This is the full Happy Eyeballs implementation for when there are
/// more than two candidate addresses.
pub async fn race_connect_multi(
    candidates: &SortedCandidates,
    config: &HappyEyeballsConfig,
) -> Result<(TcpStream, SocketAddr), String> {
    if candidates.addresses.is_empty() {
        return Err("no candidate addresses".to_string());
    }

    if candidates.addresses.len() <= 2 {
        // Delegate to the simpler two-address racer
        let (stream, result) = race_connect(candidates, config).await?;
        let addr = match result {
            RaceResult::Ipv6Won { addr, .. } => addr,
            RaceResult::Ipv4Won { addr, .. } => addr,
            RaceResult::BothFailed { .. } => unreachable!(),
        };
        return Ok((stream, addr));
    }

    // Stagger connection attempts per RFC 8305: start one every resolution_delay
    let total_timeout = config.connection_timeout;
    let delay = config.resolution_delay;

    let result = timeout(total_timeout, async {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(TcpStream, SocketAddr)>(1);
        let mut handles = Vec::new();

        for (i, &addr) in candidates.addresses.iter().enumerate() {
            let tx = tx.clone();
            let stagger = delay * i as u32;

            let handle = tokio::spawn(async move {
                sleep(stagger).await;
                if let Ok(stream) = TcpStream::connect(addr).await {
                    let _ = tx.send((stream, addr)).await;
                }
            });
            handles.push(handle);
        }
        drop(tx);

        let winner = rx.recv().await;
        // Abort remaining attempts
        for h in &handles {
            h.abort();
        }
        winner
    })
    .await;

    match result {
        Ok(Some((stream, addr))) => Ok((stream, addr)),
        Ok(None) => Err("all connection attempts failed".to_string()),
        Err(_) => Err("connection timeout".to_string()),
    }
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

    #[tokio::test]
    async fn test_race_connect_single_addr_succeeds() {
        // Bind a listener on localhost, then race_connect to it
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let candidates = SortedCandidates::new(&[addr], true);
        let config = HappyEyeballsConfig {
            connection_timeout: Duration::from_secs(5),
            ..Default::default()
        };

        let (stream, result) = race_connect(&candidates, &config).await.unwrap();
        assert!(stream.peer_addr().is_ok());
        match result {
            RaceResult::Ipv4Won { addr: won_addr, .. } => assert_eq!(won_addr, addr),
            _ => panic!("expected Ipv4Won"),
        }
    }

    #[tokio::test]
    async fn test_race_connect_unreachable_fails() {
        // Connect to a port that nobody is listening on
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let candidates = SortedCandidates::new(&[addr], true);
        let config = HappyEyeballsConfig {
            connection_timeout: Duration::from_secs(2),
            ..Default::default()
        };

        let result = race_connect(&candidates, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_race_connect_multi_first_wins() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // One reachable, one unreachable
        let unreachable: SocketAddr = "127.0.0.1:1".parse().unwrap();

        let candidates = SortedCandidates {
            addresses: vec![addr, unreachable],
        };
        let config = HappyEyeballsConfig {
            resolution_delay: Duration::from_millis(50),
            connection_timeout: Duration::from_secs(5),
            prefer_ipv6: true,
        };

        let (stream, won_addr) = race_connect_multi(&candidates, &config).await.unwrap();
        assert!(stream.peer_addr().is_ok());
        assert_eq!(won_addr, addr);
    }
}
