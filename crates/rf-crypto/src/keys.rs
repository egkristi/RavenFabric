use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::error::CryptoError;

/// Long-lived Curve25519 identity key pair.
/// The private key never leaves the system.
#[derive(Clone)]
pub struct StaticKey {
    pub public: [u8; 32],
    private: [u8; 32],
}

impl StaticKey {
    /// Generate a new random key pair.
    pub fn generate() -> Self {
        let builder = snow::Builder::new(
            "Noise_XX_25519_ChaChaPoly_BLAKE2s"
                .parse()
                .expect("static noise pattern is always valid"),
        );
        let keypair = builder
            .generate_keypair()
            .expect("keypair generation with valid pattern cannot fail");

        let mut public = [0u8; 32];
        let mut private = [0u8; 32];
        public.copy_from_slice(&keypair.public);
        private.copy_from_slice(&keypair.private);

        Self { public, private }
    }

    /// Load key pair from file (64 bytes: 32 private + 32 public).
    pub fn load(path: &Path) -> Result<Self, CryptoError> {
        let data = fs::read(path)?;
        if data.len() != 64 {
            return Err(CryptoError::InvalidKey);
        }

        let mut private = [0u8; 32];
        let mut public = [0u8; 32];
        private.copy_from_slice(&data[..32]);
        public.copy_from_slice(&data[32..]);

        Ok(Self { public, private })
    }

    /// Save key pair to file with restrictive permissions (0600).
    pub fn save(&self, path: &Path) -> Result<(), CryptoError> {
        let mut data = Vec::with_capacity(64);
        data.extend_from_slice(&self.private);
        data.extend_from_slice(&self.public);

        // Write to temp file first (atomic)
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &data)?;

        // Set permissions before rename (no window of wrong perms)
        #[cfg(unix)]
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;

        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load from file if exists, otherwise generate and save.
    pub fn load_or_generate(path: &Path) -> Result<Self, CryptoError> {
        if path.exists() {
            Self::load(path)
        } else {
            let key = Self::generate();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            key.save(path)?;
            Ok(key)
        }
    }

    /// Get the private key bytes (internal use only — for Noise builder).
    pub(crate) fn private_bytes(&self) -> &[u8; 32] {
        &self.private
    }

    /// Hex-encoded public key (for display/logging).
    pub fn public_hex(&self) -> String {
        hex::encode(self.public)
    }
}

impl Drop for StaticKey {
    fn drop(&mut self) {
        // Zero private key on drop
        self.private.iter_mut().for_each(|b| *b = 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let key = StaticKey::generate();
        assert_ne!(key.public, [0u8; 32]);
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.key");

        let original = StaticKey::generate();
        original.save(&path).unwrap();

        let loaded = StaticKey::load(&path).unwrap();
        assert_eq!(original.public, loaded.public);
    }

    #[test]
    fn test_load_or_generate_creates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subdir/new.key");

        let key = StaticKey::load_or_generate(&path).unwrap();
        assert!(path.exists());

        // Loading again returns same key
        let key2 = StaticKey::load_or_generate(&path).unwrap();
        assert_eq!(key.public, key2.public);
    }

    #[test]
    fn test_invalid_key_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.key");
        fs::write(&path, b"too short").unwrap();

        let result = StaticKey::load(&path);
        assert!(result.is_err());
    }
}
