//! Minimal HTTP/1.1 server for the embedded Web UI and controller API.
//!
//! Uses `tokio::net::TcpListener` directly — no external HTTP framework dependencies.
//! Serves the dashboard at `/` and routes `/api/*` requests through [`ApiDispatcher`].
//!
//! # Security
//!
//! - Bind address is configurable (default: `127.0.0.1:9091`, localhost only)
//! - Optional bearer token authentication for API endpoints
//! - Response headers include security defaults (no-sniff, deny framing)
//! - Request size limited to 1 MB to prevent memory exhaustion

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::controller::{ApiDispatcher, ApiRequest, HttpMethod};
use crate::webui;

/// Maximum request size (1 MB).
const MAX_REQUEST_SIZE: usize = 1_048_576;

/// Configuration for the HTTP server.
#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    /// Address to bind to (default: `127.0.0.1:9091`).
    pub bind_addr: String,
    /// Optional bearer token for API authentication.
    pub auth_token: Option<String>,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9091".to_string(),
            auth_token: None,
        }
    }
}

/// Run the HTTP server, serving the Web UI dashboard and controller API.
///
/// This function runs until the provided shutdown signal completes.
///
/// # Arguments
///
/// * `config` — Server configuration (bind address, auth token)
/// * `dispatcher` — Shared API dispatcher for routing requests
/// * `shutdown` — Future that completes when the server should stop
pub async fn serve(
    config: HttpServerConfig,
    dispatcher: Arc<RwLock<ApiDispatcher>>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(&config.bind_addr).await?;
    info!(addr = %config.bind_addr, "Web UI server listening");

    let config = Arc::new(config);

    loop {
        tokio::select! {
            accept = listener.accept() => {
                match accept {
                    Ok((stream, addr)) => {
                        let dispatcher = dispatcher.clone();
                        let config = config.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, addr, &dispatcher, &config).await {
                                warn!(peer = %addr, error = %e, "HTTP connection error");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to accept connection");
                    }
                }
            }
            _ = shutdown_signal(&shutdown) => {
                info!("Web UI server shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn shutdown_signal(rx: &tokio::sync::watch::Receiver<bool>) {
    let mut rx = rx.clone();
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Handle a single HTTP connection.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    dispatcher: &Arc<RwLock<ApiDispatcher>>,
    config: &HttpServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Read request line.
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let request_line = request_line.trim().to_string();

    if request_line.is_empty() {
        return Ok(());
    }

    // Parse method and path.
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        send_response(&mut writer, 400, "text/plain", b"Bad Request").await?;
        return Ok(());
    }

    let method = parts[0];
    let path = parts[1];

    // Read headers.
    let mut headers = Vec::new();
    let mut content_length: usize = 0;
    let mut auth_header: Option<String> = None;
    let mut total_header_bytes = request_line.len();

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        total_header_bytes += line.len();
        if total_header_bytes > MAX_REQUEST_SIZE {
            send_response(&mut writer, 413, "text/plain", b"Request Too Large").await?;
            return Ok(());
        }
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key_lower = key.trim().to_lowercase();
            let value_trimmed = value.trim().to_string();
            if key_lower == "content-length" {
                content_length = value_trimmed.parse().unwrap_or(0);
            } else if key_lower == "authorization" {
                auth_header = Some(value_trimmed.clone());
            }
            headers.push((key_lower, value_trimmed));
        }
    }

    // Read body if present.
    let body = if content_length > 0 {
        if content_length > MAX_REQUEST_SIZE {
            send_response(&mut writer, 413, "text/plain", b"Request Too Large").await?;
            return Ok(());
        }
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await?;
        Some(buf)
    } else {
        None
    };

    tracing::debug!(peer = %addr, method = method, path = path, "HTTP request");

    // Route the request.
    match path {
        "/" | "/index.html" => {
            let (status, content_type, html) = webui::dashboard_response();
            send_response(&mut writer, status, content_type, html.as_bytes()).await?;
        }
        p if p.starts_with("/api/") => {
            // Check auth if configured.
            if let Some(ref expected_token) = config.auth_token {
                let provided = auth_header
                    .as_deref()
                    .and_then(|h| h.strip_prefix("Bearer "));
                match provided {
                    Some(token) if token == expected_token.as_str() => {}
                    _ => {
                        send_response(
                            &mut writer,
                            401,
                            "application/json",
                            b"{\"error\":\"unauthorized\"}",
                        )
                        .await?;
                        return Ok(());
                    }
                }
            }

            let http_method = match method {
                "GET" => HttpMethod::Get,
                "POST" => HttpMethod::Post,
                "PUT" => HttpMethod::Put,
                "DELETE" => HttpMethod::Delete,
                _ => {
                    send_response(
                        &mut writer,
                        405,
                        "application/json",
                        b"{\"error\":\"method not allowed\"}",
                    )
                    .await?;
                    return Ok(());
                }
            };

            // Map convenience paths to controller API paths.
            let api_path = if p == "/api/agents" {
                "/api/v1/agents"
            } else if p == "/api/health" || p == "/healthz" {
                "/healthz"
            } else {
                p
            };

            let api_request = ApiRequest {
                method: http_method,
                path: api_path.to_string(),
                body: body.as_ref().and_then(|b| serde_json::from_slice(b).ok()),
                // If the HTTP server already validated auth (or no auth is configured),
                // pass a synthetic token so the dispatcher's role check succeeds.
                auth_token: Some(
                    auth_header
                        .as_deref()
                        .map(|h| h.strip_prefix("Bearer ").unwrap_or(h).to_string())
                        .unwrap_or_else(|| "http-server-authenticated".to_string()),
                ),
                trace_context: None,
            };

            let dispatcher = dispatcher.read().await;
            let response = dispatcher.dispatch(&api_request);
            let json = serde_json::to_vec(&response.body).unwrap_or_default();
            send_response_with_status(&mut writer, response.status_code, "application/json", &json)
                .await?;
        }
        _ => {
            send_response(&mut writer, 404, "text/plain", b"Not Found").await?;
        }
    }

    Ok(())
}

/// Send an HTTP response with status 200.
async fn send_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    send_response_with_status(writer, status, content_type, body).await
}

