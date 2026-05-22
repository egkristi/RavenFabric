//! RavenFabric cryptographic primitives.
//!
//! With `std` feature (default): full Noise XX handshake, SecureChannel, key management.
//! Without `std`: minimal frame encryption primitives only (no_std compatible).

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(feature = "std")]
pub mod channel;
#[cfg(feature = "std")]
pub mod error;
#[cfg(feature = "std")]
pub mod keys;
#[cfg(feature = "std")]
pub mod noise;
#[cfg(feature = "std")]
pub mod pq;
#[cfg(feature = "std")]
pub mod resumption;
#[cfg(feature = "std")]
pub mod secrets;

/// HSM/PKCS#11 key provider.
/// Requires feature = "hsm".
pub mod hsm;

/// TPM 2.0 key sealing and remote attestation.
/// Requires feature = "tpm".
pub mod tpm;

/// Minimal frame encryption for no_std environments.
///
/// Provides ChaCha20Poly1305 encrypt/decrypt for framed messages
/// without requiring std, tokio, or snow.
pub mod frame_codec {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    /// Encrypt a plaintext frame with ChaCha20Poly1305.
    ///
    /// Returns ciphertext with appended 16-byte authentication tag.
    pub fn encrypt_frame(
        key: &[u8; 32],
        nonce_bytes: &[u8; 12],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, FrameError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| FrameError::EncryptionFailed)
    }

    /// Decrypt a ciphertext frame with ChaCha20Poly1305.
    ///
    /// Input must include the 16-byte authentication tag.
    pub fn decrypt_frame(
        key: &[u8; 32],
        nonce_bytes: &[u8; 12],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, FrameError> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| FrameError::DecryptionFailed)
    }

    /// Errors from frame encryption/decryption.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FrameError {
        /// Encryption failed (should not happen with valid inputs).
        EncryptionFailed,
        /// Decryption failed (authentication tag mismatch — tampered data).
        DecryptionFailed,
    }

    impl core::fmt::Display for FrameError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Self::EncryptionFailed => write!(f, "frame encryption failed"),
                Self::DecryptionFailed => {
                    write!(f, "frame decryption failed (authentication error)")
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn encrypt_decrypt_roundtrip() {
            let key = [0x42u8; 32];
            let nonce = [0x01u8; 12];
            let plaintext = b"hello ravenfabric no_std";

            let ciphertext = encrypt_frame(&key, &nonce, plaintext).unwrap();
            assert_ne!(&ciphertext[..], plaintext);
            assert_eq!(ciphertext.len(), plaintext.len() + 16); // +16 for auth tag

            let decrypted = decrypt_frame(&key, &nonce, &ciphertext).unwrap();
            assert_eq!(&decrypted[..], plaintext);
        }

        #[test]
        fn decrypt_tampered_data_fails() {
            let key = [0x42u8; 32];
            let nonce = [0x01u8; 12];
            let plaintext = b"sensitive data";

            let mut ciphertext = encrypt_frame(&key, &nonce, plaintext).unwrap();
            // Tamper with ciphertext
            ciphertext[0] ^= 0xFF;

            let result = decrypt_frame(&key, &nonce, &ciphertext);
            assert_eq!(result, Err(FrameError::DecryptionFailed));
        }

        #[test]
        fn wrong_key_fails() {
            let key1 = [0x42u8; 32];
            let key2 = [0x43u8; 32];
            let nonce = [0x01u8; 12];
            let plaintext = b"key mismatch test";

            let ciphertext = encrypt_frame(&key1, &nonce, plaintext).unwrap();
            let result = decrypt_frame(&key2, &nonce, &ciphertext);
            assert_eq!(result, Err(FrameError::DecryptionFailed));
        }

        #[test]
        fn wrong_nonce_fails() {
            let key = [0x42u8; 32];
            let nonce1 = [0x01u8; 12];
            let nonce2 = [0x02u8; 12];
            let plaintext = b"nonce mismatch test";

            let ciphertext = encrypt_frame(&key, &nonce1, plaintext).unwrap();
            let result = decrypt_frame(&key, &nonce2, &ciphertext);
            assert_eq!(result, Err(FrameError::DecryptionFailed));
        }

        #[test]
        fn empty_plaintext() {
            let key = [0xAA; 32];
            let nonce = [0xBB; 12];
            let plaintext = b"";

            let ciphertext = encrypt_frame(&key, &nonce, plaintext).unwrap();
            assert_eq!(ciphertext.len(), 16); // Just the auth tag

            let decrypted = decrypt_frame(&key, &nonce, &ciphertext).unwrap();
            assert_eq!(&decrypted[..], plaintext);
        }

        #[test]
        fn large_plaintext() {
            let key = [0xCC; 32];
            let nonce = [0xDD; 12];
            let plaintext = vec![0x42u8; 65536]; // 64KB

            let ciphertext = encrypt_frame(&key, &nonce, &plaintext).unwrap();
            let decrypted = decrypt_frame(&key, &nonce, &ciphertext).unwrap();
            assert_eq!(decrypted, plaintext);
        }

        #[test]
        fn frame_error_display() {
            assert_eq!(
                FrameError::EncryptionFailed.to_string(),
                "frame encryption failed"
            );
            assert_eq!(
                FrameError::DecryptionFailed.to_string(),
                "frame decryption failed (authentication error)"
            );
        }
    }
}
