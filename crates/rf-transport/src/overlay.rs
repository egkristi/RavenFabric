//! Overlay network protocol types.
//!
//! Defines configuration for overlay/anonymous network integrations:
//! Reticulum, Yggdrasil, I2P, Veilid, and mixnets.

use std::collections::HashMap;

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

// --- Kademlia DHT Routing Table ---

/// 256-bit node ID for Kademlia routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    /// Create from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// XOR distance between two node IDs.
    pub fn distance(&self, other: &NodeId) -> [u8; 32] {
        let mut result = [0u8; 32];
        for (i, (a, b)) in self.0.iter().zip(other.0.iter()).enumerate() {
            result[i] = a ^ b;
        }
        result
    }

    /// Leading zero bits in distance — determines k-bucket index.
    pub fn bucket_index(&self, other: &NodeId) -> usize {
        let dist = self.distance(other);
        for (byte_idx, &byte) in dist.iter().enumerate() {
            if byte != 0 {
                return byte_idx * 8 + byte.leading_zeros() as usize;
            }
        }
        255 // Same node
    }
}

/// Entry in the DHT routing table.
#[derive(Debug, Clone)]
pub struct DhtNode {
    /// Node ID (256-bit).
    pub id: NodeId,
    /// Network address for this node.
    pub addr: String,
    /// Last time this node was seen (Unix ms).
    pub last_seen_ms: u64,
}

/// Kademlia-style DHT routing table.
///
/// Organizes known nodes into 256 k-buckets, where each bucket
/// holds nodes at a specific XOR distance from the local node.
pub struct KademliaTable {
    /// Our node ID.
    local_id: NodeId,
    /// K-buckets (index = leading zero bits of XOR distance).
    buckets: Vec<Vec<DhtNode>>,
    /// Max nodes per bucket (k parameter).
    k: usize,
}

impl KademliaTable {
    /// Create a new routing table.
    pub fn new(local_id: NodeId, k: usize) -> Self {
        Self {
            local_id,
            buckets: (0..256).map(|_| Vec::new()).collect(),
            k,
        }
    }

    /// Insert or update a node in the routing table.
    /// Returns true if the node was inserted (new or updated).
    pub fn insert(&mut self, node: DhtNode) -> bool {
        if node.id == self.local_id {
            return false;
        }

        let idx = self.local_id.bucket_index(&node.id);
        let bucket = &mut self.buckets[idx];

        // Update if already present
        if let Some(existing) = bucket.iter_mut().find(|n| n.id == node.id) {
            existing.last_seen_ms = node.last_seen_ms;
            existing.addr = node.addr;
            return true;
        }

        // Insert if bucket has room
        if bucket.len() < self.k {
            bucket.push(node);
            return true;
        }

        false // Bucket full
    }

    /// Find the `count` closest nodes to a target ID.
    pub fn closest(&self, target: &NodeId, count: usize) -> Vec<&DhtNode> {
        let mut all_nodes: Vec<(&DhtNode, [u8; 32])> = self
            .buckets
            .iter()
            .flat_map(|b| b.iter())
            .map(|n| (n, target.distance(&n.id)))
            .collect();

        all_nodes.sort_by(|a, b| a.1.cmp(&b.1));
        all_nodes.into_iter().take(count).map(|(n, _)| n).collect()
    }

    /// Remove a node from the routing table.
    pub fn remove(&mut self, id: &NodeId) -> bool {
        if *id == self.local_id {
            return false;
        }
        let idx = self.local_id.bucket_index(id);
        let bucket = &mut self.buckets[idx];
        let len_before = bucket.len();
        bucket.retain(|n| n.id != *id);
        bucket.len() < len_before
    }

    /// Total number of nodes in the routing table.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.len()).sum()
    }

    /// Whether the routing table is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get our local node ID.
    pub fn local_id(&self) -> &NodeId {
        &self.local_id
    }
}

// --- Multi-hop Store-Carry-Forward ---

/// A forwarding decision for a DTN bundle at a relay node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardDecision {
    /// Forward directly to the destination (within reach).
    Direct { next_hop: String },
    /// Forward via a known intermediate node.
    Relay { next_hop: String, via: String },
    /// Store locally and wait for a contact opportunity.
    Store,
    /// Drop — TTL expired or max hops exceeded.
    Drop { reason: String },
}

/// Multi-hop forwarder — makes routing decisions for DTN bundles.
pub struct HopForwarder {
    local_id: String,
    /// Known direct neighbors.
    neighbors: Vec<String>,
}

