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
use rf_policy::templates::TemplateRegistry;
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use rf_transport::driver::{AsyncStream, Driver, Target};
use rf_transport::websocket::WebSocketDriver;

type AgentChannel = SecureChannel<
    tokio::io::ReadHalf<Box<dyn AsyncStream>>,
    tokio::io::WriteHalf<Box<dyn AsyncStream>>,
>;

#[derive(Parser)]
#[command(name = "rf", about = "RavenFabric — secure remote execution", version)]
struct Cli {
    /// Relay URL (ignored when --connect is used)
    #[arg(short, long, env = "RF_RELAY", default_value = "ws://127.0.0.1:9090")]
    relay: String,

    /// Direct connect to an agent (e.g., ws://host:9999). Bypasses relay.
    #[arg(short = 'C', long, env = "RF_CONNECT")]
    connect: Option<String>,

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

        /// Stream output incrementally (real-time stdout/stderr)
        #[arg(short, long, default_value_t = false)]
        stream: bool,

        /// Run in background (returns job ID immediately)
        #[arg(short, long, default_value_t = false)]
        background: bool,

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
    /// Port forward (local port → remote target via agent)
    Forward {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,

        /// Local bind address (e.g., 127.0.0.1:8080)
        #[arg(short = 'L', long)]
        local: String,

        /// Remote target address (e.g., db.internal:5432)
        #[arg(short = 'R', long)]
        remote: String,
    },
    /// Execute a playbook (multi-agent orchestration)
    Playbook {
        /// Path to playbook YAML file
        file: PathBuf,

        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,
    },
    /// Start local development mode (relay + agent in one process, permissive policy)
    Dev {
        /// Listen port for the local relay
        #[arg(short, long, default_value = "9090")]
        port: u16,
        /// Bind address for the relay listener
        #[arg(short, long, default_value = "127.0.0.1")]
        bind: String,
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
    /// Policy template management and validation
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Copy files to/from a remote agent (e.g., rf cp agent:/path local or rf cp local agent:/path)
    Cp {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,

        /// Source path (local path or agent:/remote/path)
        source: String,

        /// Destination path (local path or agent:/remote/path)
        dest: String,

        /// Chunk size in bytes (default 256KB)
        #[arg(long, default_value = "262144")]
        chunk_size: u32,

        /// Recursive copy (for directories)
        #[arg(short, long)]
        recursive: bool,
    },
    /// Open a TCP proxy tunnel through an agent
    #[command(name = "proxy")]
    Proxy {
        /// Meet token (shared secret for relay pairing)
        #[arg(short, long)]
        token: String,

        /// Target address the agent connects to (host:port)
        #[arg(long)]
        target: String,

        /// Local address to listen on
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        listen: String,

        /// Idle timeout in seconds (close connection if no data flows)
        #[arg(long)]
        idle_timeout: Option<u32>,

        /// Maximum connection duration in seconds (hard cap)
        #[arg(long)]
        max_duration: Option<u32>,

        /// Enable HTTP-aware mode: parse incoming HTTP requests and forward
        /// them via HttpForward RPC (with per-request audit + policy enforcement)
        #[arg(long)]
        http: bool,
    },
}

#[derive(Subcommand)]
enum PolicyAction {
    /// List all available policy templates
    List,
    /// Show a template's full YAML policy
    Show {
        /// Template name (e.g., "safe-dev-mode", "production-ai-guardrails")
        name: String,
    },
    /// Validate a policy YAML file
    Validate {
        /// Path to policy YAML file (or --template for built-in)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Validate a built-in template by name
        #[arg(short, long)]
        template: Option<String>,
    },
    /// Compose multiple templates (deny-wins conflict resolution)
    Compose {
        /// Template names to compose (comma-separated)
        templates: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("rf=info,rf_relay=info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Exec {
            token,
            command,
            stream,
            background,
        } => {
            exec_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                &command,
                stream,
                background,
            )
            .await?;
        }
        Commands::Shell { token, cols, rows } => {
            shell_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                cols,
                rows,
            )
            .await?;
        }
        Commands::Forward {
            token,
            local,
            remote,
        } => {
            forward_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                &local,
                &remote,
            )
            .await?;
        }
        Commands::Playbook { file, token } => {
            playbook_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                &file,
            )
            .await?;
        }
        Commands::Dev { port, bind } => {
            dev_mode(port, &bind).await?;
        }
        Commands::Status { token } => {
            status_command(&cli.relay, cli.connect.as_deref(), &cli.key_path, &token).await?;
        }
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "rf", &mut std::io::stdout());
        }
        Commands::Policy { action } => {
            policy_command(action)?;
        }
        Commands::Cp {
            token,
            source,
            dest,
            chunk_size,
            recursive,
        } => {
            cp_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                &source,
                &dest,
                chunk_size,
                recursive,
            )
            .await?;
        }
        Commands::Proxy {
            token,
            target,
            listen,
            idle_timeout,
            max_duration,
            http,
        } => {
            proxy_command(
                &cli.relay,
                cli.connect.as_deref(),
                &cli.key_path,
                &token,
                &target,
                &listen,
                idle_timeout,
                max_duration,
                http,
            )
            .await?;
        }
    }

    Ok(())
}

