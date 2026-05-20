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
    /// Key version counter — starts at 1, incremented on each rotation
    #[serde(default = "default_key_version")]
    pub version: u32,
    /// Previous public keys, oldest first (secret versioning / key history)
    #[serde(default)]
    pub key_history: Vec<String>,
    /// If true, this agent is immediately revoked — emergency revocation
    #[serde(default)]
    pub revoked: bool,
    /// Timestamp of revocation, if revoked (RFC 3339)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

fn default_key_version() -> u32 {
    1
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
            let file: TrustStoreFile = serde_json::from_str(&content).unwrap_or_default();
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
    pub fn register(&self, agent_id: String, public_key: String) -> Result<(), std::io::Error> {
        let agent = TrustedAgent {
            agent_id,
            public_key: public_key.clone(),
            enrolled_at: chrono::Utc::now().to_rfc3339(),
            version: 1,
            key_history: Vec::new(),
            revoked: false,
            revoked_at: None,
        };

        let mut agents = self.agents.write().unwrap_or_else(|p| p.into_inner());
        agents.insert(public_key, agent);

        if let Some(path) = &self.path {
            self.save_to_file(path, &agents)?;
        }

        Ok(())
    }

    /// Check if a public key is trusted (not revoked).
    pub fn is_trusted(&self, public_key: &str) -> bool {
        let agents = self.agents.read().unwrap_or_else(|p| p.into_inner());
        agents.get(public_key).is_some_and(|a| !a.revoked)
    }

    /// Revoke a trusted agent by public key.
    pub fn revoke(&self, public_key: &str) -> Result<bool, std::io::Error> {
        let mut agents = self.agents.write().unwrap_or_else(|p| p.into_inner());
        let existed = agents.remove(public_key).is_some();

        if existed {
            if let Some(path) = &self.path {
                self.save_to_file(path, &agents)?;
            }
        }

        Ok(existed)
    }

    /// Immediately revoke an agent — emergency revocation.
    /// The entry is preserved for audit; `is_trusted` returns false immediately.
    pub fn revoke_immediate(&self, public_key: &str) -> Result<bool, std::io::Error> {
        let mut agents = self.agents.write().unwrap_or_else(|p| p.into_inner());
        let existed = if let Some(agent) = agents.get_mut(public_key) {
            agent.revoked = true;
            agent.revoked_at = Some(chrono::Utc::now().to_rfc3339());
            true
        } else {
            false
        };
        if existed {
            if let Some(path) = &self.path {
                self.save_to_file(path, &agents)?;
            }
        }
        Ok(existed)
    }

    /// Rotate an agent's public key — increments version, moves old key to history.
    /// The old key becomes untrusted; only the new key is valid.
    pub fn rotate_key(
        &self,
        old_public_key: &str,
        new_public_key: String,
    ) -> Result<bool, std::io::Error> {
        let mut agents = self.agents.write().unwrap_or_else(|p| p.into_inner());
        let agent = match agents.remove(old_public_key) {
            Some(a) => a,
            None => return Ok(false),
        };
        let mut updated = agent;
        updated.key_history.push(old_public_key.to_string());
        updated.public_key = new_public_key.clone();
        updated.version += 1;
        updated.revoked = false;
        updated.revoked_at = None;
        agents.insert(new_public_key, updated);
        if let Some(path) = &self.path {
            self.save_to_file(path, &agents)?;
        }
        Ok(true)
    }

    /// List all trusted agents.
    pub fn list(&self) -> Vec<TrustedAgent> {
        let agents = self.agents.read().unwrap_or_else(|p| p.into_inner());
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
            EnrollmentResult::Success {
                agent_id,
                public_key,
            } => {
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

    #[test]
    fn test_is_trusted_false_when_revoked() {
        let store = TrustStore::new();
        store
            .register("agent-rev".into(), "revkey1".into())
            .unwrap();
        assert!(store.is_trusted("revkey1"));
        store.revoke_immediate("revkey1").unwrap();
        assert!(
            !store.is_trusted("revkey1"),
            "revoked key must not be trusted"
        );
    }

    #[test]
    fn test_revoke_immediate_sets_fields() {
        let store = TrustStore::new();
        store
            .register("agent-rev2".into(), "revkey2".into())
            .unwrap();
        let ok = store.revoke_immediate("revkey2").unwrap();
        assert!(ok, "revoke_immediate should return true for known key");

        let agents = store.list();
        let agent = agents.iter().find(|a| a.public_key == "revkey2").unwrap();
        assert!(agent.revoked);
        assert!(agent.revoked_at.is_some());
    }

    #[test]
    fn test_revoke_immediate_unknown_key() {
        let store = TrustStore::new();
        let ok = store.revoke_immediate("no-such-key").unwrap();
        assert!(!ok, "revoke_immediate should return false for unknown key");
    }

    #[test]
    fn test_rotate_key() {
        let store = TrustStore::new();
        store.register("agent-rot".into(), "oldkey".into()).unwrap();
        assert!(store.is_trusted("oldkey"));

        let ok = store.rotate_key("oldkey", "newkey".into()).unwrap();
        assert!(ok, "rotate_key should return true");

        assert!(
            !store.is_trusted("oldkey"),
            "old key must no longer be trusted"
        );
        assert!(store.is_trusted("newkey"), "new key must be trusted");
    }

    #[test]
    fn test_key_history_after_rotation() {
        let store = TrustStore::new();
        store
            .register("agent-hist".into(), "key-v1".into())
            .unwrap();
        store.rotate_key("key-v1", "key-v2".into()).unwrap();
        store.rotate_key("key-v2", "key-v3".into()).unwrap();

        let agents = store.list();
        let agent = agents.iter().find(|a| a.public_key == "key-v3").unwrap();
        assert_eq!(agent.version, 3);
        assert_eq!(agent.key_history, vec!["key-v1", "key-v2"]);
    }
}
