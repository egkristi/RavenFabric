//! Transport catalog and path selection strategies.
//!
//! Provides a tiered classification of transports and strategies for
//! selecting which transport path to use based on network environment,
//! measured performance, and policy constraints.

use std::collections::HashMap;
use std::time::Duration;

/// Tier classification for a transport path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransportTier {
    /// Direct peer-to-peer (lowest latency, best throughput).
    Direct = 0,
    /// NAT-traversed (hole-punched, slightly higher latency).
    NatTraversal = 1,
    /// Relayed via a relay server (always works, higher latency).
    Relay = 2,
    /// Overlay network (WireGuard, mesh, additional encapsulation).
    Overlay = 3,
    /// Hostile-environment transport (domain fronting, steganography).
    Hostile = 4,
    /// Out-of-band (USB, serial, physical media — DTN).
    OutOfBand = 5,
}

/// A registered transport driver with its capabilities.
#[derive(Debug, Clone)]
pub struct TransportEntry {
    /// Unique name (e.g., "websocket", "quic", "wireguard").
    pub name: String,
    /// Tier classification.
    pub tier: TransportTier,
    /// Whether this driver requires UDP.
    pub requires_udp: bool,
    /// Whether this driver works through HTTP proxies.
    pub proxy_compatible: bool,
    /// Minimum measured RTT (if probed), None if not yet measured.
    pub measured_rtt: Option<Duration>,
    /// Whether this driver is currently available (probed successfully).
    pub available: bool,
    /// Whether this path is blacklisted (compromised).
    pub blacklisted: bool,
}

/// Strategy for selecting transport paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStrategy {
    /// Try transports sequentially in tier order (lowest tier first).
    Sequential,
    /// Race all available transports in parallel, pick fastest.
    Race,
    /// Try all in parallel, use all that succeed (multi-path).
    Parallel,
    /// Race within each tier, escalate to next tier on failure.
    TieredRace,
    /// Policy-driven: only use transports allowed by policy for this command.
    PolicyDriven,
}

/// The transport catalog — registry of all available transports.
#[derive(Debug, Clone)]
pub struct TransportCatalog {
    entries: HashMap<String, TransportEntry>,
}

impl TransportCatalog {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a transport driver.
    pub fn register(&mut self, entry: TransportEntry) {
        self.entries.insert(entry.name.clone(), entry);
    }

    /// Get all available (non-blacklisted) transports, sorted by tier then RTT.
    pub fn available(&self) -> Vec<&TransportEntry> {
        let mut entries: Vec<_> = self
            .entries
            .values()
            .filter(|e| e.available && !e.blacklisted)
            .collect();
        entries.sort_by(|a, b| {
            a.tier.cmp(&b.tier).then_with(|| {
                let rtt_a = a.measured_rtt.unwrap_or(Duration::MAX);
                let rtt_b = b.measured_rtt.unwrap_or(Duration::MAX);
                rtt_a.cmp(&rtt_b)
            })
        });
        entries
    }

    /// Get transports filtered by tier.
    pub fn by_tier(&self, tier: TransportTier) -> Vec<&TransportEntry> {
        self.entries
            .values()
            .filter(|e| e.tier == tier && e.available && !e.blacklisted)
            .collect()
    }

    /// Get transports compatible with the current network environment.
    pub fn compatible_with_udp(&self, udp_available: bool) -> Vec<&TransportEntry> {
        self.entries
            .values()
            .filter(|e| e.available && !e.blacklisted && (!e.requires_udp || udp_available))
            .collect()
    }

