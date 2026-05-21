//! rf-ingress — HTTP reverse-proxy ingress gateway for RavenFabric.
//!
//! Usage:
//!   rf-ingress --listen 0.0.0.0:8088 [--key TOKEN] [--rate-limit 300]

use anyhow::Result;
use clap::Parser;
use rf_ingress::{router::RoutingTable, server::IngressConfig};

/// RavenFabric HTTP ingress gateway.
#[derive(Parser)]
#[command(name = "rf-ingress", about = "RavenFabric HTTP ingress gateway")]
struct Args {
    /// Address to bind the HTTP listener.
    #[arg(long, default_value = "0.0.0.0:8088")]
    listen: String,

    /// API key(s) required in the X-RF-Key header.  May be specified multiple
    /// times.  If omitted, the server operates in open / dev mode.
    #[arg(long = "key")]
    keys: Vec<String>,

    /// Maximum inbound requests per minute per IP before rate-limiting.
    #[arg(long, default_value_t = 300)]
    rate_limit: u32,

    /// Upstream request timeout in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    upstream_timeout_ms: u64,

    /// Maximum upstream response size in bytes.
    #[arg(long, default_value_t = 10_485_760)]
    max_response_bytes: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rf_ingress=info".parse()?),
        )
        .init();

    let args = Args::parse();

    let listen: std::net::SocketAddr = args.listen.parse()?;

    let config = IngressConfig {
        listen,
        api_keys: args.keys,
        rate_limit_rpm: args.rate_limit,
        upstream_timeout_ms: args.upstream_timeout_ms,
        max_response_bytes: args.max_response_bytes,
        audit_path: None,
    };

    let routing_table = RoutingTable::new();
    rf_ingress::run_ingress(config, routing_table).await
}
