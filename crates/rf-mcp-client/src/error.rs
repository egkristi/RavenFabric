//! Error types for the MCP client SDK.

use thiserror::Error;

/// Errors that can occur during MCP client operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Transport I/O error (stdio pipe broken, process died).
    #[error("transport error: {0}")]
    Transport(String),

    /// JSON serialization/deserialization error.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Server returned a JSON-RPC error response.
    #[error("server error ({code}): {message}")]
    ServerError { code: i64, message: String },

    /// The requested tool was denied by policy.
    #[error("policy denied: {0}")]
    PolicyDenied(String),

    /// Request timed out.
    #[error("request timed out after {0}ms")]
    Timeout(u64),

    /// Client not initialized (must call initialize() first).
    #[error("client not initialized — call initialize() first")]
    NotInitialized,

    /// Server process exited unexpectedly.
    #[error("server process exited: {0}")]
    ProcessExited(String),
}

impl From<serde_json::Error> for McpError {
    fn from(e: serde_json::Error) -> Self {
        Self::Protocol(e.to_string())
    }
}

impl From<std::io::Error> for McpError {
    fn from(e: std::io::Error) -> Self {
        Self::Transport(e.to_string())
    }
}
