//! Post-quantum cryptography types.
//!
//! Defines hybrid key exchange (ML-KEM + X25519) and
//! harvest-now-decrypt-later resistance primitives.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Hybrid key exchange algorithm combining classical and post-quantum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridKem {
    /// ML-KEM-768 (Kyber) + X25519 (NIST approved, ~AES-192 equivalent).
    MlKem768X25519,
    /// ML-KEM-1024 (Kyber) + X25519 (highest security, ~AES-256).
    MlKem1024X25519,
    /// X-Wing (ML-KEM-768 + X25519, combined combiner).
    XWing,
    /// Classic McEliece + X25519 (conservative, large keys).
    McElieceX25519,
}

/// Post-quantum handshake configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqHandshakeConfig {
    /// Hybrid KEM algorithm to use.
    pub kem: HybridKem,
    /// Whether to include classical-only fallback.
    pub classical_fallback: bool,
    /// Key encapsulation size limit (bytes).
    pub max_kem_size: u32,
    /// Require PQ for all new connections (no downgrade).
    pub require_pq: bool,
}

impl Default for PqHandshakeConfig {
    fn default() -> Self {
        Self {
            kem: HybridKem::MlKem768X25519,
            classical_fallback: true,
            max_kem_size: 2048,
            require_pq: false,
        }
    }
}

/// PQXDH-inspired key exchange for long-lived sessions.
/// (Signal's Post-Quantum Extended Diffie-Hellman)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PqxdhConfig {
    /// Ratchet interval (messages before new KEM key).
    pub ratchet_interval: u32,
    /// Maximum stored message keys (for out-of-order delivery).
    pub max_skip: u32,
    /// KEM algorithm for ratchet.
    pub ratchet_kem: HybridKem,
    /// Whether to embed last-resort pre-key (one-time PQ bootstrap).
    pub last_resort_prekey: bool,
}

impl Default for PqxdhConfig {
    fn default() -> Self {
        Self {
            ratchet_interval: 100,
            max_skip: 1000,
            ratchet_kem: HybridKem::MlKem768X25519,
            last_resort_prekey: true,
        }
    }
}

/// Harvest-now-decrypt-later (HNDL) resistance configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HndlConfig {
    /// Require PQ protection for stored data.
    pub protect_at_rest: bool,
    /// Re-encrypt existing data with PQ when available.
    pub opportunistic_reencrypt: bool,
    /// Key rotation interval (seconds).
    pub rotation_interval_secs: u64,
    /// Minimum security level (classical equivalent bits).
    pub min_security_bits: u16,
}

impl Default for HndlConfig {
    fn default() -> Self {
        Self {
            protect_at_rest: true,
            opportunistic_reencrypt: true,
            rotation_interval_secs: 86400, // Daily
            min_security_bits: 192,
        }
    }
}

/// Key encapsulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KemResult {
    /// Shared secret (combined from classical + PQ).
    pub shared_secret_hash: String,
    /// Ciphertext to send to peer.
    pub ciphertext: Vec<u8>,
    /// Algorithm used.
    pub algorithm: HybridKem,
    /// Classical component size (bytes).
    pub classical_size: u32,
    /// PQ component size (bytes).
    pub pq_size: u32,
}

/// Key pair for hybrid KEM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridKeyPair {
    /// Classical public key (X25519, 32 bytes).
    pub classical_public: Vec<u8>,
    /// PQ public key (ML-KEM, variable size).
    pub pq_public: Vec<u8>,
    /// Algorithm identifier.
    pub algorithm: HybridKem,
    /// Creation timestamp.
    pub created_at: u64,
    /// Whether this key has been used for decapsulation.
    pub used: bool,
}

/// Hybrid KEM context — encapsulates the key exchange lifecycle.
///
/// Combines a classical X25519 shared secret with a post-quantum
/// KEM shared secret using HKDF to derive the final session key.
pub struct HybridKemContext {
    /// Algorithm in use.
    algorithm: HybridKem,
    /// Classical shared secret (32 bytes, from X25519).
    classical_secret: Option<Vec<u8>>,
    /// Post-quantum shared secret (32 bytes, from ML-KEM).
    pq_secret: Option<Vec<u8>>,
    /// Combined session key.
    combined_key: Option<Vec<u8>>,
}