/// Establish a connection to an agent, either directly or via relay.
/// Returns the handshaked SecureChannel and the peer's public key.
async fn dial_agent(
    relay_url: &str,
    direct_addr: Option<&str>,
    key: &StaticKey,
    token: &str,
) -> anyhow::Result<(AgentChannel, [u8; 32])> {
    let driver = WebSocketDriver::new();

    let mut stream = if let Some(addr) = direct_addr {
        info!("connecting directly to agent: {}", addr);
        let target = Target {
            agent_id: String::new(),
            relay_url: Some(addr.to_string()),
            meet_token: None,
        };
        driver.dial(&target, &Default::default()).await?
    } else {
        info!("connecting to relay: {}", relay_url);
        let target = Target {
            agent_id: String::new(),
            relay_url: Some(relay_url.to_string()),
            meet_token: Some(token.to_string()),
        };
        driver.dial(&target, &Default::default()).await?
    };

    // Noise handshake (client is initiator)
    info!("performing Noise XX handshake...");
    let (state, peer_key) = handshake(&mut stream, true, key).await?;
    info!("connected to agent: {}", hex::encode(peer_key));

    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);
    Ok((chan, peer_key))
}

async fn exec_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    command: &str,
    streaming: bool,
    background: bool,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let (chan, _peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;

    // Select action based on mode
    let action = if background {
        Action::BackgroundExec {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        }
    } else if streaming {
        Action::StreamExecute {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        }
    } else {
        Action::Execute {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        }
    };

    // Send RPC request
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action,
        timeout_ms: Some(30_000),
        reason: None,
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    // For streaming mode, read multiple responses until StreamEnd
    if streaming {
        loop {
            let resp_data = chan.recv().await?;
            let response: Response = codec::decode(&resp_data)?;
            match response.result {
                RpcResult::StreamChunk {
                    stream: stream_type,
                    data,
                } => {
                    use rf_rpc::types::StreamType;
                    match stream_type {
                        StreamType::Stdout => {
                            let out = String::from_utf8_lossy(&data);
                            print!("{out}");
                        }
                        StreamType::Stderr => {
                            let err = String::from_utf8_lossy(&data);
                            eprint!("{err}");
                        }
                    }
                }
                RpcResult::StreamEnd {
                    exit_code,
                    duration_ms,
                } => {
                    info!("exit_code={} duration={}ms", exit_code, duration_ms);
                    if exit_code != 0 {
                        std::process::exit(exit_code);
                    }
                    break;
                }
                RpcResult::Denied { reason, rule } => {
                    error!("DENIED: {} (rule: {})", reason, rule);
                    std::process::exit(1);
                }
                RpcResult::Error { message } => {
                    error!("ERROR: {}", message);
                    std::process::exit(1);
                }
                // Fallback: agent returned batch result for StreamExecute
                RpcResult::Success {
                    stdout,
                    stderr,
                    exit_code,
                    duration_ms,
                } => {
                    if !stdout.is_empty() {
                        print!("{stdout}");
                    }
                    if !stderr.is_empty() {
                        eprint!("{stderr}");
                    }
                    info!("exit_code={} duration={}ms", exit_code, duration_ms);
                    if exit_code != 0 {
                        std::process::exit(exit_code);
                    }
                    break;
                }
                _ => {
                    error!("unexpected response in streaming mode");
                    std::process::exit(1);
                }
            }
        }
        let _ = chan.close_notify().await;
        // Give the WebSocket bridge task time to forward the close-notify
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        return Ok(());
    }

    // Await response (standard / background mode)
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
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
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
            println!("background job started: {job_id} (pid {pid})");
        }
        RpcResult::JobStatus {
            job_id,
            running,
            exit_code,
            stdout,
            stderr,
        } => {
            if running {
                println!("job {job_id} is still running");
            } else {
                if let Some(out) = stdout {
                    print!("{out}");
                }
                if let Some(err) = stderr {
                    eprint!("{err}");
                }
                let code = exit_code.unwrap_or(-1);
                info!("job {} completed, exit_code={}", job_id, code);
                if code != 0 {
                    std::process::exit(code);
                }
            }
        }
        RpcResult::Pong { timestamp_ms } => {
            println!("pong (timestamp: {timestamp_ms}ms)");
        }
        RpcResult::ShellOpened { session_id } => {
            println!("shell session opened: {session_id}");
        }
        RpcResult::ShellOutput { data, .. } => {
            let output = String::from_utf8_lossy(&data);
            print!("{output}");
        }
        RpcResult::ShellExited {
            session_id,
            exit_code,
        } => {
            println!("shell session {session_id} exited (code {exit_code})");
        }
        RpcResult::ForwardStarted {
            forward_id,
            bind_addr,
        } => {
            println!("port forward started: {forward_id} on {bind_addr}");
        }
        RpcResult::ForwardStopped { forward_id } => {
            println!("port forward stopped: {forward_id}");
        }
        RpcResult::HealthCheckResult {
            success,
            latency_ms,
            error,
        } => {
            if success {
                println!("health check OK ({latency_ms}ms)");
            } else {
                println!(
                    "health check FAILED ({}ms): {}",
                    latency_ms,
                    error.unwrap_or_default()
                );
            }
        }
        RpcResult::TailOutput { lines, path } => {
            println!("--- {path} ---");
            for line in lines {
                println!("{line}");
            }
        }
        _ => {
            error!("unexpected response: {:?}", response.result);
            std::process::exit(1);
        }
    }

    let _ = chan.close_notify().await;
    // Give the WebSocket bridge task time to forward the close-notify
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

async fn status_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let (chan, peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;

    // Send Status request
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Status,
        timeout_ms: Some(5_000),
        reason: None,
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
            println!("Agent:   {agent_id}");
            println!("Version: {version}");
            println!("Uptime:  {uptime_seconds}s");
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

    let _ = chan.close_notify().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    Ok(())
}

