//! RavenFabric Relay — Stateless encrypted relay broker.
//!
//! The relay never decrypts traffic (end-to-end encryption between agent and client).
//! It simply pairs agents and clients by meet token, then bridges their byte streams.

use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Parser)]
#[command(name = "rf-relay", about = "RavenFabric stateless relay broker")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:9090")]
    listen: String,

    /// HMAC secret for meet token verification (optional).
    /// Can also be set via RELAY_SECRET env var.
    #[arg(short, long, env = "RELAY_SECRET")]
    secret: Option<String>,

    /// Enable compatibility mode for cross-platform relay connections.
    /// Adds a small delay between forwarded messages to prevent race conditions
    /// on certain platform combinations (e.g., macOS→Linux via snow-0.10.0).
    /// Use this if you see "Noise XX handshake failed: Error::Input" errors
    /// when connecting from macOS through a Linux relay.
    #[arg(long)]
    compat_mode: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let cancel = CancellationToken::new();

    // Graceful shutdown on Ctrl+C
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    let forward_config = rf_relay::cross_region::ForwardConfig {
        compat_mode: args.compat_mode,
        ..Default::default()
    };

    if args.compat_mode {
        info!("compatibility mode enabled — adding inter-message delays for cross-platform relay");
    }

    rf_relay::run_relay_full(&args.listen, cancel, args.secret, forward_config).await
}
