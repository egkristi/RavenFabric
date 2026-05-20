//! Sealed secret store — encrypted at rest, decrypted only at execution time.
//!
//! Uses ChaCha20-Poly1305 (IETF) for authenticated encryption of secrets.
//! Secrets are stored as encrypted blobs that can only be decrypted
//! by the agent holding the sealing key.
//!
//! ## Secret Rotation
//!
//! Each secret can be configured with an automatic rotation policy:
//! - **TTL**: how long until the secret expires and must be replaced
//! - **Rotation hook**: shell command whose stdout becomes the new secret value
//! - **Grace period**: how long the old value remains valid after rotation
//! - **Health check**: command that must exit 0 before old value is retired

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use rand::RngCore;

use crate::error::CryptoError;

/// Configuration for automatic secret rotation.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// How long until the secret expires and needs rotation.
    pub ttl: Duration,
    /// Shell command to execute to produce the new secret value (stdout used as plaintext).
    pub hook: Option<String>,
    /// How long the previous value remains valid after rotation (zero-downtime overlap).
    pub grace_period: Duration,
    /// Optional shell command to verify the new secret is working (must exit 0).
    pub health_check: Option<String>,
    /// When the current value was sealed (for TTL calculation).
    pub sealed_at: SystemTime,
    /// When the most recent rotation occurred (None if never rotated).
    pub rotated_at: Option<SystemTime>,
    /// Previous value's encrypted bytes, kept for grace period.
    pub previous_nonce: Option<[u8; 12]>,
    /// Previous value's ciphertext, kept for grace period.
    pub previous_ciphertext: Option<Vec<u8>>,
}

impl RotationConfig {
    /// Create a new rotation config with the given TTL and optional hook.
    pub fn new(ttl: Duration, hook: Option<String>, grace_period: Duration) -> Self {
        Self {
            ttl,
            hook,
            grace_period,
            health_check: None,
            sealed_at: SystemTime::now(),
            rotated_at: None,
            previous_nonce: None,
            previous_ciphertext: None,
        }
    }

    /// Set an optional health-check command (must exit 0 before old value is retired).
    pub fn with_health_check(mut self, cmd: String) -> Self {
        self.health_check = Some(cmd);
        self
    }

    /// Whether the secret's TTL has elapsed and it needs rotation.
    pub fn is_expired(&self) -> bool {
        self.sealed_at.elapsed().unwrap_or(Duration::ZERO) >= self.ttl
    }

    /// Whether the old value is still within its grace period after a rotation.
    pub fn in_grace_period(&self) -> bool {
        if let Some(rotated_at) = self.rotated_at {
            let elapsed = rotated_at.elapsed().unwrap_or(Duration::MAX);
            elapsed < self.grace_period
        } else {
            false
        }
    }

    /// Seconds remaining before the secret expires (0 if already expired).
    pub fn ttl_remaining_secs(&self) -> u64 {
        let age = self.sealed_at.elapsed().unwrap_or(self.ttl);
        self.ttl.saturating_sub(age).as_secs()
    }
}

/// A sealed (encrypted) secret value.
#[derive(Debug, Clone)]
pub struct SealedSecret {
    /// 12-byte nonce used for encryption.
    nonce: [u8; 12],
    /// Encrypted data (ciphertext + 16-byte Poly1305 tag appended by AEAD).
    ciphertext: Vec<u8>,
}

/// The secret store — holds sealed secrets indexed by name.
#[derive(Debug)]
pub struct SecretStore {
    /// Sealing key (256-bit).
    seal_key: [u8; 32],
    /// Stored secrets indexed by name.
    secrets: HashMap<String, SealedSecret>,
    /// Per-secret rotation configuration and state.
    rotation: HashMap<String, RotationConfig>,
}

impl SecretStore {
    /// Create a new secret store with the given 256-bit sealing key.
    pub fn new(seal_key: [u8; 32]) -> Self {
        Self {
            seal_key,
            secrets: HashMap::new(),
            rotation: HashMap::new(),
        }
    }

    /// Seal (encrypt) a secret and store it under the given name.
    pub fn seal(&mut self, name: &str, plaintext: &[u8]) -> Result<(), CryptoError> {
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);

