//! API key authentication middleware.
//!
//! Clients must supply a valid key in the `X-RF-Key` header.
//! Keys are stored as SHA-256 hashes to avoid keeping plaintext in memory.

use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Validated set of allowed API key hashes.
#[derive(Debug, Clone)]
pub struct ApiKeyStore {
    hashes: HashSet<[u8; 32]>,
}

impl ApiKeyStore {
    /// Create a new store pre-loaded with the given plaintext keys.
    pub fn new(keys: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let hashes = keys
            .into_iter()
            .map(|k| {
                let mut h = Sha256::new();
                h.update(k.as_ref().as_bytes());
                h.finalize().into()
            })
            .collect();
        Self { hashes }
    }

    /// Returns `true` if the presented key is valid.
    pub fn is_valid(&self, key: &str) -> bool {
        let mut h = Sha256::new();
        h.update(key.as_bytes());
        let hash: [u8; 32] = h.finalize().into();
        self.hashes.contains(&hash)
    }

    /// Returns `true` if no keys are configured (open / dev mode).
    pub fn is_open(&self) -> bool {
        self.hashes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key_accepted() {
        let store = ApiKeyStore::new(["secret-token"]);
        assert!(store.is_valid("secret-token"));
    }

    #[test]
    fn invalid_key_rejected() {
        let store = ApiKeyStore::new(["secret-token"]);
        assert!(!store.is_valid("wrong-token"));
    }

    #[test]
    fn empty_store_is_open() {
        let store: ApiKeyStore = ApiKeyStore::new(std::iter::empty::<&str>());
        assert!(store.is_open());
    }
}