impl HybridKemContext {
    /// Create a new context for the given algorithm.
    pub fn new(algorithm: HybridKem) -> Self {
        Self {
            algorithm,
            classical_secret: None,
            pq_secret: None,
            combined_key: None,
        }
    }

    /// Set the classical (X25519) shared secret.
    pub fn set_classical_secret(&mut self, secret: Vec<u8>) {
        self.classical_secret = Some(secret);
        self.try_combine();
    }

    /// Set the post-quantum shared secret.
    pub fn set_pq_secret(&mut self, secret: Vec<u8>) {
        self.pq_secret = Some(secret);
        self.try_combine();
    }

    /// Combine both secrets using HKDF-SHA256.
    /// combined = HKDF-Extract(salt=algorithm_id, IKM=classical||pq) → Expand(info="hybrid-kem", L=32)
    fn try_combine(&mut self) {
        if let (Some(classical), Some(pq)) = (&self.classical_secret, &self.pq_secret) {
            // HKDF-Extract: PRK = HMAC-SHA256(salt, IKM)
            let salt = format!("{:?}", self.algorithm);
            let mut ikm = Vec::with_capacity(classical.len() + pq.len());
            ikm.extend_from_slice(classical);
            ikm.extend_from_slice(pq);
            let prk = hmac_sha256(salt.as_bytes(), &ikm);

            // HKDF-Expand: OKM = HMAC-SHA256(PRK, info || 0x01)
            let mut expand_input = b"hybrid-kem-session-key".to_vec();
            expand_input.push(0x01);
            let key = hmac_sha256(&prk, &expand_input);

            self.combined_key = Some(key.to_vec());
        }
    }

    /// Get the combined session key (available after both secrets are set).
    pub fn session_key(&self) -> Option<&[u8]> {
        self.combined_key.as_deref()
    }

    /// Whether both components have been provided and combined.
    pub fn is_complete(&self) -> bool {
        self.combined_key.is_some()
    }

    /// Algorithm in use.
    pub fn algorithm(&self) -> &HybridKem {
        &self.algorithm
    }

    /// Expected public key sizes for the algorithm.
    pub fn expected_sizes(&self) -> (usize, usize) {
        match self.algorithm {
            HybridKem::MlKem768X25519 => (32, 1184),
            HybridKem::MlKem1024X25519 => (32, 1568),
            HybridKem::XWing => (32, 1184), // Same as ML-KEM-768
            HybridKem::McElieceX25519 => (32, 261120), // McEliece has very large keys
        }
    }
}

/// PQXDH double-ratchet state for long-lived sessions.
///
/// Each ratchet step generates a new KEM keypair, providing
/// post-compromise security even against quantum adversaries.
pub struct PqxdhRatchet {
    /// Configuration.
    config: PqxdhConfig,
    /// Current sending chain key.
    send_chain_key: Vec<u8>,
    /// Current receiving chain key.
    recv_chain_key: Vec<u8>,
    /// Messages sent since last ratchet.
    messages_since_ratchet: u32,
    /// Stored message keys for out-of-order delivery.
    skipped_keys: HashMap<(u32, u32), Vec<u8>>,
    /// Current ratchet step.
    ratchet_step: u32,
}

impl PqxdhRatchet {
    /// Create a new ratchet from initial shared secret.
    pub fn new(config: PqxdhConfig, initial_secret: Vec<u8>) -> Self {
        let send_chain_key = initial_secret.clone();
        let mut recv_chain_key = initial_secret;
        // Differentiate send/recv chains.
        if let Some(b) = recv_chain_key.first_mut() {
            *b ^= 0xFF;
        }
        Self {
            config,
            send_chain_key,
            recv_chain_key,
            messages_since_ratchet: 0,
            skipped_keys: HashMap::new(),
            ratchet_step: 0,
        }
    }

    /// Advance the send chain and return the message key.
    pub fn next_send_key(&mut self) -> Vec<u8> {
        let key = derive_chain_key(&self.send_chain_key, self.messages_since_ratchet);
        self.messages_since_ratchet += 1;
        key
    }

