//! RavenFabric Agent — connects to relay, authenticates, and executes RPC requests.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{error, info};

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
    /// Agent ID
    #[arg(short = 'i', long)]
    id: String,

    /// Relay WebSocket URL (e.g. ws://relay:9090)
    #[arg(short, long)]
    relay: String,

    /// Meet token for relay pairing
    #[arg(short, long)]
    token: String,

    /// Path to agent key file
    #[arg(short, long, default_value = "agent.key")]
    key_path: PathBuf,

    /// Path to policy YAML file
    #[arg(short, long, default_value = "policy.yaml")]
    policy_path: PathBuf,

    /// Path to audit log file
    #[arg(short, long, default_value = "audit.jsonl")]
    audit_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Load or generate identity key
    let key = StaticKey::load_or_generate(&args.key_path)?;
    info!("agent {} public key: {}", args.id, key.public_hex());

    // Load policy
    let policy = RpcPolicy::load(&args.policy_path)
        .map_err(|e| anyhow::anyhow!("failed to load policy: {}", e))?;
    let policy = Arc::new(RwLock::new(policy));
    info!("policy loaded from {}", args.policy_path.display());

    // Open audit logger
    let audit = Arc::new(FileAuditLogger::new(args.audit_path.clone())?);
    info!("audit log: {}", args.audit_path.display());

    // Connect to relay
    let driver = WebSocketDriver::new();
    let target = Target {
        agent_id: args.id.clone(),
        relay_url: Some(args.relay.clone()),
        meet_token: Some(args.token.clone()),
    };

    info!("connecting to relay: {}", args.relay);
    let mut stream = driver.dial(&target, &Default::default()).await?;

    // Send meet token as first WS binary message (relay pairing protocol).
    // The WebSocket bridge converts our writes into WS Binary messages.
    stream.write_all(args.token.as_bytes()).await?;
    stream.flush().await?;

    // Noise handshake (agent is responder)
    info!("performing Noise XX handshake...");
    let (state, peer_key) = handshake(&mut stream, false, &key).await?;
    info!("handshake complete, peer key: {}", hex::encode(peer_key));

    // Split the stream for SecureChannel
    let (stream_read, stream_write) = tokio::io::split(stream);
    let chan = SecureChannel::new(stream_read, stream_write, state, peer_key);

    // Create executor
    let executor = Executor::new(policy, audit, hex::encode(peer_key));

    // RPC loop
    info!("agent {} ready, waiting for RPC requests", args.id);
    loop {
        let data = match chan.recv().await {
            Ok(d) => d,
            Err(e) => {
                error!("channel recv error: {}", e);
                break;
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
            error!("channel send error: {}", e);
            break;
        }
    }

    info!("agent {} shutting down", args.id);
    Ok(())
}
