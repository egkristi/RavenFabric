//! RavenFabric Agent — connects to relay, authenticates, and executes RPC requests.
//! Supports configuration via raven.toml, reconnect with exponential backoff, and graceful shutdown.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use rand::Rng as _;
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

    /// Prometheus metrics endpoint address (e.g., 127.0.0.1:9100). Empty to disable.
    #[arg(long)]
    metrics_addr: Option<String>,
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
    metrics_addr: Option<String>,
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
    metrics_addr: Option<String>,
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
        metrics_addr: args.metrics_addr.clone().or(config.agent.metrics_addr),
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

    // Start Prometheus metrics endpoint if configured
    if let Some(ref addr) = cfg.metrics_addr {
        use rf_executor::metrics_server::{MetricsServerConfig, start_metrics_server};
        let config = MetricsServerConfig {
            bind_addr: addr.clone(),
        };
        match start_metrics_server(config).await {
            Ok(_handle) => info!("prometheus metrics endpoint on {}", addr),
            Err(e) => warn!("failed to start metrics endpoint on {}: {}", addr, e),
        }
    }

    // Set up SIGHUP handler for policy hot-reload (Unix only)
    #[cfg(unix)]
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

    // Spawn policy reload task (Unix only)
    #[cfg(unix)]
    {
        let policy_reload = policy.clone();
        let policy_path_reload = cfg.policy_path.clone();
        tokio::spawn(async move {
            loop {
                sighup.recv().await;
                info!(
                    "SIGHUP received, reloading policy from {}",
                    policy_path_reload.display()
                );
                match RpcPolicy::load(&policy_path_reload) {
                    Ok(new_policy) => {
                        let mut w = policy_reload.write().await;
                        *w = new_policy;
                        info!("policy reloaded successfully");
                    }
                    Err(e) => {
                        error!("policy reload failed (keeping old policy): {}", e);
                    }
                }
            }
        });
    }

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
        let jitter = rand::rng().random_range(0..=capped / 4);
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
    let executor = Executor::new(policy.clone(), audit.clone(), hex::encode(peer_key))
        .with_agent_id(cfg.id.clone())
        .with_start_time(std::time::Instant::now());

    // RPC loop with graceful shutdown
    info!("agent {} ready, waiting for RPC requests", cfg.id);
    loop {
        let data = tokio::select! {
            result = chan.recv() => {
                match result {
                    Ok(d) => {
                        // Empty payload = close-notify from peer
                        if d.is_empty() {
                            info!("received close-notify from peer");
                            return Ok(());
                        }
                        d
                    }
                    Err(rf_crypto::error::CryptoError::TamperDetected) => {
                        error!("TAMPER DETECTED: MAC verification failed — possible MITM attack");
                        let _ = audit.log(rf_audit::types::AuditEntry {
                            timestamp: chrono::Utc::now(),
                            request_id: "SECURITY".into(),
                            action: "tamper_detected".into(),
                            command: None,
                            decision: "abandon_path".into(),
                            matched_rule: "MAC verification failure".into(),
                            exit_code: None,
                            duration_ms: 0,
                            caller_key: String::new(),
                        });
                        return Err(anyhow::anyhow!("tamper detected: MAC verification failed"));
                    }
                    Err(rf_crypto::error::CryptoError::FrameInjection) => {
                        error!("FRAME INJECTION: unexpected bytes in protocol framing");
                        let _ = audit.log(rf_audit::types::AuditEntry {
                            timestamp: chrono::Utc::now(),
                            request_id: "SECURITY".into(),
                            action: "frame_injection".into(),
                            command: None,
                            decision: "abandon_path".into(),
                            matched_rule: "invalid frame size".into(),
                            exit_code: None,
                            duration_ms: 0,
                            caller_key: String::new(),
                        });
                        return Err(anyhow::anyhow!("frame injection detected"));
                    }
                    Err(e) => return Err(anyhow::anyhow!("channel recv: {}", e)),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT during session, sending close-notify...");
                if let Err(e) = chan.close_notify().await {
                    warn!("failed to send close-notify: {}", e);
                }
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
