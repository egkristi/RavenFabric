//! RavenFabric Relay library — stateless encrypted relay broker.
//!
//! Exposes `run_relay()` for embedding in other binaries (e.g., `rf dev`).

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

/// A pending connection waiting for its pair.
struct PendingPeer {
    to_peer: mpsc::UnboundedSender<Message>,
    from_peer: mpsc::UnboundedReceiver<Message>,
}

type MeetState = Arc<Mutex<HashMap<String, PendingPeer>>>;

/// Run the relay server on the given address.
/// This function runs indefinitely until the provided cancellation token is triggered.
pub async fn run_relay(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    let state: MeetState = Arc::new(Mutex::new(HashMap::new()));
    let listener = TcpListener::bind(listen_addr).await?;
    info!("rf-relay listening on {}", listen_addr);

    let mut connections = tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("relay shutting down");
                connections.abort_all();
                break;
            }
            result = listener.accept() => {
                let (tcp_stream, addr) = result?;
                let state = Arc::clone(&state);
                let cancel = cancel.clone();
                connections.spawn(async move {
                    let ws_stream = match tokio_tungstenite::accept_async(tcp_stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            warn!("WS accept failed from {}: {}", addr, e);
                            return;
                        }
                    };
                    if let Err(e) = handle_connection(ws_stream, state, cancel).await {
                        warn!("Connection from {} ended: {}", addr, e);
                    }
                });
            }
            // Reap completed connection tasks
            Some(_) = connections.join_next() => {}
        }
    }

    Ok(())
}

async fn handle_connection(
    ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: MeetState,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    let (mut ws_sink, mut ws_source) = ws.split();

    // First message must be the meet token
    let meet_token = match ws_source.next().await {
        Some(Ok(Message::Text(token))) => token.to_string(),
        Some(Ok(Message::Binary(data))) => String::from_utf8_lossy(&data).to_string(),
        _ => return Err(anyhow::anyhow!("expected meet token as first message")),
    };

    info!("peer connected with meet token: {}", meet_token);

    let mut pending = state.lock().await;

    if let Some(other) = pending.remove(&meet_token) {
        drop(pending);
        info!("paired meet token: {}", meet_token);

        let to_first = other.to_peer;
        let mut from_first = other.from_peer;

        // Forward between peers without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {}
            _ = async {
                while let Some(msg) = ws_source.next().await {
                    match msg {
                        Ok(msg @ Message::Binary(_)) => {
                            if to_first.send(msg).is_err() { break; }
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            } => {}
            _ = async {
                while let Some(msg) = from_first.recv().await {
                    if ws_sink.send(msg).await.is_err() { break; }
                }
            } => {}
        }
    } else {
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

        // Forward without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {}
            _ = async {
                while let Some(msg) = ws_source.next().await {
                    match msg {
                        Ok(msg @ Message::Binary(_)) => {
                            if outbound_tx.send(msg).is_err() { break; }
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            } => {}
            _ = async {
                while let Some(msg) = inbound_rx.recv().await {
                    if ws_sink.send(msg).await.is_err() { break; }
                }
            } => {}
        }

        // Clean up if disconnected before pairing
        let mut pending = state.lock().await;
        pending.remove(&meet_token);
    }

    Ok(())
}
