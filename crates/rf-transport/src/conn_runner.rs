//! Async connection runner — wires ConnectionManager state machine to real transports.
//!
//! This module implements the async runtime that connects the ConnectionManager's
//! state transitions to actual transport drivers (WebSocket, QUIC, Memory, etc.).

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::catalog::TransportCatalog;
use crate::connmgr::{ConnectionEvent, ConnectionManager, ConnectionState};
use crate::driver::{AsyncStream, Driver, DriverConfig, Target};
use crate::error::TransportError;

/// A live connection managed by the connection runner.
pub struct ManagedConnection {
    /// The active stream (encrypted or raw, depending on layer above).
    pub stream: Box<dyn AsyncStream>,
    /// Name of the transport that produced this connection.
    pub transport_name: String,
    /// Time the connection was established.
    pub established_at: Instant,
}

/// Result of a background probe attempt.
pub struct ProbeResult {
    pub transport_name: String,
    pub rtt: Duration,
    pub success: bool,
}

/// Async connection runner — connects ConnectionManager state machine to real transport drivers.
pub struct ConnectionRunner {
    manager: Arc<Mutex<ConnectionManager>>,
    drivers: Vec<(String, Arc<dyn Driver>)>,
    target: Target,
    config: DriverConfig,
}

impl ConnectionRunner {
    /// Create a new runner with the given catalog and drivers.
    pub fn new(catalog: TransportCatalog, target: Target) -> Self {
        Self {
            manager: Arc::new(Mutex::new(ConnectionManager::new(catalog))),
            drivers: Vec::new(),
            target,
            config: DriverConfig::new(),
        }
    }

    /// Register a transport driver.
    pub fn add_driver(mut self, name: &str, driver: Arc<dyn Driver>) -> Self {
        self.drivers.push((name.to_string(), driver));
        self
    }

    /// Set driver configuration.
    pub fn with_config(mut self, config: DriverConfig) -> Self {
        self.config = config;
        self
    }

    /// Establish initial connection (relay-first strategy).
    /// Returns the first successful connection.
    pub async fn connect(&self) -> Result<ManagedConnection, TransportError> {
        let relay_entries = {
            let mut mgr = self.manager.lock().await;
            mgr.connect_relay_first()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()
        };

        // Try each relay-tier transport in order
        for relay_name in &relay_entries {
            if let Some((_, driver)) = self.drivers.iter().find(|(n, _)| n == relay_name) {
                match driver.dial(&self.target, &self.config).await {
                    Ok(stream) => {
                        info!(transport = %relay_name, "relay connection established");
                        return Ok(ManagedConnection {
                            stream,
                            transport_name: relay_name.clone(),
                            established_at: Instant::now(),
                        });
                    }
                    Err(e) => {
                        warn!(transport = %relay_name, error = %e, "relay dial failed");
                    }
                }
            }
        }

        Err(TransportError::Connection(
            "all relay transports failed".into(),
        ))
    }

    /// Probe a specific transport for direct connectivity.
    /// Returns the RTT if successful.
    pub async fn probe_transport(&self, transport_name: &str) -> Option<ProbeResult> {
        let driver = self
            .drivers
            .iter()
            .find(|(n, _)| n == transport_name)
            .map(|(_, d)| d.clone())?;

        let start = Instant::now();
        match driver.dial(&self.target, &self.config).await {
            Ok(_stream) => {
                let rtt = start.elapsed();
                let mut mgr = self.manager.lock().await;
                mgr.direct_path_found(transport_name, rtt);
                Some(ProbeResult {
                    transport_name: transport_name.to_string(),
                    rtt,
                    success: true,
                })
            }
            Err(_) => Some(ProbeResult {
                transport_name: transport_name.to_string(),
                rtt: start.elapsed(),
                success: false,
            }),
        }
    }

    /// Run background probing for direct paths.
    /// Returns the first successful direct connection found.
    pub async fn probe_direct_paths(&self) -> Option<ManagedConnection> {
        let probe_targets = {
            let mgr = self.manager.lock().await;
            mgr.background_probe_targets()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()
        };

        for transport_name in &probe_targets {
            if let Some(result) = self.probe_transport(transport_name).await {
                if result.success {
                    // Try to establish a real connection for use
                    if let Some((_, driver)) =
                        self.drivers.iter().find(|(n, _)| n == transport_name)
                    {
                        if let Ok(stream) = driver.dial(&self.target, &self.config).await {
                            let mut mgr = self.manager.lock().await;
                            mgr.migrate_to_direct(transport_name);
                            info!(
                                transport = %transport_name,
                                rtt_ms = result.rtt.as_millis(),
                                "migrated to direct path"
                            );
                            return Some(ManagedConnection {
                                stream,
                                transport_name: transport_name.clone(),
                                established_at: Instant::now(),
                            });
                        }
                    }
                }
            }
        }
        None
    }