    /// Derive the receive key for a given message index.
    pub fn recv_key(&self, index: u32) -> Vec<u8> {
        derive_chain_key(&self.recv_chain_key, index)
    }

    /// Whether a ratchet step is needed (too many messages on current chain).
    pub fn needs_ratchet(&self) -> bool {
        self.messages_since_ratchet >= self.config.ratchet_interval
    }

    /// Perform a ratchet step with new PQ KEM shared secret.
    pub fn ratchet(&mut self, new_shared_secret: Vec<u8>) {
        self.send_chain_key = new_shared_secret.clone();
        self.recv_chain_key = new_shared_secret;
        if let Some(b) = self.recv_chain_key.first_mut() {
            *b ^= 0xFF;
        }
        self.messages_since_ratchet = 0;
        self.ratchet_step += 1;
    }

    /// Store a skipped message key for later retrieval.
    pub fn store_skipped_key(&mut self, ratchet: u32, index: u32, key: Vec<u8>) -> bool {
        if self.skipped_keys.len() >= self.config.max_skip as usize {
            return false; // Too many skipped keys.
        }
        self.skipped_keys.insert((ratchet, index), key);
        true
    }

    /// Try to retrieve a skipped message key.
    pub fn get_skipped_key(&mut self, ratchet: u32, index: u32) -> Option<Vec<u8>> {
        self.skipped_keys.remove(&(ratchet, index))
    }

    /// Current ratchet step number.
    pub fn ratchet_step(&self) -> u32 {
        self.ratchet_step
    }

    /// Messages since the last ratchet.
    pub fn messages_since_ratchet(&self) -> u32 {
        self.messages_since_ratchet
    }
}

/// Derive a chain key using HMAC-SHA256.
/// message_key = HMAC-SHA256(chain_key, "ratchet-msg" || index)
fn derive_chain_key(chain_key: &[u8], index: u32) -> Vec<u8> {
    let mut info = b"ratchet-msg-key".to_vec();
    info.extend_from_slice(&index.to_be_bytes());
    hmac_sha256(chain_key, &info).to_vec()
}