impl HopForwarder {
    /// Create a new forwarder.
    pub fn new(local_id: String) -> Self {
        Self {
            local_id,
            neighbors: Vec::new(),
        }
    }

    /// Get the local node ID.
    pub fn local_id(&self) -> &str {
        &self.local_id
    }

    /// Add a known neighbor.
    pub fn add_neighbor(&mut self, peer_id: String) {
        if !self.neighbors.contains(&peer_id) {
            self.neighbors.push(peer_id);
        }
    }

    /// Remove a neighbor.
    pub fn remove_neighbor(&mut self, peer_id: &str) {
        self.neighbors.retain(|n| n != peer_id);
    }

    /// Decide how to handle an incoming bundle.
    pub fn decide(&self, destination: &str, hop_count: u32, max_hops: u32) -> ForwardDecision {
        // Check hop limit
        if max_hops > 0 && hop_count >= max_hops {
            return ForwardDecision::Drop {
                reason: "max hops exceeded".to_string(),
            };
        }

        // Direct delivery if destination is a neighbor
        if self.neighbors.contains(&destination.to_string()) {
            return ForwardDecision::Direct {
                next_hop: destination.to_string(),
            };
        }

        // If we have neighbors, pick the first one as relay
        // (in a real implementation this would use DHT/routing table)
        if let Some(relay) = self.neighbors.first() {
            return ForwardDecision::Relay {
                next_hop: relay.clone(),
                via: relay.clone(),
            };
        }

        // No neighbors — store for later
        ForwardDecision::Store
    }

    /// Number of known neighbors.
    pub fn neighbor_count(&self) -> usize {
        self.neighbors.len()
    }
}

// --- Connection Migration Across Interfaces ---

/// Interface-aware migration controller.
///
/// Links netwatch (OS network change detection) to the session
/// migration state machine, triggering migration when interfaces change.
pub struct InterfaceMigration {
    /// Current active interface name.
    active_interface: Option<String>,
    /// Preferred interface patterns (e.g., "en0", "wlan0").
    preferred: Vec<String>,
    /// Whether to auto-migrate on interface change.
    auto_migrate: bool,
    /// Migrations performed.
    migration_count: u32,
}

impl InterfaceMigration {
    /// Create a new migration controller.
    pub fn new(auto_migrate: bool) -> Self {
        Self {
            active_interface: None,
            preferred: Vec::new(),
            auto_migrate,
            migration_count: 0,
        }
    }

    /// Set preferred interface patterns.
    pub fn with_preferred(mut self, patterns: Vec<String>) -> Self {
        self.preferred = patterns;
        self
    }

    /// Set the current active interface.
    pub fn set_active(&mut self, interface: String) {
        self.active_interface = Some(interface);
    }

    /// Process a network change event.
    /// Returns Some(new_interface) if migration should occur.
    pub fn on_interface_change(&mut self, available: &[String]) -> Option<String> {
        if available.is_empty() {
            return None;
        }

        // Pick best interface from available
        let best = self.pick_best(available);

        // Check if we need to migrate
        if let Some(ref active) = self.active_interface {
            if active == &best {
                return None; // Already on best interface
            }

            if !available.contains(active) || self.auto_migrate {
                self.active_interface = Some(best.clone());
                self.migration_count += 1;
                return Some(best);
            }

            None
        } else {
            // No active interface — just set it
            self.active_interface = Some(best.clone());
            Some(best)
        }
    }

    /// Pick the best interface from available ones.
    fn pick_best(&self, available: &[String]) -> String {
        // Check preferred patterns first
        for pattern in &self.preferred {
            if let Some(iface) = available.iter().find(|a| a.starts_with(pattern.as_str())) {
                return iface.clone();
            }
        }
        // Default to first available
        available[0].clone()
    }

    /// Number of migrations performed.
    pub fn migration_count(&self) -> u32 {
        self.migration_count
    }

    /// Current active interface.
    pub fn active_interface(&self) -> Option<&str> {
        self.active_interface.as_deref()
    }
}

// --- Censorship Escalation ---

/// Path metrics for multipath scheduling decisions.
#[derive(Debug, Clone)]
pub struct PathMetrics {
    /// Path identifier.
    pub name: String,
    /// Measured round-trip time (ms).
    pub rtt_ms: u32,
    /// Measured bandwidth (bytes/sec).
    pub bandwidth_bps: u64,
    /// Packet loss rate (0.0 - 1.0).
    pub loss_rate: f64,
    /// Whether this path is currently active.
    pub active: bool,
}

