use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CryptoError {
    #[error("noise handshake failed: {0}")]
    Handshake(String),

    #[error("noise handshake input error (buffer too small or invalid state): {0}")]
    HandshakeInput(String),

    #[error("encryption failed: {0}")]
    Encrypt(String),

    #[error("decryption failed: {0}")]
    Decrypt(String),

    #[error("tamper detected: MAC verification failed (possible MITM)")]
    TamperDetected,

    #[error("frame injection detected: unexpected bytes in protocol framing")]
    FrameInjection,

    #[error("key file error: {0}")]
    KeyFile(#[from] std::io::Error),

    #[error("invalid key format")]
    InvalidKey,

    #[error("peer disconnected")]
    Disconnected,

    #[error("frame too large: {size} bytes (max {max})")]
    FrameTooLarge { size: usize, max: usize },

    #[error("HSM error: {0}")]
    Hsm(String),

    #[error("HSM unavailable — worker thread disconnected")]
    HsmUnavailable,

    #[error("HSM does not support X25519 key derivation; use a PKCS#11 v3.0 module")]
    HsmX25519Unsupported,

    #[error("FIPS mode violation: {0}")]
    FipsViolation(String),

    #[error("TPM error: {0}")]
    Tpm(String),

    #[error("TPM seal failed: PCR mismatch or TPM unavailable")]
    TpmSealFailed,

    #[error("TPM unseal failed: PCR state changed or tampered key")]
    TpmUnsealFailed,
}
