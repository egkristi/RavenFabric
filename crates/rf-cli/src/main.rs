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
        Commands::Forward {
            token,
            local,
            remote,
        } => {
            forward_command(&cli.relay, &cli.key_path, &token, &local, &remote).await?;
        }
        Commands::Playbook { file, token } => {
            playbook_command(&cli.relay, &cli.key_path, &token, &file).await?;
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

    Ok(())
}

/// Port forwarding: binds a local port and forwards connections through the agent.
async fn forward_command(
    relay_url: &str,
    key_path: &std::path::Path,
    token: &str,
    local_addr: &str,
    remote_addr: &str,
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

    // Request port forward
    let request = Request {
        id: uuid::Uuid::new_v4().to_string(),
        action: Action::PortForward {
            bind_addr: local_addr.to_string(),
            target_addr: remote_addr.to_string(),
        },
        timeout_ms: Some(10_000),
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
            println!(
                "Port forward active: {bind_addr} → {remote_addr} (id: {forward_id})"
            );
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
    key_path: &std::path::Path,
    token: &str,
    cols: u16,
    rows: u16,
) -> anyhow::Result<()> {
    use std::os::unix::io::AsRawFd;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
    let chan = Arc::new(SecureChannel::new(
        stream_read,
        stream_write,
        state,
        peer_key,
    ));

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
    _key_path: &std::path::Path,
    _token: &str,
    _cols: u16,
    _rows: u16,
) -> anyhow::Result<()> {
    anyhow::bail!("interactive shell is not supported on this platform");
}

/// Development mode: starts a relay and agent in a single process with permissive policy.
async fn dev_mode(port: u16) -> anyhow::Result<()> {
    let listen_addr = format!("127.0.0.1:{port}");
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