/// Multipath frame scheduler — distributes frames across multiple paths.
pub struct MultipathFrameScheduler {
    /// Path metrics for scheduling.
    paths: Vec<PathMetrics>,
    /// Scheduling algorithm.
    scheduler: MultipathScheduler,
    /// Round-robin index (for RoundRobin mode).
    rr_index: usize,
    /// Sequence number for dedup at receiver.
    sequence: u64,
    /// Whether to duplicate critical frames.
    redundant_critical: bool,
}

/// A scheduled frame with path assignment.
#[derive(Debug, Clone)]
pub struct ScheduledFrame {
    /// Path to send on.
    pub path: String,
    /// Sequence number (for dedup).
    pub sequence: u64,
    /// Whether this is a redundant copy.
    pub redundant: bool,
}

impl MultipathFrameScheduler {
    /// Create from config.
    pub fn new(config: &MultipathConfig) -> Self {
        Self {
            paths: Vec::new(),
            scheduler: config.scheduler.clone(),
            rr_index: 0,
            sequence: 0,
            redundant_critical: config.redundant_critical,
        }
    }

    /// Update path metrics.
    pub fn update_path(&mut self, metrics: PathMetrics) {
        if let Some(existing) = self.paths.iter_mut().find(|p| p.name == metrics.name) {
            *existing = metrics;
        } else {
            self.paths.push(metrics);
        }
    }

    /// Remove a path.
    pub fn remove_path(&mut self, name: &str) {
        self.paths.retain(|p| p.name != name);
    }

    /// Schedule a frame for sending. Returns path assignments.
    pub fn schedule(&mut self, is_critical: bool) -> Vec<ScheduledFrame> {
        let active: Vec<&PathMetrics> = self.paths.iter().filter(|p| p.active).collect();
        if active.is_empty() {
            return Vec::new();
        }

        self.sequence += 1;
        let seq = self.sequence;

        match &self.scheduler {
            MultipathScheduler::RoundRobin => {
                self.rr_index %= active.len();
                let path = active[self.rr_index].name.clone();
                self.rr_index += 1;
                let mut frames = vec![ScheduledFrame {
                    path,
                    sequence: seq,
                    redundant: false,
                }];
                if is_critical && self.redundant_critical {
                    self.add_redundant(&active, &mut frames, seq);
                }
                frames
            }
            MultipathScheduler::LowestLatency => {
                let best = active.iter().min_by_key(|p| p.rtt_ms)
                    .expect("LowestLatency scheduler called with non-empty active paths");
                let mut frames = vec![ScheduledFrame {
                    path: best.name.clone(),
                    sequence: seq,
                    redundant: false,
                }];
                if is_critical && self.redundant_critical {
                    self.add_redundant(&active, &mut frames, seq);
                }
                frames
            }
            MultipathScheduler::LatencyWeighted => {
                // Weighted selection: inverse RTT
                let total_inv_rtt: f64 = active
                    .iter()
                    .map(|p| 1.0 / (p.rtt_ms as f64).max(1.0))
                    .sum();
                let mut cumulative = 0.0;
                let threshold = (seq as f64 * 0.618033988) % 1.0; // Deterministic golden ratio selection
                let mut chosen = &active[0].name;
                for p in &active {
                    cumulative += (1.0 / (p.rtt_ms as f64).max(1.0)) / total_inv_rtt;
                    if cumulative >= threshold {
                        chosen = &p.name;
                        break;
                    }
                }
                let mut frames = vec![ScheduledFrame {
                    path: chosen.clone(),
                    sequence: seq,
                    redundant: false,
                }];
                if is_critical && self.redundant_critical {
                    self.add_redundant(&active, &mut frames, seq);
                }
                frames
            }
            MultipathScheduler::Redundant => {
                // Send on all paths
                active
                    .iter()
                    .enumerate()
                    .map(|(i, p)| ScheduledFrame {
                        path: p.name.clone(),
                        sequence: seq,
                        redundant: i > 0,
                    })
                    .collect()
            }
            MultipathScheduler::BandwidthWeighted => {
                let total_bw: u64 = active.iter().map(|p| p.bandwidth_bps).sum();
                if total_bw == 0 {
                    return vec![ScheduledFrame {
                        path: active[0].name.clone(),
                        sequence: seq,
                        redundant: false,
                    }];
                }
                let mut cumulative: u64 = 0;
                let threshold = seq % total_bw;
                let mut chosen = &active[0].name;
                for p in &active {
                    cumulative += p.bandwidth_bps;
                    if cumulative > threshold {
                        chosen = &p.name;
                        break;
                    }
                }
                let mut frames = vec![ScheduledFrame {
                    path: chosen.clone(),
                    sequence: seq,
                    redundant: false,
                }];
                if is_critical && self.redundant_critical {
                    self.add_redundant(&active, &mut frames, seq);
                }
                frames
            }
        }
    }

