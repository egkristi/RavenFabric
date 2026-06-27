//! Axum HTTP server for the ingress gateway.
//!
//! Accepts HTTP requests, authenticates them via `X-RF-Key`, applies rate
//! limiting, resolves the target agent from the routing table, and forwards
//! the request as a `ReverseProxy` RPC action over the agent's Noise XX
//! channel.

use std::{net::SocketAddr, sync::Arc, time::Instant};

use anyhow::Result;
use axum::{
    Router,
    body::Bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use chrono::Utc;
use rf_audit::{
    logger::{AuditLogger, FileAuditLogger, NullAuditLogger},
    types::AuditEntry,
};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{auth::ApiKeyStore, rate_limit::RateLimiter, router::RoutingTable};

/// Shared ingress state passed to every Axum handler.
#[derive(Clone)]
pub struct IngressState {
    pub routing_table: RoutingTable,
    pub api_keys: Arc<ApiKeyStore>,
    pub rate_limiter: Arc<RateLimiter>,
    /// HTTP client for forwarding requests to local upstream services.
    pub http_client: reqwest::Client,
    /// Audit logger — records every request decision (deny and allow).
    pub audit: Arc<dyn AuditLogger>,
}

/// Configuration for the ingress server.
#[derive(Debug, Clone)]
pub struct IngressConfig {
    /// Address to bind the HTTP listener.
    pub listen: SocketAddr,
    /// API keys allowed to send requests through the ingress.
    /// Empty = open / dev mode (no authentication).
    pub api_keys: Vec<String>,
    /// Requests per minute per IP before rate-limiting kicks in.
    pub rate_limit_rpm: u32,
    /// Upstream request timeout in milliseconds.
    pub upstream_timeout_ms: u64,
    /// Maximum upstream response body size.
    pub max_response_bytes: u64,
    /// Optional path for the structured JSON-lines audit log.
    /// If `None`, audit entries are discarded (no-op logger).
    pub audit_path: Option<String>,
    /// Optional path to the HMAC key file (32-byte raw or 64-char hex) for audit chain integrity.
    /// Required if `audit_path` is set.
    pub audit_key_path: Option<String>,
}

impl Default for IngressConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8088".parse().expect("static parse"),
            api_keys: Vec::new(),
            rate_limit_rpm: 300,
            upstream_timeout_ms: 30_000,
            max_response_bytes: 10 * 1024 * 1024, // 10 MiB
            audit_path: None,
            audit_key_path: None,
        }
    }
}

/// Run the ingress HTTP server until the process is interrupted.
pub async fn run_ingress(config: IngressConfig, routing_table: RoutingTable) -> Result<()> {
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(config.upstream_timeout_ms))
        .build()?;

    let audit: Arc<dyn AuditLogger> = if let Some(ref path) = config.audit_path {
        let hmac_key = match config.audit_key_path {
            Some(ref key_path) => {
                let key_bytes = std::fs::read(key_path)
                    .map_err(|e| anyhow::anyhow!("failed to read audit key '{key_path}': {e}"))?;
                if key_bytes.len() == 64 {
                    hex::decode(&key_bytes).unwrap_or(key_bytes)
                } else {
                    key_bytes
                }
            }
            None => anyhow::bail!("audit_key_path is required when audit_path is set"),
        };
        if hmac_key.len() != 32 {
            anyhow::bail!("audit HMAC key must be 32 bytes (got {})", hmac_key.len());
        }
        Arc::new(FileAuditLogger::new(path.into(), hmac_key)?)
    } else {
        Arc::new(NullAuditLogger)
    };

    let state = IngressState {
        routing_table,
        api_keys: Arc::new(ApiKeyStore::new(config.api_keys.iter())),
        rate_limiter: Arc::new(RateLimiter::new(60, config.rate_limit_rpm)),
        http_client,
        audit,
    };

    let app = Router::new()
        .route("/health", any(health_handler))
        .fallback(any(proxy_handler))
        .with_state(state);

    info!("rf-ingress listening on {}", config.listen);

    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

