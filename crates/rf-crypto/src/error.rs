use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("noise handshake failed: {0}")]
    Handshake(String),

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
}
