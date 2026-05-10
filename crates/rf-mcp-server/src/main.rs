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
    /// Supports multiple tokens (comma-separated) for rotation grace periods.
    #[arg(long, env = "RF_API_TOKEN")]
    api_token: Option<String>,

    /// Path to a file containing the API token. Re-read on each connection for rotation.
    /// Takes precedence over --api-token if both are set.
    #[arg(long, env = "RF_API_TOKEN_FILE")]
    api_token_file: Option<std::path::PathBuf>,

    /// Webhook URL for anomaly/security alert notifications.
    /// Receives POST with JSON payload when anomaly score exceeds threshold.
    #[arg(long, env = "RF_ALERT_WEBHOOK")]
    alert_webhook: Option<String>,

    /// Path to callers config (TOML) for RBAC per-caller policy profiles.
    /// Maps API tokens to different policy files for fine-grained access control.
    #[arg(long, env = "RF_CALLERS")]
    callers: Option<std::path::PathBuf>,

    /// Maximum tool calls per minute (rate limiting). Default: 60.
    #[arg(long, env = "RF_RATE_LIMIT")]
    rate_limit: Option<u32>,

    /// Run in HTTP+SSE mode instead of stdio. Specify listen address (e.g., "0.0.0.0:8080").
    /// Requires the `http-sse` feature to be enabled.
    #[arg(long, env = "RF_HTTP_LISTEN")]
    http_listen: Option<String>,

    /// Log level for stderr diagnostics.
    #[arg(long, default_value = "info", env = "RF_LOG_LEVEL")]
    log_level: String,

    /// Regex patterns for commands requiring human approval before execution.
    /// Commands matching any pattern will be blocked until an operator approves.
    /// Can be specified multiple times: --approval-pattern "rm .*" --approval-pattern "shutdown.*"
    #[arg(long = "approval-pattern", env = "RF_APPROVAL_PATTERNS")]
    approval_patterns: Vec<String>,
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

    // Resolve API token: file takes precedence over CLI/env
    let api_token = if let Some(ref path) = cli.api_token_file {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read token file {}: {}", path.display(), e))?;
        let token = contents.trim().to_string();
        if token.is_empty() { None } else { Some(token) }
    } else {
        cli.api_token
    };

    // Load RBAC caller profiles if configured
    let caller_profiles = if let Some(ref callers_path) = cli.callers {
        let config = rf_mcp_server::CallersConfig::load(callers_path)?;
        info!(count = config.callers.len(), "loaded caller profiles");
        config.callers
    } else {
        Vec::new()
    };

    // HTTP+SSE mode (multi-user server deployment)
    #[cfg(feature = "http-sse")]
    if let Some(ref listen_addr) = cli.http_listen {
        let config = rf_mcp_server::http_sse::HttpSseConfig {
            listen_addr: listen_addr.clone(),
            policy_path: cli.policy.clone(),
            audit_path: cli.audit.clone(),
            caller_key: cli.caller_key.clone(),
            api_token: api_token.clone(),
            max_requests_per_minute: cli.rate_limit,
            alert_webhook: cli.alert_webhook.clone(),
            caller_profiles,
            approval_patterns: cli.approval_patterns.clone(),
        };
        return rf_mcp_server::http_sse::run_http_sse(config).await;
    }

    #[cfg(not(feature = "http-sse"))]
    if cli.http_listen.is_some() {
        anyhow::bail!(
            "HTTP+SSE mode requires the 'http-sse' feature. Rebuild with: cargo build -p rf-mcp-server --features http-sse"
        );
    }

    let server = McpServer::new(
        cli.policy.as_deref(),
        cli.audit.as_deref(),
        &cli.caller_key,
        api_token,
        cli.rate_limit,
        cli.alert_webhook,
        caller_profiles,
        &cli.approval_patterns,
    )?;

    server.run_stdio().await?;

    Ok(())
}
