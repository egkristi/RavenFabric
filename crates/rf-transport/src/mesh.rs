//! Mesh IP allocation and MagicDNS.
//!
//! Derives deterministic IPv6 addresses from agent public keys,
//! provides a petname system (human-readable ↔ cryptographic ID),
//! and resolves agent-name.rf.local to mesh IPs.

use std::collections::HashMap;
use std::net::Ipv6Addr;

/// Mesh network configuration.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// IPv6 prefix for the mesh (default: fd00:rvnf::/32 — ULA).
    pub prefix: [u8; 4],
    /// DNS suffix (default: "rf.local").
    pub dns_suffix: String,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            prefix: [0xfd, 0x00, 0x52, 0x56], // fd00:5256 ("RV")
            dns_suffix: "rf.local".to_string(),
        }
    }
}

/// Derive a deterministic IPv6 address from a public key.
///
/// Uses first 4 bytes as prefix, remaining 12 bytes from BLAKE2 hash of the key.
/// This ensures stable addresses that don't change unless the key changes.
pub fn derive_mesh_ip(config: &MeshConfig, public_key: &[u8; 32]) -> Ipv6Addr {
    // Simple hash: XOR-fold the 32-byte key into 12 bytes for the suffix
    let mut suffix = [0u8; 12];
    for (i, &byte) in public_key.iter().enumerate() {
        suffix[i % 12] ^= byte;
    }

    let segments: [u16; 8] = [
        u16::from_be_bytes([config.prefix[0], config.prefix[1]]),
        u16::from_be_bytes([config.prefix[2], config.prefix[3]]),
        u16::from_be_bytes([suffix[0], suffix[1]]),
        u16::from_be_bytes([suffix[2], suffix[3]]),
        u16::from_be_bytes([suffix[4], suffix[5]]),
        u16::from_be_bytes([suffix[6], suffix[7]]),
        u16::from_be_bytes([suffix[8], suffix[9]]),
        u16::from_be_bytes([suffix[10], suffix[11]]),
    ];

    Ipv6Addr::new(
        segments[0],
        segments[1],
        segments[2],
        segments[3],
        segments[4],
        segments[5],
        segments[6],
        segments[7],
    )
}

/// A petname entry (human-readable name ↔ cryptographic identity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetnameEntry {
    /// Human-readable local name (e.g., "web-01", "kitchen-pi").
    pub name: String,
    /// Agent's public key (hex-encoded).
    pub public_key_hex: String,
    /// Derived mesh IPv6 address.
    pub mesh_ip: Ipv6Addr,
    /// Optional group/label.
    pub group: Option<String>,
}

/// DNS record for MagicDNS resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRecord {
    /// Fully qualified domain name (e.g., "web-01.rf.local").
    pub fqdn: String,
    /// IPv6 address (AAAA record).
    pub addr: Ipv6Addr,
    /// TTL in seconds.
    pub ttl: u32,
}

/// MagicDNS resolver — maps agent names to mesh IPs.
#[derive(Debug)]
pub struct MeshDns {
    config: MeshConfig,
    /// Name → entry mapping.
    entries: HashMap<String, PetnameEntry>,
    /// Public key (hex) → name reverse mapping.
    key_to_name: HashMap<String, String>,
}

impl MeshDns {
    pub fn new(config: MeshConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            key_to_name: HashMap::new(),
        }
    }

    /// Register an agent with a petname.
    pub fn register(&mut self, name: &str, public_key_hex: &str, group: Option<String>) {
        // Derive IP from key bytes (use first 32 hex chars)
        let key_bytes = hex_to_key(public_key_hex);
        let mesh_ip = derive_mesh_ip(&self.config, &key_bytes);

        let entry = PetnameEntry {
            name: name.to_string(),
            public_key_hex: public_key_hex.to_string(),
            mesh_ip,
            group,
        };

        self.key_to_name
            .insert(public_key_hex.to_string(), name.to_string());
        self.entries.insert(name.to_string(), entry);
    }

    /// Unregister an agent.
    pub fn unregister(&mut self, name: &str) {
        if let Some(entry) = self.entries.remove(name) {
            self.key_to_name.remove(&entry.public_key_hex);
        }
    }

    /// Resolve a name to a DNS record.
    pub fn resolve(&self, query: &str) -> Option<DnsRecord> {
        // Strip the DNS suffix if present
        let name = query
            .strip_suffix(&format!(".{}", self.config.dns_suffix))
            .unwrap_or(query);

        self.entries.get(name).map(|entry| DnsRecord {
            fqdn: format!("{}.{}", entry.name, self.config.dns_suffix),
            addr: entry.mesh_ip,
            ttl: 60,
        })
    }

    /// Resolve a public key to a name.
    pub fn resolve_key(&self, public_key_hex: &str) -> Option<&str> {
        self.key_to_name.get(public_key_hex).map(|s| s.as_str())
    }

    /// Get all entries in a group.
    pub fn group_members(&self, group: &str) -> Vec<&PetnameEntry> {
        self.entries
            .values()
            .filter(|e| e.group.as_deref() == Some(group))
            .collect()
    }

    /// Number of registered agents.
    pub fn agent_count(&self) -> usize {
        self.entries.len()
    }

    /// Get all DNS records (for zone transfer / full listing).
    pub fn all_records(&self) -> Vec<DnsRecord> {
        self.entries
            .values()
            .map(|entry| DnsRecord {
                fqdn: format!("{}.{}", entry.name, self.config.dns_suffix),
                addr: entry.mesh_ip,
                ttl: 60,
            })
            .collect()
    }
}

