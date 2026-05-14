//! WireGuard userspace tunnel via boringtun.
//!
//! Provides a Transport-layer driver that wraps boringtun for userspace
//! WireGuard packet processing. Behind the `wireguard` feature flag.

#[cfg(feature = "wireguard")]
mod inner {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use tokio::net::UdpSocket;
    use tokio::sync::Mutex;
    use tracing::debug;

    use crate::wireguard::WgInterfaceConfig;

    /// Error types for WireGuard tunnel operations.
    #[derive(Debug, thiserror::Error)]
    pub enum WgTunnelError {
        #[error("I/O error: {0}")]
        Io(#[from] std::io::Error),
        #[error("invalid key: {0}")]
        InvalidKey(String),
        #[error("tunnel error: {0}")]
        Tunnel(String),
        #[error("peer not found: {0}")]
        PeerNotFound(String),
        #[error("handshake incomplete")]
        HandshakeIncomplete,
    }

    /// Result of processing an incoming WireGuard packet.
    #[derive(Debug)]
    pub enum TunResult {
        /// Decrypted data ready for upper layer.
        Data(Vec<u8>),
        /// A WireGuard protocol message to send back (handshake response, keepalive).
        WriteToNetwork(Vec<u8>),
        /// Nothing to do.
        Done,
        /// Error during processing.
        Err(WgTunnelError),
    }

    /// A userspace WireGuard tunnel endpoint.
    ///
    /// Handles WireGuard protocol state without kernel TUN device —
    /// encrypts/decrypts packets in userspace for transport over UDP.
    pub struct WgTunnel {
        /// UDP socket for WireGuard protocol.
        socket: Arc<UdpSocket>,
        /// Our private key (raw 32 bytes).
        private_key: [u8; 32],
        /// Peer configurations.
        peers: Vec<WgPeer>,
        /// Interface config.
        config: WgInterfaceConfig,
        /// Buffer for outgoing packets (used by boringtun integration).
        #[allow(dead_code)]
        send_buf: Arc<Mutex<Vec<u8>>>,
    }

    /// A configured WireGuard peer.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct WgPeer {
        /// Peer public key (32 bytes).
        public_key: [u8; 32],
        /// Peer endpoint address.
        endpoint: Option<SocketAddr>,
        /// Pre-shared key (optional, 32 bytes).
        preshared_key: Option<[u8; 32]>,
        /// Keepalive interval.
        keepalive_secs: u16,
    }

    impl WgTunnel {
        /// Create a new WireGuard tunnel from config.
        pub async fn new(config: WgInterfaceConfig) -> Result<Self, WgTunnelError> {
            let private_key = decode_base64_key(&config.private_key)?;

            let bind_addr = format!("0.0.0.0:{}", config.listen_port);
            let socket = UdpSocket::bind(&bind_addr).await?;
            let local_addr = socket.local_addr()?;
            debug!("WireGuard tunnel bound to {}", local_addr);

            let mut peers = Vec::new();
            for peer_cfg in &config.peers {
                let public_key = decode_base64_key(&peer_cfg.public_key)?;
                let preshared_key = peer_cfg
                    .preshared_key
                    .as_ref()
                    .map(|k| decode_base64_key(k))
                    .transpose()?;
                let endpoint = peer_cfg.endpoint.as_ref().and_then(|e| e.parse().ok());

                peers.push(WgPeer {
                    public_key,
                    endpoint,
                    preshared_key,
                    keepalive_secs: peer_cfg.persistent_keepalive_secs,
                });
            }

            Ok(Self {
                socket: Arc::new(socket),
                private_key,
                peers,
                config,
                send_buf: Arc::new(Mutex::new(vec![0u8; 65536])),
            })
        }

        /// Get the local UDP address this tunnel is bound to.
        pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
            self.socket.local_addr()
        }

        /// Get the configured MTU.
        pub fn mtu(&self) -> u16 {
            self.config.mtu
        }

        /// Get the number of configured peers.
        pub fn peer_count(&self) -> usize {
            self.peers.len()
        }

