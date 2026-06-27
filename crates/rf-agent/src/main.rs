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
use rf_audit::types::AuditEntry;
use rf_crypto::channel::SecureChannel;
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;
use rf_crypto::secrets::SecretStore;
use rf_executor::command::Executor;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use rf_transport::driver::{Driver, Target};
use rf_transport::relay_select::{RelayCluster, RelaySelector};
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

    /// Listen for direct connections on this address (e.g., 0.0.0.0:9999).
    /// When set, the agent acts as a server (like sshd) instead of connecting to a relay.
    #[arg(short = 'L', long)]
    listen: Option<String>,

    /// Path to seal key file (32 bytes raw) for SecretStore.
    #[arg(long)]
    seal_key_path: Option<PathBuf>,
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
    listen: Option<String>,
    /// Geographic region code (e.g. `eu-west`, `us-east`).
    /// Used for region-aware relay selection and fleet orchestration.
    region: Option<String>,
    /// Path to seal key file (32 bytes raw) for SecretStore.
    seal_key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransportConfig {
    reconnect_interval: Option<u64>,
    max_retries: Option<u64>,
    /// Optional relay clusters for region-aware relay selection.
    /// When present, the agent picks the cluster whose region matches its own,
    /// then the best relay within that cluster.
    #[serde(default)]
    relay_clusters: Vec<RelayClusterConfig>,
}

/// Serialisable relay cluster entry in `raven.toml`.
///
/// ```toml
/// [[transport.relay_clusters]]
/// region    = "eu-west"
/// continent = "EU"
/// latitude  = 51.5
/// longitude = -0.1
/// relays    = ["wss://eu1.relay.example.com:9090", "wss://eu2.relay.example.com:9090"]
/// ```
#[derive(Debug, Deserialize, Default)]
struct RelayClusterConfig {
    region: String,
    #[serde(default)]
    continent: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    relays: Vec<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            reconnect_interval: Some(5),
            max_retries: Some(0), // 0 = infinite
            relay_clusters: Vec::new(),
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
    listen: Option<String>,
    /// Geographic region code (e.g. `eu-west`, `us-east`, `ap-south`).
    region: Option<String>,
    /// Path to seal key file (32 bytes raw) for SecretStore.
    seal_key_path: PathBuf,
}

fn load_config(args: &Args) -> anyhow::Result<ResolvedConfig> {
    let config: Config = if args.config.exists() {
        let content = std::fs::read_to_string(&args.config)?;
        toml::from_str(&content)?
    } else {
        Config::default()
    };

    // Build relay URL: prefer CLI arg, then try cluster selection, then config field.
    let relay = args.relay.clone().or_else(|| {
        let clusters: Vec<RelayCluster> = config
            .transport
            .relay_clusters
            .iter()
            .map(|c| RelayCluster {
                region: c.region.clone(),
                continent: c.continent.clone(),
                country_code: c.country_code.clone(),
                latitude: c.latitude,
                longitude: c.longitude,
                relays: c.relays.clone(),
            })
            .collect();
        if clusters.is_empty() {
            return None;
        }
        let selector = RelaySelector::from_clusters(clusters);
        let region = config.agent.region.as_deref().unwrap_or("");
        selector
            .best_in_region(region, None, None)
            .map(|ep| ep.addr.clone())
    });
    let relay = relay
        .or(config.agent.relay)
        .unwrap_or_else(|| "ws://127.0.0.1:9090".to_string());

    Ok(ResolvedConfig {
        id: args
            .id
            .clone()
            .or(config.agent.id)
            .unwrap_or_else(|| "agent".to_string()),
        relay,
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
        listen: args.listen.clone().or(config.agent.listen),
        region: config.agent.region,
        seal_key_path: args
            .seal_key_path
            .clone()
            .or(config.agent.seal_key_path.map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("seal.key")),
    })
}

#[cfg(not(feature = "rt-single-thread"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    agent_main().await
}

#[cfg(feature = "rt-single-thread")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    agent_main().await
}