        let cipher = ChaCha20Poly1305::new((&self.seal_key).into());
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| CryptoError::Decrypt("encryption failed".into()))?;

        self.secrets.insert(
            name.to_string(),
            SealedSecret {
                nonce: nonce_bytes,
                ciphertext,
            },
        );
        Ok(())
    }

    /// Seal a secret and configure automatic rotation.
    pub fn seal_with_rotation(
        &mut self,
        name: &str,
        plaintext: &[u8],
        config: RotationConfig,
    ) -> Result<(), CryptoError> {
        self.seal(name, plaintext)?;
        self.rotation.insert(name.to_string(), config);
        Ok(())
    }

    /// Unseal (decrypt) a secret by name. Only succeeds with the correct seal key.
    pub fn unseal(&self, name: &str) -> Result<Vec<u8>, CryptoError> {
        let sealed = self
            .secrets
            .get(name)
            .ok_or_else(|| CryptoError::Decrypt(format!("secret '{name}' not found")))?;

        let cipher = ChaCha20Poly1305::new((&self.seal_key).into());
        let nonce = Nonce::from_slice(&sealed.nonce);
        cipher
            .decrypt(nonce, sealed.ciphertext.as_ref())
            .map_err(|_| CryptoError::TamperDetected)
    }

    /// Unseal the current secret, or the previous value if it is within the grace period.
    ///
    /// Returns `(plaintext, from_previous)` where `from_previous` is `true` when
    /// the grace-period copy was used.
    pub fn unseal_with_grace(&self, name: &str) -> Result<(Vec<u8>, bool), CryptoError> {
        // Always try the current value first.
        match self.unseal(name) {
            Ok(val) => Ok((val, false)),
            Err(e) => {
                // Fall back to previous value if within grace period.
                if let Some(rc) = self.rotation.get(name) {
                    if rc.in_grace_period() {
                        if let (Some(nonce_bytes), Some(ct)) =
                            (&rc.previous_nonce, &rc.previous_ciphertext)
                        {
                            let cipher = ChaCha20Poly1305::new((&self.seal_key).into());
                            let nonce = Nonce::from_slice(nonce_bytes.as_ref());
                            return cipher
                                .decrypt(nonce, ct.as_ref())
                                .map(|v| (v, true))
                                .map_err(|_| CryptoError::TamperDetected);
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// Rotate a secret: archive the current value for the grace period, then seal the new value.
    ///
    /// Updates `rotated_at` and resets the TTL clock (`sealed_at`). If no rotation config
    /// exists the call simply re-seals the value without any grace period.
    pub fn rotate(&mut self, name: &str, new_plaintext: &[u8]) -> Result<(), CryptoError> {
        // Archive current sealed bytes before overwriting.
        let (prev_nonce, prev_ct) = if let Some(current) = self.secrets.get(name) {
            (Some(current.nonce), Some(current.ciphertext.clone()))
        } else {
            (None, None)
        };

        // Seal the new value.
        self.seal(name, new_plaintext)?;

        // Update rotation state.
        if let Some(rc) = self.rotation.get_mut(name) {
            rc.previous_nonce = prev_nonce;
            rc.previous_ciphertext = prev_ct;
            rc.rotated_at = Some(SystemTime::now());
            rc.sealed_at = SystemTime::now(); // reset TTL
        }

        Ok(())
    }

    /// Return names of all secrets whose TTL has elapsed and require rotation.
    pub fn needs_rotation(&self) -> Vec<String> {
        self.rotation
            .iter()
            .filter(|(_, rc)| rc.is_expired())
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Return the rotation config for a secret, if any.
    pub fn rotation_config(&self, name: &str) -> Option<&RotationConfig> {
        self.rotation.get(name)
    }

    /// Return a mutable reference to the rotation config for a secret.
    pub fn rotation_config_mut(&mut self, name: &str) -> Option<&mut RotationConfig> {
        self.rotation.get_mut(name)
    }

    /// Attach or replace the rotation config for an existing sealed secret.
    ///
    /// Unlike [`seal_with_rotation`], this does **not** re-seal the value — the
    /// current plaintext is preserved.  Returns an error if the secret does not exist.
    pub fn set_rotation_config(
        &mut self,
        name: &str,
        config: RotationConfig,
    ) -> Result<(), CryptoError> {
        if !self.secrets.contains_key(name) {
            return Err(CryptoError::Decrypt(format!("secret '{name}' not found")));
        }
        self.rotation.insert(name.to_string(), config);
        Ok(())
    }

    /// Check if a secret exists.
    pub fn contains(&self, name: &str) -> bool {
        self.secrets.contains_key(name)
    }

    /// Remove a secret from the store (and its rotation config).
    pub fn remove(&mut self, name: &str) -> bool {
        self.rotation.remove(name);
        self.secrets.remove(name).is_some()
    }

    /// List all secret names.
    pub fn list(&self) -> Vec<&str> {
        self.secrets.keys().map(|s| s.as_str()).collect()
    }

    /// Resolve `{{ secrets.KEY }}` patterns in a string.
    /// Returns the string with all secret references replaced by their decrypted values.
    /// Uses grace-period fallback so rotations don't break in-flight template resolution.
    pub fn resolve_template(&self, template: &str) -> Result<String, CryptoError> {
        let mut result = template.to_string();
        while let Some(start) = result.find("{{ secrets.") {
            let after_prefix = start + "{{ secrets.".len();
            let end = result[after_prefix..]
                .find(" }}")
                .ok_or_else(|| CryptoError::Decrypt("unclosed secret reference".into()))?
                + after_prefix;

            let name = result[after_prefix..end].to_string();
            let plaintext = self
                .unseal_with_grace(&name)
                .map(|(v, _)| v)
                .or_else(|_| self.unseal(&name))?;
            let value = String::from_utf8(plaintext)
                .map_err(|_| CryptoError::Decrypt("secret is not valid UTF-8".into()))?;

            result = format!("{}{}{}", &result[..start], value, &result[end + 3..]);
        }
        Ok(result)
    }
}

impl Drop for SecretStore {
    fn drop(&mut self) {
        // Zeroize the seal key on drop (defense in depth)
        for b in self.seal_key.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        key[0] = 0x42;
        key[1] = 0xDE;
        key[31] = 0xFF;
        key
    }

    #[test]
    fn test_seal_and_unseal() {
        let mut store = SecretStore::new(test_key());
        store.seal("db_password", b"super_secret_123").unwrap();
        let plaintext = store.unseal("db_password").unwrap();
        assert_eq!(plaintext, b"super_secret_123");
    }

    #[test]
    fn test_unseal_nonexistent() {
        let store = SecretStore::new(test_key());
        assert!(store.unseal("missing").is_err());
    }

    #[test]
    fn test_wrong_key_fails() {
        let mut store = SecretStore::new(test_key());
        store.seal("secret", b"data").unwrap();

        // Create a new store with wrong key and insert the sealed data
        let sealed = store.secrets.get("secret").unwrap().clone();
        let mut wrong_store = SecretStore::new([0xAA; 32]);
        wrong_store.secrets.insert("secret".to_string(), sealed);
        assert!(wrong_store.unseal("secret").is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let mut store = SecretStore::new(test_key());
        store.seal("secret", b"important data").unwrap();

        // Tamper with the ciphertext
        let sealed = store.secrets.get_mut("secret").unwrap();
        if !sealed.ciphertext.is_empty() {
            sealed.ciphertext[0] ^= 0xFF;
        }

        assert!(store.unseal("secret").is_err());
    }

    #[test]
    fn test_list_and_remove() {
        let mut store = SecretStore::new(test_key());
        store.seal("key1", b"val1").unwrap();
        store.seal("key2", b"val2").unwrap();

        assert!(store.contains("key1"));
        assert!(store.contains("key2"));
        assert_eq!(store.list().len(), 2);

        store.remove("key1");
        assert!(!store.contains("key1"));
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn test_resolve_template() {
        let mut store = SecretStore::new(test_key());
        store.seal("DB_PASS", b"mypassword").unwrap();
        store.seal("API_KEY", b"abc123").unwrap();

        let template = "postgres://user:{{ secrets.DB_PASS }}@host/db?key={{ secrets.API_KEY }}";
        let resolved = store.resolve_template(template).unwrap();
        assert_eq!(resolved, "postgres://user:mypassword@host/db?key=abc123");
    }

    #[test]
    fn test_resolve_template_no_secrets() {
        let store = SecretStore::new(test_key());
        let template = "no secrets here";
        let resolved = store.resolve_template(template).unwrap();
        assert_eq!(resolved, "no secrets here");
    }

    #[test]
    fn test_empty_secret() {
        let mut store = SecretStore::new(test_key());
        store.seal("empty", b"").unwrap();
        let plaintext = store.unseal("empty").unwrap();
        assert!(plaintext.is_empty());
    }

    // ── Rotation tests ─────────────────────────────────────────────────────

    #[test]
    fn test_rotation_config_is_expired() {
        let mut cfg = RotationConfig::new(Duration::from_secs(1), None, Duration::ZERO);
        assert!(!cfg.is_expired());
        // Backdate sealed_at by 2 seconds.
        cfg.sealed_at = SystemTime::now() - Duration::from_secs(2);
        assert!(cfg.is_expired());
    }

    #[test]
    fn test_rotation_config_ttl_remaining() {
        let cfg = RotationConfig::new(Duration::from_secs(3600), None, Duration::ZERO);
        let remaining = cfg.ttl_remaining_secs();
        assert!(remaining > 3590 && remaining <= 3600);
    }

    #[test]
    fn test_seal_with_rotation_and_needs_rotation() {
        let mut store = SecretStore::new(test_key());
        let mut cfg = RotationConfig::new(Duration::from_secs(1), None, Duration::ZERO);
        // Already expired.
        cfg.sealed_at = SystemTime::now() - Duration::from_secs(2);
        store
            .seal_with_rotation("api_key", b"old_value", cfg)
            .unwrap();
        let expired = store.needs_rotation();
        assert_eq!(expired, vec!["api_key".to_string()]);
    }

    #[test]
    fn test_rotate_updates_value() {
        let mut store = SecretStore::new(test_key());
        let cfg = RotationConfig::new(Duration::from_secs(3600), None, Duration::ZERO);
        store
            .seal_with_rotation("db_pass", b"old_pass", cfg)
            .unwrap();
        store.rotate("db_pass", b"new_pass").unwrap();
        let val = store.unseal("db_pass").unwrap();
        assert_eq!(val, b"new_pass");
    }

    #[test]
    fn test_rotate_archives_previous_for_grace_period() {
        let mut store = SecretStore::new(test_key());
        // 1-hour grace period so previous value stays valid.
        let cfg = RotationConfig::new(Duration::from_secs(3600), None, Duration::from_secs(3600));
        store
            .seal_with_rotation("token", b"old_token", cfg)
            .unwrap();
        store.rotate("token", b"new_token").unwrap();

        // Current value is the new one.
        let (val, from_prev) = store.unseal_with_grace("token").unwrap();
        assert_eq!(val, b"new_token");
        assert!(!from_prev);

        // Overwrite current with garbage so unseal fails, proving grace period kicks in.
        store.secrets.get_mut("token").unwrap().ciphertext[0] ^= 0xFF;
        let (val, from_prev) = store.unseal_with_grace("token").unwrap();
        assert_eq!(val, b"old_token");
        assert!(from_prev);
    }

    #[test]
    fn test_grace_period_expired_no_fallback() {
        let mut store = SecretStore::new(test_key());
        // Grace period of zero — previous is never valid.
        let cfg = RotationConfig::new(Duration::from_secs(3600), None, Duration::ZERO);
        store.seal_with_rotation("cred", b"old", cfg).unwrap();
        store.rotate("cred", b"new").unwrap();

        // Corrupt the current value.
        store.secrets.get_mut("cred").unwrap().ciphertext[0] ^= 0xFF;
        // No grace period → should fail.
        assert!(store.unseal_with_grace("cred").is_err());
    }

    #[test]
    fn test_rotation_config_removed_on_delete() {
        let mut store = SecretStore::new(test_key());
        let cfg = RotationConfig::new(Duration::from_secs(60), None, Duration::ZERO);
        store.seal_with_rotation("key", b"val", cfg).unwrap();
        assert!(store.rotation_config("key").is_some());
        store.remove("key");
        assert!(store.rotation_config("key").is_none());
        assert!(!store.contains("key"));
    }

    #[test]
    fn test_resolve_template_uses_grace_period() {
        let mut store = SecretStore::new(test_key());
        let cfg = RotationConfig::new(Duration::from_secs(3600), None, Duration::from_secs(3600));
        store.seal_with_rotation("PW", b"old_pw", cfg).unwrap();
        store.rotate("PW", b"new_pw").unwrap();

        // Template resolution should return the new (current) value.
        let resolved = store.resolve_template("pass={{ secrets.PW }}").unwrap();
        assert_eq!(resolved, "pass=new_pw");
    }
}
