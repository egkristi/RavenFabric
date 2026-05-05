//! WireGuard userspace integration types.
//!
//! Defines configuration and state for userspace WireGuard (boringtun-compatible).

use serde::{Deserialize, Serialize};

/// WireGuard peer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgPeerConfig {
    /// Peer's public key (base64 encoded, 32 bytes).
    pub public_key: String,
    /// Optional pre-shared key for post-quantum resistance.
    pub preshared_key: Option<String>,
    /// Peer endpoint (IP:port).
    pub endpoint: Option<String>,
    /// Allowed IP ranges for this peer.
    pub allowed_ips: Vec<String>,
    /// Keepalive interval in seconds (0 = disabled).
    pub persistent_keepalive_secs: u16,
}

/// WireGuard interface configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WgInterfaceConfig {
    /// Private key (base64 encoded).
    pub private_key: String,
    /// Listen port (0 = random).
    pub listen_port: u16,
    /// Interface address (CIDR).
    pub address: String,
    /// DNS servers to configure.
    pub dns: Vec<String>,
    /// MTU (default 1420).
    pub mtu: u16,
    /// Configured peers.
    pub peers: Vec<WgPeerConfig>,
}

impl Default for WgInterfaceConfig {
    fn default() -> Self {
        Self {
            private_key: String::new(),
            listen_port: 0,
            address: String::new(),
            dns: Vec::new(),
            mtu: 1420,
            peers: Vec::new(),
        }
    }
}

/// WireGuard handshake state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WgHandshakeState {
    /// No handshake initiated.
    None,
    /// Initiation message sent.
    InitSent,
    /// Response received, handshake complete.
    Complete,
    /// Handshake expired (needs re-key).
    Expired,
}

/// Per-peer statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WgPeerStats {
    /// Last handshake timestamp (Unix seconds).
    pub last_handshake: u64,
    /// Bytes received from this peer.
    pub rx_bytes: u64,
    /// Bytes sent to this peer.
    pub tx_bytes: u64,
    /// Current handshake state.
    pub handshake_state: WgHandshakeState,
}

impl Default for WgHandshakeState {
    fn default() -> Self {
        Self::None
    }
}

/// Corporate proxy detection and configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorporateProxy {
    /// Proxy type.
    pub protocol: CorporateProxyProtocol,
    /// Proxy address (host:port).
    pub address: String,
    /// Authentication credentials.
    pub auth: Option<ProxyAuth>,
    /// Custom headers to add (for HTTP CONNECT).
    pub headers: Vec<(String, String)>,
    /// CONNECT tunnel target.
    pub connect_target: Option<String>,
}

/// Corporate proxy protocol types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorporateProxyProtocol {
    /// HTTP CONNECT tunnel (most corporate proxies).
    HttpConnect,
    /// Authenticated HTTPS proxy.
    HttpsConnect,
    /// SOCKS5 (some corporate environments).
    Socks5,
    /// PAC (Proxy Auto-Configuration) script.
    Pac { url: String },
    /// WPAD (Web Proxy Auto-Discovery).
    Wpad,
}

/// Proxy authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuth {
    /// Username.
    pub username: String,
    /// Password (should be loaded from secure storage).
    pub password: String,
    /// NTLM domain (for Windows corporate environments).
    pub ntlm_domain: Option<String>,
}

/// Birthday paradox port prediction for symmetric NAT traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BirthdayPortPrediction {
    /// Number of simultaneous binding attempts.
    pub num_attempts: u16,
    /// Port range start for prediction window.
    pub port_range_start: u16,
    /// Port range end for prediction window.
    pub port_range_end: u16,
    /// Time window for simultaneous attempts (ms).
    pub window_ms: u32,
}

impl Default for BirthdayPortPrediction {
    fn default() -> Self {
        Self {
            num_attempts: 256,
            port_range_start: 1024,
            port_range_end: 65535,
            window_ms: 200,
        }
    }
}

impl BirthdayPortPrediction {
    /// Generate a list of candidate ports using deterministic selection.
    ///
    /// Both peers use the same shared secret to derive identical port lists,
    /// then simultaneously bind to them, relying on the birthday paradox
    /// for a collision on the NAT's external port mapping.
    pub fn generate_candidates(&self, shared_seed: &[u8]) -> Vec<u16> {
        let range = self.port_range_end.saturating_sub(self.port_range_start) + 1;
        if range == 0 {
            return Vec::new();
        }

        let mut ports = Vec::with_capacity(self.num_attempts as usize);
        let mut state: u64 = 0;
        for byte in shared_seed {
            state = state.wrapping_mul(31).wrapping_add(u64::from(*byte));
        }

        for i in 0..self.num_attempts {
            // Simple deterministic PRNG seeded by shared secret + index
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(u64::from(i));
            let port = self.port_range_start + (state as u16 % range);
            ports.push(port);
        }

        ports
    }

    /// Collision probability for the current configuration.
    pub fn collision_probability(&self) -> f64 {
        let range = self.port_range_end.saturating_sub(self.port_range_start) + 1;
        birthday_collision_probability(self.num_attempts, range)
    }
}