/// Port forwarding: binds a local port and forwards connections through the agent.
async fn forward_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    local_addr: &str,
    remote_addr: &str,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let (chan, _peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;

    // Request port forward
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::PortForward {
            bind_addr: local_addr.to_string(),
            target_addr: remote_addr.to_string(),
        },
        timeout_ms: Some(10_000),
        reason: None,
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    match response.result {
        RpcResult::ForwardStarted {
            forward_id,
            bind_addr,
        } => {
            println!("Port forward active: {bind_addr} → {remote_addr} (id: {forward_id})");
            println!("Press Ctrl+C to stop.");

            // Keep connection alive until interrupted
            tokio::signal::ctrl_c().await?;

            // Close the forward
            let close_req = Request {
                id: uuid::Uuid::new_v4().to_string(),
                action: Action::PortForwardClose {
                    forward_id: forward_id.clone(),
                },
                timeout_ms: Some(5_000),
                reason: None,
            };
            let close_data = codec::encode(&close_req)?;
            chan.send(&close_data).await?;
            println!("\nForward stopped.");
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

/// Execute a playbook — multi-agent orchestrated execution.
async fn playbook_command(
    relay_url: &str,
    _direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    file: &std::path::Path,
) -> anyhow::Result<()> {
    use rf_executor::orchestrator::{AgentResult, OrchestrationPlan, Orchestrator, TargetGrain};
    use std::time::Instant;

    // Load playbook from YAML file
    let yaml_content = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("failed to read playbook {}: {}", file.display(), e))?;

    let plan: OrchestrationPlan = serde_yaml::from_str(&yaml_content)
        .map_err(|e| anyhow::anyhow!("failed to parse playbook YAML: {e}"))?;

    // Resolve target agents
    let agents = match &plan.target {
        TargetGrain::Agents(ids) => ids.clone(),
        _ => {
            // For non-explicit targeting, we need a known-agents registry.
            // For now, require explicit agent list.
            anyhow::bail!("playbook target must use 'agents: [...]' for CLI execution");
        }
    };

    println!("Playbook: {}", file.display());
    println!("Command:  {}", plan.command);
    println!("Strategy: {:?}", plan.strategy);
    println!("Agents:   {agents:?}");
    println!("---");

    let key = StaticKey::load_or_generate(key_path)?;
    let start = Instant::now();
    let mut orch = Orchestrator::new(plan.clone(), agents);

    while let Some(batch) = orch.next_batch() {
        println!("Executing batch: {batch:?}");

        let mut batch_results = Vec::new();

        for agent_id in &batch {
            let agent_start = Instant::now();
            // Connect to agent via relay (each agent uses the same token for pairing)
            let result =
                execute_on_agent(relay_url, &key, token, &plan.command, plan.timeout_secs).await;

            let agent_result = match result {
                Ok((stdout, stderr, exit_code)) => {
                    let success = exit_code == 0;
                    let symbol = if success { "✓" } else { "✗" };
                    println!("  {symbol} {agent_id} (exit {exit_code})");
                    AgentResult {
                        agent_id: agent_id.clone(),
                        success,
                        exit_code: Some(exit_code),
                        stdout,
                        stderr,
                        duration_ms: agent_start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => {
                    println!("  ✗ {agent_id} (error: {e})");
                    AgentResult {
                        agent_id: agent_id.clone(),
                        success: false,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        duration_ms: agent_start.elapsed().as_millis() as u64,
                    }
                }
            };
            batch_results.push(agent_result);
        }

        let should_continue = orch.record_batch(batch_results);
        if !should_continue {
            println!("--- Batch failed, stopping rollout ---");

            // Execute rollback if configured
            if let Some(rollback_cmd) = orch.rollback_command() {
                let agents_to_rollback = orch.agents_needing_rollback();
                if !agents_to_rollback.is_empty() {
                    println!(
                        "Rolling back {} agents: {}",
                        agents_to_rollback.len(),
                        rollback_cmd
                    );
                    for agent_id in agents_to_rollback {
                        let rb_result = execute_on_agent(
                            relay_url,
                            &key,
                            token,
                            rollback_cmd,
                            plan.timeout_secs,
                        )
                        .await;
                        let symbol = if rb_result.is_ok() { "↩" } else { "!" };
                        println!("  {symbol} {agent_id} rollback");
                    }
                }
            }
            break;
        }
    }

    let result = orch.finalize(start.elapsed().as_millis() as u64);
    println!("---");
    println!(
        "Result: {} ({}/{} agents succeeded, {}ms)",
        if result.success { "SUCCESS" } else { "FAILED" },
        result.results.iter().filter(|r| r.success).count(),
        result.results.len(),
        result.total_duration_ms,
    );
    if result.rollback_triggered {
        println!("Rollback was triggered.");
    }

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}

/// Execute a single command on one agent via relay.
async fn execute_on_agent(
    relay_url: &str,
    key: &StaticKey,
    token: &str,
    command: &str,
    timeout_secs: u64,
) -> anyhow::Result<(String, String, i32)> {
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    let mut stream = driver.dial(&target, &Default::default()).await?;
    let (state, peer_key) = handshake(&mut stream, true, key).await?;

    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Execute {
            command: command.to_string(),
            env: Default::default(),
            workdir: None,
        },
        timeout_ms: Some(timeout_secs * 1000),
        reason: None,
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    match response.result {
        RpcResult::Success {
            stdout,
            stderr,
            exit_code,
            ..
        } => Ok((stdout, stderr, exit_code)),
        RpcResult::Denied { reason, .. } => {
            anyhow::bail!("denied: {reason}");
        }
        RpcResult::Error { message } => {
            anyhow::bail!("{message}");
        }
        _ => anyhow::bail!("unexpected response"),
    }
}

/// Interactive shell session through the fabric.
/// Connects to agent, requests a PTY, and proxies stdin/stdout bidirectionally.
#[cfg(unix)]
async fn shell_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    cols: u16,
    rows: u16,
) -> anyhow::Result<()> {
    use std::os::unix::io::AsRawFd;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    let (chan, _peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;
    let chan = Arc::new(chan);

    // Request shell session with proper Action::Shell
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::Shell {
            shell: None, // Use agent's default shell
            rows,
            cols,
            env: Default::default(),
        },
        timeout_ms: None,
        reason: None,
    };

    let req_data = codec::encode(&request)?;
    chan.send(&req_data).await?;

    // Wait for ShellOpened response
    let resp_data = chan.recv().await?;
    let response: Response = codec::decode(&resp_data)?;

    let session_id = match response.result {
        RpcResult::ShellOpened { session_id } => session_id,
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
    };

    info!("shell session opened: {}", session_id);

    // Set terminal to raw mode
    let stdin_fd = std::io::stdin().as_raw_fd();
    let orig_termios = unsafe {
        let mut termios = std::mem::zeroed::<libc::termios>();
        libc::tcgetattr(stdin_fd, &mut termios);
        let orig = termios;
        libc::cfmakeraw(&mut termios);
        libc::tcsetattr(stdin_fd, libc::TCSANOW, &termios);
        orig
    };

    // Ensure we restore terminal on exit
    struct RawModeGuard {
        fd: i32,
        termios: libc::termios,
    }
    impl Drop for RawModeGuard {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.termios);
            }
        }
    }
    let _guard = RawModeGuard {
        fd: stdin_fd,
        termios: orig_termios,
    };

    let cancel = CancellationToken::new();
    let session_id_clone = session_id.clone();

    // Spawn stdin reader task: reads from stdin and sends ShellInput
    let chan_write = chan.clone();
    let cancel_stdin = cancel.clone();
    let sid_write = session_id.clone();
    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            tokio::select! {
                () = cancel_stdin.cancelled() => break,
                result = stdin.read(&mut buf) => {
                    match result {
                        Ok(0) => break,
                        Ok(n) => {
                            let req = Request {
                                id: uuid::Uuid::new_v4().to_string(),
                                action: Action::ShellInput {
                                    session_id: sid_write.clone(),
                                    data: buf[..n].to_vec(),
                                },
                                timeout_ms: None,
                                reason: None,
                            };
                            if let Ok(data) = codec::encode(&req) {
                                if chan_write.send(&data).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    // Main loop: read responses from agent and write to stdout
    let mut stdout = tokio::io::stdout();
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = chan.recv() => {
                match result {
                    Ok(data) => {
                        let response: Response = match codec::decode(&data) {
                            Ok(r) => r,
                            Err(_) => continue,
                        };
                        match response.result {
                            RpcResult::ShellOutput { data, .. } => {
                                if !data.is_empty() {
                                    let _ = stdout.write_all(&data).await;
                                    let _ = stdout.flush().await;
                                }
                            }
                            RpcResult::ShellExited { exit_code, .. } => {
                                cancel.cancel();
                                drop(_guard);
                                std::process::exit(exit_code);
                            }
                            _ => {}
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    // Clean up: close the shell session
    let close_req = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::ShellClose {
            session_id: session_id_clone,
        },
        timeout_ms: Some(5_000),
        reason: None,
    };
    if let Ok(data) = codec::encode(&close_req) {
        let _ = chan.send(&data).await;
    }

    stdin_task.abort();

    Ok(())
}

/// Interactive shell session — not supported on non-Unix platforms.
#[cfg(not(unix))]
async fn shell_command(
    _relay_url: &str,
    _direct_addr: Option<&str>,
    _key_path: &std::path::Path,
    _token: &str,
    _cols: u16,
    _rows: u16,
) -> anyhow::Result<()> {
    anyhow::bail!("interactive shell is not supported on this platform");
}

/// Development mode: starts a relay and agent in a single process with permissive policy.
async fn dev_mode(port: u16, bind: &str) -> anyhow::Result<()> {
    let listen_addr = format!("{bind}:{port}");
    let relay_url = format!("ws://127.0.0.1:{port}");
    let dev_token = "dev";

    println!("RavenFabric Dev Mode");
    println!("====================");
    println!("Relay:  {listen_addr}");
    println!("Token:  {dev_token}");
    println!();
    println!("Usage:");
    println!("  rf exec --token {dev_token} \"<command>\"");
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

fn policy_command(action: PolicyAction) -> anyhow::Result<()> {
    let registry = TemplateRegistry::new();

    match action {
        PolicyAction::List => {
            println!("Available policy templates:\n");
            for template in registry.list() {
                println!("  {:<30} [{}]", template.name, template.category);
                println!("    {}\n", template.description);
            }
        }
        PolicyAction::Show { name } => {
            let template = registry.get(&name).ok_or_else(|| {
                anyhow::anyhow!(
                    "template '{name}' not found. Use 'rf policy list' to see available templates."
                )
            })?;
            println!("# Template: {}", template.name);
            println!("# Category: {}", template.category);
            println!("# {}\n", template.description);
            println!("{}", template.yaml);
        }
        PolicyAction::Validate { file, template } => {
            if let Some(template_name) = template {
                let tmpl = registry
                    .get(&template_name)
                    .ok_or_else(|| anyhow::anyhow!("template '{template_name}' not found"))?;
                match TemplateRegistry::validate_yaml(&tmpl.yaml) {
                    Ok(()) => println!("OK: template '{template_name}' is valid YAML"),
                    Err(e) => {
                        eprintln!("ERROR: template '{template_name}' has invalid YAML: {e}");
                        std::process::exit(1);
                    }
                }
            } else if let Some(path) = file {
                let content = std::fs::read_to_string(&path)?;
                match TemplateRegistry::validate_yaml(&content) {
                    Ok(()) => println!("OK: {} is valid policy YAML", path.display()),
                    Err(e) => {
                        eprintln!("ERROR: {} has invalid YAML: {}", path.display(), e);
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("ERROR: specify either --file or --template");
                std::process::exit(1);
            }
        }
        PolicyAction::Compose { templates } => {
            let names: Vec<&str> = templates.split(',').map(|s| s.trim()).collect();
            let mut refs = Vec::new();
            for name in &names {
                let tmpl = registry.get(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "template '{name}' not found. Use 'rf policy list' to see available."
                    )
                })?;
                refs.push(tmpl);
            }
            let composed = TemplateRegistry::compose(&refs)?;
            println!("# Composed policy from: {templates}");
            println!("# Conflict resolution: deny-wins\n");
            println!("{composed}");
        }
    }

    Ok(())
}

/// Copy files between local and remote agent.
/// Source/dest format: "agent:/remote/path" for remote, or plain path for local.
async fn cp_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    source: &str,
    dest: &str,
    chunk_size: u32,
    recursive: bool,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;
    let (chan, _peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;
    let chan = Arc::new(tokio::sync::Mutex::new(chan));

    let is_push = !source.contains(':') && dest.contains(':');
    let is_pull = source.contains(':') && !dest.contains(':');

    if is_push && recursive {
        // Recursive upload: walk local directory tree
        let remote_base = dest.split_once(':').map_or(dest, |(_, p)| p);
        let source_path = std::path::Path::new(source);
        if !source_path.is_dir() {
            anyhow::bail!("{source} is not a directory (use -r only with directories)");
        }
        let mut entries = Vec::new();
        collect_dir_entries(source_path, source_path, &mut entries)?;
        let total_files = entries.len();
        eprintln!("uploading {total_files} files from {source} → agent:{remote_base}");

        for (i, (rel_path, local_path)) in entries.iter().enumerate() {
            let remote_file = format!("{remote_base}/{rel_path}");
            let local_data = tokio::fs::read(local_path).await?;
            push_single_file(&chan, &local_data, &remote_file, chunk_size).await?;
            eprintln!("[{}/{}] {}", i + 1, total_files, rel_path);
        }
        eprintln!("done: {total_files} files transferred");
    } else if is_push {
        // Upload local file(s) to agent.
        // If the source contains glob characters, expand the pattern first.
        let remote_path = dest.split_once(':').map_or(dest, |(_, p)| p);

        let sources: Vec<std::path::PathBuf> = if source.contains('*')
            || source.contains('?')
            || (source.contains('[') && source.contains(']'))
        {
            let matches: Result<Vec<_>, _> = glob::glob(source)
                .map_err(|e| anyhow::anyhow!("invalid glob pattern '{source}': {e}"))?
                .collect();
            let paths = matches.map_err(|e| anyhow::anyhow!("glob error: {e}"))?;
            if paths.is_empty() {
                anyhow::bail!("glob pattern '{source}' matched no files");
            }
            paths
        } else {
            vec![std::path::PathBuf::from(source)]
        };

        for local_path in &sources {
            let local_str = local_path.display().to_string();
            // For glob expansions, destination is a directory on the agent: append filename
            let effective_remote = if sources.len() > 1 {
                let fname = local_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| local_str.clone());
                format!("{remote_path}/{fname}")
            } else {
                remote_path.to_string()
            };

            let local_data = tokio::fs::read(local_path).await?;
            let total = local_data.len();

            // Streaming upload: one round-trip to negotiate, then raw frames
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(&local_data);
            let checksum: String = digest.iter().map(|b| format!("{b:02x}")).collect();

            let ch = chan.lock().await;
            // 1. Send FilePushStream negotiation request
            let request = Request {
                id: format!("cp-push-{}", hex::encode(&local_data[..4.min(local_data.len())])),
                action: Action::FilePushStream {
                    path: effective_remote.clone(),
                    total_size: total as u64,
                    checksum: Some(checksum),
                    mode: None,
                    compress: false,
                },
                timeout_ms: Some(120000),
                reason: None,
            };
            let encoded = codec::encode(&request)?;
            ch.send(&encoded).await?;
            // 2. Wait for FileStreamReady
            let resp_bytes = ch.recv().await?;
            let resp: Response = codec::decode(&resp_bytes)?;
            match resp.result {
                RpcResult::FileStreamReady { .. } => {}
                RpcResult::Denied { reason, rule } => {
                    anyhow::bail!("denied: {reason} (rule: {rule})");
                }
                RpcResult::Error { message } => {
                    anyhow::bail!("error: {message}");
                }
                _ => anyhow::bail!("unexpected response to FilePushStream"),
            }
            // 3. Stream raw file data in 64 KB frames
            const STREAM_CHUNK: usize = 65536;
            let mut sent = 0usize;
            while sent < total {
                let end = (sent + STREAM_CHUNK).min(total);
                ch.send(&local_data[sent..end]).await?;
                sent = end;
                let pct = (sent * 100) / total.max(1);
                eprint!("\r{local_str} → {effective_remote}: {pct}%");
            }
            // 4. Wait for FileStreamDone
            let done_bytes = ch.recv().await?;
            drop(ch);
            let done_resp: Response = codec::decode(&done_bytes)?;
            match done_resp.result {
                RpcResult::FileStreamDone { bytes_transferred, checksum_verified } => {
                    eprintln!(
                        "\r{local_str} → {effective_remote}: 100% ({bytes_transferred} bytes{})",
                        if checksum_verified { ", checksum verified" } else { "" }
                    );
                }
                RpcResult::Error { message } => {
                    anyhow::bail!("upload failed: {message}");
                }
                _ => anyhow::bail!("unexpected response after stream upload"),
            }
            eprintln!();
        }
    } else if is_pull {
        // Streaming download: one round-trip to get metadata, then raw frames
        let remote_path = source.split_once(':').map_or(source, |(_, p)| p);
        let ch = chan.lock().await;
        // 1. Send FilePullStream request
        let request = Request {
            id: "cp-pull-stream".into(),
            action: Action::FilePullStream {
                path: remote_path.to_string(),
                compress: false,
            },
            timeout_ms: Some(120000),
            reason: None,
        };
        let encoded = codec::encode(&request)?;
        ch.send(&encoded).await?;
        // 2. Receive FileStreamReady with total_size + checksum
        let resp_bytes = ch.recv().await?;
        let resp: Response = codec::decode(&resp_bytes)?;
        let (total_size, expected_checksum) = match resp.result {
            RpcResult::FileStreamReady { total_size, checksum } => (total_size, checksum),
            RpcResult::Denied { reason, rule } => {
                anyhow::bail!("denied: {reason} (rule: {rule})");
            }
            RpcResult::Error { message } => {
                anyhow::bail!("error: {message}");
            }
            _ => anyhow::bail!("unexpected response to FilePullStream"),
        };
        // 3. Receive raw file data frames until total_size bytes collected
        let mut file_data: Vec<u8> = Vec::with_capacity(total_size as usize);
        while (file_data.len() as u64) < total_size {
            let chunk = ch.recv().await?;
            if chunk.is_empty() {
                anyhow::bail!("connection closed before transfer complete");
            }
            file_data.extend_from_slice(&chunk);
            let pct = (file_data.len() as u64 * 100) / total_size.max(1);
            eprint!("\r{source} → {dest}: {pct}%");
        }
        drop(ch);
        // 4. Verify checksum
        if let Some(expected) = expected_checksum {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest(&file_data);
            let actual: String = d.iter().map(|b| format!("{b:02x}")).collect();
            if actual != expected {
                anyhow::bail!("checksum mismatch: expected {expected}, got {actual}");
            }
        }
        // 5. Write to local file
        tokio::fs::write(dest, &file_data).await?;
        eprintln!("\r{source} → {dest}: 100% ({total_size} bytes, checksum verified)");

    } else {
        anyhow::bail!(
            "invalid copy syntax. Use: rf cp <local> <agent>:/path  or  rf cp <agent>:/path <local>"
        );
    }

    // Send close-notify
    let close_req = Request {
        id: "close".into(),
        action: Action::Ping,
        timeout_ms: None,
        reason: None,
    };
    let encoded = codec::encode(&close_req)?;
    let ch = chan.lock().await;
    let _ = ch.send(&encoded).await;
    drop(ch);

    Ok(())
}

/// Open a TCP proxy tunnel through an agent.
#[allow(clippy::too_many_arguments)]
async fn proxy_command(
    relay_url: &str,
    direct_addr: Option<&str>,
    key_path: &std::path::Path,
    token: &str,
    target: &str,
    listen: &str,
    idle_timeout: Option<u32>,
    max_duration: Option<u32>,
    http_mode: bool,
) -> anyhow::Result<()> {
    let key = StaticKey::load_or_generate(key_path)?;

    // Test connectivity to target via a short-lived probe connection
    let (probe_chan, _peer_key) = dial_agent(relay_url, direct_addr, &key, token).await?;
    let request = Request {
        id: "proxy-test".into(),
        action: Action::Proxy {
            target: target.to_string(),
            idle_timeout_secs: idle_timeout,
            max_duration_secs: max_duration,
        },
        timeout_ms: Some(10000),
        reason: None,
    };
    let encoded = codec::encode(&request)?;
    probe_chan.send(&encoded).await?;
    let resp_bytes = probe_chan.recv().await?;

    let resp: Response = codec::decode(&resp_bytes)?;
    match resp.result {
        RpcResult::ProxyConnected {
            proxy_id,
            idle_timeout_secs: eff_idle,
            max_duration_secs: eff_max,
        } => {
            eprintln!("proxy established: {listen} → agent → {target} (id: {proxy_id})");
            eprintln!("  idle timeout: {eff_idle}s, max duration: {eff_max}s");
            eprintln!("  concurrent tunnels: each connection uses a dedicated agent channel");
            eprintln!("listening on {listen} (press Ctrl+C to stop)");

            if http_mode {
                eprintln!("  mode: HTTP-aware (per-request policy enforcement)");
            }

            // Listen for local connections. Each accepted connection spawns a task
            // that creates its own dedicated agent connection (concurrent tunnels).
            let listener = tokio::net::TcpListener::bind(listen).await?;
            let cancel = CancellationToken::new();
            let cancel_clone = cancel.clone();

            // Handle Ctrl+C
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                cancel_clone.cancel();
            });

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        eprintln!("\nproxy stopped.");
                        break;
                    }
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, addr)) => {
                                eprintln!("connection from {addr}");
                                let relay_url = relay_url.to_string();
                                let direct_addr = direct_addr.map(str::to_string);
                                let key_clone = key.clone();
                                let token_clone = token.to_string();
                                let target_clone = target.to_string();
                                if http_mode {
                                    // HTTP-aware mode reuses a shared channel (one request at a time)
                                    let (http_chan, _) = match dial_agent(&relay_url, direct_addr.as_deref(), &key_clone, &token_clone).await {
                                        Ok(c) => c,
                                        Err(e) => {
                                            eprintln!("http proxy connect failed: {e}");
                                            continue;
                                        }
                                    };
                                    let http_chan = Arc::new(tokio::sync::Mutex::new(http_chan));
                                    tokio::spawn(async move {
                                        if let Err(e) = handle_http_proxy_connection(
                                            stream,
                                            http_chan,
                                            &target_clone,
                                        )
                                        .await
                                        {
                                            eprintln!("http proxy error: {e}");
                                        }
                                    });
                                } else {
                                    tokio::spawn(async move {
                                        if let Err(e) = handle_proxy_connection(
                                            stream,
                                            relay_url,
                                            direct_addr,
                                            key_clone,
                                            token_clone,
                                            target_clone,
                                            eff_idle,
                                            eff_max,
                                        )
                                        .await
                                        {
                                            eprintln!("proxy connection error: {e}");
                                        }
                                    });
                                }
                            }
                            Err(e) => {
                                eprintln!("accept error: {e}");
                            }
                        }
                    }
                }
            }
        }
        RpcResult::Denied { reason, rule } => {
            anyhow::bail!("proxy denied: {reason} (rule: {rule})");
        }
        RpcResult::Error { message } => {
            anyhow::bail!("proxy error: {message}");
        }
        _ => anyhow::bail!("unexpected response"),
    }

    Ok(())
}