/// Send an HTTP response with the given status code.
async fn send_response_with_status(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        body.len(),
    );

    writer.write_all(response.as_bytes()).await?;
    writer.write_all(body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::AgentRegistry;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Read full response until connection closes.
    async fn read_response(stream: &mut tokio::net::TcpStream) -> String {
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).to_string()
    }

    async fn setup_server() -> (String, tokio::sync::watch::Sender<bool>) {
        let registry = AgentRegistry::new(100, 30_000);
        let dispatcher = Arc::new(RwLock::new(ApiDispatcher::new(registry)));
        let (tx, rx) = tokio::sync::watch::channel(false);

        let config = HttpServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            auth_token: None,
        };

        // Bind first to get port.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        // Spawn server on the pre-bound listener.
        tokio::spawn(async move {
            let config_arc = Arc::new(config);
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer_addr)) => {
                                let d = dispatcher.clone();
                                let c = config_arc.clone();
                                tokio::spawn(async move {
                                    let _ = handle_connection(stream, peer_addr, &d, &c).await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                    _ = shutdown_signal(&rx) => break,
                }
            }
        });

        // Brief yield to let the server start.
        tokio::task::yield_now().await;

        (addr, tx)
    }

    #[tokio::test]
    async fn test_serves_dashboard() {
        let (addr, tx) = setup_server().await;

        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let response = read_response(&mut stream).await;

        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.contains("text/html"));
        assert!(response.contains("RavenFabric Dashboard"));

        tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_api_health() {
        let (addr, tx) = setup_server().await;

        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let response = read_response(&mut stream).await;

        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.contains("application/json"));
        assert!(response.contains("healthy"));

        tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_api_agents_list() {
        let (addr, tx) = setup_server().await;

        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /api/agents HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let response = read_response(&mut stream).await;

        assert!(response.contains("HTTP/1.1 200 OK"));
        assert!(response.contains("agents"));
        assert!(response.contains("total"));

        tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_404_unknown_path() {
        let (addr, tx) = setup_server().await;

        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /nonexistent HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let response = read_response(&mut stream).await;

        assert!(response.contains("HTTP/1.1 404"));

        tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_auth_required() {
        let registry = AgentRegistry::new(100, 30_000);
        let dispatcher = Arc::new(RwLock::new(ApiDispatcher::new(registry)));
        let (tx, rx) = tokio::sync::watch::channel(false);

        let config = HttpServerConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            auth_token: Some("secret-token".to_string()),
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let config_arc = Arc::new(config);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept = listener.accept() => {
                        match accept {
                            Ok((stream, peer_addr)) => {
                                let d = dispatcher.clone();
                                let c = config_arc.clone();
                                tokio::spawn(async move {
                                    let _ = handle_connection(stream, peer_addr, &d, &c).await;
                                });
                            }
                            Err(_) => break,
                        }
                    }
                    _ = shutdown_signal(&rx) => break,
                }
            }
        });
        tokio::task::yield_now().await;

        // Without token — should get 401.
        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let response = read_response(&mut stream).await;
        assert!(response.contains("401"));

        // With valid token — should get 200.
        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(
                b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer secret-token\r\n\r\n",
            )
            .await
            .unwrap();
        let response = read_response(&mut stream).await;
        assert!(response.contains("200 OK"));
        assert!(response.contains("healthy"));

        tx.send(true).unwrap();
    }

    #[tokio::test]
    async fn test_security_headers() {
        let (addr, tx) = setup_server().await;

        let mut stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let response = read_response(&mut stream).await;

        assert!(response.contains("X-Content-Type-Options: nosniff"));
        assert!(response.contains("X-Frame-Options: DENY"));
        assert!(response.contains("Cache-Control: no-store"));

        tx.send(true).unwrap();
    }
}