    /// Add redundant copies on all other paths.
    fn add_redundant(&self, active: &[&PathMetrics], frames: &mut Vec<ScheduledFrame>, seq: u64) {
        let primary = frames[0].path.clone();
        for p in active {
            if p.name != primary {
                frames.push(ScheduledFrame {
                    path: p.name.clone(),
                    sequence: seq,
                    redundant: true,
                });
            }
        }
    }

    /// Number of active paths.
    pub fn active_path_count(&self) -> usize {
        self.paths.iter().filter(|p| p.active).count()
    }

    /// Receiver-side deduplication: track seen sequences.
    pub fn dedup_filter(seen: &mut std::collections::HashSet<u64>, seq: u64) -> bool {
        seen.insert(seq) // Returns true if new (not duplicate)
    }
}

/// Censorship resistance tier (ordered from least to most aggressive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensorshipTier {
    /// Direct connection — no censorship resistance needed.
    Direct = 0,
    /// Relay-based — basic indirection.
    Relay = 1,
    /// Overlay network — WireGuard, mesh, additional encapsulation.
    Overlay = 2,
    /// Hostile environment — domain fronting, ECH, protocol mimicry.
    Hostile = 3,
    /// Maximum resistance — steganography, physical media, DTN.
    Maximum = 4,
}

/// Censorship escalation state machine.
///
/// Detects transport failures or tamper events and escalates through
/// increasingly censorship-resistant transport tiers automatically.
pub struct CensorshipEscalation {
    /// Current tier.
    current_tier: CensorshipTier,
    /// Maximum tier allowed by policy.
    max_tier: CensorshipTier,
    /// Consecutive failures at current tier.
    consecutive_failures: u32,
    /// Failure threshold before escalation.
    escalation_threshold: u32,
    /// Whether tamper was detected (immediate escalation).
    tamper_detected: bool,
    /// History of tier changes.
    history: Vec<(CensorshipTier, CensorshipTier, String)>,
}

impl CensorshipEscalation {
    /// Create a new escalation controller.
    pub fn new(max_tier: CensorshipTier, escalation_threshold: u32) -> Self {
        Self {
            current_tier: CensorshipTier::Direct,
            max_tier,
            consecutive_failures: 0,
            escalation_threshold,
            tamper_detected: false,
            history: Vec::new(),
        }
    }

    /// Record a connection success (resets failure counter).
    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// Record a connection failure. Returns the new tier if escalation occurred.
    pub fn on_failure(&mut self, reason: &str) -> Option<CensorshipTier> {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.escalation_threshold {
            self.escalate(reason)
        } else {
            None
        }
    }

    /// Record a tamper detection event. Always escalates immediately.
    pub fn on_tamper_detected(&mut self, reason: &str) -> Option<CensorshipTier> {
        self.tamper_detected = true;
        self.escalate(&format!("tamper: {reason}"))
    }

    /// Attempt to de-escalate one tier (e.g., after sustained success).
    pub fn de_escalate(&mut self) -> Option<CensorshipTier> {
        if self.tamper_detected {
            return None; // Never de-escalate after tamper
        }
        let prev = self.current_tier;
        match self.current_tier {
            CensorshipTier::Direct => None,
            CensorshipTier::Relay => {
                self.current_tier = CensorshipTier::Direct;
                self.consecutive_failures = 0;
                self.history
                    .push((prev, self.current_tier, "de-escalation".into()));
                Some(self.current_tier)
            }
            CensorshipTier::Overlay => {
                self.current_tier = CensorshipTier::Relay;
                self.consecutive_failures = 0;
                self.history
                    .push((prev, self.current_tier, "de-escalation".into()));
                Some(self.current_tier)
            }
            CensorshipTier::Hostile => {
                self.current_tier = CensorshipTier::Overlay;
                self.consecutive_failures = 0;
                self.history
                    .push((prev, self.current_tier, "de-escalation".into()));
                Some(self.current_tier)
            }
            CensorshipTier::Maximum => {
                self.current_tier = CensorshipTier::Hostile;
                self.consecutive_failures = 0;
                self.history
                    .push((prev, self.current_tier, "de-escalation".into()));
                Some(self.current_tier)
            }
        }
    }

