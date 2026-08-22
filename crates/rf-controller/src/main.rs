//! rf-controller — RavenFabric management plane binary.
//!
//! Serves the embedded Web UI dashboard and REST API for fleet management.
//! Agents register via `POST /api/v1/agents/heartbeat`; operators query the
//! agent registry via `GET /api/v1/agents` and the dashboard at `/`.
//!
//! # Usage
//!
//! ```text
//! rf-controller --listen 0.0.0.0:9091 [--token SECRET]
//! ```

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use rf_rpc::controller::{AgentRegistry, ApiDispatcher};
use rf_rpc::http_server::{HttpServerConfig, serve};
use tokio::sync::RwLock;

/// RavenFabric controller (management plane).
#[derive(Parser)]
#[command(
    name = "rf-controller",
    about = "RavenFabric controller — REST API + Web UI dashboard"
)]
struct Args {
    /// Address to bind the HTTP listener (dashboard + REST API).
    #[arg(long, default_value = "0.0.0.0:9091")]
    listen: String,

    /// Optional bearer token required for authenticated API endpoints.
    /// Omit to run in open / dev mode.
    #[arg(long, env = "RF_CONTROLLER_TOKEN")]
    token: Option<String>,

    /// Maximum number of agents tracked in the registry.
    #[arg(long, default_value_t = 10_000)]
    max_agents: u32,

    /// Heartbeat timeout in milliseconds — agents not heard from within this
    /// window are marked stale.
    #[arg(long, default_value_t = 30_000)]
    heartbeat_timeout_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    let registry = AgentRegistry::new(args.max_agents, args.heartbeat_timeout_ms);
    let dispatcher = Arc::new(RwLock::new(ApiDispatcher::new(registry)));

    let config = HttpServerConfig {
        bind_addr: args.listen.clone(),
        auth_token: args.token.clone(),
    };

    tracing::info!(
        listen = %args.listen,
        auth = if args.token.is_some() { "bearer-token" } else { "open" },
        "rf-controller starting"
    );

    // Graceful shutdown on Ctrl+C.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = shutdown_tx.send(true);
    });

    serve(config, dispatcher, shutdown_rx).await?;
    tracing::info!("rf-controller shut down");

    Ok(())
}
