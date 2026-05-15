//! RPC error types.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RpcError {
    #[error("codec error: {0}")]
    Codec(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("session closed")]
    SessionClosed,

    #[error("timeout")]
    Timeout,
}
