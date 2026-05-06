//! RavenFabric MCP Server binary.
//!
//! Runs as a child process communicating over stdio (JSON-RPC 2.0).
//! Used by Claude Desktop, Cursor, and other MCP-compatible AI tools.
//!
//! # Usage
//!
//! ```bash
//! # With Claude Desktop (claude_desktop_config.json):
//! { "mcpServers": { "ravenfabric": { "command": "rf-mcp-server", "args": ["--policy", "policy.yaml"] } } }
//!
//! # With Claude Code:
//! claude mcp add ravenfabric -- rf-mcp-server --policy policy.yaml
//!
//! # Standalone test:
//! echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | rf-mcp-server --policy policy.yaml
//! ```

use std::path::PathBuf;

use clap::Parser;
use tracing::info;

use rf_mcp_server::McpServer;

#[derive(Parser)]
#[command(
    name = "rf-mcp-server",
    about = "RavenFabric MCP server — policy-controlled AI agent access",
    version
)]
struct Cli {
    /// Path to the RPC policy YAML file.
    #[arg(short, long, env = "RF_POLICY_PATH")]
    policy: Option<PathBuf>,

    /// Path to the audit log file (JSON-lines).
    #[arg(short, long, env = "RF_AUDIT_PATH")]
    audit: Option<PathBuf>,

    /// Caller identity key (defaults to 'mcp-session').
    #[arg(long, default_value = "mcp-session")]
    caller_key: String,

    /// API token for authentication. If set, clients must include it in initialize params.
    /// Can also be set via RF_API_TOKEN environment variable.
    #[arg(long, env = "RF_API_TOKEN")]
    api_token: Option<String>,

    /// Maximum tool calls per minute (rate limiting). Default: 60.
    #[arg(long, env = "RF_RATE_LIMIT")]
    rate_limit: Option<u32>,

    /// Log level for stderr diagnostics.
    #[arg(long, default_value = "info", env = "RF_LOG_LEVEL")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Logging goes to stderr (stdout is reserved for MCP JSON-RPC protocol)
    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .with_writer(std::io::stderr)
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        policy = ?cli.policy,
        "rf-mcp-server starting"
    );

    let server = McpServer::new(
        cli.policy.as_deref(),
        cli.audit.as_deref(),
        &cli.caller_key,
        cli.api_token,
        cli.rate_limit,
    )?;

    server.run_stdio().await?;

    Ok(())
}
