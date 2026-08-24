//! RavenFabric Relay library — stateless encrypted relay broker.
//!
//! Exposes `run_relay()` for embedding in other binaries (e.g., `rf dev`).

pub mod cross_region;
pub mod geoip;
pub mod tokens;

use tokens::TokenVerifier;

use cross_region::{ForwardConfig, bridge_to_remote_relay_inner, parse_forward_token_with_hops};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

/// Relay operational metrics, shared across connection tasks and the metrics
/// endpoint (ROADMAP R0.5/F8).
#[derive(Debug, Default)]
pub struct RelayMetrics {
    /// Total connections by outcome.
    pub connections_accepted: AtomicU64,
    pub connections_rate_limited: AtomicU64,
    pub connections_auth_failed: AtomicU64,
    pub connections_over_capacity: AtomicU64,
    /// Currently active paired sessions (gauge).
    pub sessions_active: AtomicI64,
    /// Currently pending (unpaired) peer slots (gauge).
    pub pending_pairings: AtomicI64,
    /// Total bytes forwarded in each direction.
    pub bytes_a_to_b: AtomicU64,
    pub bytes_b_to_a: AtomicU64,
    /// Sessions closed by reason (peer_closed / quota_bytes / quota_time / idle / shutdown).
    pub closed_peer_closed: AtomicU64,
    pub closed_quota_bytes: AtomicU64,
    pub closed_quota_time: AtomicU64,
    pub closed_idle: AtomicU64,
    pub closed_shutdown: AtomicU64,
    /// Channel send blocks (>100ms) for backpressure observability.
    pub channel_send_blocked: AtomicU64,
    /// Cross-region forward outcomes.
    pub forward_allowed: AtomicU64,
    pub forward_denied: AtomicU64,
    pub forward_hop_limit: AtomicU64,
}