    /// Blacklist a transport (compromised path).
    pub fn blacklist(&mut self, name: &str) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.blacklisted = true;
        }
    }

    /// Remove blacklist (operator acknowledgment).
    pub fn unblacklist(&mut self, name: &str) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.blacklisted = false;
        }
    }

    /// Mark a transport as probed with RTT result.
    pub fn record_probe(&mut self, name: &str, rtt: Duration) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.measured_rtt = Some(rtt);
            entry.available = true;
        }
    }

    /// Mark a transport as unavailable (probe failed).
    pub fn mark_unavailable(&mut self, name: &str) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.available = false;
        }
    }

    /// Select transports according to a strategy.
    pub fn select(&self, strategy: PathStrategy) -> Vec<&TransportEntry> {
        match strategy {
            PathStrategy::Sequential | PathStrategy::TieredRace => self.available(),
            PathStrategy::Race | PathStrategy::Parallel => {
                // Return all available for parallel probing
                self.available()
            }
            PathStrategy::PolicyDriven => {
                // Without policy context, fall back to sequential
                self.available()
            }
        }
    }

    /// Select transports filtered by policy (only allowed tiers).
    pub fn select_with_policy(
        &self,
        _strategy: PathStrategy,
        allowed_tiers: &[TransportTier],
    ) -> Vec<&TransportEntry> {
        let mut entries: Vec<_> = self
            .entries
            .values()
            .filter(|e| e.available && !e.blacklisted && allowed_tiers.contains(&e.tier))
            .collect();
        entries.sort_by(|a, b| {
            a.tier.cmp(&b.tier).then_with(|| {
                let rtt_a = a.measured_rtt.unwrap_or(Duration::MAX);
                let rtt_b = b.measured_rtt.unwrap_or(Duration::MAX);
                rtt_a.cmp(&rtt_b)
            })
        });
        entries
    }
}

impl Default for TransportCatalog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog() -> TransportCatalog {
        let mut cat = TransportCatalog::new();
        cat.register(TransportEntry {
            name: "websocket-relay".into(),
            tier: TransportTier::Relay,
            requires_udp: false,
            proxy_compatible: true,
            measured_rtt: Some(Duration::from_millis(45)),
            available: true,
            blacklisted: false,
        });
        cat.register(TransportEntry {
            name: "quic-direct".into(),
            tier: TransportTier::Direct,
            requires_udp: true,
            proxy_compatible: false,
            measured_rtt: Some(Duration::from_millis(12)),
            available: true,
            blacklisted: false,
        });
        cat.register(TransportEntry {
            name: "wireguard".into(),
            tier: TransportTier::Overlay,
            requires_udp: true,
            proxy_compatible: false,
            measured_rtt: Some(Duration::from_millis(8)),
            available: true,
            blacklisted: false,
        });
        cat
    }

    #[test]
    fn test_available_sorted_by_tier_then_rtt() {
        let cat = sample_catalog();
        let available = cat.available();
        assert_eq!(available[0].name, "quic-direct"); // Direct tier
        assert_eq!(available[1].name, "websocket-relay"); // Relay tier
        assert_eq!(available[2].name, "wireguard"); // Overlay tier
    }

    #[test]
    fn test_blacklist_excludes_transport() {
        let mut cat = sample_catalog();
        cat.blacklist("quic-direct");
        let available = cat.available();
        assert_eq!(available.len(), 2);
        assert!(available.iter().all(|e| e.name != "quic-direct"));
    }

    #[test]
    fn test_unblacklist_restores() {
        let mut cat = sample_catalog();
        cat.blacklist("quic-direct");
        assert_eq!(cat.available().len(), 2);
        cat.unblacklist("quic-direct");
        assert_eq!(cat.available().len(), 3);
    }

    #[test]
    fn test_compatible_with_no_udp() {
        let cat = sample_catalog();
        let compat = cat.compatible_with_udp(false);
        assert_eq!(compat.len(), 1);
        assert_eq!(compat[0].name, "websocket-relay");
    }

    #[test]
    fn test_by_tier() {
        let cat = sample_catalog();
        let direct = cat.by_tier(TransportTier::Direct);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].name, "quic-direct");
    }

    #[test]
    fn test_select_with_policy() {
        let cat = sample_catalog();
        let selected = cat.select_with_policy(
            PathStrategy::PolicyDriven,
            &[TransportTier::Direct, TransportTier::Relay],
        );
        assert_eq!(selected.len(), 2);
        // Should not include overlay (wireguard)
        assert!(selected.iter().all(|e| e.name != "wireguard"));
    }

    #[test]
    fn test_record_probe() {
        let mut cat = TransportCatalog::new();
        cat.register(TransportEntry {
            name: "test".into(),
            tier: TransportTier::Direct,
            requires_udp: false,
            proxy_compatible: false,
            measured_rtt: None,
            available: false,
            blacklisted: false,
        });
        assert_eq!(cat.available().len(), 0);
        cat.record_probe("test", Duration::from_millis(25));
        assert_eq!(cat.available().len(), 1);
        assert_eq!(
            cat.available()[0].measured_rtt,
            Some(Duration::from_millis(25))
        );
    }
}
