//! Post-quantum cryptography types.
//!
//! Defines hybrid key exchange (ML-KEM + X25519) and
//! harvest-now-decrypt-later resistance primitives.

use serde::{Deserialize, Serialize};

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
}
