use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::TransportError;

/// Identifies a remote endpoint for connection.
pub struct Target {
    pub agent_id: String,
    pub relay_url: Option<String>,
    pub meet_token: Option<String>,
}

/// Driver-specific configuration (from policy YAML).
pub type DriverConfig = HashMap<String, String>;

/// A bidirectional async stream (combined AsyncRead + AsyncWrite).
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

/// A transport driver that can establish connections over a specific protocol.
#[async_trait::async_trait]
pub trait Driver: Send + Sync + 'static {
    /// Unique name (e.g., "websocket", "quic", "wireguard").
    fn name(&self) -> &str;

    /// Fast non-blocking check if this driver could work.
    fn available(&self) -> bool;

    /// Establish a connection to a remote target.
    async fn dial(
        &self,
        target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError>;
}
