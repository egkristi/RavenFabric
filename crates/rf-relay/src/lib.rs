//! RavenFabric Relay library — stateless encrypted relay broker.
//!
//! Exposes `run_relay()` for embedding in other binaries (e.g., `rf dev`).

pub mod geoip;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

/// Per-IP rate limiter using a sliding window counter.
struct RateLimiter {
    /// Maximum connections per window.
    max_connections: u32,
    /// Window duration in seconds.
    window_secs: u64,
    /// Connections tracked as timestamps.
    connections: HashMap<IpAddr, Vec<Instant>>,
}

impl RateLimiter {
    fn new(max_connections: u32, window_secs: u64) -> Self {
        Self {
            max_connections,
            window_secs,
            connections: HashMap::new(),
        }
    }

    /// Check if a connection from this IP is allowed. Returns true if allowed.
    fn check_and_record(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let timestamps = self.connections.entry(ip).or_default();

        // Remove expired entries
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= self.max_connections as usize {
            false
        } else {
            timestamps.push(now);
            true
        }
    }

    /// Periodically clean up entries for IPs with no recent connections.
    fn cleanup(&mut self) {
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);
        self.connections
            .retain(|_, timestamps| timestamps.iter().any(|t| now.duration_since(*t) < window));
    }
}

/// A pending connection waiting for its pair.
struct PendingPeer {
    to_peer: mpsc::UnboundedSender<Message>,
    from_peer: mpsc::UnboundedReceiver<Message>,
}

type MeetState = Arc<Mutex<HashMap<String, PendingPeer>>>;
type RateLimiterState = Arc<Mutex<RateLimiter>>;

type HmacSha256 = Hmac<Sha256>;

/// Verify that a meet token is properly HMAC-signed.
/// Token format: `<payload>.<hex_mac>`
/// If no secret is configured, all tokens are accepted.
fn verify_meet_token(token: &str, secret: Option<&str>) -> bool {
    let Some(secret) = secret else {
        return true; // No secret configured, accept all tokens
    };

    let Some((payload, mac_hex)) = token.rsplit_once('.') else {
        return false; // No separator found
    };

    let Ok(mac_bytes) = hex::decode(mac_hex) else {
        return false; // Invalid hex
    };

    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload.as_bytes());
    mac.verify_slice(&mac_bytes).is_ok()
}

/// Run the relay server on the given address.
/// This function runs indefinitely until the provided cancellation token is triggered.
/// If `meet_secret` is provided, meet tokens must be HMAC-signed with that secret.
pub async fn run_relay(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    run_relay_with_secret(listen_addr, cancel, None).await
}

/// Run the relay server with optional HMAC token verification.
pub async fn run_relay_with_secret(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
) -> anyhow::Result<()> {
    let state: MeetState = Arc::new(Mutex::new(HashMap::new()));
    // Rate limit: 20 connections per IP per 60 seconds
    let rate_limiter: RateLimiterState = Arc::new(Mutex::new(RateLimiter::new(20, 60)));
    let listener = TcpListener::bind(listen_addr).await?;
    info!("rf-relay listening on {}", listen_addr);
    if meet_secret.is_some() {
        info!("HMAC meet token verification enabled");
    }

    let meet_secret = Arc::new(meet_secret);
    let mut connections = tokio::task::JoinSet::new();
    let mut cleanup_interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("relay shutting down");
                connections.abort_all();
                break;
            }
            _ = cleanup_interval.tick() => {
                let mut rl = rate_limiter.lock().await;
                rl.cleanup();
            }
            result = listener.accept() => {
                let (tcp_stream, addr) = result?;
                let ip = addr.ip();

                // Rate limit check
                {
                    let mut rl = rate_limiter.lock().await;
                    if !rl.check_and_record(ip) {
                        warn!("rate limit exceeded for {}, dropping connection", ip);
                        continue;
                    }
                }

                let state = Arc::clone(&state);
                let cancel = cancel.clone();
                let meet_secret = Arc::clone(&meet_secret);
                connections.spawn(async move {
                    let ws_stream = match tokio_tungstenite::accept_async(tcp_stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            warn!("WS accept failed from {}: {}", addr, e);
                            return;
                        }
                    };
                    if let Err(e) = handle_connection(ws_stream, state, cancel, &meet_secret).await {
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
    meet_secret: &Option<String>,
) -> anyhow::Result<()> {
    let (mut ws_sink, mut ws_source) = ws.split();

    // First message must be the meet token
    let meet_token = match ws_source.next().await {
        Some(Ok(Message::Text(token))) => token.to_string(),
        Some(Ok(Message::Binary(data))) => String::from_utf8_lossy(&data).to_string(),
        _ => return Err(anyhow::anyhow!("expected meet token as first message")),
    };

    // HMAC verification if secret is configured
    if !verify_meet_token(&meet_token, meet_secret.as_deref()) {
        warn!("invalid meet token (HMAC verification failed)");
        return Err(anyhow::anyhow!("meet token HMAC verification failed"));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_hmac_verification_no_secret() {
        // No secret → all tokens pass
        assert!(verify_meet_token("anything", None));
        assert!(verify_meet_token("", None));
    }

    #[test]
    fn test_hmac_verification_valid_token() {
        use hmac::Mac;
        let secret = "my-secret";
        let payload = "agent-001";

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let mac_hex = hex::encode(result.into_bytes());

        let token = format!("{payload}.{mac_hex}");
        assert!(verify_meet_token(&token, Some(secret)));
    }

    #[test]
    fn test_hmac_verification_invalid_token() {
        let secret = "my-secret";

        // No separator
        assert!(!verify_meet_token("noseparator", Some(secret)));

        // Wrong HMAC
        assert!(!verify_meet_token("payload.deadbeef", Some(secret)));

        // Invalid hex
        assert!(!verify_meet_token("payload.notvalidhex!!!", Some(secret)));
    }

    #[test]
    fn test_hmac_verification_wrong_secret() {
        use hmac::Mac;
        let payload = "agent-001";

        let mut mac = HmacSha256::new_from_slice(b"secret-a").unwrap();
        mac.update(payload.as_bytes());
        let mac_hex = hex::encode(mac.finalize().into_bytes());

        let token = format!("{payload}.{mac_hex}");
        // Verify with different secret fails
        assert!(!verify_meet_token(&token, Some("secret-b")));
    }

    #[test]
    fn test_rate_limiter_allows_under_limit() {
        let mut rl = RateLimiter::new(3, 60);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        assert!(rl.check_and_record(ip));
        assert!(rl.check_and_record(ip));
        assert!(rl.check_and_record(ip));
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let mut rl = RateLimiter::new(2, 60);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        assert!(rl.check_and_record(ip));
        assert!(rl.check_and_record(ip));
        assert!(!rl.check_and_record(ip)); // Over limit
    }

    #[test]
    fn test_rate_limiter_different_ips_independent() {
        let mut rl = RateLimiter::new(1, 60);
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        assert!(rl.check_and_record(ip1));
        assert!(!rl.check_and_record(ip1)); // Over limit for ip1
        assert!(rl.check_and_record(ip2)); // ip2 is independent
    }
}