/// Convert hex string to 32-byte key (zero-padded if short).
fn hex_to_key(hex: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 2 <= hex.len() {
                u8::from_str_radix(&hex[i..i + 2], 16).ok()
            } else {
                None
            }
        })
        .collect();
    let copy_len = bytes.len().min(32);
    key[..copy_len].copy_from_slice(&bytes[..copy_len]);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_mesh_ip_deterministic() {
        let config = MeshConfig::default();
        let key = [0xAB; 32];
        let ip1 = derive_mesh_ip(&config, &key);
        let ip2 = derive_mesh_ip(&config, &key);
        assert_eq!(ip1, ip2);
    }

    #[test]
    fn test_derive_mesh_ip_prefix() {
        let config = MeshConfig::default();
        let key = [0x42; 32];
        let ip = derive_mesh_ip(&config, &key);
        let segments = ip.segments();
        assert_eq!(segments[0], 0xfd00);
        assert_eq!(segments[1], 0x5256);
    }

    #[test]
    fn test_different_keys_different_ips() {
        let config = MeshConfig::default();
        let key1 = [0x01; 32];
        let key2 = [0x02; 32];
        let ip1 = derive_mesh_ip(&config, &key1);
        let ip2 = derive_mesh_ip(&config, &key2);
        assert_ne!(ip1, ip2);
    }

    #[test]
    fn test_mesh_dns_register_resolve() {
        let mut dns = MeshDns::new(MeshConfig::default());
        dns.register(
            "web-01",
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            None,
        );

        let record = dns.resolve("web-01").unwrap();
        assert_eq!(record.fqdn, "web-01.rf.local");
        assert_eq!(record.ttl, 60);
    }

    #[test]
    fn test_mesh_dns_resolve_with_suffix() {
        let mut dns = MeshDns::new(MeshConfig::default());
        dns.register(
            "db-01",
            "1111111111111111111111111111111111111111111111111111111111111111",
            None,
        );

        let record = dns.resolve("db-01.rf.local").unwrap();
        assert_eq!(record.fqdn, "db-01.rf.local");
    }

    #[test]
    fn test_mesh_dns_groups() {
        let mut dns = MeshDns::new(MeshConfig::default());
        dns.register("web-01", "aa".repeat(32).as_str(), Some("web".to_string()));
        dns.register("web-02", "bb".repeat(32).as_str(), Some("web".to_string()));
        dns.register("db-01", "cc".repeat(32).as_str(), Some("db".to_string()));

        let web_members = dns.group_members("web");
        assert_eq!(web_members.len(), 2);
    }

    #[test]
    fn test_mesh_dns_unregister() {
        let mut dns = MeshDns::new(MeshConfig::default());
        dns.register("temp", "dd".repeat(32).as_str(), None);
        assert_eq!(dns.agent_count(), 1);

        dns.unregister("temp");
        assert_eq!(dns.agent_count(), 0);
        assert!(dns.resolve("temp").is_none());
    }

    #[test]
    fn test_resolve_key() {
        let mut dns = MeshDns::new(MeshConfig::default());
        let key = "ee".repeat(32);
        dns.register("my-agent", &key, None);

        assert_eq!(dns.resolve_key(&key), Some("my-agent"));
        assert_eq!(dns.resolve_key("unknown"), None);
    }
}