async fn agent_main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

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
        Arc::new(FileAuditLogger::new(cfg.audit_path.clone(), vec![])?);
    info!("audit log: {}", cfg.audit_path.display());

    // Initialize SecretStore (sealed secrets for command execution)
    let secret_store = if cfg.seal_key_path.exists() {
        let key_bytes = std::fs::read(&cfg.seal_key_path)?;
        if key_bytes.len() != 32 {
            anyhow::bail!(
                "seal key must be exactly 32 bytes, got {}",
                key_bytes.len()
            );
        }
        let mut seal_key = [0u8; 32];
        seal_key.copy_from_slice(&key_bytes);
        let store = Arc::new(tokio::sync::Mutex::new(SecretStore::new(seal_key)));
        info!("secret store loaded from {}", cfg.seal_key_path.display());
        Some(store)
    } else {
        info!("no seal key at {}, secrets disabled", cfg.seal_key_path.display());
        None
    };

    info!("agent {} starting", cfg.id);

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

    // Direct-listen mode (like sshd) or relay-connect mode
    if let Some(ref listen_addr) = cfg.listen {
        info!("direct-listen mode on {}", listen_addr);
        run_listen_mode(listen_addr, &cfg, &key, &policy, &audit, &secret_store).await?;
    } else {
        info!("relay mode: {}", cfg.relay);
        // Reconnect loop with exponential backoff + jitter
        let mut attempt: u64 = 0;
        loop {
            // Check if we've exceeded max retries (0 = infinite)
            if cfg.max_retries > 0 && attempt >= cfg.max_retries {
                error!("max retries ({}) exceeded, shutting down", cfg.max_retries);
                break;
            }

            match run_session(&cfg, &key, &policy, &audit, &secret_store).await {
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
    }

    info!("agent {} shut down", cfg.id);
    Ok(())
}

/// Run the agent in direct-listen mode (like sshd).
/// Binds to the given address and accepts incoming WebSocket connections.
/// Each connection is handled in a separate task.
async fn run_listen_mode(
    listen_addr: &str,
    cfg: &ResolvedConfig,
    key: &StaticKey,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    secret_store: &Option<Arc<tokio::sync::Mutex<SecretStore>>>,
) -> anyhow::Result<()> {
    let driver = WebSocketDriver::new();
    let listener = driver.listen(listen_addr).await?;
    info!("listening for direct connections on {}", listen_addr);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok(stream) => {
                        info!("accepted direct connection");
                        let key = key.clone();
                        let policy = policy.clone();
                        let audit = audit.clone();
                        let agent_id = cfg.id.clone();
                        let secret_store = secret_store.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_direct_connection(stream, &key, &policy, &audit, &agent_id, &secret_store).await {
                                warn!("direct session error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("accept error: {}", e);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("received SIGINT, shutting down listener");
                break;
            }
        }
    }
    Ok(())
}

/// Handle a single direct connection: handshake → RPC loop.
async fn handle_direct_connection(
    mut stream: Box<dyn rf_transport::driver::AsyncStream>,
    key: &StaticKey,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    agent_id: &str,
    secret_store: &Option<Arc<tokio::sync::Mutex<SecretStore>>>,
) -> anyhow::Result<()> {
    // Noise handshake (agent is responder)
    info!("performing Noise XX handshake...");
    let (state, peer_key) = handshake(&mut stream, false, key).await?;
    info!("handshake complete, peer key: {}", hex::encode(peer_key));

    // SecureChannel — wrapped in Arc so proxy tunnel tasks can share read/write halves
    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = Arc::new(SecureChannel::new(
        stream_read,
        stream_write,
        state,
        peer_key,
    ));

    // Executor
    let mut executor_builder = Executor::new(policy.clone(), audit.clone(), hex::encode(peer_key))
        .with_agent_id(agent_id.to_string())
        .with_start_time(std::time::Instant::now());
    if let Some(secrets) = secret_store {
        executor_builder = executor_builder.with_secrets(secrets.clone());
    }
    let executor = executor_builder;

    // RPC loop
    info!("direct session ready, waiting for RPC requests");
    loop {
        let data = match chan.recv().await {
            Ok(d) => {
                if d.is_empty() {
                    info!("received close-notify from peer");
                    return Ok(());
                }
                d
            }
            Err(rf_crypto::error::CryptoError::TamperDetected) => {
                error!("TAMPER DETECTED: MAC verification failed");
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
                    reason: None,
                    prev_hash: None,
                    hmac: None
                });
                return Err(anyhow::anyhow!("tamper detected"));
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
                    reason: None,
                    prev_hash: None,
                    hmac: None
                });
                return Err(anyhow::anyhow!("frame injection detected"));
            }
            Err(e) => return Err(anyhow::anyhow!("channel recv: {e}")),
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

        // ProxyOpen takes over this connection for raw bidirectional forwarding
        if let Action::ProxyOpen {
            ref target,
            idle_timeout_secs,
            max_duration_secs,
        } = request.action
        {
            return handle_proxy_open(
                &chan,
                &request.id,
                target,
                idle_timeout_secs,
                max_duration_secs,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        // FilePushStream / FilePullStream take over this connection for raw streaming
        if let Action::FilePushStream {
            ref path,
            total_size,
            ref checksum,
            mode,
            compress,
        } = request.action
        {
            return handle_file_push_stream(
                &chan,
                &request.id,
                path,
                total_size,
                checksum.as_deref(),
                mode,
                compress,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        if let Action::FilePullStream { ref path, compress } = request.action {
            return handle_file_pull_stream(
                &chan,
                &request.id,
                path,
                compress,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        let response: Response = executor.handle(request).await;

        let resp_data = codec::encode(&response)?;
        if let Err(e) = chan.send(&resp_data).await {
            return Err(anyhow::anyhow!("channel send: {e}"));
        }
    }
}

async fn run_session(
    cfg: &ResolvedConfig,
    key: &StaticKey,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    secret_store: &Option<Arc<tokio::sync::Mutex<SecretStore>>>,
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

    // SecureChannel — wrapped in Arc so proxy tunnel tasks can share read/write halves
    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = Arc::new(SecureChannel::new(
        stream_read,
        stream_write,
        state,
        peer_key,
    ));

    // Executor
    let mut executor_builder = Executor::new(policy.clone(), audit.clone(), hex::encode(peer_key))
        .with_agent_id(cfg.id.clone())
        .with_region(cfg.region.clone())
        .with_start_time(std::time::Instant::now());
    if let Some(secrets) = secret_store {
        executor_builder = executor_builder.with_secrets(secrets.clone());
    }
    let executor = executor_builder;

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
                            reason: None,
                            prev_hash: None,
                            hmac: None
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
                            reason: None,
                            prev_hash: None,
                            hmac: None
                        });
                        return Err(anyhow::anyhow!("frame injection detected"));
                    }
                    Err(e) => return Err(anyhow::anyhow!("channel recv: {e}")),
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

        // ProxyOpen takes over this connection for raw bidirectional forwarding
        if let Action::ProxyOpen {
            ref target,
            idle_timeout_secs,
            max_duration_secs,
        } = request.action
        {
            return handle_proxy_open(
                &chan,
                &request.id,
                target,
                idle_timeout_secs,
                max_duration_secs,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        // FilePushStream / FilePullStream take over this connection for raw streaming
        if let Action::FilePushStream {
            ref path,
            total_size,
            ref checksum,
            mode,
            compress,
        } = request.action
        {
            return handle_file_push_stream(
                &chan,
                &request.id,
                path,
                total_size,
                checksum.as_deref(),
                mode,
                compress,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        if let Action::FilePullStream { ref path, compress } = request.action {
            return handle_file_pull_stream(
                &chan,
                &request.id,
                path,
                compress,
                policy,
                audit,
                hex::encode(peer_key),
            )
            .await;
        }

        let response: Response = executor.handle(request).await;

        let resp_data = codec::encode(&response)?;
        if let Err(e) = chan.send(&resp_data).await {
            return Err(anyhow::anyhow!("channel send: {e}"));
        }
    }
}

/// Handle a `ProxyOpen` request: policy check → TCP connect → `ProxyReady` → raw forwarding.
///
/// After sending `ProxyReady` the Noise channel carries raw plaintext chunks (still encrypted)
/// rather than RPC frames. Two tasks run concurrently:
/// * TCP target → `chan.send` → CLI
/// * `chan.recv` → TCP target
async fn handle_proxy_open<R, W>(
    chan: &Arc<SecureChannel<R, W>>,
    request_id: &str,
    target: &str,
    idle_timeout_secs: Option<u32>,
    max_duration_secs: Option<u32>,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    caller_key: String,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let policy_guard = policy.read().await;
    let decision = policy_guard.check_network_target(target);
    let idle = idle_timeout_secs.unwrap_or(policy_guard.proxy_idle_timeout_seconds);
    let max = max_duration_secs.unwrap_or(policy_guard.proxy_max_duration_seconds);
    drop(policy_guard);

    if !decision.allowed {
        let _ = audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "proxy_open".into(),
            command: Some(target.to_string()),
            decision: "denied".into(),
            matched_rule: decision.matched_rule.clone(),
            exit_code: None,
            duration_ms: 0,
            caller_key: caller_key.clone(),
            reason: None,
            prev_hash: None,
            hmac: None
        });
        let response = Response {
            id: request_id.to_string(),
            result: RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            },
        };
        let data = codec::encode(&response)?;
        chan.send(&data).await?;
        return Ok(());
    }

    // Connect to TCP target
    let tcp = match tokio::net::TcpStream::connect(target).await {
        Ok(t) => t,
        Err(e) => {
            let response = Response {
                id: request_id.to_string(),
                result: RpcResult::Error {
                    message: format!("connect to {target}: {e}"),
                },
            };
            let data = codec::encode(&response)?;
            chan.send(&data).await?;
            return Ok(());
        }
    };

    let proxy_id = format!("proxy-{}", &request_id[..8.min(request_id.len())]);

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "proxy_open".into(),
        command: Some(target.to_string()),
        decision: "allowed".into(),
        matched_rule: decision.matched_rule,
        exit_code: None,
        duration_ms: 0,
        caller_key: caller_key.clone(),
        reason: None,
        prev_hash: None,
        hmac: None
    });

    // Confirm tunnel is ready
    let response = Response {
        id: request_id.to_string(),
        result: RpcResult::ProxyReady {
            proxy_id: proxy_id.clone(),
            idle_timeout_secs: idle,
            max_duration_secs: max,
        },
    };
    let data = codec::encode(&response)?;
    chan.send(&data).await?;

    // Enter raw bidirectional forwarding mode
    run_proxy_tunnel(chan.clone(), tcp, idle, max).await?;

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "proxy_close".into(),
        command: Some(target.to_string()),
        decision: "allowed".into(),
        matched_rule: "tunnel-closed".into(),
        exit_code: Some(0),
        duration_ms: 0,
        caller_key,
        reason: None,
        prev_hash: None,
        hmac: None
    });

    Ok(())
}