    /// Escalate to next tier. Returns None if already at max.
    fn escalate(&mut self, reason: &str) -> Option<CensorshipTier> {
        let prev = self.current_tier;
        let next = match self.current_tier {
            CensorshipTier::Direct => CensorshipTier::Relay,
            CensorshipTier::Relay => CensorshipTier::Overlay,
            CensorshipTier::Overlay => CensorshipTier::Hostile,
            CensorshipTier::Hostile => CensorshipTier::Maximum,
            CensorshipTier::Maximum => return None,
        };

        if next > self.max_tier {
            return None;
        }

        self.current_tier = next;
        self.consecutive_failures = 0;
        self.history.push((prev, next, reason.to_string()));
        Some(next)
    }

    /// Current tier.
    pub fn current_tier(&self) -> CensorshipTier {
        self.current_tier
    }

    /// Whether tamper has been detected.
    pub fn is_tampered(&self) -> bool {
        self.tamper_detected
    }

    /// Escalation history.
    pub fn history(&self) -> &[(CensorshipTier, CensorshipTier, String)] {
        &self.history
    }
}

// --- Announce-Flood Protocol ---

/// Announce message for flood-fill discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceMessage {
    /// Announcing node ID.
    pub node_id: String,
    /// Transport address(es) of the announcing node.
    pub addresses: Vec<String>,
    /// Announce sequence number (monotonically increasing per node).
    pub sequence: u64,
    /// Timestamp of this announcement (Unix ms).
    pub timestamp_ms: u64,
    /// TTL: remaining hops this announce can traverse.
    pub ttl: u8,
    /// Capabilities offered by this node.
    pub capabilities: Vec<String>,
}

/// Announce-flood protocol — gossip-based node discovery.
///
/// Incoming announces are deduplicated, rate-limited, and re-broadcast
/// to neighbors with decremented TTL.
pub struct AnnounceFlood {
    /// Known announces: node_id → highest sequence seen.
    seen: HashMap<String, u64>,
    /// Rate limit: max announces per node per window.
    max_per_node: u32,
    /// Counters for rate limiting: node_id → count in current window.
    counters: HashMap<String, u32>,
    /// Collected peer announcements.
    peers: HashMap<String, AnnounceMessage>,
}

impl AnnounceFlood {
    /// Create a new announce-flood handler.
    pub fn new(max_per_node: u32) -> Self {
        Self {
            seen: HashMap::new(),
            max_per_node,
            counters: HashMap::new(),
            peers: HashMap::new(),
        }
    }

    /// Process an incoming announce message.
    /// Returns Some(message with decremented TTL) if it should be re-broadcast.
    pub fn process(&mut self, announce: AnnounceMessage) -> Option<AnnounceMessage> {
        // Deduplication: skip if we've seen equal or higher sequence from this node
        if let Some(&last_seq) = self.seen.get(&announce.node_id) {
            if announce.sequence <= last_seq {
                return None;
            }
        }

        // Rate limiting
        let count = self.counters.entry(announce.node_id.clone()).or_insert(0);
        if *count >= self.max_per_node {
            return None;
        }
        *count += 1;

        // Record this announce
        self.seen
            .insert(announce.node_id.clone(), announce.sequence);
        self.peers
            .insert(announce.node_id.clone(), announce.clone());

        // Re-broadcast with decremented TTL
        if announce.ttl > 1 {
            Some(AnnounceMessage {
                ttl: announce.ttl - 1,
                ..announce
            })
        } else {
            None // TTL exhausted — do not re-broadcast
        }
    }

    /// Get all known peers.
    pub fn known_peers(&self) -> Vec<&AnnounceMessage> {
        self.peers.values().collect()
    }

    /// Reset rate limit counters (call at window boundary).
    pub fn reset_counters(&mut self) {
        self.counters.clear();
    }

    /// Create an announce message for the local node.
    pub fn create_announce(
        node_id: String,
        addresses: Vec<String>,
        sequence: u64,
        timestamp_ms: u64,
        ttl: u8,
        capabilities: Vec<String>,
    ) -> AnnounceMessage {
        AnnounceMessage {
            node_id,
            addresses,
            sequence,
            timestamp_ms,
            ttl,
            capabilities,
        }
    }

