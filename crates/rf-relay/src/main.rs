//! RavenFabric Relay — Stateless encrypted relay broker.
//!
//! The relay never decrypts traffic (end-to-end encryption between agent and client).
//! It simply pairs agents and clients by meet token, then bridges their byte streams.

use clap::Parser;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(name = "rf-relay", about = "RavenFabric stateless relay broker")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:9090")]
    listen: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let cancel = CancellationToken::new();

    // Graceful shutdown on Ctrl+C
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    rf_relay::run_relay(&args.listen, cancel).await
}
