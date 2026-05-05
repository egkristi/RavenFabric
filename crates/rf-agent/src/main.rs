//! RavenFabric Agent — connects to relay, authenticates, and executes RPC requests.
//! Supports configuration via raven.toml, reconnect with exponential backoff, and graceful shutdown.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use rand::Rng;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use rf_audit::logger::FileAuditLogger;
use rf_crypto::channel::SecureChannel;
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;
use rf_executor::command::Executor;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::codec;
use rf_rpc::types::{Request, Response};
use rf_transport::driver::{Driver, Target};
use rf_transport::websocket::WebSocketDriver;

#[derive(Parser)]
#[command(name = "rf-agent", about = "RavenFabric agent")]
struct Args {
    /// Path to config file (raven.toml)
    #[arg(short, long, default_value = "raven.toml")]
    config: PathBuf,

    /// Agent ID (overrides config)
    #[arg(short = 'i', long)]
    id: Option<String>,

    /// Relay WebSocket URL (overrides config)
    #[arg(short, long)]
    relay: Option<String>,

    /// Meet token for relay pairing (overrides config)
    #[arg(short, long)]
    token: Option<String>,

    /// Path to agent key file (overrides config)
    #[arg(short, long)]
    key_path: Option<PathBuf>,

    /// Path to policy YAML file (overrides config)
    #[arg(short, long)]
    policy_path: Option<PathBuf>,

    /// Path to audit log file (overrides config)
    #[arg(short, long)]
    audit_path: Option<PathBuf>,
}

/// Configuration file format (raven.toml).
#[derive(Debug, Deserialize, Default)]
struct Config {
    #[serde(default)]
    agent: AgentConfig,
    #[serde(default)]
    transport: TransportConfig,
}

#[derive(Debug, Deserialize, Default)]
struct AgentConfig {
    id: Option<String>,
    relay: Option<String>,
    token: Option<String>,
    key_path: Option<String>,
    policy_path: Option<String>,
    audit_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransportConfig {
    reconnect_interval: Option<u64>,
    max_retries: Option<u64>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            reconnect_interval: Some(5),
            max_retries: Some(0), // 0 = infinite
        }
    }
}

/// Resolved configuration (CLI > config file > defaults).
struct ResolvedConfig {
    id: String,
    relay: String,
    token: String,
    key_path: PathBuf,
    policy_path: PathBuf,
    audit_path: PathBuf,
    reconnect_interval: u64,
    max_retries: u64,
}

fn load_config(args: &Args) -> anyhow::Result<ResolvedConfig> {
    let config: Config = if args.config.exists() {
        let content = std::fs::read_to_string(&args.config)?;
        toml::from_str(&content)?
    } else {
        Config::default()
    };

    Ok(ResolvedConfig {
        id: args
            .id
            .clone()
            .or(config.agent.id)
            .unwrap_or_else(|| "agent".to_string()),
        relay: args
            .relay
            .clone()
            .or(config.agent.relay)
            .unwrap_or_else(|| "ws://127.0.0.1:9090".to_string()),
        token: args
            .token
            .clone()
            .or(config.agent.token)
            .unwrap_or_else(|| "default".to_string()),
        key_path: args
            .key_path
            .clone()
            .or(config.agent.key_path.map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("agent.key")),
        policy_path: args
            .policy_path
            .clone()
            .or(config.agent.policy_path.map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("policy.yaml")),
        audit_path: args
            .audit_path
            .clone()
            .or(config.agent.audit_path.map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("audit.jsonl")),
        reconnect_interval: config.transport.reconnect_interval.unwrap_or(5),
        max_retries: config.transport.max_retries.unwrap_or(0),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let cfg = load_config(&args)?;

    // Load or generate identity key
    let key = StaticKey::load_or_generate(&cfg.key_path)?;
    info!("agent {} public key: {}", cfg.id, key.public_hex());

    // Load policy
    let policy = RpcPolicy::load(&cfg.policy_path)?;
    let policy = Arc::new(RwLock::new(policy));
    info!("policy loaded from {}", cfg.policy_path.display());

    // Open audit logger
    let audit: Arc<dyn rf_audit::logger::AuditLogger> =
        Arc::new(FileAuditLogger::new(cfg.audit_path.clone())?);
    info!("audit log: {}", cfg.audit_path.display());

    info!("agent {} starting, relay: {}", cfg.id, cfg.relay);

    // Reconnect loop with exponential backoff + jitter
    let mut attempt: u64 = 0;
    loop {
        // Check if we've exceeded max retries (0 = infinite)
        if cfg.max_retries > 0 && attempt >= cfg.max_retries {
            error!("max retries ({}) exceeded, shutting down", cfg.max_retries);
            break;
        }

        match run_session(&cfg, &key, &policy, &audit).await {
            Ok(()) => {
                info!("session ended cleanly");
                attempt = 0; // Reset on successful session
            }
            Err(e) => {
                attempt += 1;
                warn!("session error (attempt {}): {}", attempt, e);
            }
        }

        // Exponential backoff: base * 2^attempt, capped at 60s, with jitter
        let base = cfg.reconnect_interval;
        let backoff = base.saturating_mul(1u64 << attempt.min(5));
        let capped = backoff.min(60);
        let jitter = rand::thread_rng().gen_range(0..=capped / 4);
        let wait = capped + jitter;

        info!("reconnecting in {}s...", wait);

        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(wait)) => {}
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down");
                break;
            }
        }
    }

    info!("agent {} shut down", cfg.id);
    Ok(())
}

async fn run_session(
    cfg: &ResolvedConfig,
    key: &StaticKey,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
) -> anyhow::Result<()> {
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: cfg.id.clone(),
        relay_url: Some(cfg.relay.clone()),
        meet_token: Some(cfg.token.clone()),
    };

    info!("connecting to relay: {}", cfg.relay);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Noise handshake (agent is responder)
    info!("performing Noise XX handshake...");
    let (state, peer_key) = handshake(&mut stream, false, key).await?;
    info!("handshake complete, peer key: {}", hex::encode(peer_key));

    // SecureChannel
    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    // Executor
    let executor = Executor::new(policy.clone(), audit.clone(), hex::encode(peer_key));

    // RPC loop with graceful shutdown
    info!("agent {} ready, waiting for RPC requests", cfg.id);
    loop {
        let data = tokio::select! {
            result = chan.recv() => {
                match result {
                    Ok(d) => d,
                    Err(e) => return Err(anyhow::anyhow!("channel recv: {}", e)),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT during session, draining...");
                return Ok(());
            }
        };

        let request: Request = match codec::decode(&data) {
            Ok(r) => r,
            Err(e) => {
                error!("failed to decode request: {}", e);
                continue;
            }
        };

        info!(
            "received request: {} action={:?}",
            request.id, request.action
        );
        let response: Response = executor.handle(request).await;

        let resp_data = codec::encode(&response)?;
        if let Err(e) = chan.send(&resp_data).await {
            return Err(anyhow::anyhow!("channel send: {}", e));
        }
    }
}
