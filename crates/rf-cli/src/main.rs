//! RavenFabric CLI — `rf` command for remote execution and management.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use rf_crypto::channel::SecureChannel;
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use rf_transport::driver::{Driver, Target};
use rf_transport::websocket::WebSocketDriver;

#[derive(Parser)]
#[command(name = "rf", about = "RavenFabric — secure remote execution")]
struct Cli {
    /// Relay URL
    #[arg(short, long, env = "RF_RELAY", default_value = "ws://127.0.0.1:9090")]
    relay: String,

    /// Path to client key file
    #[arg(short, long, default_value = "client.key")]
    key_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a command on a remote agent
    Exec {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,

        /// Command to execute
        command: String,
    },
    /// Open interactive shell on a remote agent
    Shell {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,

        /// Terminal columns
        #[arg(long, default_value = "80")]
        cols: u16,

        /// Terminal rows
        #[arg(long, default_value = "24")]
        rows: u16,
    },
    /// Start local development mode (relay + agent in one process, permissive policy)
    Dev {
        /// Listen port for the local relay
        #[arg(short, long, default_value = "9090")]
        port: u16,
    },
    /// Show agent status
    Status {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("rf=info,rf_relay=info")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Exec { token, command } => {
            exec_command(&cli.relay, &cli.key_path, &token, &command).await?;
        }
        Commands::Shell { token, cols, rows } => {
            shell_command(&cli.relay, &cli.key_path, &token, cols, rows).await?;
        }
        Commands::Dev { port } => {
            dev_mode(port).await?;
        }
        Commands::Status { token } => {
            status_command(&cli.relay, &cli.key_path, &token).await?;
        }
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "rf", &mut std::io::stdout());
        }
    }

    Ok(())
}

