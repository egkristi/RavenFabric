//! Happy Eyeballs (RFC 8305) implementation with NAT64 awareness.
//!
//! Races IPv4 and IPv6 connections in parallel, selecting the first
//! successful connection. Provides fairness by giving IPv6 a head start.
//! Detects NAT64/464XLAT environments for IPv6-only networks.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
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

// --- NAT64/464XLAT Detection (RFC 7050) ---

/// Well-known IPv4 addresses for `ipv4only.arpa` (RFC 7050).
const WKA_1: Ipv4Addr = Ipv4Addr::new(192, 0, 0, 170);
const WKA_2: Ipv4Addr = Ipv4Addr::new(192, 0, 0, 171);

/// Result of NAT64 prefix detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nat64Status {
    /// NAT64 detected with the given prefix (96 bits from synthesized AAAA).
    Detected { prefix: Ipv6Addr, prefix_len: u8 },
    /// No NAT64 — network has native IPv4 or dual-stack.
    NotDetected,
    /// Detection failed (DNS resolution error).
    Error(String),
}

/// Extract the NAT64 prefix from a synthesized IPv6 address.
///
/// Per RFC 6052, for a /96 prefix (most common):
/// The IPv4 address occupies the last 32 bits of the IPv6 address.
/// The prefix is the first 96 bits.
fn extract_nat64_prefix(synthesized: Ipv6Addr, expected_v4: Ipv4Addr) -> Option<Ipv6Addr> {
    let v6_octets = synthesized.octets();
    let v4_octets = expected_v4.octets();

    // Check if IPv4 address is embedded at bits 96-127 (most common /96 prefix)
    if v6_octets[12] == v4_octets[0]
        && v6_octets[13] == v4_octets[1]
        && v6_octets[14] == v4_octets[2]
        && v6_octets[15] == v4_octets[3]
    {
        // Prefix is the first 96 bits, zero-padded to 128
        let mut prefix = [0u8; 16];
        prefix[..12].copy_from_slice(&v6_octets[..12]);
        return Some(Ipv6Addr::from(prefix));
    }

    None
}

/// Detect NAT64 prefix by resolving `ipv4only.arpa` (RFC 7050).
///
/// On an IPv6-only network with NAT64, the DNS64 resolver synthesizes
/// AAAA records for `ipv4only.arpa` by embedding the well-known IPv4
/// addresses (192.0.0.170/171) in the NAT64 prefix.
///
/// Returns the detected NAT64 prefix or `NotDetected` if we have native IPv4.
pub async fn detect_nat64() -> Nat64Status {
    // Resolve ipv4only.arpa for AAAA records
    let addrs = match tokio::net::lookup_host("ipv4only.arpa:80").await {
        Ok(addrs) => addrs.collect::<Vec<_>>(),
        Err(e) => return Nat64Status::Error(format!("DNS lookup failed: {}", e)),
    };

    // Look for synthesized IPv6 addresses
    for addr in &addrs {
        if let SocketAddr::V6(v6_addr) = addr {
            let ip = *v6_addr.ip();

            // Skip if it's a well-known address without NAT64 prefix
            if ip == Ipv6Addr::from(WKA_1.to_ipv6_mapped())
                || ip == Ipv6Addr::from(WKA_2.to_ipv6_mapped())
            {
                continue;
            }

            // Try to extract NAT64 prefix from synthesized address
            if let Some(prefix) = extract_nat64_prefix(ip, WKA_1) {
                return Nat64Status::Detected {
                    prefix,
                    prefix_len: 96,
                };
            }
            if let Some(prefix) = extract_nat64_prefix(ip, WKA_2) {
                return Nat64Status::Detected {
                    prefix,
                    prefix_len: 96,
                };
            }
        }
    }

    Nat64Status::NotDetected
}

/// Synthesize an IPv6 address from an IPv4 address using a NAT64 prefix.
///
/// Given a NAT64 /96 prefix and an IPv4 address, produces the corresponding
/// synthesized IPv6 address that the NAT64 gateway will translate.
pub fn synthesize_ipv6(prefix: Ipv6Addr, prefix_len: u8, ipv4: Ipv4Addr) -> Option<Ipv6Addr> {
    if prefix_len != 96 {
        return None; // Only /96 supported for now
    }
    let mut octets = prefix.octets();
    let v4 = ipv4.octets();
    octets[12] = v4[0];
    octets[13] = v4[1];
    octets[14] = v4[2];
    octets[15] = v4[3];
    Some(Ipv6Addr::from(octets))
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

    #[test]
    fn test_extract_nat64_prefix_96() {
        // NAT64 prefix 64:ff9b::/96 with embedded 192.0.0.170
        let prefix_bytes: [u8; 16] = [
            0x00, 0x64, 0xff, 0x9b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x00,
            0x00, 0xaa,
        ];
        let synthesized = Ipv6Addr::from(prefix_bytes);
        let result = extract_nat64_prefix(synthesized, WKA_1);
        assert!(result.is_some());
        let prefix = result.unwrap();
        let octets = prefix.octets();
        // First 12 bytes should match, last 4 should be zero
        assert_eq!(&octets[..4], &[0x00, 0x64, 0xff, 0x9b]);
        assert_eq!(&octets[12..], &[0, 0, 0, 0]);
    }

    #[test]
    fn test_extract_nat64_no_match() {
        // Random IPv6 address — not a synthesized NAT64 address
        let random = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
        let result = extract_nat64_prefix(random, WKA_1);
        assert!(result.is_none());
    }

    #[test]
    fn test_synthesize_ipv6() {
        let prefix = Ipv6Addr::new(0x0064, 0xff9b, 0, 0, 0, 0, 0, 0);
        let ipv4 = Ipv4Addr::new(10, 0, 0, 1);
        let result = synthesize_ipv6(prefix, 96, ipv4).unwrap();
        let octets = result.octets();
        // Last 4 bytes should be the IPv4 address
        assert_eq!(&octets[12..], &[10, 0, 0, 1]);
        // First 12 bytes should be the prefix
        assert_eq!(&octets[..4], &[0x00, 0x64, 0xff, 0x9b]);
    }

    #[test]
    fn test_synthesize_ipv6_unsupported_prefix_len() {
        let prefix = Ipv6Addr::new(0x0064, 0xff9b, 0, 0, 0, 0, 0, 0);
        let ipv4 = Ipv4Addr::new(10, 0, 0, 1);
        // Only /96 supported
        assert!(synthesize_ipv6(prefix, 64, ipv4).is_none());
    }
}
