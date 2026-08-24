//! RavenFabric Relay library — stateless encrypted relay broker.
//!
//! Exposes `run_relay()` for embedding in other binaries (e.g., `rf dev`).

pub mod cross_region;
pub mod geoip;

use cross_region::{ForwardConfig, bridge_to_remote_relay_inner, parse_forward_token};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
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
///
/// IPv6 addresses are bucketed by their /64 network prefix (ROADMAP F6): a
/// single /64 is the smallest block a client is typically assigned, so
/// rate-limiting the full /128 would let one /64 bypass the limiter with 2^64
/// distinct source addresses.
struct RateLimiter {
    /// Maximum connections per window.
    max_connections: u32,
    /// Window duration in seconds.
    window_secs: u64,
    /// Connections tracked as timestamps.
    connections: HashMap<RateKey, Vec<Instant>>,
}

/// Key used to bucket an IP for rate limiting.
///
/// IPv4 uses the full address; IPv6 uses the first 64 bits (the network prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RateKey {
    V4(Ipv4Addr),
    V6([u8; 8]),
}

impl RateKey {
    fn from_ip(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(v4) => RateKey::V4(v4),
            IpAddr::V6(v6) => {
                let octets = v6.octets();
                RateKey::V6(octets[..8].try_into().expect("8-byte slice"))
            }
        }
    }
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

        let timestamps = self.connections.entry(RateKey::from_ip(ip)).or_default();

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
    to_peer: mpsc::Sender<Message>,
    from_peer: mpsc::Receiver<Message>,
}

/// Why a session ended. Used for structured logging and metrics (ROADMAP R0.4/R0.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseReason {
    /// One of the peers closed the connection.
    PeerClosed,
    /// `max_session_bytes` quota exceeded.
    QuotaBytes,
    /// `max_session_secs` quota exceeded.
    QuotaTime,
    /// No activity within `idle_timeout_secs`.
    Idle,
    /// Relay is shutting down.
    Shutdown,
}

impl CloseReason {
    fn as_str(&self) -> &'static str {
        match self {
            CloseReason::PeerClosed => "peer_closed",
            CloseReason::QuotaBytes => "quota_bytes",
            CloseReason::QuotaTime => "quota_time",
            CloseReason::Idle => "idle",
            CloseReason::Shutdown => "shutdown",
        }
    }
}

/// Per-session usage meter, shared between the two shuttle directions.
///
/// Tracks bytes in each direction plus last-activity timestamp so that the
/// shuttle loop can enforce byte/time/idle quotas without unbounded buffering
/// (ROADMAP F4).
struct SessionMeter {
    bytes_a_to_b: std::sync::atomic::AtomicU64,
    bytes_b_to_a: std::sync::atomic::AtomicU64,
    last_activity_ms: std::sync::atomic::AtomicU64,
}

impl SessionMeter {
    fn new() -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            bytes_a_to_b: std::sync::atomic::AtomicU64::new(0),
            bytes_b_to_a: std::sync::atomic::AtomicU64::new(0),
            last_activity_ms: std::sync::atomic::AtomicU64::new(now_ms),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Record `n` bytes flowing a→b. Returns `Some(CloseReason)` if a quota trips.
    fn record_a_to_b(&self, n: usize, limits: &RelayLimits) -> Option<CloseReason> {
        self.last_activity_ms
            .store(Self::now_ms(), std::sync::atomic::Ordering::Relaxed);
        if limits.max_session_bytes == 0 {
            return None;
        }
        let total = self
            .bytes_a_to_b
            .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed)
            + n as u64;
        if total > limits.max_session_bytes {
            Some(CloseReason::QuotaBytes)
        } else {
            None
        }
    }

    /// Record `n` bytes flowing b→a. Returns `Some(CloseReason)` if a quota trips.
    fn record_b_to_a(&self, n: usize, limits: &RelayLimits) -> Option<CloseReason> {
        self.last_activity_ms
            .store(Self::now_ms(), std::sync::atomic::Ordering::Relaxed);
        if limits.max_session_bytes == 0 {
            return None;
        }
        let total = self
            .bytes_b_to_a
            .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed)
            + n as u64;
        if total > limits.max_session_bytes {
            Some(CloseReason::QuotaBytes)
        } else {
            None
        }
    }

    /// Check time-based limits. `elapsed_secs` is seconds since session start.
    /// Returns `Some(CloseReason)` if a limit trips.
    fn check_time(&self, elapsed_secs: u64, limits: &RelayLimits) -> Option<CloseReason> {
        if limits.max_session_secs > 0 && elapsed_secs > limits.max_session_secs {
            return Some(CloseReason::QuotaTime);
        }
        if limits.idle_timeout_secs > 0 {
            let last = self
                .last_activity_ms
                .load(std::sync::atomic::Ordering::Relaxed);
            let idle_ms = Self::now_ms().saturating_sub(last);
            if idle_ms > limits.idle_timeout_secs * 1000 {
                return Some(CloseReason::Idle);
            }
        }
        None
    }
}