/// Handle a single proxied TCP connection.
///
/// Each concurrent tunnel creates its own dedicated connection to the agent so that
/// multiple tunnels can run in parallel without contending on a shared channel.
/// After `ProxyReady` the Noise channel carries raw forwarded bytes.
async fn handle_proxy_connection(
    local: tokio::net::TcpStream,
    relay_url: String,
    direct_addr: Option<String>,
    key: StaticKey,
    token: String,
    target: String,
    idle_timeout_secs: u32,
    max_duration_secs: u32,
) -> anyhow::Result<()> {
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, Instant};

    // Dedicated connection per tunnel for concurrent operation
    let (chan, _peer_key) = dial_agent(&relay_url, direct_addr.as_deref(), &key, &token).await?;
    let chan = Arc::new(chan);

    // Open the proxy tunnel — agent connects to target and sends ProxyReady
    let request = Request {
        id: format!("proxy-{}", rand_id()),
        action: Action::ProxyOpen {
            target: target.clone(),
            idle_timeout_secs: Some(idle_timeout_secs),
            max_duration_secs: Some(max_duration_secs),
        },
        timeout_ms: Some(15000),
        reason: None,
    };
    let encoded = codec::encode(&request)?;
    chan.send(&encoded).await?;
    let resp_bytes = chan.recv().await?;
    let resp: Response = codec::decode(&resp_bytes)?;

    let (eff_idle, eff_max) = match resp.result {
        RpcResult::ProxyReady {
            idle_timeout_secs: i,
            max_duration_secs: m,
            ..
        } => (i, m),
        RpcResult::Denied { reason, rule } => {
            anyhow::bail!("proxy denied: {reason} (rule: {rule})");
        }
        RpcResult::Error { message } => {
            anyhow::bail!("proxy error: {message}");
        }
        _ => anyhow::bail!("unexpected response to ProxyOpen"),
    };

    let deadline = Instant::now() + Duration::from_secs(u64::from(eff_max));
    let idle_dur = Duration::from_secs(u64::from(eff_idle));

    // Split local TCP stream into independent halves
    let (mut local_r, mut local_w) = local.into_split();

    // Task A: local TCP → SecureChannel → agent → target
    let chan_a = chan.clone();
    let t_local_to_chan = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        loop {
            match local_r.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if chan_a.send(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Task B: agent → SecureChannel → local TCP
    let chan_b = chan;
    let t_chan_to_local = tokio::spawn(async move {
        loop {
            match chan_b.recv().await {
                Ok(data) if data.is_empty() => break, // close-notify
                Ok(data) => {
                    if local_w.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Apply idle timeout: cancel both tasks if no activity within idle window.
    // The idle check is implemented via a max-duration deadline; the agent closes
    // from its side when the target TCP connection goes idle.
    let remaining = deadline.saturating_duration_since(Instant::now());
    let _ = idle_dur; // enforced agent-side as well
    tokio::select! {
        _ = t_local_to_chan => {}
        _ = t_chan_to_local => {}
        _ = tokio::time::sleep(remaining) => {
            eprintln!("proxy connection closed: max duration ({eff_max}s) reached");
        }
    }

    Ok(())
}

fn rand_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{t:x}")
}

/// Handle an incoming HTTP connection in HTTP-aware proxy mode.
/// Reads the full HTTP request, sends HttpForward RPC to the agent,
/// and writes the response back to the local client.
async fn handle_http_proxy_connection(
    mut local: tokio::net::TcpStream,
    chan: Arc<tokio::sync::Mutex<AgentChannel>>,
    target: &str,
) -> anyhow::Result<()> {
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Read incoming HTTP request (headers + body)
    let mut buf = vec![0u8; 65536];
    let mut total_read = 0usize;

    // Read until we have complete headers
    loop {
        if total_read >= buf.len() {
            anyhow::bail!("request too large for header buffer");
        }
        let n = local.read(&mut buf[total_read..]).await?;
        if n == 0 {
            return Ok(()); // Client disconnected
        }
        total_read += n;

        // Check if we have complete headers
        if buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    // Parse the HTTP request
    let mut headers_arr = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers_arr);
    let header_len = match req.parse(&buf[..total_read]) {
        Ok(httparse::Status::Complete(len)) => len,
        Ok(httparse::Status::Partial) => {
            anyhow::bail!("incomplete HTTP request");
        }
        Err(e) => {
            anyhow::bail!("invalid HTTP request: {e}");
        }
    };

    let method = req.method.unwrap_or("GET").to_string();
    let path = req.path.unwrap_or("/").to_string();

    // Extract headers
    let mut headers = HashMap::new();
    let mut content_length: usize = 0;
    for h in req.headers.iter() {
        if h.name.is_empty() {
            break;
        }
        let value = String::from_utf8_lossy(h.value).to_string();
        if h.name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().unwrap_or(0);
        }
        headers.insert(h.name.to_string(), value);
    }

    // Read remaining body if content-length specified
    let _body_so_far = total_read - header_len;
    let mut body = buf[header_len..total_read].to_vec();
    while body.len() < content_length {
        let remaining = content_length - body.len();
        let mut chunk = vec![0u8; remaining.min(8192)];
        let n = local.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }

    // Send HttpForward RPC
    let request = Request {
        id: format!("http-{}", rand_id()),
        action: Action::HttpForward {
            target: target.to_string(),
            method: method.clone(),
            path: path.clone(),
            headers,
            body,
        },
        timeout_ms: Some(30000),
        reason: None,
    };

    let encoded = codec::encode(&request)?;
    let ch = chan.lock().await;
    ch.send(&encoded).await?;
    let resp_bytes = ch.recv().await?;
    drop(ch);

    let resp: Response = codec::decode(&resp_bytes)?;
    match resp.result {
        RpcResult::HttpResponse {
            status_code,
            headers: resp_headers,
            body: resp_body,
            latency_ms,
        } => {
            // Build raw HTTP response
            let status_text = http_status_text(status_code);
            let mut raw_response = format!("HTTP/1.1 {status_code} {status_text}\r\n");
            for (key, value) in &resp_headers {
                raw_response.push_str(&format!("{key}: {value}\r\n"));
            }
            // Ensure content-length is set
            if !resp_headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-length"))
            {
                raw_response.push_str(&format!("Content-Length: {}\r\n", resp_body.len()));
            }
            raw_response.push_str("\r\n");

            local.write_all(raw_response.as_bytes()).await?;
            if !resp_body.is_empty() {
                local.write_all(&resp_body).await?;
            }

            eprintln!(
                "  {method} {path} → {status_code} ({} bytes, {latency_ms}ms)",
                resp_body.len()
            );
        }
        RpcResult::Denied { reason, rule } => {
            // Return 403 to client
            let body_text = format!("Denied: {reason} (rule: {rule})");
            let raw = format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{body_text}",
                body_text.len()
            );
            local.write_all(raw.as_bytes()).await?;
            eprintln!("  {method} {path} → 403 (denied: {reason})");
        }
        RpcResult::Error { message } => {
            let body_text = format!("Proxy error: {message}");
            let raw = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Length: {}\r\nContent-Type: text/plain\r\n\r\n{body_text}",
                body_text.len()
            );
            local.write_all(raw.as_bytes()).await?;
            eprintln!("  {method} {path} → 502 ({message})");
        }
        _ => {
            let raw = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
            local.write_all(raw.as_bytes()).await?;
        }
    }

    Ok(())
}

/// Map HTTP status code to reason phrase.
fn http_status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "OK",
    }
}