/// HMAC-SHA256: RFC 2104.
/// Returns a 32-byte MAC.
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    const BLOCK_SIZE: usize = 64;

    // If key > block size, hash it first.
    let key_block = if key.len() > BLOCK_SIZE {
        let mut h = Sha256::new();
        h.update(key);
        let hash = h.finalize();
        let mut block = [0u8; BLOCK_SIZE];
        block[..32].copy_from_slice(&hash);
        block
    } else {
        let mut block = [0u8; BLOCK_SIZE];
        block[..key.len()].copy_from_slice(key);
        block
    };

    // Inner hash: SHA-256((key XOR ipad) || data)
    let mut ipad = [0x36u8; BLOCK_SIZE];
    for (i, b) in ipad.iter_mut().enumerate() {
        *b ^= key_block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    // Outer hash: SHA-256((key XOR opad) || inner_hash)
    let mut opad = [0x5cu8; BLOCK_SIZE];
    for (i, b) in opad.iter_mut().enumerate() {
        *b ^= key_block[i];
    }
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    let result = outer.finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_kem_serde() {
        let kems = [
            HybridKem::MlKem768X25519,
            HybridKem::MlKem1024X25519,
            HybridKem::XWing,
            HybridKem::McElieceX25519,
        ];
        for k in &kems {
            let json = serde_json::to_string(k).unwrap();
            let parsed: HybridKem = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, k);
        }
    }

    #[test]
    fn test_pq_handshake_default() {
        let config = PqHandshakeConfig::default();
        assert_eq!(config.kem, HybridKem::MlKem768X25519);
        assert!(config.classical_fallback);
        assert!(!config.require_pq);
    }

    #[test]
    fn test_pqxdh_config() {
        let config = PqxdhConfig::default();
        assert_eq!(config.ratchet_interval, 100);
        assert!(config.last_resort_prekey);
    }

    #[test]
    fn test_hndl_config() {
        let config = HndlConfig::default();
        assert!(config.protect_at_rest);
        assert_eq!(config.min_security_bits, 192);
    }

    #[test]
    fn test_kem_result() {
        let result = KemResult {
            shared_secret_hash: "abc123".into(),
            ciphertext: vec![0u8; 1088], // ML-KEM-768 ciphertext size
            algorithm: HybridKem::MlKem768X25519,
            classical_size: 32,
            pq_size: 1088,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("ml_kem768_x25519"));
    }

    #[test]
    fn test_hybrid_keypair() {
        let kp = HybridKeyPair {
            classical_public: vec![0u8; 32],
            pq_public: vec![0u8; 1184], // ML-KEM-768 public key size
            algorithm: HybridKem::MlKem768X25519,
            created_at: 1700000000,
            used: false,
        };
        assert!(!kp.used);
        assert_eq!(kp.classical_public.len(), 32);
    }

    #[test]
    fn test_hybrid_kem_context() {
        let mut ctx = HybridKemContext::new(HybridKem::MlKem768X25519);
        assert!(!ctx.is_complete());
        assert!(ctx.session_key().is_none());

        ctx.set_classical_secret(vec![1u8; 32]);
        assert!(!ctx.is_complete()); // Need PQ too.

        ctx.set_pq_secret(vec![2u8; 32]);
        assert!(ctx.is_complete());
        assert_eq!(ctx.session_key().unwrap().len(), 32);
    }

    #[test]
    fn test_hybrid_kem_different_secrets_different_keys() {
        let mut ctx1 = HybridKemContext::new(HybridKem::MlKem768X25519);
        ctx1.set_classical_secret(vec![1u8; 32]);
        ctx1.set_pq_secret(vec![2u8; 32]);

        let mut ctx2 = HybridKemContext::new(HybridKem::MlKem768X25519);
        ctx2.set_classical_secret(vec![3u8; 32]);
        ctx2.set_pq_secret(vec![4u8; 32]);

        assert_ne!(ctx1.session_key(), ctx2.session_key());
    }

    #[test]
    fn test_expected_sizes() {
        let ctx = HybridKemContext::new(HybridKem::MlKem768X25519);
        let (classical, pq) = ctx.expected_sizes();
        assert_eq!(classical, 32);
        assert_eq!(pq, 1184);
    }

    #[test]
    fn test_pqxdh_ratchet() {
        let config = PqxdhConfig {
            ratchet_interval: 3,
            max_skip: 10,
            ratchet_kem: HybridKem::MlKem768X25519,
            last_resort_prekey: true,
        };
        let mut ratchet = PqxdhRatchet::new(config, vec![42u8; 32]);

        // Generate keys.
        let k1 = ratchet.next_send_key();
        let k2 = ratchet.next_send_key();
        assert_ne!(k1, k2); // Different message keys.
        assert_eq!(ratchet.messages_since_ratchet(), 2);
        assert!(!ratchet.needs_ratchet());

        // Third message triggers ratchet need.
        let _k3 = ratchet.next_send_key();
        assert!(ratchet.needs_ratchet());

        // Perform ratchet.
        ratchet.ratchet(vec![99u8; 32]);
        assert_eq!(ratchet.messages_since_ratchet(), 0);
        assert_eq!(ratchet.ratchet_step(), 1);
        assert!(!ratchet.needs_ratchet());
    }

    #[test]
    fn test_pqxdh_skipped_keys() {
        let config = PqxdhConfig::default();
        let mut ratchet = PqxdhRatchet::new(config, vec![1u8; 32]);

        let key = vec![0xAA; 32];
        assert!(ratchet.store_skipped_key(0, 5, key.clone()));
        assert_eq!(ratchet.get_skipped_key(0, 5), Some(key));
        assert_eq!(ratchet.get_skipped_key(0, 5), None); // Consumed.
    }

    #[test]
    fn test_recv_key_deterministic() {
        let config = PqxdhConfig::default();
        let ratchet = PqxdhRatchet::new(config, vec![1u8; 32]);
        let k1 = ratchet.recv_key(0);
        let k2 = ratchet.recv_key(0);
        assert_eq!(k1, k2); // Same index → same key.
        let k3 = ratchet.recv_key(1);
        assert_ne!(k1, k3); // Different index → different key.
    }
}
