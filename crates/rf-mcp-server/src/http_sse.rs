//! HTTP+SSE transport for multi-user MCP server deployment.
//!
//! Implements the MCP Streamable HTTP transport:
//! - `POST /message` — clients send JSON-RPC requests
//! - `GET /sse` — Server-Sent Events stream for responses
//!
//! Each connection gets its own session with independent policy,
//! rate limiting, and audit logging.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::sync::{Mutex, broadcast};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info};

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::server::{CallerProfile, McpServer};

/// Configuration for the HTTP+SSE server.
#[derive(Clone)]
pub struct HttpSseConfig {
    pub listen_addr: String,
    pub policy_path: Option<PathBuf>,
    pub audit_path: Option<PathBuf>,
    pub caller_key: String,
    pub api_token: Option<String>,
    pub max_requests_per_minute: Option<u32>,
    pub alert_webhook: Option<String>,
    pub caller_profiles: Vec<CallerProfile>,
    pub approval_patterns: Vec<String>,
    pub require_approval: bool,
}

/// Shared state for the HTTP+SSE server.
struct AppState {
    config: HttpSseConfig,
    /// Active sessions indexed by session ID.
    sessions: Mutex<HashMap<String, SessionState>>,
}

/// Per-session state in the HTTP+SSE server.
struct SessionState {
    server: McpServer,
    /// Broadcast channel for SSE events directed to this session.
    tx: broadcast::Sender<String>,
}

/// Run the MCP server in HTTP+SSE mode.
pub async fn run_http_sse(config: HttpSseConfig) -> anyhow::Result<()> {
    let listen_addr = config.listen_addr.clone();

    let state = Arc::new(AppState {
        config,
        sessions: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/message", post(handle_message))
        .route("/sse", get(handle_sse))
        .route("/health", get(handle_health))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    info!(addr = %listen_addr, "HTTP+SSE MCP server listening");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Extract session ID from request headers.
fn extract_session_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// Extract API token from Authorization header.
fn extract_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(String::from)
}

/// Handle POST /message — process a JSON-RPC request.
async fn handle_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let session_id = extract_session_id(&headers);

    // For `initialize`, create a new session
    if request.method == "initialize" {
        let token = extract_token(&headers);
        // Inject token into params for the server's auth logic
        let mut params = request.params.clone();
        if let Some(ref t) = token {
            if let Some(obj) = params.as_object_mut() {
                obj.insert("apiToken".to_string(), serde_json::Value::String(t.clone()));
            }
        }

        let server = match McpServer::new(
            state.config.policy_path.as_deref(),
            state.config.audit_path.as_deref(),
            &state.config.caller_key,
            state.config.api_token.clone(),
            state.config.max_requests_per_minute,
            state.config.alert_webhook.clone(),
            state.config.caller_profiles.clone(),
            &state.config.approval_patterns,
            state.config.require_approval,
        ) {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "failed to create session");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(JsonRpcResponse::error(
                        request.id,
                        -32603,
                        format!("Session creation failed: {e}"),
                    )),
                );
            }
        };

        let modified_request = JsonRpcRequest {
            jsonrpc: request.jsonrpc,
            id: request.id,
            method: request.method,
            params,
        };

        let response = server.handle_request(&modified_request).await;
        let new_session_id = response
            .result
            .as_ref()
            .and_then(|r| r.get("sessionId"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();

        let (tx, _rx) = broadcast::channel(64);

        state
            .sessions
            .lock()
            .await
            .insert(new_session_id.clone(), SessionState { server, tx });

        info!(session_id = %new_session_id, "new HTTP+SSE session created");
        return (StatusCode::OK, Json(response));
    }

    // For all other requests, look up the existing session
    let Some(session_id) = session_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                request.id,
                -32600,
                "Missing X-Session-Id header. Call initialize first.",
            )),
        );
    };

    let sessions = state.sessions.lock().await;
    let Some(session) = sessions.get(&session_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(JsonRpcResponse::error(
                request.id,
                -32600,
                "Session not found. Call initialize first.",
            )),
        );
    };

    let response = session.server.handle_request(&request).await;

    // Also broadcast via SSE for clients that are listening
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = session.tx.send(json);
    }

    (StatusCode::OK, Json(response))
}

/// Handle GET /sse — Server-Sent Events stream for a session.
async fn handle_sse(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    let Some(session_id) = extract_session_id(&headers) else {
        return Err((StatusCode::BAD_REQUEST, "Missing X-Session-Id header"));
    };

    let sessions = state.sessions.lock().await;
    let Some(session) = sessions.get(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "Session not found"));
    };

    let rx = session.tx.subscribe();
    drop(sessions);

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(msg) => Some(Ok::<_, std::convert::Infallible>(
            Event::default().data(msg),
        )),
        Err(_) => None,
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    ))
}

/// Handle GET /health — simple health check.
async fn handle_health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-token-123".parse().unwrap());
        assert_eq!(extract_token(&headers), Some("test-token-123".to_string()));
    }

    #[test]
    fn test_extract_token_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_token(&headers), None);
    }

    #[test]
    fn test_extract_session_id() {
        let mut headers = HeaderMap::new();
        headers.insert("x-session-id", "abc-123".parse().unwrap());
        assert_eq!(extract_session_id(&headers), Some("abc-123".to_string()));
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let resp = handle_health().await;
        let (status, _body) = resp.into_response().into_parts();
        assert_eq!(status.status, StatusCode::OK);
    }
}