/// Recursively collect all file entries in a directory.
fn collect_dir_entries(
    base: &std::path::Path,
    dir: &std::path::Path,
    entries: &mut Vec<(String, std::path::PathBuf)>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_entries(base, &path, entries)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            entries.push((rel, path));
        }
    }
    Ok(())
}

/// Push a single file to the agent via chunked FilePush.
async fn push_single_file(
    chan: &Arc<tokio::sync::Mutex<AgentChannel>>,
    data: &[u8],
    remote_path: &str,
    _chunk_size: u32,
) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    let checksum: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let total = data.len();

    let ch = chan.lock().await;

    // 1. Negotiate upload
    let request = Request {
        id: format!("cp-push-{}", hex::encode(&data[..4.min(total)])),
        action: Action::FilePushStream {
            path: remote_path.to_string(),
            total_size: total as u64,
            checksum: Some(checksum),
            mode: None,
            compress: false,
        },
        timeout_ms: Some(120000),
        reason: None,
    };
    let encoded = codec::encode(&request)?;
    ch.send(&encoded).await?;

    // 2. Wait for FileStreamReady
    let resp_bytes = ch.recv().await?;
    let resp: Response = codec::decode(&resp_bytes)?;
    match resp.result {
        RpcResult::FileStreamReady { .. } => {}
        RpcResult::Denied { reason, rule } => {
            anyhow::bail!("denied: {reason} (rule: {rule})");
        }
        RpcResult::Error { message } => {
            anyhow::bail!("error: {message}");
        }
        _ => anyhow::bail!("unexpected response to FilePushStream"),
    }

    // 3. Stream raw data in 64 KB frames
    const CHUNK: usize = 65536;
    let mut sent = 0;
    while sent < total {
        let end = (sent + CHUNK).min(total);
        ch.send(&data[sent..end]).await?;
        sent = end;
    }

    // 4. Wait for FileStreamDone
    let done_bytes = ch.recv().await?;
    drop(ch);
    let done_resp: Response = codec::decode(&done_bytes)?;
    match done_resp.result {
        RpcResult::FileStreamDone { .. } => Ok(()),
        RpcResult::Error { message } => anyhow::bail!("upload failed: {message}"),
        _ => anyhow::bail!("unexpected response after stream upload"),
    }
}