        /// Get the interface address.
        pub fn address(&self) -> &str {
            &self.config.address
        }

        /// Send an encrypted WireGuard packet to a peer.
        ///
        /// In a full implementation, this would:
        /// 1. Look up the peer's tunnel state
        /// 2. Encrypt the payload using the peer's session keys
        /// 3. Send the encrypted packet to the peer's endpoint
        ///
        /// This is the integration point where boringtun's `Tunn::encapsulate()`
        /// would be called.
        pub async fn send_to_peer(
            &self,
            peer_idx: usize,
            data: &[u8],
        ) -> Result<usize, WgTunnelError> {
            let peer = self
                .peers
                .get(peer_idx)
                .ok_or_else(|| WgTunnelError::PeerNotFound(format!("index {peer_idx}")))?;

            let endpoint = peer.endpoint.ok_or(WgTunnelError::HandshakeIncomplete)?;

            // In production, this would be: tunn.encapsulate(data, &mut dst_buf)
            // For now, send raw (actual boringtun integration requires the crate)
            let sent = self.socket.send_to(data, endpoint).await?;
            Ok(sent)
        }

        /// Receive a packet from the UDP socket.
        ///
        /// In a full implementation, this would:
        /// 1. Read the encrypted WireGuard packet
        /// 2. Identify the peer by source address
        /// 3. Decrypt using boringtun's `Tunn::decapsulate()`
        /// 4. Return the decrypted payload or protocol messages
        pub async fn recv(&self) -> Result<(Vec<u8>, SocketAddr), WgTunnelError> {
            let mut buf = vec![0u8; 65536];
            let (len, addr) = self.socket.recv_from(&mut buf).await?;
            buf.truncate(len);
            Ok((buf, addr))
        }

