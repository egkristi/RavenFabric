//! One-Time Password generation and validation for agent enrollment.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use rand::RngCore;
use sha2::{Digest, Sha256};

/// OTP store for bootstrap enrollment.
pub struct OtpStore {
    tokens: RwLock<HashMap<String, OtpEntry>>,
    ttl: Duration,
}

struct OtpEntry {
    #[allow(dead_code)]
    agent_id: Option<String>,
    created_at: Instant,
    used: bool,
}

impl OtpStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Generate a new OTP. Returns (plaintext_token, hash).
    /// The plaintext is given to the agent. The hash is stored.
    pub fn generate(&self, agent_id: Option<String>) -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = format!("rf-otp-{}", hex::encode(bytes));

        let hash = hash_token(&token);
        let mut tokens = self.tokens.write().unwrap();
        tokens.insert(
            hash,
            OtpEntry {
                agent_id,
                created_at: Instant::now(),
                used: false,
            },
        );

        token
    }

    /// Validate and consume a token (single-use).
    pub fn validate_and_consume(&self, token: &str) -> Result<(), &'static str> {
        let hash = hash_token(token);
        let mut tokens = self.tokens.write().unwrap();

        let entry = tokens.get_mut(&hash).ok_or("token not found")?;

        if entry.used {
            return Err("token already used");
        }

        if entry.created_at.elapsed() > self.ttl {
            return Err("token expired");
        }

        entry.used = true;
        Ok(())
    }

    /// Remove expired tokens.
    pub fn purge_expired(&self) {
        let mut tokens = self.tokens.write().unwrap();
        tokens.retain(|_, entry| entry.created_at.elapsed() < self.ttl);
    }
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate() {
        let store = OtpStore::new(Duration::from_secs(3600));
        let token = store.generate(Some("test-agent".into()));

        assert!(token.starts_with("rf-otp-"));
        assert!(store.validate_and_consume(&token).is_ok());
    }

    #[test]
    fn test_single_use() {
        let store = OtpStore::new(Duration::from_secs(3600));
        let token = store.generate(None);

        assert!(store.validate_and_consume(&token).is_ok());
        assert!(store.validate_and_consume(&token).is_err());
    }

    #[test]
    fn test_invalid_token() {
        let store = OtpStore::new(Duration::from_secs(3600));
        assert!(store.validate_and_consume("rf-otp-invalid").is_err());
    }

    #[test]
    fn test_expired_token() {
        let store = OtpStore::new(Duration::from_millis(1));
        let token = store.generate(None);

        std::thread::sleep(Duration::from_millis(10));
        assert!(store.validate_and_consume(&token).is_err());
    }
}