/// Default depth for the bounded per-session shuttle channels.
///
/// A bounded channel gives natural backpressure: `send().await` on a full
/// channel blocks until the peer drains it, propagating the pressure back to
/// the TCP window instead of buffering without bound in RAM (ROADMAP F1).
pub const DEFAULT_CHANNEL_DEPTH: usize = 256;

/// Per-session resource limits, shared across all connection tasks.
///
/// Values of `0` mean "unlimited" for byte/time quotas.
#[derive(Debug, Clone)]
pub struct RelayLimits {
    /// Depth of the bounded shuttle channels (backpressure). Always >= 1.
    pub channel_depth: usize,
    /// Maximum bytes a session may carry before being closed (`0` = off).
    pub max_session_bytes: u64,
    /// Maximum seconds a session may live before being closed (`0` = off).
    pub max_session_secs: u64,
    /// Idle timeout in seconds before a session is closed.
    pub idle_timeout_secs: u64,
    /// How long an unpaired peer waits for its counterpart before being dropped.
    pub pairing_timeout_secs: u64,
    /// Maximum concurrent connections (hard cap on the task JoinSet).
    pub max_connections: usize,
}

impl Default for RelayLimits {
    fn default() -> Self {
        Self {
            channel_depth: DEFAULT_CHANNEL_DEPTH,
            max_session_bytes: 0,
            max_session_secs: 0,
            idle_timeout_secs: 300,
            pairing_timeout_secs: 60,
            max_connections: 5000,
        }
    }
}

type MeetState = Arc<Mutex<HashMap<String, PendingPeer>>>;
type RateLimiterState = Arc<Mutex<RateLimiter>>;

type HmacSha256 = Hmac<Sha256>;

/// Redacted token identifier — the first 8 hex characters of `SHA-256(token)`.
///
/// Safe to log: a rendezvous meet token is a 256-bit one-time secret, and its
/// full value must never appear in logs (see ROADMAP finding F2). The truncated
/// hash is enough to correlate "connected" with "paired" in log output without
/// revealing the token.
fn token_id(token: &str) -> String {
    use sha2::Digest;
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)[..8].to_string()
}

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

/// Run the relay server with optional HMAC token verification and cross-region forwarding.
pub async fn run_relay_with_secret(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
) -> anyhow::Result<()> {
    run_relay_full(listen_addr, cancel, meet_secret, ForwardConfig::default()).await
}

/// Run the relay with full configuration including cross-region forwarding policy.
pub async fn run_relay_full(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
    forward_config: ForwardConfig,
) -> anyhow::Result<()> {
    run_relay_with_limits(
        listen_addr,
        cancel,
        meet_secret,
        forward_config,
        RelayLimits::default(),
    )
    .await
}