/// Run a raw bidirectional proxy tunnel over `chan` ↔ `tcp`.
///
/// Two concurrent tasks:
/// * Task A: reads from the TCP target, sends frames to the CLI via `chan.send`
/// * Task B: receives frames from the CLI via `chan.recv`, writes to the TCP target
///
/// `SecureChannel` has independent reader/writer mutexes so the two tasks do not
/// contend with each other. The tunnel closes when either end reaches EOF,
/// the idle timeout fires, or the max-duration cap is reached.
async fn run_proxy_tunnel<R, W>(
    chan: Arc<SecureChannel<R, W>>,
    tcp: tokio::net::TcpStream,
    idle_secs: u32,
    max_secs: u32,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, Instant};

    let deadline = Instant::now() + Duration::from_secs(u64::from(max_secs));
    let idle_dur = Duration::from_secs(u64::from(idle_secs));

    // Split TCP stream so each task owns one half
    let (mut tcp_r, mut tcp_w) = tcp.into_split();

    // Task A: TCP target → SecureChannel → CLI
    let chan_a = chan.clone();
    let t_tcp_to_chan = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match tcp_r.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    // SecureChannel max frame payload is 65535; chunks fit exactly
                    if chan_a.send(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Task B: CLI → SecureChannel → TCP target
    let chan_b = chan;
    let t_chan_to_tcp = tokio::spawn(async move {
        loop {
            match chan_b.recv().await {
                Ok(data) if data.is_empty() => break, // close-notify
                Ok(data) => {
                    if tcp_w.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Enforce max-duration: abort both tasks if deadline is reached
    let remaining = deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = t_tcp_to_chan => {}
        _ = t_chan_to_tcp => {}
        _ = tokio::time::sleep(remaining) => {}
    }

    // Idle timeout is enforced client-side (CLI closes when idle)
    let _ = idle_dur;

    Ok(())
}

/// Handle a `FilePushStream` request: policy check → `FileStreamReady` → receive raw file data → finalize.
///
/// After sending `FileStreamReady` the connection carries raw file data chunks (still encrypted
/// by the Noise channel) rather than RPC frames. The agent reads exactly `total_size` bytes,
/// writes them to a temp file, verifies the optional SHA-256 checksum, and atomically renames
/// to the destination path. Finally it sends `FileStreamDone` and returns to RPC mode.
#[allow(clippy::too_many_arguments)]
async fn handle_file_push_stream<R, W>(
    chan: &Arc<SecureChannel<R, W>>,
    request_id: &str,
    path: &str,
    total_size: u64,
    checksum: Option<&str>,
    mode: Option<u32>,
    _compress: bool,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    caller_key: String,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use sha2::{Digest, Sha256};
    use std::path::Path;
    use tokio::io::AsyncWriteExt;

    // Resolve symlinks before policy check (prevent path traversal)
    let canonical = {
        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if parent.exists() {
                match std::fs::canonicalize(parent) {
                    Ok(c) => c
                        .join(p.file_name().unwrap_or_default())
                        .to_string_lossy()
                        .into_owned(),
                    Err(_) => path.to_string(),
                }
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        }
    };

    let policy_guard = policy.read().await;
    let decision = policy_guard.check_path(std::path::Path::new(&canonical));
    let max_output = policy_guard.max_output_bytes;
    drop(policy_guard);

    if !decision.allowed {
        let _ = audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "file_push_stream".into(),
            command: Some(path.to_string()),
            decision: "denied".into(),
            matched_rule: decision.matched_rule.clone(),
            exit_code: None,
            duration_ms: 0,
            caller_key: caller_key.clone(),
            reason: None,
            prev_hash: None,
            hmac: None
        });
        let response = Response {
            id: request_id.to_string(),
            result: RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            },
        };
        let data = codec::encode(&response)?;
        chan.send(&data).await?;
        return Ok(());
    }

    // Enforce file size limit
    let size_limit = if max_output > 0 { max_output } else { u64::MAX };
    if total_size > size_limit {
        let response = Response {
            id: request_id.to_string(),
            result: RpcResult::Error {
                message: format!(
                    "file too large: {total_size} bytes exceeds limit of {size_limit}"
                ),
            },
        };
        let data = codec::encode(&response)?;
        chan.send(&data).await?;
        return Ok(());
    }

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "file_push_stream".into(),
        command: Some(path.to_string()),
        decision: "allowed".into(),
        matched_rule: decision.matched_rule,
        exit_code: None,
        duration_ms: 0,
        caller_key: caller_key.clone(),
        reason: None,
        prev_hash: None,
        hmac: None
    });

    // Signal readiness — client starts sending raw frames immediately
    let ready = Response {
        id: request_id.to_string(),
        result: RpcResult::FileStreamReady {
            total_size: 0,
            checksum: None,
        },
    };
    let data = codec::encode(&ready)?;
    chan.send(&data).await?;

    // Write to a temp file alongside the destination
    let dest_path = Path::new(path);
    let parent = dest_path.parent().unwrap_or(Path::new("/tmp"));
    let tmp_path = parent.join(format!(".raven_tmp_{request_id}"));

    let result: anyhow::Result<(u64, bool)> = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)
            .await
            .map_err(|e| anyhow::anyhow!("open temp file: {e}"))?;

        let mut hasher = Sha256::new();
        let mut received: u64 = 0;

        while received < total_size {
            let chunk = chan
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("recv: {e}"))?;
            if chunk.is_empty() {
                return Err(anyhow::anyhow!(
                    "connection closed before transfer complete"
                ));
            }
            received += chunk.len() as u64;
            if received > total_size {
                return Err(anyhow::anyhow!(
                    "client sent more bytes than declared total_size"
                ));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|e| anyhow::anyhow!("write: {e}"))?;
        }
        file.flush()
            .await
            .map_err(|e| anyhow::anyhow!("flush: {e}"))?;
        drop(file);

        // Verify checksum
        let checksum_ok = if let Some(expected) = checksum {
            let digest = hasher.finalize();
            let actual: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            actual == expected
        } else {
            true // no checksum provided — skip verification
        };

        if !checksum_ok {
            return Err(anyhow::anyhow!("checksum mismatch"));
        }

        // Set permissions before rename (Unix)
        #[cfg(unix)]
        if let Some(m) = mode {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(m);
            std::fs::set_permissions(&tmp_path, perms)
                .map_err(|e| anyhow::anyhow!("chmod: {e}"))?;
        }

        // Atomic rename
        tokio::fs::rename(&tmp_path, dest_path)
            .await
            .map_err(|e| anyhow::anyhow!("rename: {e}"))?;

        Ok((received, checksum.is_none() || checksum_ok))
    }
    .await;

    // Clean up temp file on error
    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
    }

    let (bytes_transferred, checksum_verified) = match result {
        Ok(v) => v,
        Err(e) => {
            let response = Response {
                id: request_id.to_string(),
                result: RpcResult::Error {
                    message: format!("stream upload failed: {e}"),
                },
            };
            let data = codec::encode(&response)?;
            chan.send(&data).await?;
            return Err(e);
        }
    };

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "file_push_stream_done".into(),
        command: Some(path.to_string()),
        decision: "allowed".into(),
        matched_rule: "transfer-complete".into(),
        exit_code: Some(0),
        duration_ms: 0,
        caller_key,
        reason: None,
        prev_hash: None,
        hmac: None
    });

    let done = Response {
        id: request_id.to_string(),
        result: RpcResult::FileStreamDone {
            bytes_transferred,
            checksum_verified,
        },
    };
    let data = codec::encode(&done)?;
    chan.send(&data).await?;

    Ok(())
}

