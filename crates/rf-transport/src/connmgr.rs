//! Connection manager implementing relay-first with background upgrade.
//!
//! Strategy:
//! 1. Connect via relay immediately (always works)
//! 2. In background, probe for direct paths
//! 3. When a direct path is found and verified, migrate to it
//! 4. If direct path fails later, fall back to relay
//! 5. On network change, re-probe all drivers

use std::time::Duration;

use crate::catalog::{PathStrategy, TransportCatalog, TransportEntry, TransportTier};

/// State of the connection manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected yet.
    Disconnected,
    /// Connected via relay (initial, always-available path).
    RelayConnected,
    /// Connected via direct path (upgraded from relay).
    DirectConnected,
    /// Failing over from direct back to relay.
    Failover,
    /// Re-probing after network change.
    Reprobing,
}

/// Events produced by the connection manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// Initial relay connection established.
    RelayEstablished { transport: String },
    /// Direct path found during background probing.
    DirectPathFound { transport: String, rtt_ms: u64 },
    /// Migrated to direct path (relay kept warm as fallback).
    MigratedToDirect { transport: String },
    /// Direct path failed, falling back to relay.
    FallbackToRelay { reason: String },
    /// Network change detected, re-probing all drivers.
    NetworkChange,
    /// All paths failed.
    AllPathsFailed,
    /// Tamper detected on a transport — path abandoned and blacklisted.
    TamperDetected { transport: String, reason: String },
    /// Session migrated to alternative transport after tamper/failure.
    SessionMigrated { from: String, to: String },
}

/// Connection manager that implements relay-first with background probing.
#[derive(Debug)]
pub struct ConnectionManager {
    state: ConnectionState,
    catalog: TransportCatalog,
    /// Name of the active transport.
    active_transport: Option<String>,
    /// Name of the fallback transport (kept warm).
    fallback_transport: Option<String>,
    /// Background probe interval.
    probe_interval: Duration,
    /// Events accumulated since last drain.
    events: Vec<ConnectionEvent>,
}

impl ConnectionManager {
    pub fn new(catalog: TransportCatalog) -> Self {
        Self {
            state: ConnectionState::Disconnected,
            catalog,
            active_transport: None,
            fallback_transport: None,
            probe_interval: Duration::from_secs(30),
            events: Vec::new(),
        }
    }

    /// Set the background probe interval.
    pub fn with_probe_interval(mut self, interval: Duration) -> Self {
        self.probe_interval = interval;
        self
    }

    /// Current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Name of the currently active transport.
    pub fn active_transport(&self) -> Option<&str> {
        self.active_transport.as_deref()
    }

    /// Start connection: establish relay first.
    pub fn connect_relay_first(&mut self) -> Vec<&TransportEntry> {
        // Get relay-tier transports for initial connection
        let relays = self.catalog.by_tier(TransportTier::Relay);
        if !relays.is_empty() {
            self.state = ConnectionState::RelayConnected;
            self.active_transport = Some(relays[0].name.clone());
            self.events.push(ConnectionEvent::RelayEstablished {
                transport: relays[0].name.clone(),
            });
        }
        relays
    }

    /// Get transports to probe in background (direct paths).
    pub fn background_probe_targets(&self) -> Vec<&TransportEntry> {
        let mut targets: Vec<_> = self
            .catalog
            .available()
            .into_iter()
            .filter(|e| e.tier < TransportTier::Relay)
            .collect();
        targets.sort_by_key(|e| e.tier);
        targets
    }

    /// Report that a direct path was successfully probed.
    pub fn direct_path_found(&mut self, transport_name: &str, rtt: Duration) {
        self.catalog.record_probe(transport_name, rtt);
        self.events.push(ConnectionEvent::DirectPathFound {
            transport: transport_name.to_string(),
            rtt_ms: rtt.as_millis() as u64,
        });
    }

    /// Migrate to a direct path (make-before-break).
    pub fn migrate_to_direct(&mut self, transport_name: &str) {
        self.fallback_transport = self.active_transport.take();
        self.active_transport = Some(transport_name.to_string());
        self.state = ConnectionState::DirectConnected;
        self.events.push(ConnectionEvent::MigratedToDirect {
            transport: transport_name.to_string(),
        });
    }

    /// Direct path failed — fall back to relay.
    pub fn failback_to_relay(&mut self, reason: &str) {
        self.state = ConnectionState::Failover;
        if let Some(fallback) = self.fallback_transport.take() {
            self.active_transport = Some(fallback);
            self.state = ConnectionState::RelayConnected;
        }
        self.events.push(ConnectionEvent::FallbackToRelay {
            reason: reason.to_string(),
        });
    }

    /// Network change detected — re-probe all.
    pub fn network_changed(&mut self) {
        self.state = ConnectionState::Reprobing;
        self.events.push(ConnectionEvent::NetworkChange);
    }

