//! Sealed secret store — encrypted at rest, decrypted only at execution time.
//!
//! Uses ChaCha20-Poly1305 (IETF) for authenticated encryption of secrets.
//! Secrets are stored as encrypted blobs that can only be decrypted
//! by the agent holding the sealing key.

use std::collections::HashMap;

use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use rand::RngCore;

use crate::error::CryptoError;

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
}

impl SecretStore {
    /// Create a new secret store with the given 256-bit sealing key.
    pub fn new(seal_key: [u8; 32]) -> Self {
        Self {
            seal_key,
            secrets: HashMap::new(),
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

    /// Unseal (decrypt) a secret by name. Only succeeds with the correct seal key.
    pub fn unseal(&self, name: &str) -> Result<Vec<u8>, CryptoError> {
        let sealed = self
            .secrets
            .get(name)
            .ok_or_else(|| CryptoError::Decrypt(format!("secret '{}' not found", name)))?;

        let cipher = ChaCha20Poly1305::new((&self.seal_key).into());
        let nonce = Nonce::from_slice(&sealed.nonce);
        cipher
            .decrypt(nonce, sealed.ciphertext.as_ref())
            .map_err(|_| CryptoError::TamperDetected)
    }

    /// Check if a secret exists.
    pub fn contains(&self, name: &str) -> bool {
        self.secrets.contains_key(name)
    }

    /// Remove a secret from the store.
    pub fn remove(&mut self, name: &str) -> bool {
        self.secrets.remove(name).is_some()
    }

    /// List all secret names.
    pub fn list(&self) -> Vec<&str> {
        self.secrets.keys().map(|s| s.as_str()).collect()
    }

    /// Resolve `{{ secrets.KEY }}` patterns in a string.
    /// Returns the string with all secret references replaced by their decrypted values.
    pub fn resolve_template(&self, template: &str) -> Result<String, CryptoError> {
        let mut result = template.to_string();
        while let Some(start) = result.find("{{ secrets.") {
            let after_prefix = start + "{{ secrets.".len();
            let end = result[after_prefix..]
                .find(" }}")
                .ok_or_else(|| CryptoError::Decrypt("unclosed secret reference".into()))?
                + after_prefix;

            let name = result[after_prefix..end].to_string();
            let plaintext = self.unseal(&name)?;
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
}