async fn exec_command(
    relay_url: &str,
    key_path: &std::path::Path,
    token: &str,
    command: &str,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    info!("connecting to relay: {}", relay_url);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Noise handshake (client is initiator)
    info!("performing Noise XX handshake...");
    let (state, peer_key) = handshake(&mut stream, true, &key).await?;
    info!("connected to agent: {}", hex::encode(peer_key));

    // Create SecureChannel
    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    // Send RPC request
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Execute {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        },
        timeout_ms: Some(30_000),
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    // Await response
    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    match response.result {
        RpcResult::Success {
            stdout,
            stderr,
            exit_code,
            duration_ms,
        } => {
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            info!("exit_code={} duration={}ms", exit_code, duration_ms);
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        RpcResult::Denied { reason, rule } => {
            error!("DENIED: {} (rule: {})", reason, rule);
            std::process::exit(1);
        }
        RpcResult::Error { message } => {
            error!("ERROR: {}", message);
            std::process::exit(1);
        }
        RpcResult::StatusInfo { .. } => {
            error!("unexpected StatusInfo response for exec");
            std::process::exit(1);
        }
        RpcResult::StreamChunk { .. } | RpcResult::StreamEnd { .. } => {
            error!("unexpected streaming response for non-streaming exec");
            std::process::exit(1);
        }
        RpcResult::JobStarted { job_id, pid } => {
            println!("background job started: {} (pid {})", job_id, pid);
        }
        RpcResult::JobStatus {
            job_id,
            running,
            exit_code,
            stdout,
            stderr,
        } => {
            if running {
                println!("job {} is still running", job_id);
            } else {
                if let Some(out) = stdout {
                    print!("{}", out);
                }
                if let Some(err) = stderr {
                    eprint!("{}", err);
                }
                let code = exit_code.unwrap_or(-1);
                info!("job {} completed, exit_code={}", job_id, code);
                if code != 0 {
                    std::process::exit(code);
                }
            }
        }
        RpcResult::Pong { timestamp_ms } => {
            println!("pong (timestamp: {}ms)", timestamp_ms);
        }
    }

    Ok(())
}

async fn status_command(
    relay_url: &str,
    key_path: &std::path::Path,
    token: &str,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    info!("connecting to relay: {}", relay_url);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Noise handshake (client is initiator)
    let (state, peer_key) = handshake(&mut stream, true, &key).await?;
    info!("connected to agent: {}", hex::encode(peer_key));

    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    // Send Status request
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Status,
        timeout_ms: Some(5_000),
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    match response.result {
        RpcResult::StatusInfo {
            agent_id,
            version,
            uptime_seconds,
        } => {
            println!("Agent:   {}", agent_id);
            println!("Version: {}", version);
            println!("Uptime:  {}s", uptime_seconds);
            println!("Peer:    {}", hex::encode(peer_key));
            println!("Status:  connected");
        }
        RpcResult::Error { message } => {
            error!("ERROR: {}", message);
            std::process::exit(1);
        }
        _ => {
            error!("unexpected response type");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Interactive shell session through the fabric.
/// Connects to agent, requests a PTY, and proxies stdin/stdout.
async fn shell_command(
    relay_url: &str,
    key_path: &std::path::Path,
    token: &str,
    cols: u16,
    rows: u16,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    info!("connecting to relay: {}", relay_url);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Noise handshake (client is initiator)
    let (state, peer_key) = handshake(&mut stream, true, &key).await?;
    info!("connected to agent: {}", hex::encode(peer_key));

    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    // Request shell session
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Execute {
            command: format!("__pty__:{}x{}", cols, rows),
            env: Default::default(),
            workdir: None,
        },
        timeout_ms: None, // No timeout for interactive sessions
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    info!(
        "shell session requested ({}x{}), waiting for response...",
        cols, rows
    );

    // In a full implementation, this would:
    // 1. Set terminal to raw mode
    // 2. Forward stdin to chan.send() as PtyInput::Data
    // 3. Forward chan.recv() to stdout as PtyOutput::Data
    // 4. Handle resize events (SIGWINCH → PtyInput::Resize)
    // 5. Restore terminal on exit
    //
    // For now, just receive the initial response:
    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    match response.result {
        RpcResult::Success { stdout, .. } => {
            println!("{}", stdout);
        }
        RpcResult::Denied { reason, rule } => {
            error!("DENIED: {} (rule: {})", reason, rule);
            std::process::exit(1);
        }
        RpcResult::Error { message } => {
            error!("ERROR: {}", message);
            std::process::exit(1);
        }
        _ => {
            error!("unexpected response");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Development mode: starts a relay and agent in a single process with permissive policy.
async fn dev_mode(port: u16) -> anyhow::Result<()> {
    let listen_addr = format!("127.0.0.1:{}", port);
    let relay_url = format!("ws://127.0.0.1:{}", port);
    let dev_token = "dev";

    println!("RavenFabric Dev Mode");
    println!("====================");
    println!("Relay:  {}", listen_addr);
    println!("Token:  {}", dev_token);
    println!();
    println!("Usage:");
    println!("  rf exec --token {} \"<command>\"", dev_token);
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    let cancel = CancellationToken::new();

    // Start relay
    let relay_cancel = cancel.clone();
    let relay_addr = listen_addr.clone();
    let relay_handle = tokio::spawn(async move {
        if let Err(e) = rf_relay::run_relay(&relay_addr, relay_cancel).await {
            error!("relay error: {}", e);
        }
    });

    // Give relay time to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Start agent in-process
    let agent_cancel = cancel.clone();
    let agent_handle = tokio::spawn(async move {
        run_dev_agent(&relay_url, dev_token, agent_cancel).await;
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");
    cancel.cancel();

    let _ = tokio::join!(relay_handle, agent_handle);
    Ok(())
}

/// Run a dev-mode agent with a permissive policy (allow everything).
async fn run_dev_agent(relay_url: &str, token: &str, cancel: CancellationToken) {
    use rf_audit::logger::NullAuditLogger;
    use rf_policy::rpc_policy::RpcPolicy;

    // Permissive policy for dev mode
    let yaml = r#"
spec:
  commands:
    allow:
      - pattern: ".*"
  resources:
    maxOutputBytes: 104857600
    timeoutSeconds: 300
"#;
    let policy = RpcPolicy::from_yaml(yaml).expect("dev policy must parse");
    let policy = Arc::new(RwLock::new(policy));
    let audit: Arc<dyn rf_audit::logger::AuditLogger> = Arc::new(NullAuditLogger);

    // Generate ephemeral key for dev mode
    let key = StaticKey::generate();

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = connect_dev_agent(relay_url, token, &key, &policy, &audit) => {
                match result {
                    Ok(()) => info!("dev agent session ended"),
                    Err(e) => error!("dev agent error: {}", e),
                }
                // Reconnect after brief pause
                tokio::select! {
                    () = cancel.cancelled() => break,
                    () = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {}
                }
            }
        }
    }
}

async fn connect_dev_agent(
    relay_url: &str,
    token: &str,
    key: &StaticKey,
    policy: &Arc<RwLock<rf_policy::rpc_policy::RpcPolicy>>,
    audit: &Arc<dyn rf_audit::logger::AuditLogger>,
) -> anyhow::Result<()> {
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: "dev-agent".to_string(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    let mut stream = driver.dial(&target, &Default::default()).await?;

    let (state, peer_key) = handshake(&mut stream, false, key).await?;
    info!("dev agent connected, peer: {}", hex::encode(peer_key));

    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    let executor =
        rf_executor::command::Executor::new(policy.clone(), audit.clone(), hex::encode(peer_key))
            .with_agent_id("dev-agent".to_string())
            .with_start_time(std::time::Instant::now());

    loop {
        let data = match chan.recv().await {
            Ok(d) => d,
            Err(_) => break,
        };

        let request: Request = match codec::decode(&data) {
            Ok(r) => r,
            Err(e) => {
                error!("decode error: {}", e);
                continue;
            }
        };

        info!("request: {} action={:?}", request.id, request.action);
        let response: Response = executor.handle(request).await;

        let resp_data = codec::encode(&response)?;
        if chan.send(&resp_data).await.is_err() {
            break;
        }
    }

    Ok(())
}