impl RelayMetrics {
    /// Record a session close reason into the appropriate counter.
    fn record_close(&self, reason: CloseReason) {
        let counter = match reason {
            CloseReason::PeerClosed => &self.closed_peer_closed,
            CloseReason::QuotaBytes => &self.closed_quota_bytes,
            CloseReason::QuotaTime => &self.closed_quota_time,
            CloseReason::Idle => &self.closed_idle,
            CloseReason::Shutdown => &self.closed_shutdown,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Render the metrics in Prometheus text exposition format.
    fn render(&self) -> String {
        let mut out = String::with_capacity(1024);
        out.push_str("# HELP rf_relay_connections_total Total connections by outcome.\n");
        out.push_str("# TYPE rf_relay_connections_total counter\n");
        out.push_str(&format!(
            "rf_relay_connections_total{{result=\"accepted\"}} {}\n",
            self.connections_accepted.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_connections_total{{result=\"rate_limited\"}} {}\n",
            self.connections_rate_limited.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_connections_total{{result=\"auth_failed\"}} {}\n",
            self.connections_auth_failed.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_connections_total{{result=\"over_capacity\"}} {}\n",
            self.connections_over_capacity.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP rf_relay_sessions_active Currently active paired sessions.\n");
        out.push_str("# TYPE rf_relay_sessions_active gauge\n");
        out.push_str(&format!(
            "rf_relay_sessions_active {}\n",
            self.sessions_active.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP rf_relay_pending_pairings Currently unpaired peer slots.\n");
        out.push_str("# TYPE rf_relay_pending_pairings gauge\n");
        out.push_str(&format!(
            "rf_relay_pending_pairings {}\n",
            self.pending_pairings.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP rf_relay_session_bytes_total Total bytes forwarded per direction.\n");
        out.push_str("# TYPE rf_relay_session_bytes_total counter\n");
        out.push_str(&format!(
            "rf_relay_session_bytes_total{{direction=\"a_to_b\"}} {}\n",
            self.bytes_a_to_b.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_session_bytes_total{{direction=\"b_to_a\"}} {}\n",
            self.bytes_b_to_a.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP rf_relay_session_closed_total Sessions closed by reason.\n");
        out.push_str("# TYPE rf_relay_session_closed_total counter\n");
        out.push_str(&format!(
            "rf_relay_session_closed_total{{reason=\"peer_closed\"}} {}\n",
            self.closed_peer_closed.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_session_closed_total{{reason=\"quota_bytes\"}} {}\n",
            self.closed_quota_bytes.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_session_closed_total{{reason=\"quota_time\"}} {}\n",
            self.closed_quota_time.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_session_closed_total{{reason=\"idle\"}} {}\n",
            self.closed_idle.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_session_closed_total{{reason=\"shutdown\"}} {}\n",
            self.closed_shutdown.load(Ordering::Relaxed)
        ));

        out.push_str(
            "# HELP rf_relay_channel_send_blocked_total Channel sends that blocked >100ms.\n",
        );
        out.push_str("# TYPE rf_relay_channel_send_blocked_total counter\n");
        out.push_str(&format!(
            "rf_relay_channel_send_blocked_total {}\n",
            self.channel_send_blocked.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP rf_relay_forward_total Cross-region forward outcomes.\n");
        out.push_str("# TYPE rf_relay_forward_total counter\n");
        out.push_str(&format!(
            "rf_relay_forward_total{{result=\"allowed\"}} {}\n",
            self.forward_allowed.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_forward_total{{result=\"denied\"}} {}\n",
            self.forward_denied.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "rf_relay_forward_total{{result=\"hop_limit\"}} {}\n",
            self.forward_hop_limit.load(Ordering::Relaxed)
        ));
        out
    }
}

/// Serve `/metrics` and `/healthz` on a dedicated HTTP listener (no framework).
///
/// Uses raw tokio TCP, matching the pattern in `rf-executor`'s metrics server.
async fn run_metrics_server(
    bind_addr: &str,
    metrics: Arc<RelayMetrics>,
    cancel: tokio_util::sync::CancellationToken,
) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind(bind_addr).await?;
    info!("rf-relay metrics endpoint listening on {}", bind_addr);

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = listener.accept() => {
                let (mut stream, _addr) = match result {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let metrics = Arc::clone(&metrics);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 2048];
                    let n = match stream.read(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    let request = String::from_utf8_lossy(&buf[..n]);

                    if request.starts_with("GET /metrics") {
                        let body = metrics.render();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    } else if request.starts_with("GET /healthz") || request.starts_with("GET /health") {
                        let body = "{\"status\":\"ok\"}\n";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: application/json\r\n\
                             Content-Length: {}\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    } else {
                        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                });
            }
        }
    }

    Ok(())
}

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
    /// How long to wait for sessions to drain gracefully on shutdown before
    /// force-aborting the remaining connection tasks.
    pub drain_timeout_secs: u64,
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
            drain_timeout_secs: 30,
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
    run_relay_with_limits_and_metrics(
        listen_addr,
        cancel,
        meet_secret,
        forward_config,
        limits,
        None,
    )
    .await
}

/// Run the relay with full configuration, per-session limits, an optional
/// `/metrics` + `/healthz` endpoint, and an optional multi-key token verifier.
pub async fn run_relay_with_limits_and_metrics(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
    forward_config: ForwardConfig,
    limits: RelayLimits,
    metrics_addr: Option<String>,
) -> anyhow::Result<()> {
    run_relay_full_impl(
        listen_addr,
        cancel,
        meet_secret,
        forward_config,
        limits,
        metrics_addr,
        None,
    )
    .await
}

/// Run the relay with full configuration, per-session limits, metrics, and a
/// multi-key `TokenVerifier` (R0.6). When `verifier` is `Some`, it supersedes
/// the legacy single-secret `meet_secret` verification.
#[allow(clippy::too_many_arguments)]
pub async fn run_relay_with_verifier(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
    forward_config: ForwardConfig,
    limits: RelayLimits,
    metrics_addr: Option<String>,
    verifier: Option<Arc<TokenVerifier>>,
) -> anyhow::Result<()> {
    run_relay_full_impl(
        listen_addr,
        cancel,
        meet_secret,
        forward_config,
        limits,
        metrics_addr,
        verifier,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_relay_full_impl(
    listen_addr: &str,
    cancel: tokio_util::sync::CancellationToken,
    meet_secret: Option<String>,
    forward_config: ForwardConfig,
    limits: RelayLimits,
    metrics_addr: Option<String>,
    verifier: Option<Arc<TokenVerifier>>,
) -> anyhow::Result<()> {
    let channel_depth = limits.channel_depth.max(1);
    let max_connections = limits.max_connections.max(1);
    let state: MeetState = Arc::new(Mutex::new(HashMap::new()));
    // Rate limit: 20 connections per IP per 60 seconds
    let rate_limiter: RateLimiterState = Arc::new(Mutex::new(RateLimiter::new(20, 60)));
    let metrics = Arc::new(RelayMetrics::default());
    let listener = TcpListener::bind(listen_addr).await?;
    info!("rf-relay listening on {}", listen_addr);
    if meet_secret.is_some() {
        info!("HMAC meet token verification enabled");
    }
    if verifier.is_some() {
        info!("multi-key invitation token verification enabled");
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

    // Optional metrics / healthz endpoint.
    if let Some(addr) = metrics_addr {
        let metrics_clone = Arc::clone(&metrics);
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            if let Err(e) = run_metrics_server(&addr, metrics_clone, cancel_clone).await {
                warn!("metrics server error: {}", e);
            }
        });
    }

    let meet_secret = Arc::new(meet_secret);
    let forward_config = Arc::new(forward_config);
    let limits = Arc::new(limits);
    let mut connections = tokio::task::JoinSet::new();
    let mut cleanup_interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("relay shutting down (draining for {}s)", limits.drain_timeout_secs);
                // Gracefully drain: the cancellation token propagates into each
                // connection task, which sends a Close frame to its peer before
                // returning. Wait up to drain_timeout_secs, then force-abort.
                let drain = std::time::Duration::from_secs(limits.drain_timeout_secs);
                let mut drained = false;
                tokio::select! {
                    _ = tokio::time::sleep(drain) => {}
                    () = async {
                        while connections.join_next().await.is_some() {}
                        drained = true;
                    } => {}
                }
                if !drained {
                    connections.abort_all();
                }
                info!("relay shutdown complete");
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
                    metrics.connections_over_capacity.fetch_add(1, Ordering::Relaxed);
                    warn!("connection limit reached ({max_connections}), dropping connection from {}", ip);
                    drop(tcp_stream);
                    continue;
                }

                // Rate limit check
                {
                    let mut rl = rate_limiter.lock().await;
                    if !rl.check_and_record(ip) {
                        metrics.connections_rate_limited.fetch_add(1, Ordering::Relaxed);
                        warn!("rate limit exceeded for {}, dropping connection", ip);
                        continue;
                    }
                }

                metrics.connections_accepted.fetch_add(1, Ordering::Relaxed);

                let state = Arc::clone(&state);
                let cancel = cancel.clone();
                let meet_secret = Arc::clone(&meet_secret);
                let forward_config = Arc::clone(&forward_config);
                let limits = Arc::clone(&limits);
                let metrics = Arc::clone(&metrics);
                let verifier = verifier.clone();
                connections.spawn(async move {
                    let ws_stream = match tokio_tungstenite::accept_async(tcp_stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            warn!("WS accept failed from {}: {}", addr, e);
                            return;
                        }
                    };
                    if let Err(e) = handle_connection(ws_stream, state, cancel, &meet_secret, &forward_config, &limits, &metrics, verifier.as_deref()).await {
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
    metrics: &Arc<RelayMetrics>,
    verifier: Option<&TokenVerifier>,
) -> anyhow::Result<()> {
    handle_connection_inner(
        ws,
        state,
        cancel,
        meet_secret,
        forward_config,
        forward_config.compat_mode,
        limits,
        metrics,
        verifier,
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
    metrics: &Arc<RelayMetrics>,
    verifier: Option<&TokenVerifier>,
) -> anyhow::Result<()> {
    let (mut ws_sink, mut ws_source) = ws.split();

    // First message must be the meet token
    let meet_token = match ws_source.next().await {
        Some(Ok(Message::Text(token))) => token.to_string(),
        Some(Ok(Message::Binary(data))) => String::from_utf8_lossy(&data).to_string(),
        _ => return Err(anyhow::anyhow!("expected meet token as first message")),
    };

    // Token verification: prefer the multi-key verifier, fall back to legacy HMAC.
    let token_ok = match verifier {
        Some(v) => v.verify(&meet_token) == tokens::VerifyOutcome::Valid,
        None => verify_meet_token(&meet_token, meet_secret.as_deref()),
    };
    if !token_ok {
        metrics
            .connections_auth_failed
            .fetch_add(1, Ordering::Relaxed);
        warn!("invalid meet token (authentication failed)");
        return Err(anyhow::anyhow!("meet token authentication failed"));
    }

    // ── Cross-region forwarding check ────────────────────────────────────────
    if let Some((target_url, hops, inner_token)) = parse_forward_token_with_hops(&meet_token) {
        if !forward_config.is_target_allowed(target_url) {
            metrics.forward_denied.fetch_add(1, Ordering::Relaxed);
            warn!("cross-region forward to {} denied by policy", target_url);
            return Err(anyhow::anyhow!(
                "cross-region forwarding denied: target not in allowlist"
            ));
        }
        // Hop limit — prevents A→B→A amplification loops (ROADMAP F11).
        if forward_config.max_forward_hops > 0 && hops >= forward_config.max_forward_hops {
            metrics.forward_hop_limit.fetch_add(1, Ordering::Relaxed);
            warn!(
                "cross-region forward to {} denied: hop limit reached ({}/{})",
                target_url, hops, forward_config.max_forward_hops
            );
            return Err(anyhow::anyhow!(
                "cross-region forwarding denied: hop limit reached"
            ));
        }
        metrics.forward_allowed.fetch_add(1, Ordering::Relaxed);
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
        metrics.sessions_active.fetch_add(1, Ordering::Relaxed);
        metrics.pending_pairings.fetch_sub(1, Ordering::Relaxed);

        let to_first = other.to_peer;
        let mut from_first = other.from_peer;
        let meter = Arc::new(SessionMeter::new());
        let started = std::time::Instant::now();
        let close_reason = Arc::new(Mutex::new(CloseReason::PeerClosed));

        // Forward between peers without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {
                *close_reason.lock().await = CloseReason::Shutdown;
                // Send a Close frame so the peer sees a clean drain (F14).
                let _ = ws_sink.send(Message::Close(None)).await;
            }
            _ = async {
                let meter = Arc::clone(&meter);
                let close_reason = Arc::clone(&close_reason);
                let metrics = Arc::clone(metrics);
                while let Some(msg) = ws_source.next().await {
                    match msg {
                        Ok(Message::Binary(data)) => {
                            let n = data.len();
                            if to_first.send(Message::Binary(data)).await.is_err() { break; }
                            metrics.bytes_a_to_b.fetch_add(n as u64, Ordering::Relaxed);
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
                let metrics = Arc::clone(metrics);
                while let Some(msg) = from_first.recv().await {
                    let n = msg.len();
                    if ws_sink.send(msg).await.is_err() { break; }
                    metrics.bytes_b_to_a.fetch_add(n as u64, Ordering::Relaxed);
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
        metrics.sessions_active.fetch_sub(1, Ordering::Relaxed);
        metrics.record_close(reason);
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
        metrics.pending_pairings.fetch_add(1, Ordering::Relaxed);

        // Wait for the counterpart peer, bounded by `pairing_timeout_secs`.
        // An unpaired peer must not hold the token slot indefinitely (F4).
        let pairing_timeout = std::time::Duration::from_secs(limits.pairing_timeout_secs);

        // Forward without spawning (cancellation-safe)
        tokio::select! {
            () = cancel.cancelled() => {
                // Send a Close frame so the unpaired peer sees a clean drain (F14).
                let _ = ws_sink.send(Message::Close(None)).await;
            }
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
        metrics.pending_pairings.fetch_sub(1, Ordering::Relaxed);
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

    #[test]
    fn test_relay_metrics_render_prometheus() {
        let m = RelayMetrics::default();
        m.connections_accepted.fetch_add(3, Ordering::Relaxed);
        m.sessions_active.fetch_add(2, Ordering::Relaxed);
        m.bytes_a_to_b.fetch_add(1024, Ordering::Relaxed);

        let text = m.render();
        assert!(text.contains("rf_relay_connections_total{result=\"accepted\"} 3"));
        assert!(text.contains("rf_relay_sessions_active 2"));
        assert!(text.contains("rf_relay_session_bytes_total{direction=\"a_to_b\"} 1024"));
        // All metric families present.
        for family in [
            "rf_relay_connections_total",
            "rf_relay_sessions_active",
            "rf_relay_pending_pairings",
            "rf_relay_session_bytes_total",
            "rf_relay_session_closed_total",
            "rf_relay_channel_send_blocked_total",
            "rf_relay_forward_total",
        ] {
            assert!(text.contains(family), "missing family {family}");
        }
    }

    #[test]
    fn test_relay_metrics_record_close() {
        let m = RelayMetrics::default();
        m.record_close(CloseReason::QuotaBytes);
        m.record_close(CloseReason::Idle);
        assert_eq!(m.closed_quota_bytes.load(Ordering::Relaxed), 1);
        assert_eq!(m.closed_idle.load(Ordering::Relaxed), 1);
        assert_eq!(m.closed_peer_closed.load(Ordering::Relaxed), 0);
    }
}