/// Calculate the probability of port collision with birthday paradox.
/// Given n attempts in a range of d ports:
/// P(collision) ≈ 1 - e^(-n² / 2d)
pub fn birthday_collision_probability(attempts: u16, port_range: u16) -> f64 {
    let n = f64::from(attempts);
    let d = f64::from(port_range);
    1.0 - (-n * n / (2.0 * d)).exp()
}

/// STUN server configuration (self-hosted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StunServerConfig {
    /// Listen address for STUN requests.
    pub listen_addr: String,
    /// Secondary address for NAT type detection.
    pub secondary_addr: Option<String>,
    /// Maximum requests per second per source IP.
    pub rate_limit_rps: u32,
    /// Credential mechanism.
    pub auth: StunAuth,
}

/// STUN authentication mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StunAuth {
    /// No authentication (public STUN).
    None,
    /// Short-term credentials.
    ShortTerm { username: String, password: String },
    /// Long-term credentials with realm.
    LongTerm { realm: String },
}

/// TURN relay configuration for rf-relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnConfig {
    /// Listen address for TURN.
    pub listen_addr: String,
    /// External/public address for relayed candidates.
    pub external_addr: String,
    /// Relay port range.
    pub relay_port_start: u16,
    /// Relay port end.
    pub relay_port_end: u16,
    /// Maximum allocations per user.
    pub max_allocations: u32,
    /// Allocation lifetime (seconds).
    pub allocation_lifetime_secs: u32,
    /// Authentication realm.
    pub realm: String,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3478".into(),
            external_addr: String::new(),
            relay_port_start: 49152,
            relay_port_end: 65535,
            max_allocations: 1000,
            allocation_lifetime_secs: 600,
            realm: "ravenfabric".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wg_peer_config() {
        let peer = WgPeerConfig {
            public_key: "YWJjZGVmZw==".into(),
            preshared_key: None,
            endpoint: Some("192.168.1.1:51820".into()),
            allowed_ips: vec!["10.0.0.0/24".into()],
            persistent_keepalive_secs: 25,
        };
        let json = serde_json::to_string(&peer).unwrap();
        assert!(json.contains("51820"));
    }

    #[test]
    fn test_wg_interface_default() {
        let iface = WgInterfaceConfig::default();
        assert_eq!(iface.mtu, 1420);
        assert_eq!(iface.listen_port, 0);
    }

    #[test]
    fn test_birthday_collision() {
        // 256 attempts in 64K ports should give decent probability
        let prob = birthday_collision_probability(256, 64511);
        assert!(prob > 0.3, "Expected > 30% collision, got {prob}");

        // 1024 attempts should be much higher
        let prob2 = birthday_collision_probability(1024, 64511);
        assert!(
            prob2 > 0.99,
            "Expected > 99% with 1024 attempts, got {prob2}"
        );
    }

    #[test]
    fn test_birthday_generate_candidates() {
        let prediction = BirthdayPortPrediction::default();
        let seed = b"shared-secret-between-peers";
        let ports = prediction.generate_candidates(seed);

        assert_eq!(ports.len(), 256);
        // All ports in valid range
        assert!(ports.iter().all(|p| *p >= 1024));

        // Deterministic: same seed → same ports
        let ports2 = prediction.generate_candidates(seed);
        assert_eq!(ports, ports2);

        // Different seed → different ports
        let ports3 = prediction.generate_candidates(b"different-secret");
        assert_ne!(ports, ports3);
    }

    #[test]
    fn test_birthday_collision_probability_method() {
        let prediction = BirthdayPortPrediction::default();
        let prob = prediction.collision_probability();
        assert!(prob > 0.3);
    }

    #[test]
    fn test_corporate_proxy_http_connect() {
        let proxy = CorporateProxy {
            protocol: CorporateProxyProtocol::HttpConnect,
            address: "proxy.corp.example.com:8080".into(),
            auth: Some(ProxyAuth {
                username: "user".into(),
                password: "pass".into(),
                ntlm_domain: Some("CORP".into()),
            }),
            headers: vec![("X-Corp-Token".into(), "abc123".into())],
            connect_target: Some("relay.ravenfabric.io:443".into()),
        };
        let json = serde_json::to_string(&proxy).unwrap();
        assert!(json.contains("http_connect"));
        assert!(json.contains("CORP"));
    }

    #[test]
    fn test_stun_server_config() {
        let config = StunServerConfig {
            listen_addr: "0.0.0.0:3478".into(),
            secondary_addr: Some("0.0.0.0:3479".into()),
            rate_limit_rps: 100,
            auth: StunAuth::None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("3478"));
    }

    #[test]
    fn test_turn_config_default() {
        let config = TurnConfig::default();
        assert_eq!(config.relay_port_start, 49152);
        assert_eq!(config.realm, "ravenfabric");
    }

    #[test]
    fn test_handshake_state_lifecycle() {
        let states = [
            WgHandshakeState::None,
            WgHandshakeState::InitSent,
            WgHandshakeState::Complete,
            WgHandshakeState::Expired,
        ];
        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let parsed: WgHandshakeState = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, state);
        }
    }
}
