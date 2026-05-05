//! RavenFabric CLI — `rf` command for remote execution and management.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tokio::io::AsyncWriteExt;
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
    /// Start local development mode (agent + relay, no auth)
    Dev,
    /// Show status
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("rf=info").init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Exec { token, command } => {
            exec_command(&cli.relay, &cli.key_path, &token, &command).await?;
        }
        Commands::Dev => {
            println!("rf dev — not yet implemented");
            println!("Hint: run rf-relay and rf-agent separately for now.");
        }
        Commands::Status => {
            println!("rf status — not yet implemented");
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
    // Load or generate identity key
    let key = StaticKey::load_or_generate(key_path)?;
    info!("client public key: {}", key.public_hex());

    // Connect to relay
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: String::new(),
        relay_url: Some(relay_url.to_string()),
        meet_token: Some(token.to_string()),
    };

    info!("connecting to relay: {}", relay_url);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Send meet token for relay pairing
    stream.write_all(token.as_bytes()).await?;
    stream.flush().await?;

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
    }

    Ok(())
}