    /// Number of known peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
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

    #[test]
    fn test_node_id_distance() {
        let a = NodeId::from_bytes([0u8; 32]);
        let mut b_bytes = [0u8; 32];
        b_bytes[31] = 1;
        let b = NodeId::from_bytes(b_bytes);

        let dist = a.distance(&b);
        assert_eq!(dist[31], 1);
        assert_eq!(&dist[..31], &[0u8; 31]);
    }

    #[test]
    fn test_node_id_bucket_index() {
        let a = NodeId::from_bytes([0u8; 32]);
        let mut b_bytes = [0u8; 32];
        b_bytes[0] = 0x80; // Most significant bit differs
        let b = NodeId::from_bytes(b_bytes);

        assert_eq!(a.bucket_index(&b), 0); // Closest to root
    }

    #[test]
    fn test_kademlia_insert_and_closest() {
        let local = NodeId::from_bytes([0u8; 32]);
        let mut table = KademliaTable::new(local, 20);

        for i in 1u8..=5 {
            let mut id = [0u8; 32];
            id[31] = i;
            table.insert(DhtNode {
                id: NodeId::from_bytes(id),
                addr: format!("10.0.0.{i}:9000"),
                last_seen_ms: 1000,
            });
        }

        assert_eq!(table.len(), 5);

        // Find 3 closest to [0; 32] (which is our local ID)
        let mut target = [0u8; 32];
        target[31] = 2;
        let closest = table.closest(&NodeId::from_bytes(target), 3);
        assert_eq!(closest.len(), 3);
        // First should be the one with id[31]=2 (distance 0)
        assert_eq!(closest[0].id.0[31], 2);
    }

