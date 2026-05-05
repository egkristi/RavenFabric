//! Overlay network protocol types.
//!
//! Defines configuration for overlay/anonymous network integrations:
//! Reticulum, Yggdrasil, I2P, Veilid, and mixnets.

use serde::{Deserialize, Serialize};

/// Overlay network transport configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayTransport {
    /// Reticulum Network Stack — multi-hop mesh with announce-based discovery.
    Reticulum {
        /// Reticulum interface name.
        interface: String,
        /// Destination hash (32 hex chars).
        destination_hash: String,
        /// Enable Forward Error Correction.
        fec_enabled: bool,
        /// Announce interval in seconds.
        announce_interval_secs: u32,
    },
    /// Yggdrasil — self-configuring IPv6 mesh with key-derived addresses.
    Yggdrasil {
        /// Yggdrasil peer endpoint (e.g., "tcp://peer.example.com:9001").
        peers: Vec<String>,
        /// Listen address for incoming peerings.
        listen: Option<String>,
        /// Admin API socket path.
        admin_socket: Option<String>,
    },
    /// I2P — garlic routing for anonymous internal services.
    I2p {
        /// I2P SAM bridge address (usually 127.0.0.1:7656).
        sam_addr: String,
        /// Destination key (base64).
        destination: Option<String>,
        /// Tunnel length (hops).
        tunnel_length: u8,
    },
    /// Veilid — DHT-based, onion-routed by default.
    Veilid {
        /// Veilid API endpoint.
        api_endpoint: String,
        /// Route ID for the target.
        route_id: Option<String>,
        /// Enable privacy routing (default: true).
        privacy_route: bool,
    },
    /// Mixnet (Nym/Loopix) — traffic analysis resistant.
    Mixnet {
        /// Mixnet gateway address.
        gateway: String,
        /// Number of mix nodes in the route.
        mix_depth: u8,
        /// Average delay per hop (ms).
        avg_delay_ms: u32,
        /// Cover traffic rate (messages/sec, 0 = disabled).
        cover_traffic_rate: f64,
    },
}

/// DHT discovery configuration (Kademlia-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtConfig {
    /// Bootstrap nodes for initial discovery.
    pub bootstrap_nodes: Vec<String>,
    /// Replication factor (k parameter).
    pub replication_factor: u8,
    /// Lookup parallelism (alpha parameter).
    pub parallelism: u8,
    /// Record TTL in seconds.
    pub record_ttl_secs: u64,
    /// Local node key (derived from agent key).
    pub node_id: Option<String>,
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            bootstrap_nodes: Vec::new(),
            replication_factor: 20,
            parallelism: 3,
            record_ttl_secs: 3600,
            node_id: None,
        }
    }
}

/// Gossip protocol variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GossipProtocol {
    /// SWIM (Scalable Weakly-consistent Infection-style Membership).
    Swim,
    /// HyParView — self-healing partial membership.
    HyParView,
    /// PlumTree — epidemic broadcast tree.
    PlumTree,
}

/// Traffic analysis resistance configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficAnalysisResistance {
    /// Minimum padding to add to all frames.
    pub min_padding_bytes: u16,
    /// Target packet size (normalize all to this size).
    pub normalized_size: Option<u16>,
    /// Noise floor: dummy messages per second when idle.
    pub noise_floor_rate: f64,
    /// Maximum random delay added to outgoing messages (ms).
    pub timing_jitter_ms: u32,
    /// Batch messages to send at fixed intervals.
    pub batch_interval_ms: Option<u32>,
}

impl Default for TrafficAnalysisResistance {
    fn default() -> Self {
        Self {
            min_padding_bytes: 0,
            normalized_size: None,
            noise_floor_rate: 0.0,
            timing_jitter_ms: 0,
            batch_interval_ms: None,
        }
    }
}

/// TUN device configuration for mesh VPN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunConfig {
    /// Device name (e.g., "rvnf0").
    pub name: String,
    /// IPv6 address to assign (derived from public key).
    pub address: String,
    /// Prefix length.
    pub prefix_len: u8,
    /// MTU (default: 1420 for WireGuard compatibility).
    pub mtu: u16,
    /// Whether to set as default route.
    pub default_route: bool,
    /// Allowed IP ranges to route through the tunnel.
    pub allowed_ips: Vec<String>,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            name: "rvnf0".to_string(),
            address: String::new(),
            prefix_len: 64,
            mtu: 1420,
            default_route: false,
            allowed_ips: Vec::new(),
        }
    }
}

