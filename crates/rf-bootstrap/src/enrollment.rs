//! Agent enrollment flow: OTP → Noise XX handshake → key registration.
//!
//! The enrollment protocol:
//! 1. Admin generates OTP and gives it to the agent out-of-band
//! 2. Agent connects to relay with OTP as meet token
//! 3. Controller validates OTP (single-use, TTL-enforced)
//! 4. If valid, Noise XX handshake proceeds (mutual key exchange)
//! 5. Controller records agent's public key in the trust store
//! 6. Agent records controller's public key for future connections

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::otp::OtpStore;

/// Stores trusted agent public keys after successful enrollment.
pub struct TrustStore {
    agents: RwLock<HashMap<String, TrustedAgent>>,
    path: Option<PathBuf>,
}

/// A registered agent in the trust store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedAgent {
    /// Agent identifier (human-readable)
    pub agent_id: String,
    /// Agent's Noise public key (hex-encoded)
    pub public_key: String,
    /// When the agent was enrolled (RFC 3339)
    pub enrolled_at: String,
}

/// Result of an enrollment attempt.
#[derive(Debug)]
pub enum EnrollmentResult {
    /// OTP valid, handshake completed, agent registered.
    Success {
        agent_id: String,
        public_key: String,
    },
    /// OTP validation failed.
    OtpInvalid(String),
}

/// The trust store file format (JSON).
#[derive(Debug, Serialize, Deserialize, Default)]
struct TrustStoreFile {
    agents: Vec<TrustedAgent>,
}