/// Health check endpoint — always returns 200, no authentication required.
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Returns the SHA-256 hex digest of `data`.
fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// Catch-all reverse proxy handler.
async fn proxy_handler(
    State(state): State<IngressState>,
    axum::extract::ConnectInfo(remote_addr): axum::extract::ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    let start = Instant::now();
    let remote_ip = remote_addr.ip();
    let request_id = Uuid::new_v4().to_string();

    // Extract caller identity before consuming `req` — used in every audit entry.
    let raw_key = req
        .headers()
        .get("x-rf-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let caller_key = if raw_key.is_empty() {
        "anonymous".to_string()
    } else {
        format!("sha256:{}", sha256_hex(raw_key.as_bytes()))
    };

    // --- Rate limiting ---
    if !state.rate_limiter.check_and_record(remote_ip) {
        warn!("rate limit exceeded for {remote_ip}");
        if let Err(e) = state.audit.log(AuditEntry {
            timestamp: Utc::now(),
            request_id: request_id.clone(),
            action: "proxy".to_string(),
            command: None,
            decision: "deny".to_string(),
            matched_rule: "rate-limit-exceeded".to_string(),
            exit_code: None,
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: caller_key.clone(),
            reason: Some(format!("source_ip={remote_ip}")),
            prev_hash: None,
            hmac: None,
        }) {
            warn!("audit write failed: {e}");
        }
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
    }

    // --- API key authentication ---
    if !state.api_keys.is_open() && !state.api_keys.is_valid(&raw_key) {
        warn!("unauthenticated request from {remote_ip}");
        if let Err(e) = state.audit.log(AuditEntry {
            timestamp: Utc::now(),
            request_id: request_id.clone(),
            action: "proxy".to_string(),
            command: None,
            decision: "deny".to_string(),
            matched_rule: "auth-failed".to_string(),
            exit_code: None,
            duration_ms: start.elapsed().as_millis() as u64,
            caller_key: caller_key.clone(),
            reason: Some(format!("source_ip={remote_ip}")),
            prev_hash: None,
            hmac: None,
        }) {
            warn!("audit write failed: {e}");
        }
        return (StatusCode::UNAUTHORIZED, "invalid or missing X-RF-Key").into_response();
    }

    // --- Routing ---
    let host = req
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let path = req.uri().path().to_string();
    let method = req.method().to_string();
    let query = req.uri().query().map(|q| q.to_string());

    let entry = match state
        .routing_table
        .resolve_with_affinity(&host, &path, Some(&caller_key))
        .await
    {
        Some(e) => e,
        None => {
            warn!("no agent registered for host={host} path={path}");
            if let Err(e) = state.audit.log(AuditEntry {
                timestamp: Utc::now(),
                request_id: request_id.clone(),
                action: "proxy".to_string(),
                command: Some(format!("{method} {path}")),
                decision: "deny".to_string(),
                matched_rule: "no-route".to_string(),
                exit_code: None,
                duration_ms: start.elapsed().as_millis() as u64,
                caller_key: caller_key.clone(),
                reason: Some(format!("host={host}")),
                prev_hash: None,
                hmac: None,
            }) {
                warn!("audit write failed: {e}");
            }
            return (StatusCode::BAD_GATEWAY, "no upstream agent found").into_response();
        }
    };

    // Collect headers (excluding hop-by-hop)
    let fwd_headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .filter(|(k, _)| !is_hop_by_hop(k.as_str()))
        .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
        .collect();

    // Read body
    let body_bytes: Option<Bytes> = {
        let (_parts, body) = req.into_parts();
        match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
            Ok(b) if b.is_empty() => None,
            Ok(b) => Some(b),
            Err(e) => {
                error!("failed to read request body: {e}");
                return (StatusCode::BAD_REQUEST, "could not read request body").into_response();
            }
        }
    };

    // Build upstream URL
    let upstream_path = if let Some(q) = &query {
        format!("{}{}?{}", entry.upstream_url, path, q)
    } else {
        format!("{}{}", entry.upstream_url, path)
    };

    let req_builder = {
        let method_val = match reqwest::Method::from_bytes(method.as_bytes()) {
            Ok(m) => m,
            Err(_) => {
                return (StatusCode::METHOD_NOT_ALLOWED, "unsupported method").into_response();
            }
        };
        let mut b = state.http_client.request(method_val, &upstream_path);
        for (k, v) in &fwd_headers {
            b = b.header(k.as_str(), v.as_str());
        }
        if let Some(body) = body_bytes {
            b = b.body(body);
        }
        b
    };

    let resp = match req_builder.send().await {
        Ok(r) => r,
        Err(e) => {
            error!("upstream request to {} failed: {e}", entry.upstream_url);
            return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response();
        }
    };

    let status =
        StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // Forward response headers (excluding hop-by-hop)
    let mut resp_headers = HeaderMap::new();
    for (k, v) in resp.headers() {
        if !is_hop_by_hop(k.as_str()) {
            if let Ok(name) = axum::http::HeaderName::from_bytes(k.as_ref()) {
                resp_headers.insert(name, v.clone());
            }
        }
    }

    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!("reading upstream response body: {e}");
            return (StatusCode::BAD_GATEWAY, "upstream body read error").into_response();
        }
    };

    let latency_ms = start.elapsed().as_millis() as u64;
    info!(
        "proxy {} {} -> {} {} ({} bytes, {}ms) agent={}",
        method,
        path,
        status.as_u16(),
        status.canonical_reason().unwrap_or(""),
        body.len(),
        latency_ms,
        entry.agent_id,
    );

    if let Err(e) = state.audit.log(AuditEntry {
        timestamp: Utc::now(),
        request_id,
        action: "proxy".to_string(),
        command: Some(format!("{method} {path}")),
        decision: "allow".to_string(),
        matched_rule: entry.agent_id.clone(),
        exit_code: Some(status.as_u16() as i32),
        duration_ms: latency_ms,
        caller_key,
        reason: None,
        prev_hash: None,
        hmac: None,
    }) {
        warn!("audit write failed: {e}");
    }

    (status, resp_headers, body).into_response()
}

/// Returns `true` for HTTP/1.1 hop-by-hop headers that must not be forwarded.
fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_known_value() {
        // SHA-256 of empty bytes is a well-known constant
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_hello() {
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn hop_by_hop_detected() {
        assert!(is_hop_by_hop("Transfer-Encoding"));
        assert!(is_hop_by_hop("connection"));
        assert!(!is_hop_by_hop("content-type"));
        assert!(!is_hop_by_hop("x-custom-header"));
    }
}
