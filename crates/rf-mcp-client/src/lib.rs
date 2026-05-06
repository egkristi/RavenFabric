//! RavenFabric MCP Client SDK
//!
//! A Rust library for building MCP-aware applications that communicate with
//! RavenFabric MCP servers. Supports stdio transport for local integration
//! and provides type-safe wrappers for all RavenFabric MCP tools.
//!
//! # Example
//!
//! ```no_run
//! use rf_mcp_client::{McpClient, StdioTransport};
//!
//! # async fn example() -> Result<(), rf_mcp_client::McpError> {
//! let transport = StdioTransport::spawn("rf-mcp-server", &["--policy", "policy.yaml"])?;
//! let client = McpClient::new(transport);
//! client.initialize().await?;
//!
//! // Execute a command
//! let result = client.exec("ls -la", None, Some("listing files")).await?;
//! println!("Output: {}", result.output);
//!
//! // Query policy before executing
//! let decision = client.query_policy("rm -rf /").await?;
//! assert!(!decision.allowed);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod protocol;
mod transport;

pub use client::McpClient;
pub use error::McpError;
pub use protocol::{ExecResult, FileContent, PolicyDecision, ToolCapability};
pub use transport::StdioTransport;