/// Run the relay with full configuration plus explicit per-session limits.
pub async fn run_relay_with_limits(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
    forward_config: ForwardConfig,
    limits: RelayLimits,
) -> anyhow::Result<()> {
    let channel_depth = limits.channel_depth.max(1);
    let max_connections = limits.max_connections.max(1);
    let state: MeetState = Arc::new(Mutex::new(HashMap::new()));
    // Rate limit: 20 connections per IP per 60 seconds
    let rate_limiter: RateLimiterState = Arc::new(Mutex::new(RateLimiter::new(20, 60)));
    let listener = TcpListener::bind(listen_addr).await?;
    info!("rf-relay listening on {}", listen_addr);
    if meet_secret.is_some() {
        info!("HMAC meet token verification enabled");
    }
    info!(
        "relay limits: channel_depth={}, max_connections={}, max_session_bytes={}, max_session_secs={}, idle_timeout_secs={}, pairing_timeout_secs={}",
        channel_depth,
        max_connections,
        limits.max_session_bytes,
        limits.max_session_secs,
        limits.idle_timeout_secs,
        limits.pairing_timeout_secs,
    );
    if forward_config.allow_forwarding {
        info!(
            "cross-region forwarding ENABLED (allowlist: {})",
            if forward_config.forward_allowlist.is_empty() {
                "all targets permitted".to_string()
            } else {
                format!("{} allowed targets", forward_config.forward_allowlist.len())
            }
        );
    }

    let meet_secret = Arc::new(meet_secret);
    let forward_config = Arc::new(forward_config);
    let limits = Arc::new(limits);
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

                // Hard cap on concurrent connections (F7).
                if connections.len() >= max_connections {
                    warn!("connection limit reached ({max_connections}), dropping connection from {}", ip);
                    drop(tcp_stream);
                    continue;
                }

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
                let forward_config = Arc::clone(&forward_config);
                let limits = Arc::clone(&limits);
                connections.spawn(async move {
                    let ws_stream = match tokio_tungstenite::accept_async(tcp_stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            warn!("WS accept failed from {}: {}", addr, e);
                            return;
                        }
                    };
                    if let Err(e) = handle_connection(ws_stream, state, cancel, &meet_secret, &forward_config, &limits).await {
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
    forward_config: &ForwardConfig,
    limits: &RelayLimits,
) -> anyhow::Result<()> {
    handle_connection_inner(
        ws,
        state,
        cancel,
        meet_secret,
        forward_config,
        forward_config.compat_mode,
        limits,
    )
    .await
}

/// Internal handler with optional compat mode for cross-platform relay issues.
/// When `compat_mode` is true, adds a small yield between forwarded messages
/// to prevent race conditions on certain platform combinations (macOS→Linux).
#[allow(clippy::too_many_arguments)]
async fn handle_connection_inner(
    ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: MeetState,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: &Option<String>,
    forward_config: &ForwardConfig,
    compat_mode: bool,
    limits: &RelayLimits,
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

    // ── Cross-region forwarding check ────────────────────────────────────────
    if let Some((target_url, inner_token)) = parse_forward_token(&meet_token) {
        if !forward_config.is_target_allowed(target_url) {
            warn!("cross-region forward to {} denied by policy", target_url);
            return Err(anyhow::anyhow!(
                "cross-region forwarding denied: target not in allowlist"
            ));
        }
        // Reassemble the local WebSocket stream and hand off to bridge.
        let reassembled = ws_sink
            .reunite(ws_source)
            .map_err(|_| anyhow::anyhow!("failed to reunite WebSocket streams for forwarding"))?;
        return bridge_to_remote_relay_inner(
            reassembled,
            target_url,
            inner_token,
            cancel,
            forward_config.compat_mode,
        )
        .await;
    }

    // ── Normal same-relay pairing ─────────────────────────────────────────────
    info!("peer connected with meet token: {}", token_id(&meet_token));

    let mut pending = state.lock().await;

    if let Some(other) = pending.remove(&meet_token) {
        drop(pending);
        info!("paired meet token: {}", token_id(&meet_token));

        let to_first = other.to_peer;
        let mut from_first = other.from_peer;
        let meter = Arc::new(SessionMeter::new());
        let started = std::time::Instant::now();
        let close_reason = Arc::new(Mutex::new(CloseReason::PeerClosed));

        // Forward between peers without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {
                *close_reason.lock().await = CloseReason::Shutdown;
            }
            _ = async {
                let meter = Arc::clone(&meter);
                let close_reason = Arc::clone(&close_reason);
                while let Some(msg) = ws_source.next().await {
                    match msg {
                        Ok(Message::Binary(data)) => {
                            let n = data.len();
                            if to_first.send(Message::Binary(data)).await.is_err() { break; }
                            if let Some(r) = meter.record_a_to_b(n, limits) {
                                *close_reason.lock().await = r;
                                break;
                            }
                            // Always yield after forwarding to prevent handshake message
                            // ordering issues between the two relay connections. Without this,
                            // multiple messages arriving in the same poll batch can be delivered
                            // out of order through the mpsc channel.
                            tokio::task::yield_now().await;
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            } => {}
            _ = async {
                let meter = Arc::clone(&meter);
                let close_reason = Arc::clone(&close_reason);
                while let Some(msg) = from_first.recv().await {
                    let n = msg.len();
                    if ws_sink.send(msg).await.is_err() { break; }
                    if let Some(r) = meter.record_b_to_a(n, limits) {
                        *close_reason.lock().await = r;
                        break;
                    }
                    if compat_mode {
                        tokio::task::yield_now().await;
                    }
                }
            } => {}
        }

        // Time-based limits are checked here rather than inline to keep the
        // hot path free of syscalls. A short-lived session rarely trips them.
        let mut reason = *close_reason.lock().await;
        if reason == CloseReason::PeerClosed {
            if let Some(r) = meter.check_time(started.elapsed().as_secs(), limits) {
                reason = r;
            }
        }
        info!("session closed (reason={})", reason.as_str());
    } else {
        let (inbound_tx, mut inbound_rx) = mpsc::channel::<Message>(limits.channel_depth.max(1));
        let (outbound_tx, outbound_rx) = mpsc::channel::<Message>(limits.channel_depth.max(1));

        pending.insert(
            meet_token.clone(),
            PendingPeer {
                to_peer: inbound_tx,
                from_peer: outbound_rx,
            },
        );
        drop(pending);

        // Wait for the counterpart peer, bounded by `pairing_timeout_secs`.
        // An unpaired peer must not hold the token slot indefinitely (F4).
        let pairing_timeout = std::time::Duration::from_secs(limits.pairing_timeout_secs);

        // Forward without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {}
            _ = tokio::time::sleep(pairing_timeout) => {
                warn!("pairing timeout for meet token {}", token_id(&meet_token));
            }
            _ = async {
                while let Some(msg) = ws_source.next().await {
                    match msg {
                        Ok(msg @ Message::Binary(_)) => {
                            if outbound_tx.send(msg).await.is_err() { break; }
                            // Always yield after forwarding — prevents handshake ordering issues
                            tokio::task::yield_now().await;
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            } => {}
            _ = async {
                while let Some(msg) = inbound_rx.recv().await {
                    if ws_sink.send(msg).await.is_err() { break; }
                    if compat_mode {
                        tokio::task::yield_now().await;
                    }
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
    fn test_token_id_is_short_hash() {
        // Token IDs are 8 hex chars and deterministic.
        assert_eq!(token_id("my-token").len(), 8);
        assert_eq!(token_id("my-token"), token_id("my-token"));
        assert_ne!(token_id("my-token"), token_id("other-token"));
    }

    #[test]
    fn test_token_id_never_reveals_full_token() {
        // The truncated hash must not be a substring of (or equal to) the token.
        let token = "agent-001-super-secret-rendezvous";
        let id = token_id(token);
        assert!(!token.contains(&id));
        assert!(id.len() < token.len());
    }

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

    #[test]
    fn test_rate_limiter_ipv6_same_64_shares_bucket() {
        // Two addresses in the same /64 must share a bucket (ROADMAP F6).
        let mut rl = RateLimiter::new(2, 60);
        let a = "2001:db8:1:1::1".parse::<IpAddr>().unwrap();
        let b = "2001:db8:1:1::2".parse::<IpAddr>().unwrap();

        assert!(rl.check_and_record(a));
        assert!(rl.check_and_record(b));
        // Third connection from the same /64 exceeds the limit.
        assert!(!rl.check_and_record(a));
    }

    #[test]
    fn test_rate_limiter_ipv6_different_64_independent() {
        // Addresses in different /64 blocks must be independent.
        let mut rl = RateLimiter::new(1, 60);
        let a = "2001:db8:1:1::1".parse::<IpAddr>().unwrap();
        let b = "2001:db8:1:2::1".parse::<IpAddr>().unwrap();

        assert!(rl.check_and_record(a));
        assert!(!rl.check_and_record(a)); // Over limit in a's /64
        assert!(rl.check_and_record(b)); // b's /64 is independent
    }

    #[test]
    fn test_session_meter_byte_quota() {
        let meter = SessionMeter::new();
        let limits = RelayLimits {
            max_session_bytes: 100,
            ..Default::default()
        };

        // 60 bytes a→b: under quota
        assert_eq!(meter.record_a_to_b(60, &limits), None);
        // 50 more bytes a→b: 110 total, over quota
        assert_eq!(
            meter.record_a_to_b(50, &limits),
            Some(CloseReason::QuotaBytes)
        );
    }

    #[test]
    fn test_session_meter_unlimited_bytes() {
        let meter = SessionMeter::new();
        let limits = RelayLimits::default(); // max_session_bytes = 0 (off)
        assert_eq!(meter.record_a_to_b(1 << 30, &limits), None);
        assert_eq!(meter.record_b_to_a(1 << 30, &limits), None);
    }

    #[test]
    fn test_session_meter_directions_independent() {
        let meter = SessionMeter::new();
        let limits = RelayLimits {
            max_session_bytes: 100,
            ..Default::default()
        };
        // 80 bytes a→b and 80 bytes b→a are each under the 100-byte cap.
        assert_eq!(meter.record_a_to_b(80, &limits), None);
        assert_eq!(meter.record_b_to_a(80, &limits), None);
        // But a→b exceeding it now trips.
        assert_eq!(
            meter.record_a_to_b(30, &limits),
            Some(CloseReason::QuotaBytes)
        );
    }

    #[test]
    fn test_close_reason_strings() {
        assert_eq!(CloseReason::PeerClosed.as_str(), "peer_closed");
        assert_eq!(CloseReason::QuotaBytes.as_str(), "quota_bytes");
        assert_eq!(CloseReason::QuotaTime.as_str(), "quota_time");
        assert_eq!(CloseReason::Idle.as_str(), "idle");
        assert_eq!(CloseReason::Shutdown.as_str(), "shutdown");
    }
}
