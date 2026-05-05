//! Mesh IP allocation and MagicDNS.
//!
//! Derives deterministic IPv6 addresses from agent public keys,
//! provides a petname system (human-readable ↔ cryptographic ID),
//! and resolves agent-name.rf.local to mesh IPs.
//! Includes a UDP DNS server for local resolution.

use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

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

/// MagicDNS UDP server — listens on a local port and resolves queries
/// for `*.rf.local` to mesh IPv6 addresses.
pub struct DnsServer {
    resolver: Arc<RwLock<MeshDns>>,
    socket: UdpSocket,
}

/// DNS header flags and constants.
const DNS_FLAG_RESPONSE: u16 = 0x8000;
const DNS_FLAG_AA: u16 = 0x0400; // Authoritative Answer
const DNS_RCODE_NXDOMAIN: u16 = 0x0003;
const DNS_TYPE_AAAA: u16 = 28;
const DNS_CLASS_IN: u16 = 1;

impl DnsServer {
    /// Bind a DNS server to the given address (e.g., "127.0.0.1:5353").
    pub async fn bind(addr: &str, resolver: Arc<RwLock<MeshDns>>) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self { resolver, socket })
    }

    /// Get the local address this server is bound to.
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.socket.local_addr()
    }

    /// Run the DNS server loop. Processes one query per call (for testing).
    /// In production, call this in a loop or use `serve()`.
    pub async fn handle_one(&self) -> std::io::Result<()> {
        let mut buf = [0u8; 512];
        let (len, src) = self.socket.recv_from(&mut buf).await?;
        if len < 12 {
            return Ok(()); // Too short for a DNS header
        }

        let response = self.process_query(&buf[..len]).await;
        self.socket.send_to(&response, src).await?;
        Ok(())
    }

    /// Process a raw DNS query and return the response bytes.
    async fn process_query(&self, query: &[u8]) -> Vec<u8> {
        // Parse DNS header: ID (2), Flags (2), QDCOUNT (2), ...
        let id = u16::from_be_bytes([query[0], query[1]]);
        let qdcount = u16::from_be_bytes([query[4], query[5]]);

        if qdcount == 0 {
            return self.build_error_response(id, DNS_RCODE_NXDOMAIN);
        }

        // Parse the question section
        let (qname, offset) = match Self::parse_qname(query, 12) {
            Some(v) => v,
            None => return self.build_error_response(id, DNS_RCODE_NXDOMAIN),
        };

        if offset + 4 > query.len() {
            return self.build_error_response(id, DNS_RCODE_NXDOMAIN);
        }

        let qtype = u16::from_be_bytes([query[offset], query[offset + 1]]);
        let _qclass = u16::from_be_bytes([query[offset + 2], query[offset + 3]]);

        // Only handle AAAA queries
        if qtype != DNS_TYPE_AAAA {
            return self.build_error_response(id, DNS_RCODE_NXDOMAIN);
        }

        // Look up in MeshDns
        let resolver = self.resolver.read().await;
        match resolver.resolve(&qname) {
            Some(record) => self.build_aaaa_response(id, query, offset + 4, &record),
            None => self.build_error_response(id, DNS_RCODE_NXDOMAIN),
        }
    }

    /// Parse a DNS name from wire format (label-length encoding).
    fn parse_qname(buf: &[u8], mut pos: usize) -> Option<(String, usize)> {
        let mut labels = Vec::new();
        loop {
            if pos >= buf.len() {
                return None;
            }
            let len = buf[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if len >= 64 {
                // Compression pointer — not supported for queries
                return None;
            }
            pos += 1;
            if pos + len > buf.len() {
                return None;
            }
            labels.push(String::from_utf8_lossy(&buf[pos..pos + len]).to_string());
            pos += len;
        }
        Some((labels.join("."), pos))
    }

    /// Build an AAAA response.
    fn build_aaaa_response(
        &self,
        id: u16,
        query: &[u8],
        question_end: usize,
        record: &DnsRecord,
    ) -> Vec<u8> {
        let mut resp = Vec::with_capacity(128);

        // Header: ID, Flags (response + AA), QDCOUNT=1, ANCOUNT=1, NSCOUNT=0, ARCOUNT=0
        resp.extend_from_slice(&id.to_be_bytes());
        resp.extend_from_slice(&(DNS_FLAG_RESPONSE | DNS_FLAG_AA).to_be_bytes());
        resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        resp.extend_from_slice(&1u16.to_be_bytes()); // ANCOUNT
        resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

        // Copy the question section
        resp.extend_from_slice(&query[12..question_end]);

        // Answer: pointer to question name (0xC00C), TYPE=AAAA, CLASS=IN, TTL, RDLEN=16, RDATA
        resp.extend_from_slice(&[0xC0, 0x0C]); // Name pointer to offset 12
        resp.extend_from_slice(&DNS_TYPE_AAAA.to_be_bytes());
        resp.extend_from_slice(&DNS_CLASS_IN.to_be_bytes());
        resp.extend_from_slice(&record.ttl.to_be_bytes());
        resp.extend_from_slice(&16u16.to_be_bytes()); // RDLENGTH
        resp.extend_from_slice(&record.addr.octets()); // 16 bytes IPv6

        resp
    }

    /// Build an NXDOMAIN or error response.
    fn build_error_response(&self, id: u16, rcode: u16) -> Vec<u8> {
        let mut resp = Vec::with_capacity(12);
        resp.extend_from_slice(&id.to_be_bytes());
        resp.extend_from_slice(&(DNS_FLAG_RESPONSE | DNS_FLAG_AA | rcode).to_be_bytes());
        resp.extend_from_slice(&0u16.to_be_bytes()); // QDCOUNT
        resp.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
        resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
        resp
    }
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

    #[tokio::test]
    async fn test_dns_server_resolves_aaaa() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        // Set up resolver with a registered agent
        let mut dns = MeshDns::new(MeshConfig::default());
        dns.register(
            "web-01",
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            None,
        );
        let resolver = Arc::new(RwLock::new(dns));

        // Start DNS server on random port
        let server = DnsServer::bind("127.0.0.1:0", resolver.clone())
            .await
            .unwrap();
        let server_addr = server.local_addr().unwrap();

        // Build a DNS AAAA query for "web-01.rf.local"
        let mut query = Vec::new();
        query.extend_from_slice(&[0x12, 0x34]); // ID
        query.extend_from_slice(&[0x01, 0x00]); // Flags (standard query)
        query.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
        query.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
        query.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
        query.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0
        // QNAME: web-01.rf.local
        query.push(6);
        query.extend_from_slice(b"web-01");
        query.push(2);
        query.extend_from_slice(b"rf");
        query.push(5);
        query.extend_from_slice(b"local");
        query.push(0); // End of name
        query.extend_from_slice(&28u16.to_be_bytes()); // QTYPE = AAAA
        query.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN

        // Send query from client socket
        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(&query, server_addr).await.unwrap();

        // Server handles one query
        server.handle_one().await.unwrap();

        // Receive response
        let mut resp_buf = [0u8; 512];
        let (resp_len, _) = client.recv_from(&mut resp_buf).await.unwrap();

        // Verify response
        assert!(resp_len >= 12 + 16); // Header + at least an answer
        // Check ID matches
        assert_eq!(resp_buf[0], 0x12);
        assert_eq!(resp_buf[1], 0x34);
        // Check flags: response bit set
        assert!(resp_buf[2] & 0x80 != 0);
        // Check ANCOUNT = 1
        assert_eq!(u16::from_be_bytes([resp_buf[6], resp_buf[7]]), 1);
    }

    #[tokio::test]
    async fn test_dns_server_nxdomain_for_unknown() {
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let dns = MeshDns::new(MeshConfig::default());
        let resolver = Arc::new(RwLock::new(dns));

        let server = DnsServer::bind("127.0.0.1:0", resolver).await.unwrap();
        let server_addr = server.local_addr().unwrap();

        // Query for non-existent name
        let mut query = Vec::new();
        query.extend_from_slice(&[0xAB, 0xCD]); // ID
        query.extend_from_slice(&[0x01, 0x00]); // Flags
        query.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
        query.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        query.push(7);
        query.extend_from_slice(b"unknown");
        query.push(2);
        query.extend_from_slice(b"rf");
        query.push(5);
        query.extend_from_slice(b"local");
        query.push(0);
        query.extend_from_slice(&28u16.to_be_bytes()); // AAAA
        query.extend_from_slice(&1u16.to_be_bytes()); // IN

        let client = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.send_to(&query, server_addr).await.unwrap();
        server.handle_one().await.unwrap();

        let mut resp_buf = [0u8; 512];
        let (resp_len, _) = client.recv_from(&mut resp_buf).await.unwrap();

        // Check NXDOMAIN response
        assert!(resp_len >= 12);
        assert_eq!(resp_buf[0], 0xAB);
        assert_eq!(resp_buf[1], 0xCD);
        // RCODE = 3 (NXDOMAIN)
        assert_eq!(resp_buf[3] & 0x0F, 3);
        // ANCOUNT = 0
        assert_eq!(u16::from_be_bytes([resp_buf[6], resp_buf[7]]), 0);
    }
}