impl TrustStore {
    /// Create a new in-memory trust store.
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            path: None,
        }
    }

    /// Create a trust store backed by a file.
    pub fn with_file(path: &Path) -> std::io::Result<Self> {
        let agents = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let file: TrustStoreFile =
                serde_json::from_str(&content).unwrap_or_default();
            file.agents
                .into_iter()
                .map(|a| (a.public_key.clone(), a))
                .collect()
        } else {
            HashMap::new()
        };

        Ok(Self {
            agents: RwLock::new(agents),
            path: Some(path.to_path_buf()),
        })
    }

    /// Register a newly enrolled agent.
    pub fn register(
        &self,
        agent_id: String,
        public_key: String,
    ) -> Result<(), std::io::Error> {
        let agent = TrustedAgent {
            agent_id,
            public_key: public_key.clone(),
            enrolled_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut agents = self
            .agents
            .write()
            .unwrap_or_else(|p| p.into_inner());
        agents.insert(public_key, agent);

        if let Some(path) = &self.path {
            self.save_to_file(path, &agents)?;
        }

        Ok(())
    }

    /// Check if a public key is trusted.
    pub fn is_trusted(&self, public_key: &str) -> bool {
        let agents = self
            .agents
            .read()
            .unwrap_or_else(|p| p.into_inner());
        agents.contains_key(public_key)
    }

    /// Revoke a trusted agent by public key.
    pub fn revoke(&self, public_key: &str) -> Result<bool, std::io::Error> {
        let mut agents = self
            .agents
            .write()
            .unwrap_or_else(|p| p.into_inner());
        let existed = agents.remove(public_key).is_some();

        if existed {
            if let Some(path) = &self.path {
                self.save_to_file(path, &agents)?;
            }
        }

        Ok(existed)
    }

    /// List all trusted agents.
    pub fn list(&self) -> Vec<TrustedAgent> {
        let agents = self
            .agents
            .read()
            .unwrap_or_else(|p| p.into_inner());
        agents.values().cloned().collect()
    }

    fn save_to_file(
        &self,
        path: &Path,
        agents: &HashMap<String, TrustedAgent>,
    ) -> Result<(), std::io::Error> {
        let file = TrustStoreFile {
            agents: agents.values().cloned().collect(),
        };
        let content = serde_json::to_string_pretty(&file)?;
        std::fs::write(path, content)
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Perform controller-side enrollment validation.
///
/// Steps:
/// 1. Validate the OTP token
/// 2. If valid, record the agent's public key in the trust store
///
/// Returns Ok(EnrollmentResult) indicating success or OTP failure.
pub fn enroll_agent(
    otp_store: &OtpStore,
    trust_store: &TrustStore,
    otp_token: &str,
    agent_id: &str,
    agent_public_key: &str,
) -> Result<EnrollmentResult, std::io::Error> {
    // Validate OTP
    if let Err(reason) = otp_store.validate_and_consume(otp_token) {
        return Ok(EnrollmentResult::OtpInvalid(reason.to_string()));
    }

    // Register agent's public key
    trust_store.register(agent_id.to_string(), agent_public_key.to_string())?;

    Ok(EnrollmentResult::Success {
        agent_id: agent_id.to_string(),
        public_key: agent_public_key.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_trust_store_register_and_check() {
        let store = TrustStore::new();
        assert!(!store.is_trusted("abc123"));

        store.register("agent-1".into(), "abc123".into()).unwrap();
        assert!(store.is_trusted("abc123"));
    }

    #[test]
    fn test_trust_store_revoke() {
        let store = TrustStore::new();
        store.register("agent-1".into(), "key1".into()).unwrap();
        assert!(store.is_trusted("key1"));

        store.revoke("key1").unwrap();
        assert!(!store.is_trusted("key1"));
    }

    #[test]
    fn test_trust_store_list() {
        let store = TrustStore::new();
        store.register("agent-1".into(), "key1".into()).unwrap();
        store.register("agent-2".into(), "key2".into()).unwrap();

        let list = store.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_trust_store_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.json");

        // Create and populate
        {
            let store = TrustStore::with_file(&path).unwrap();
            store.register("agent-1".into(), "key1".into()).unwrap();
            store.register("agent-2".into(), "key2".into()).unwrap();
        }

        // Reload and verify
        {
            let store = TrustStore::with_file(&path).unwrap();
            assert!(store.is_trusted("key1"));
            assert!(store.is_trusted("key2"));
            assert!(!store.is_trusted("key3"));
        }
    }

    #[test]
    fn test_enrollment_success() {
        let otp_store = OtpStore::new(Duration::from_secs(3600));
        let trust_store = TrustStore::new();

        let token = otp_store.generate(Some("new-agent".into()));

        let result = enroll_agent(
            &otp_store,
            &trust_store,
            &token,
            "new-agent",
            "deadbeef1234",
        )
        .unwrap();

        match result {
            EnrollmentResult::Success { agent_id, public_key } => {
                assert_eq!(agent_id, "new-agent");
                assert_eq!(public_key, "deadbeef1234");
            }
            _ => panic!("expected success"),
        }

        assert!(trust_store.is_trusted("deadbeef1234"));
    }

    #[test]
    fn test_enrollment_invalid_otp() {
        let otp_store = OtpStore::new(Duration::from_secs(3600));
        let trust_store = TrustStore::new();

        let result = enroll_agent(
            &otp_store,
            &trust_store,
            "rf-otp-invalid",
            "attacker",
            "attackerkey",
        )
        .unwrap();

        match result {
            EnrollmentResult::OtpInvalid(_) => {}
            _ => panic!("expected OTP invalid"),
        }

        assert!(!trust_store.is_trusted("attackerkey"));
    }

    #[test]
    fn test_enrollment_otp_single_use() {
        let otp_store = OtpStore::new(Duration::from_secs(3600));
        let trust_store = TrustStore::new();

        let token = otp_store.generate(None);

        // First enrollment succeeds
        let result = enroll_agent(&otp_store, &trust_store, &token, "agent-1", "key1").unwrap();
        assert!(matches!(result, EnrollmentResult::Success { .. }));

        // Second attempt with same token fails
        let result = enroll_agent(&otp_store, &trust_store, &token, "agent-2", "key2").unwrap();
        assert!(matches!(result, EnrollmentResult::OtpInvalid(_)));
    }
}