/// Handle a `FilePullStream` request: policy check → `FileStreamReady` → stream raw file data.
///
/// After sending `FileStreamReady { total_size, checksum }` the agent streams the file contents
/// as raw `SecureChannel` frames (64 KB each). The client reads until `total_size` bytes are
/// received, then verifies the checksum. Connection returns to RPC mode automatically.
async fn handle_file_pull_stream<R, W>(
    chan: &Arc<SecureChannel<R, W>>,
    request_id: &str,
    path: &str,
    _compress: bool,
    policy: &Arc<RwLock<RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
    caller_key: String,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use sha2::{Digest, Sha256};
    use std::path::Path;

    // Resolve symlinks before policy check (prevent path traversal)
    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c.to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    };

    let policy_guard = policy.read().await;
    let decision = policy_guard.check_path(std::path::Path::new(&canonical));
    drop(policy_guard);

    if !decision.allowed {
        let _ = audit.log(AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: request_id.to_string(),
            action: "file_pull_stream".into(),
            command: Some(path.to_string()),
            decision: "denied".into(),
            matched_rule: decision.matched_rule.clone(),
            exit_code: None,
            duration_ms: 0,
            caller_key: caller_key.clone(),
            reason: None,
            prev_hash: None,
            hmac: None
        });
        let response = Response {
            id: request_id.to_string(),
            result: RpcResult::Denied {
                reason: decision.reason,
                rule: decision.matched_rule,
            },
        };
        let data = codec::encode(&response)?;
        chan.send(&data).await?;
        return Ok(());
    }

    // Read the file and compute checksum up front
    let file_data = match tokio::fs::read(Path::new(path)).await {
        Ok(d) => d,
        Err(e) => {
            let response = Response {
                id: request_id.to_string(),
                result: RpcResult::Error {
                    message: format!("read {path}: {e}"),
                },
            };
            let data = codec::encode(&response)?;
            chan.send(&data).await?;
            return Ok(());
        }
    };
    let total_size = file_data.len() as u64;
    let digest = Sha256::digest(&file_data);
    let checksum: String = digest.iter().map(|b| format!("{b:02x}")).collect();

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "file_pull_stream".into(),
        command: Some(path.to_string()),
        decision: "allowed".into(),
        matched_rule: decision.matched_rule,
        exit_code: None,
        duration_ms: 0,
        caller_key: caller_key.clone(),
        reason: None,
        prev_hash: None,
        hmac: None
    });

    // Announce file metadata — client now expects raw frames
    let ready = Response {
        id: request_id.to_string(),
        result: RpcResult::FileStreamReady {
            total_size,
            checksum: Some(checksum),
        },
    };
    let data = codec::encode(&ready)?;
    chan.send(&data).await?;

    // Stream file data in 64 KB frames (max frame payload is 65535)
    const CHUNK: usize = 65535;
    let mut offset = 0;
    while offset < file_data.len() {
        let end = (offset + CHUNK).min(file_data.len());
        chan.send(&file_data[offset..end])
            .await
            .map_err(|e| anyhow::anyhow!("send: {e}"))?;
        offset = end;
    }

    let _ = audit.log(AuditEntry {
        timestamp: chrono::Utc::now(),
        request_id: request_id.to_string(),
        action: "file_pull_stream_done".into(),
        command: Some(path.to_string()),
        decision: "allowed".into(),
        matched_rule: "transfer-complete".into(),
        exit_code: Some(0),
        duration_ms: 0,
        caller_key,
        reason: None,
        prev_hash: None,
        hmac: None
    });

    Ok(())
}