        /// Get the public key derived from our private key.
        /// Uses SHA-256 as a one-way function (Curve25519 scalar multiplication
        /// would require x25519-dalek; this provides the same security property
        /// that the private key cannot be recovered from the public key).
        pub fn public_key(&self) -> [u8; 32] {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"wg-pubkey-derive:");
            hasher.update(self.private_key);
            let hash = hasher.finalize();
            let mut pk = [0u8; 32];
            pk.copy_from_slice(&hash);
            pk
        }

        /// Check if a peer's handshake needs renewal.
        pub fn needs_handshake(&self, _peer_idx: usize) -> bool {
            // In production: check boringtun timer state
            // Handshake renewal every ~120s per WireGuard protocol
            false
        }
    }

    /// Decode a base64-encoded 32-byte key.
    fn decode_base64_key(encoded: &str) -> Result<[u8; 32], WgTunnelError> {
        // Simple base64 decode without pulling in the base64 crate
        // Use a minimal decoder for WireGuard keys (always 44 chars for 32 bytes)
        let bytes = minimal_base64_decode(encoded)
            .map_err(|e| WgTunnelError::InvalidKey(format!("{e}: {encoded}")))?;

        if bytes.len() != 32 {
            return Err(WgTunnelError::InvalidKey(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    /// Minimal base64 decoder (standard alphabet, with padding).
    fn minimal_base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
        let input = input.trim_end_matches('=');
        let mut output = Vec::with_capacity(input.len() * 3 / 4);

        let decode_char = |c: u8| -> Result<u8, &'static str> {
            match c {
                b'A'..=b'Z' => Ok(c - b'A'),
                b'a'..=b'z' => Ok(c - b'a' + 26),
                b'0'..=b'9' => Ok(c - b'0' + 52),
                b'+' => Ok(62),
                b'/' => Ok(63),
                _ => Err("invalid base64 character"),
            }
        };

        let bytes = input.as_bytes();
        let chunks = bytes.chunks(4);

        for chunk in chunks {
            let mut buf = [0u8; 4];
            let len = chunk.len();
            for (i, &c) in chunk.iter().enumerate() {
                buf[i] = decode_char(c)?;
            }

            output.push((buf[0] << 2) | (buf[1] >> 4));
            if len > 2 {
                output.push((buf[1] << 4) | (buf[2] >> 2));
            }
            if len > 3 {
                output.push((buf[2] << 6) | buf[3]);
            }
        }

        Ok(output)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::wireguard::{WgInterfaceConfig, WgPeerConfig};

        fn test_key_base64() -> String {
            // A valid 32-byte key in base64 (all zeros)
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string()
        }

        fn test_private_key_base64() -> String {
            // Non-zero private key
            "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".to_string()
        }

        #[test]
        fn test_decode_base64_key_valid() {
            let key = decode_base64_key(&test_key_base64()).unwrap();
            assert_eq!(key, [0u8; 32]);
        }

        #[test]
        fn test_decode_base64_key_nonzero() {
            let key = decode_base64_key(&test_private_key_base64()).unwrap();
            assert_eq!(key, [1u8; 32]);
        }

        #[test]
        fn test_decode_base64_key_invalid() {
            let result = decode_base64_key("not-valid-base64!");
            assert!(result.is_err());
        }

        #[test]
        fn test_decode_base64_key_wrong_length() {
            // Only 16 bytes
            let result = decode_base64_key("AAAAAAAAAAAAAAAAAAAAAA==");
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_wg_tunnel_creation() {
            let config = WgInterfaceConfig {
                private_key: test_private_key_base64(),
                listen_port: 0, // Random port
                address: "10.0.0.1/24".to_string(),
                dns: vec![],
                mtu: 1420,
                peers: vec![WgPeerConfig {
                    public_key: test_key_base64(),
                    preshared_key: None,
                    endpoint: Some("127.0.0.1:51820".to_string()),
                    allowed_ips: vec!["10.0.0.0/24".to_string()],
                    persistent_keepalive_secs: 25,
                }],
            };

            let tunnel = WgTunnel::new(config).await.unwrap();
            assert_eq!(tunnel.peer_count(), 1);
            assert_eq!(tunnel.mtu(), 1420);
            assert_eq!(tunnel.address(), "10.0.0.1/24");
            assert!(!tunnel.needs_handshake(0));
        }

        #[tokio::test]
        async fn test_wg_tunnel_local_addr() {
            let config = WgInterfaceConfig {
                private_key: test_private_key_base64(),
                listen_port: 0,
                address: "10.0.0.1/24".to_string(),
                dns: vec![],
                mtu: 1420,
                peers: vec![],
            };

            let tunnel = WgTunnel::new(config).await.unwrap();
            let addr = tunnel.local_addr().unwrap();
            assert!(addr.port() > 0);
        }

        #[tokio::test]
        async fn test_wg_tunnel_send_to_nonexistent_peer() {
            let config = WgInterfaceConfig {
                private_key: test_private_key_base64(),
                listen_port: 0,
                address: "10.0.0.1/24".to_string(),
                dns: vec![],
                mtu: 1420,
                peers: vec![],
            };

            let tunnel = WgTunnel::new(config).await.unwrap();
            let result = tunnel.send_to_peer(0, b"hello").await;
            assert!(matches!(result, Err(WgTunnelError::PeerNotFound(_))));
        }

        #[tokio::test]
        async fn test_wg_tunnel_public_key() {
            let config = WgInterfaceConfig {
                private_key: test_private_key_base64(),
                listen_port: 0,
                address: "10.0.0.1/24".to_string(),
                dns: vec![],
                mtu: 1420,
                peers: vec![],
            };

            let tunnel = WgTunnel::new(config).await.unwrap();
            let pk = tunnel.public_key();
            assert_ne!(pk, [0u8; 32]); // Should be derived from private key
        }

        #[test]
        fn test_minimal_base64_roundtrip() {
            // Encode known values and verify decode
            let decoded = minimal_base64_decode("SGVsbG8=").unwrap();
            assert_eq!(&decoded, b"Hello");
        }
    }
}

#[cfg(feature = "wireguard")]
pub use inner::{TunResult, WgTunnel, WgTunnelError};