    #[test]
    fn test_kademlia_remove() {
        let local = NodeId::from_bytes([0u8; 32]);
        let mut table = KademliaTable::new(local, 20);

        let mut id = [0u8; 32];
        id[31] = 1;
        table.insert(DhtNode {
            id: NodeId::from_bytes(id),
            addr: "10.0.0.1:9000".into(),
            last_seen_ms: 1000,
        });

        assert_eq!(table.len(), 1);
        assert!(table.remove(&NodeId::from_bytes(id)));
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_kademlia_bucket_full() {
        let local = NodeId::from_bytes([0u8; 32]);
        let mut table = KademliaTable::new(local, 2); // k=2

        // Insert 3 nodes that all differ only in the last byte's low bits.
        // id[0] = 0x80 for all three so they share the same bucket (index 0).
        for i in 0u8..3 {
            let mut id = [0u8; 32];
            id[0] = 0x80;
            id[31] = i;
            let inserted = table.insert(DhtNode {
                id: NodeId::from_bytes(id),
                addr: format!("10.0.0.{i}:9000"),
                last_seen_ms: 1000,
            });
            if i < 2 {
                assert!(inserted);
            } else {
                assert!(!inserted); // Bucket full
            }
        }

        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_hop_forwarder_direct() {
        let mut fwd = HopForwarder::new("node-a".into());
        fwd.add_neighbor("node-b".into());
        fwd.add_neighbor("node-c".into());

        let decision = fwd.decide("node-b", 0, 10);
        assert_eq!(
            decision,
            ForwardDecision::Direct {
                next_hop: "node-b".into()
            }
        );
    }

    #[test]
    fn test_hop_forwarder_relay() {
        let mut fwd = HopForwarder::new("node-a".into());
        fwd.add_neighbor("node-b".into());

        let decision = fwd.decide("node-z", 0, 10);
        assert!(matches!(decision, ForwardDecision::Relay { .. }));
    }

    #[test]
    fn test_hop_forwarder_store() {
        let fwd = HopForwarder::new("node-a".into());
        let decision = fwd.decide("node-z", 0, 10);
        assert_eq!(decision, ForwardDecision::Store);
    }

    #[test]
    fn test_hop_forwarder_max_hops() {
        let fwd = HopForwarder::new("node-a".into());
        let decision = fwd.decide("node-z", 5, 5);
        assert!(matches!(decision, ForwardDecision::Drop { .. }));
    }

    #[test]
    fn test_interface_migration_auto() {
        let mut mig =
            InterfaceMigration::new(true).with_preferred(vec!["en".into(), "wlan".into()]);

        // Initial setup
        let result = mig.on_interface_change(&["en0".into(), "wwan0".into()]);
        assert_eq!(result, Some("en0".into()));
        assert_eq!(mig.active_interface(), Some("en0"));

        // WiFi drops, cellular only
        let result = mig.on_interface_change(&["wwan0".into()]);
        assert_eq!(result, Some("wwan0".into()));
        assert_eq!(mig.migration_count(), 1);
    }

    #[test]
    fn test_interface_migration_no_change() {
        let mut mig = InterfaceMigration::new(true);
        mig.set_active("en0".into());

        // Same interface still available
        let result = mig.on_interface_change(&["en0".into(), "lo0".into()]);
        assert!(result.is_none());
        assert_eq!(mig.migration_count(), 0);
    }

    #[test]
    fn test_interface_migration_prefers_pattern() {
        let mut mig = InterfaceMigration::new(true).with_preferred(vec!["wlan".into()]);

        let result = mig.on_interface_change(&["eth0".into(), "wlan0".into()]);
        assert_eq!(result, Some("wlan0".into()));
    }

    fn make_path(name: &str, rtt: u32, bw: u64) -> PathMetrics {
        PathMetrics {
            name: name.into(),
            rtt_ms: rtt,
            bandwidth_bps: bw,
            loss_rate: 0.0,
            active: true,
        }
    }

    #[test]
    fn test_multipath_round_robin() {
        let config = MultipathConfig {
            scheduler: MultipathScheduler::RoundRobin,
            redundant_critical: false,
            ..MultipathConfig::default()
        };
        let mut sched = MultipathFrameScheduler::new(&config);
        sched.update_path(make_path("quic", 10, 1_000_000));
        sched.update_path(make_path("ws", 50, 500_000));

        let f1 = sched.schedule(false);
        let f2 = sched.schedule(false);
        assert_eq!(f1.len(), 1);
        assert_eq!(f2.len(), 1);
        assert_ne!(f1[0].path, f2[0].path);
    }

    #[test]
    fn test_multipath_lowest_latency() {
        let config = MultipathConfig {
            scheduler: MultipathScheduler::LowestLatency,
            redundant_critical: false,
            ..MultipathConfig::default()
        };
        let mut sched = MultipathFrameScheduler::new(&config);
        sched.update_path(make_path("quic", 10, 1_000_000));
        sched.update_path(make_path("ws", 50, 500_000));

        let frames = sched.schedule(false);
        assert_eq!(frames[0].path, "quic"); // Lowest RTT
    }

    #[test]
    fn test_multipath_redundant() {
        let config = MultipathConfig {
            scheduler: MultipathScheduler::Redundant,
            ..MultipathConfig::default()
        };
        let mut sched = MultipathFrameScheduler::new(&config);
        sched.update_path(make_path("quic", 10, 1_000_000));
        sched.update_path(make_path("ws", 50, 500_000));

        let frames = sched.schedule(false);
        assert_eq!(frames.len(), 2);
        assert!(!frames[0].redundant);
        assert!(frames[1].redundant);
        assert_eq!(frames[0].sequence, frames[1].sequence);
    }

    #[test]
    fn test_multipath_critical_redundancy() {
        let config = MultipathConfig {
            scheduler: MultipathScheduler::LowestLatency,
            redundant_critical: true,
            ..MultipathConfig::default()
        };
        let mut sched = MultipathFrameScheduler::new(&config);
        sched.update_path(make_path("quic", 10, 1_000_000));
        sched.update_path(make_path("ws", 50, 500_000));

        // Non-critical: single path
        let frames = sched.schedule(false);
        assert_eq!(frames.len(), 1);

        // Critical: all paths
        let frames = sched.schedule(true);
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn test_multipath_dedup() {
        let mut seen = std::collections::HashSet::new();
        assert!(MultipathFrameScheduler::dedup_filter(&mut seen, 1));
        assert!(MultipathFrameScheduler::dedup_filter(&mut seen, 2));
        assert!(!MultipathFrameScheduler::dedup_filter(&mut seen, 1)); // Duplicate
    }

    #[test]
    fn test_multipath_no_active_paths() {
        let config = MultipathConfig::default();
        let mut sched = MultipathFrameScheduler::new(&config);
        let frames = sched.schedule(false);
        assert!(frames.is_empty());
    }

    #[test]
    fn test_censorship_escalation_on_failure() {
        let mut esc = CensorshipEscalation::new(CensorshipTier::Maximum, 3);
        assert_eq!(esc.current_tier(), CensorshipTier::Direct);

        // 2 failures — no escalation
        assert!(esc.on_failure("timeout").is_none());
        assert!(esc.on_failure("timeout").is_none());
        // 3rd failure — escalation
        let new_tier = esc.on_failure("timeout");
        assert_eq!(new_tier, Some(CensorshipTier::Relay));
        assert_eq!(esc.current_tier(), CensorshipTier::Relay);
    }

    #[test]
    fn test_censorship_escalation_tamper() {
        let mut esc = CensorshipEscalation::new(CensorshipTier::Maximum, 3);
        // Tamper always escalates immediately
        let new_tier = esc.on_tamper_detected("MITM detected");
        assert_eq!(new_tier, Some(CensorshipTier::Relay));
        assert!(esc.is_tampered());
    }

    #[test]
    fn test_censorship_escalation_max_tier() {
        let mut esc = CensorshipEscalation::new(CensorshipTier::Relay, 1);
        let _ = esc.on_failure("blocked");
        assert_eq!(esc.current_tier(), CensorshipTier::Relay);

        // Already at max_tier (Relay), cannot escalate further
        assert!(esc.on_failure("blocked").is_none());
    }

    #[test]
    fn test_censorship_de_escalation() {
        let mut esc = CensorshipEscalation::new(CensorshipTier::Maximum, 1);
        esc.on_failure("timeout");
        assert_eq!(esc.current_tier(), CensorshipTier::Relay);

        esc.on_success();
        let de = esc.de_escalate();
        assert_eq!(de, Some(CensorshipTier::Direct));
    }

    #[test]
    fn test_censorship_no_de_escalation_after_tamper() {
        let mut esc = CensorshipEscalation::new(CensorshipTier::Maximum, 1);
        esc.on_tamper_detected("injection");
        assert!(esc.de_escalate().is_none()); // Never de-escalate after tamper
    }

    #[test]
    fn test_announce_flood_basic() {
        let mut flood = AnnounceFlood::new(10);

        let announce = AnnounceFlood::create_announce(
            "node-a".into(),
            vec!["10.0.0.1:9000".into()],
            1,
            1000,
            3,
            vec!["relay".into()],
        );

        let rebroadcast = flood.process(announce);
        assert!(rebroadcast.is_some());
        let rb = rebroadcast.unwrap();
        assert_eq!(rb.ttl, 2); // Decremented
        assert_eq!(rb.node_id, "node-a");
        assert_eq!(flood.peer_count(), 1);
    }

    #[test]
    fn test_announce_flood_dedup() {
        let mut flood = AnnounceFlood::new(10);

        let announce = AnnounceFlood::create_announce(
            "node-a".into(),
            vec!["10.0.0.1:9000".into()],
            1,
            1000,
            3,
            vec![],
        );

        flood.process(announce.clone());
        // Same sequence — deduplicated
        assert!(flood.process(announce).is_none());
        assert_eq!(flood.peer_count(), 1);

        // Higher sequence — accepted
        let announce2 = AnnounceFlood::create_announce(
            "node-a".into(),
            vec!["10.0.0.2:9000".into()],
            2,
            2000,
            3,
            vec![],
        );
        assert!(flood.process(announce2).is_some());
    }

    #[test]
    fn test_announce_flood_ttl_exhausted() {
        let mut flood = AnnounceFlood::new(10);
        let announce = AnnounceFlood::create_announce(
            "node-a".into(),
            vec![],
            1,
            1000,
            1, // TTL=1 → no rebroadcast
            vec![],
        );
        assert!(flood.process(announce).is_none());
        assert_eq!(flood.peer_count(), 1); // Still recorded
    }

    #[test]
    fn test_announce_flood_rate_limit() {
        let mut flood = AnnounceFlood::new(2);

        for seq in 1..=3 {
            flood.process(AnnounceFlood::create_announce(
                "node-a".into(),
                vec![],
                seq,
                1000,
                3,
                vec![],
            ));
        }
        // 3rd should be rate-limited
        assert_eq!(flood.peer_count(), 1); // Still same node
        // Reset counters
        flood.reset_counters();
        let result = flood.process(AnnounceFlood::create_announce(
            "node-a".into(),
            vec![],
            4,
            2000,
            3,
            vec![],
        ));
        assert!(result.is_some()); // Accepted after reset
    }
}