    /// Tamper detected on current transport — blacklist it and migrate.
    /// Returns the transport that was abandoned.
    pub fn tamper_detected(&mut self, reason: &str) -> Option<String> {
        let abandoned = self.active_transport.take();

        // Blacklist the compromised transport
        if let Some(ref name) = abandoned {
            self.catalog.blacklist(name);
            self.events.push(ConnectionEvent::TamperDetected {
                transport: name.clone(),
                reason: reason.to_string(),
            });
        }

        // Try to migrate to fallback
        if let Some(fallback) = self.fallback_transport.take() {
            self.active_transport = Some(fallback.clone());
            self.state = ConnectionState::RelayConnected;
            self.events.push(ConnectionEvent::SessionMigrated {
                from: abandoned.clone().unwrap_or_default(),
                to: fallback,
            });
        } else {
            // No fallback — try to find any available non-blacklisted transport
            let available = self.catalog.available();
            if let Some(entry) = available.first() {
                self.active_transport = Some(entry.name.clone());
                self.state = ConnectionState::RelayConnected;
                self.events.push(ConnectionEvent::SessionMigrated {
                    from: abandoned.clone().unwrap_or_default(),
                    to: entry.name.clone(),
                });
            } else {
                self.state = ConnectionState::Disconnected;
                self.events.push(ConnectionEvent::AllPathsFailed);
            }
        }

        abandoned
    }

    /// Drain accumulated events.
    pub fn drain_events(&mut self) -> Vec<ConnectionEvent> {
        std::mem::take(&mut self.events)
    }

    /// Access the underlying catalog.
    pub fn catalog(&self) -> &TransportCatalog {
        &self.catalog
    }

    /// Mutable access to the catalog (for probing results).
    pub fn catalog_mut(&mut self) -> &mut TransportCatalog {
        &mut self.catalog
    }

    /// Select transports based on strategy.
    pub fn select(&self, strategy: PathStrategy) -> Vec<&TransportEntry> {
        self.catalog.select(strategy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TransportEntry;

    fn test_catalog() -> TransportCatalog {
        let mut cat = TransportCatalog::new();
        cat.register(TransportEntry {
            name: "ws-relay".into(),
            tier: TransportTier::Relay,
            requires_udp: false,
            proxy_compatible: true,
            measured_rtt: Some(Duration::from_millis(50)),
            available: true,
            blacklisted: false,
        });
        cat.register(TransportEntry {
            name: "quic-direct".into(),
            tier: TransportTier::Direct,
            requires_udp: true,
            proxy_compatible: false,
            measured_rtt: None,
            available: true,
            blacklisted: false,
        });
        cat
    }

    #[test]
    fn test_connect_relay_first() {
        let mut mgr = ConnectionManager::new(test_catalog());
        assert_eq!(mgr.state(), ConnectionState::Disconnected);

        mgr.connect_relay_first();
        assert_eq!(mgr.state(), ConnectionState::RelayConnected);
        assert_eq!(mgr.active_transport(), Some("ws-relay"));
    }

    #[test]
    fn test_background_probe_targets() {
        let mgr = ConnectionManager::new(test_catalog());
        let targets = mgr.background_probe_targets();
        // Should only return direct-tier (lower than relay)
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "quic-direct");
    }

    #[test]
    fn test_migrate_to_direct() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();
        mgr.direct_path_found("quic-direct", Duration::from_millis(12));
        mgr.migrate_to_direct("quic-direct");

        assert_eq!(mgr.state(), ConnectionState::DirectConnected);
        assert_eq!(mgr.active_transport(), Some("quic-direct"));
    }

    #[test]
    fn test_failback_to_relay() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();
        mgr.migrate_to_direct("quic-direct");
        mgr.failback_to_relay("connection timeout");

        assert_eq!(mgr.state(), ConnectionState::RelayConnected);
        assert_eq!(mgr.active_transport(), Some("ws-relay"));
    }

    #[test]
    fn test_events_accumulated() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();
        mgr.direct_path_found("quic-direct", Duration::from_millis(10));
        mgr.migrate_to_direct("quic-direct");

        let events = mgr.drain_events();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            ConnectionEvent::RelayEstablished { .. }
        ));
        assert!(matches!(events[1], ConnectionEvent::DirectPathFound { .. }));
        assert!(matches!(
            events[2],
            ConnectionEvent::MigratedToDirect { .. }
        ));
    }

    #[test]
    fn test_network_change() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();
        mgr.network_changed();
        assert_eq!(mgr.state(), ConnectionState::Reprobing);
    }

    #[test]
    fn test_tamper_detected_migrates_to_fallback() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();
        mgr.migrate_to_direct("quic-direct");

        // Simulate tamper on direct path
        let abandoned = mgr.tamper_detected("MAC failure");
        assert_eq!(abandoned, Some("quic-direct".to_string()));
        // Should migrate back to relay
        assert_eq!(mgr.state(), ConnectionState::RelayConnected);
        assert_eq!(mgr.active_transport(), Some("ws-relay"));

        // Verify quic-direct is blacklisted
        let events = mgr.drain_events();
        assert!(events.iter().any(|e| matches!(e, ConnectionEvent::TamperDetected { transport, .. } if transport == "quic-direct")));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ConnectionEvent::SessionMigrated { .. }))
        );
    }

    #[test]
    fn test_tamper_no_fallback_finds_available() {
        let mut mgr = ConnectionManager::new(test_catalog());
        mgr.connect_relay_first();

        // Tamper on relay with no fallback set — should find quic-direct
        let abandoned = mgr.tamper_detected("frame injection");
        assert_eq!(abandoned, Some("ws-relay".to_string()));
        // Should have migrated to quic-direct (the only other available)
        assert_eq!(mgr.active_transport(), Some("quic-direct"));
    }
}