/// Multipath configuration for using multiple transports simultaneously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipathConfig {
    /// Scheduling policy for traffic across paths.
    pub scheduler: MultipathScheduler,
    /// Minimum number of active paths to maintain.
    pub min_paths: u8,
    /// Maximum number of active paths.
    pub max_paths: u8,
    /// Failover timeout: switch path if no response in this many ms.
    pub failover_timeout_ms: u32,
    /// Whether to duplicate critical packets across all paths.
    pub redundant_critical: bool,
}

/// Multipath scheduling algorithm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultipathScheduler {
    /// Round-robin across available paths.
    RoundRobin,
    /// Weighted by measured latency (lower latency = more traffic).
    LatencyWeighted,
    /// Lowest latency path only (failover to next).
    LowestLatency,
    /// Redundant: send on all paths, deduplicate at receiver.
    Redundant,
    /// Bandwidth-weighted: distribute based on available bandwidth.
    BandwidthWeighted,
}

impl Default for MultipathConfig {
    fn default() -> Self {
        Self {
            scheduler: MultipathScheduler::LatencyWeighted,
            min_paths: 1,
            max_paths: 4,
            failover_timeout_ms: 5000,
            redundant_critical: true,
        }
    }
}

/// Connection migration trigger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationTrigger {
    /// Network interface changed (WiFi ↔ cellular).
    InterfaceChange,
    /// Current path latency exceeds threshold.
    LatencyThreshold { max_ms: u32 },
    /// Current path packet loss exceeds threshold.
    PacketLoss { max_percent: u8 },
    /// Manual request.
    Manual,
    /// Tamper detected on current path.
    TamperDetected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reticulum_config() {
        let transport = OverlayTransport::Reticulum {
            interface: "udp0".into(),
            destination_hash: "a".repeat(64),
            fec_enabled: true,
            announce_interval_secs: 300,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("reticulum"));
    }

    #[test]
    fn test_yggdrasil_config() {
        let transport = OverlayTransport::Yggdrasil {
            peers: vec!["tcp://peer1.example.com:9001".into()],
            listen: Some("tcp://0.0.0.0:9001".into()),
            admin_socket: None,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("yggdrasil"));
    }

    #[test]
    fn test_mixnet_config() {
        let transport = OverlayTransport::Mixnet {
            gateway: "gateway.nym.example.com:9000".into(),
            mix_depth: 3,
            avg_delay_ms: 200,
            cover_traffic_rate: 0.5,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("mixnet"));
        assert!(json.contains("cover_traffic"));
    }

    #[test]
    fn test_dht_config_default() {
        let config = DhtConfig::default();
        assert_eq!(config.replication_factor, 20);
        assert_eq!(config.parallelism, 3);
    }

    #[test]
    fn test_tun_config() {
        let config = TunConfig {
            name: "rvnf0".into(),
            address: "fd00:5256::1".into(),
            prefix_len: 64,
            mtu: 1420,
            default_route: false,
            allowed_ips: vec!["fd00:5256::/32".into()],
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("rvnf0"));
        assert!(json.contains("1420"));
    }

    #[test]
    fn test_multipath_config() {
        let config = MultipathConfig::default();
        assert_eq!(config.scheduler, MultipathScheduler::LatencyWeighted);
        assert!(config.redundant_critical);
    }

    #[test]
    fn test_traffic_resistance_config() {
        let config = TrafficAnalysisResistance {
            min_padding_bytes: 64,
            normalized_size: Some(1024),
            noise_floor_rate: 1.0,
            timing_jitter_ms: 100,
            batch_interval_ms: Some(50),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("1024"));
    }

    #[test]
    fn test_migration_triggers_serde() {
        let trigger = MigrationTrigger::LatencyThreshold { max_ms: 500 };
        let json = serde_json::to_string(&trigger).unwrap();
        assert!(json.contains("latency_threshold"));
        let parsed: MigrationTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, trigger);
    }
}
