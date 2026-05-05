//! RavenFabric Relay — Stateless encrypted relay broker.
//!
//! The relay never decrypts traffic (end-to-end encryption between agent and client).
//! It simply pairs agents and clients by meet token, then bridges their byte streams.

use std::collections::HashMap;
use std::sync::Arc;

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "rf-relay", about = "RavenFabric stateless relay broker")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:9090")]
    listen: String,
}

/// A pending connection waiting for its pair.
struct PendingPeer {
    /// Send messages TO this peer (their inbound).
    to_peer: mpsc::UnboundedSender<Message>,
    /// Receive messages FROM this peer (their outbound).
    from_peer: mpsc::UnboundedReceiver<Message>,
}

/// Relay state: map of meet tokens to pending peers.
type MeetState = Arc<Mutex<HashMap<String, PendingPeer>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let state: MeetState = Arc::new(Mutex::new(HashMap::new()));

    let listener = TcpListener::bind(&args.listen).await?;
    info!("rf-relay listening on {}", args.listen);

    loop {
        let (tcp_stream, addr) = listener.accept().await?;
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let ws_stream = match tokio_tungstenite::accept_async(tcp_stream).await {
                Ok(ws) => ws,
                Err(e) => {
                    warn!("WS accept failed from {}: {}", addr, e);
                    return;
                }
            };

            if let Err(e) = handle_connection(ws_stream, state).await {
                warn!("Connection from {} ended: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(
    ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: MeetState,
) -> anyhow::Result<()> {
    let (mut ws_sink, mut ws_source) = ws.split();

    // First message must be the meet token
    let meet_token = match ws_source.next().await {
        Some(Ok(Message::Text(token))) => token.to_string(),
        Some(Ok(Message::Binary(data))) => String::from_utf8_lossy(&data).to_string(),
        _ => return Err(anyhow::anyhow!("expected meet token as first message")),
    };

    info!("peer connected with meet token: {}", meet_token);

    // Check if there's already a peer waiting with this token
    let mut pending = state.lock().await;

    if let Some(other) = pending.remove(&meet_token) {
        drop(pending);
        info!("paired meet token: {}", meet_token);

        // We are the second peer. We got the first peer's channels:
        // - other.to_peer: send TO first peer
        // - other.from_peer: receive FROM first peer
        let to_first = other.to_peer;
        let mut from_first = other.from_peer;

        // Forward: our WS → first peer
        let forward_to_first = tokio::spawn(async move {
            while let Some(msg) = ws_source.next().await {
                match msg {
                    Ok(msg @ Message::Binary(_)) => {
                        if to_first.send(msg).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        // Forward: first peer → our WS
        let forward_from_first = tokio::spawn(async move {
            while let Some(msg) = from_first.recv().await {
                if ws_sink.send(msg).await.is_err() {
                    break;
                }
            }
        });

        let _ = tokio::join!(forward_to_first, forward_from_first);
    } else {
        // We are the first peer. Register and wait.
        // Create channels:
        // - inbound_tx/inbound_rx: other peer sends TO us via inbound_tx, we read from inbound_rx
        // - outbound_tx/outbound_rx: we send outbound via outbound_tx, other peer reads from outbound_rx
        let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel::<Message>();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<Message>();

        pending.insert(
            meet_token.clone(),
            PendingPeer {
                to_peer: inbound_tx,
                from_peer: outbound_rx,
            },
        );
        drop(pending);

        // Forward: our WS → outbound channel (for second peer to receive)
        let forward_outbound = tokio::spawn(async move {
            while let Some(msg) = ws_source.next().await {
                match msg {
                    Ok(msg @ Message::Binary(_)) => {
                        if outbound_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        // Forward: inbound channel → our WS (messages from second peer)
        let forward_inbound = tokio::spawn(async move {
            while let Some(msg) = inbound_rx.recv().await {
                if ws_sink.send(msg).await.is_err() {
                    break;
                }
            }
        });

        let _ = tokio::join!(forward_outbound, forward_inbound);

        // Clean up if disconnected before pairing
        let mut pending = state.lock().await;
        pending.remove(&meet_token);
    }

    Ok(())
}
