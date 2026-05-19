//! RavenFabric MCP Server — Model Context Protocol server for AI agent integration.
//!
//! Implements the MCP protocol (JSON-RPC 2.0 over stdio) to provide
//! policy-controlled system access to AI agents like Claude, Cursor, and Aider.
//!
//! The server exposes RavenFabric capabilities as MCP tools:
//! - `rf_exec` — policy-validated command execution
//! - `rf_query_policy` — pre-flight policy check without execution
//! - `rf_file_read` / `rf_file_write` — filesystem operations subject to path policy
//! - `rf_file_transfer` — copy files with policy enforcement and integrity verification
//! - `rf_list_my_capabilities` — dynamic capability discovery
//! - `rf_audit_query` — self-audit (query own recent actions)
//! - `rf_request_approval` — human-in-loop approval for sensitive ops

mod protocol;
mod server;
mod tools;

#[cfg(feature = "http-sse")]
pub mod http_sse;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, MCP_VERSION};
pub use server::{CallerProfile, CallersConfig, McpServer};