    /// Report tamper detection — blacklist current transport and attempt migration.
    pub async fn report_tamper(&self, reason: &str) -> Result<ManagedConnection, TransportError> {
        let abandoned = {
            let mut mgr = self.manager.lock().await;
            mgr.tamper_detected(reason)
        };

        if let Some(ref name) = abandoned {
            warn!(transport = %name, reason = %reason, "tamper detected, migrating");
        }

        // Try to establish a new connection on the new active transport
        let active = {
            let mgr = self.manager.lock().await;
            mgr.active_transport().map(|s| s.to_string())
        };

        if let Some(transport_name) = active {
            if let Some((_, driver)) = self.drivers.iter().find(|(n, _)| *n == transport_name) {
                if let Ok(stream) = driver.dial(&self.target, &self.config).await {
                    return Ok(ManagedConnection {
                        stream,
                        transport_name,
                        established_at: Instant::now(),
                    });
                }
            }
        }

        Err(TransportError::Connection(
            "no available transport after tamper".into(),
        ))
    }

    /// Report that the current connection failed — trigger failback.
    pub async fn report_failure(&self, reason: &str) -> Result<ManagedConnection, TransportError> {
        {
            let mut mgr = self.manager.lock().await;
            mgr.failback_to_relay(reason);
        }

        // Re-connect via relay
        self.connect().await
    }

    /// Get current connection state.
    pub async fn state(&self) -> ConnectionState {
        self.manager.lock().await.state()
    }

    /// Drain accumulated events.
    pub async fn drain_events(&self) -> Vec<ConnectionEvent> {
        self.manager.lock().await.drain_events()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{TransportEntry, TransportTier};
    use crate::memory::{MemoryBroker, MemoryDriver};

    fn test_catalog() -> TransportCatalog {
        let mut cat = TransportCatalog::new();
        cat.register(TransportEntry {
            name: "memory-relay".into(),
            tier: TransportTier::Relay,
            requires_udp: false,
            proxy_compatible: true,
            measured_rtt: Some(Duration::from_millis(5)),
            available: true,
            blacklisted: false,
        });
        cat.register(TransportEntry {
            name: "memory-direct".into(),
            tier: TransportTier::Direct,
            requires_udp: false,
            proxy_compatible: true,
            measured_rtt: Some(Duration::from_millis(1)),
            available: true,
            blacklisted: false,
        });
        cat
    }

    /// Helper to create a broker with a listener for the "test" agent.
    async fn setup_broker_with_listener(broker: &MemoryBroker) {
        let driver = MemoryDriver::new(broker.clone());
        let _listener = driver.listen("test").await.unwrap();
        // Keep listener alive by leaking it (test only)
        std::mem::forget(_listener);
    }

    #[tokio::test]
    async fn test_connect_relay_first() {
        let catalog = test_catalog();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let broker = MemoryBroker::new();
        setup_broker_with_listener(&broker).await;
        let driver = Arc::new(MemoryDriver::new(broker)) as Arc<dyn Driver>;
        let runner = ConnectionRunner::new(catalog, target)
            .add_driver("memory-relay", driver.clone())
            .add_driver("memory-direct", driver);

        let conn = runner.connect().await.unwrap();
        assert_eq!(conn.transport_name, "memory-relay");
        assert_eq!(runner.state().await, ConnectionState::RelayConnected);
    }

    #[tokio::test]
    async fn test_probe_and_migrate() {
        let catalog = test_catalog();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let broker = MemoryBroker::new();
        setup_broker_with_listener(&broker).await;
        let driver = Arc::new(MemoryDriver::new(broker)) as Arc<dyn Driver>;
        let runner = ConnectionRunner::new(catalog, target)
            .add_driver("memory-relay", driver.clone())
            .add_driver("memory-direct", driver);

        // First, establish relay connection
        let _conn = runner.connect().await.unwrap();
        assert_eq!(runner.state().await, ConnectionState::RelayConnected);

        // Probe for direct paths
        let direct = runner.probe_direct_paths().await;
        assert!(direct.is_some());
        assert_eq!(direct.unwrap().transport_name, "memory-direct");
        assert_eq!(runner.state().await, ConnectionState::DirectConnected);
    }

    #[tokio::test]
    async fn test_tamper_triggers_migration() {
        let catalog = test_catalog();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let broker = MemoryBroker::new();
        setup_broker_with_listener(&broker).await;
        let driver = Arc::new(MemoryDriver::new(broker)) as Arc<dyn Driver>;
        let runner = ConnectionRunner::new(catalog, target)
            .add_driver("memory-relay", driver.clone())
            .add_driver("memory-direct", driver);

        // Establish and upgrade
        let _conn = runner.connect().await.unwrap();
        let _direct = runner.probe_direct_paths().await;
        assert_eq!(runner.state().await, ConnectionState::DirectConnected);

        // Tamper on direct — should fall back to relay
        let new_conn = runner.report_tamper("MAC failure").await.unwrap();
        assert_eq!(new_conn.transport_name, "memory-relay");

        let events = runner.drain_events().await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ConnectionEvent::TamperDetected { .. }))
        );
    }

    #[tokio::test]
    async fn test_failure_triggers_failback() {
        let catalog = test_catalog();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let broker = MemoryBroker::new();
        setup_broker_with_listener(&broker).await;
        let driver = Arc::new(MemoryDriver::new(broker)) as Arc<dyn Driver>;
        let runner = ConnectionRunner::new(catalog, target)
            .add_driver("memory-relay", driver.clone())
            .add_driver("memory-direct", driver);

        let _conn = runner.connect().await.unwrap();
        let _direct = runner.probe_direct_paths().await;

        // Report failure on direct
        let fallback = runner.report_failure("connection reset").await.unwrap();
        assert_eq!(fallback.transport_name, "memory-relay");
    }
}
