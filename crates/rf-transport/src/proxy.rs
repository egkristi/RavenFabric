//! Corporate proxy detection and environment resolution.
//!
//! Detects HTTP/HTTPS/SOCKS proxy settings from environment variables
//! and system configuration, producing connection parameters for
//! upstream transport drivers. Includes active probing to verify
//! proxy connectivity.

use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Proxy protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyProtocol {
    Http,
    Https,
    Socks5,
}

/// A detected proxy endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// Protocol to use when connecting to the proxy.
    pub protocol: ProxyProtocol,
    /// Proxy host (IP or hostname).
    pub host: String,
    /// Proxy port.
    pub port: u16,
    /// Optional username for proxy authentication.
    pub username: Option<String>,
    /// Optional password for proxy authentication.
    pub password: Option<String>,
    /// Hosts that bypass the proxy (no_proxy).
    pub no_proxy: Vec<String>,
}

/// Source of proxy configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxySource {
    /// Detected from environment variables.
    Environment(String),
    /// Manually configured.
    Manual,
}

/// Result of proxy detection.
#[derive(Debug, Clone)]
pub struct ProxyDetection {
    /// Detected proxy, if any.
    pub proxy: Option<ProxyConfig>,
    /// Where the proxy was detected from.
    pub source: Option<ProxySource>,
    /// All environment variables checked.
    pub checked_vars: Vec<String>,
}

/// Parse a proxy URL into components.
fn parse_proxy_url(url: &str) -> Option<ProxyConfig> {
    // Format: [protocol://][user:pass@]host:port
    let (protocol, rest) = if let Some(stripped) = url.strip_prefix("socks5://") {
        (ProxyProtocol::Socks5, stripped)
    } else if let Some(stripped) = url.strip_prefix("https://") {
        (ProxyProtocol::Https, stripped)
    } else if let Some(stripped) = url.strip_prefix("http://") {
        (ProxyProtocol::Http, stripped)
    } else {
        (ProxyProtocol::Http, url)
    };

    let (auth, hostport) = if let Some(at_pos) = rest.rfind('@') {
        let auth_part = &rest[..at_pos];
        let host_part = &rest[at_pos + 1..];
        (Some(auth_part), host_part)
    } else {
        (None, rest)
    };

    let (username, password) = if let Some(auth_str) = auth {
        if let Some(colon_pos) = auth_str.find(':') {
            (
                Some(auth_str[..colon_pos].to_string()),
                Some(auth_str[colon_pos + 1..].to_string()),
            )
        } else {
            (Some(auth_str.to_string()), None)
        }
    } else {
        (None, None)
    };

    // Strip trailing slash
    let hostport = hostport.trim_end_matches('/');

    let (host, port) = if let Some(colon_pos) = hostport.rfind(':') {
        let port_str = &hostport[colon_pos + 1..];
        if let Ok(p) = port_str.parse::<u16>() {
            (hostport[..colon_pos].to_string(), p)
        } else {
            return None;
        }
    } else {
        // Default ports by protocol
        let default_port = match protocol {
            ProxyProtocol::Http => 8080,
            ProxyProtocol::Https => 8443,
            ProxyProtocol::Socks5 => 1080,
        };
        (hostport.to_string(), default_port)
    };

    if host.is_empty() {
        return None;
    }

    Some(ProxyConfig {
        protocol,
        host,
        port,
        username,
        password,
        no_proxy: Vec::new(),
    })
}

/// Parse no_proxy environment variable into a list of patterns.
fn parse_no_proxy(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Check if a target host should bypass the proxy.
pub fn should_bypass(host: &str, no_proxy: &[String]) -> bool {
    let host_lower = host.to_lowercase();
    for pattern in no_proxy {
        if pattern == "*" {
            return true;
        }
        if host_lower == *pattern {
            return true;
        }
        // Suffix match: .example.com matches sub.example.com
        if pattern.starts_with('.') && host_lower.ends_with(pattern.as_str()) {
            return true;
        }
        // Also match without leading dot
        let with_dot = format!(".{pattern}");
        if host_lower.ends_with(&with_dot) {
            return true;
        }
    }
    false
}

/// Detect proxy configuration from environment variables.
///
/// Checks (in order): HTTPS_PROXY, https_proxy, HTTP_PROXY, http_proxy, ALL_PROXY, all_proxy.
/// Also reads NO_PROXY/no_proxy for bypass rules.
pub fn detect_from_env() -> ProxyDetection {
    detect_from_env_map(&env::vars().collect())
}

/// Detect proxy from a provided environment map (testable).
pub fn detect_from_env_map(env_vars: &HashMap<String, String>) -> ProxyDetection {
    let check_order = [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];

    let checked_vars: Vec<String> = check_order.iter().map(|s| s.to_string()).collect();

    for var_name in &check_order {
        if let Some(value) = env_vars.get(*var_name) {
            if value.is_empty() {
                continue;
            }
            if let Some(mut config) = parse_proxy_url(value) {
                // Load no_proxy
                let no_proxy_val = env_vars
                    .get("NO_PROXY")
                    .or_else(|| env_vars.get("no_proxy"))
                    .map(|s| s.as_str())
                    .unwrap_or("");
                config.no_proxy = parse_no_proxy(no_proxy_val);

                return ProxyDetection {
                    proxy: Some(config),
                    source: Some(ProxySource::Environment(var_name.to_string())),
                    checked_vars,
                };
            }
        }
    }

    ProxyDetection {
        proxy: None,
        source: None,
        checked_vars,
    }
}

/// Result of probing a proxy for connectivity.
#[derive(Debug)]
pub struct ProbeResult {
    /// Whether the proxy is reachable via TCP.
    pub tcp_reachable: bool,
    /// Whether HTTP CONNECT succeeded (tunnel establishment).
    pub connect_supported: bool,
    /// HTTP status code from CONNECT response (e.g., 200, 407).
    pub status_code: Option<u16>,
    /// RTT to establish TCP connection to proxy.
    pub tcp_rtt: Option<Duration>,
    /// Whether authentication is required (407 response).
    pub auth_required: bool,
    /// Error message if probe failed.
    pub error: Option<String>,
}

/// Probe a proxy to verify it's functional.
///
/// Performs:
/// 1. TCP connect to proxy address (verifies reachability)
/// 2. HTTP CONNECT request to target through proxy (verifies tunnel support)
pub async fn probe_proxy(
    proxy_addr: SocketAddr,
    target_host: &str,
    target_port: u16,
    timeout_dur: Duration,
) -> ProbeResult {
    // Step 1: TCP connect to proxy
    let start = Instant::now();
    let stream = match tokio::time::timeout(timeout_dur, TcpStream::connect(proxy_addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return ProbeResult {
                tcp_reachable: false,
                connect_supported: false,
                status_code: None,
                tcp_rtt: None,
                auth_required: false,
                error: Some(format!("TCP connect failed: {e}")),
            };
        }
        Err(_) => {
            return ProbeResult {
                tcp_reachable: false,
                connect_supported: false,
                status_code: None,
                tcp_rtt: None,
                auth_required: false,
                error: Some("TCP connect timed out".to_string()),
            };
        }
    };
    let tcp_rtt = start.elapsed();

    // Step 2: Send HTTP CONNECT request
    let connect_req = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n\r\n"
    );

    let mut stream = stream;
    if let Err(e) = stream.write_all(connect_req.as_bytes()).await {
        return ProbeResult {
            tcp_reachable: true,
            connect_supported: false,
            status_code: None,
            tcp_rtt: Some(tcp_rtt),
            auth_required: false,
            error: Some(format!("Write failed: {e}")),
        };
    }

    // Read response (just the status line)
    let mut buf = [0u8; 256];
    let n = match tokio::time::timeout(timeout_dur, stream.read(&mut buf)).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            return ProbeResult {
                tcp_reachable: true,
                connect_supported: false,
                status_code: None,
                tcp_rtt: Some(tcp_rtt),
                auth_required: false,
                error: Some(format!("Read failed: {e}")),
            };
        }
        Err(_) => {
            return ProbeResult {
                tcp_reachable: true,
                connect_supported: false,
                status_code: None,
                tcp_rtt: Some(tcp_rtt),
                auth_required: false,
                error: Some("Read timed out".to_string()),
            };
        }
    };

    // Parse HTTP status line: "HTTP/1.1 200 Connection Established\r\n"
    let response = String::from_utf8_lossy(&buf[..n]);
    let status_code = parse_http_status(&response);
    let auth_required = status_code == Some(407);
    let connect_supported = status_code == Some(200);

    ProbeResult {
        tcp_reachable: true,
        connect_supported,
        status_code,
        tcp_rtt: Some(tcp_rtt),
        auth_required,
        error: if connect_supported {
            None
        } else {
            Some(format!("CONNECT returned status: {status_code:?}"))
        },
    }
}

/// Parse HTTP status code from response first line.
fn parse_http_status(response: &str) -> Option<u16> {
    // "HTTP/1.1 200 Connection Established"
    let first_line = response.lines().next()?;
    let parts: Vec<&str> = first_line.splitn(3, ' ').collect();
    if parts.len() >= 2 {
        parts[1].parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_proxy() {
        let config = parse_proxy_url("http://proxy.corp.example.com:8080").unwrap();
        assert_eq!(config.protocol, ProxyProtocol::Http);
        assert_eq!(config.host, "proxy.corp.example.com");
        assert_eq!(config.port, 8080);
        assert!(config.username.is_none());
    }

    #[test]
    fn test_parse_proxy_with_auth() {
        let config = parse_proxy_url("http://user:pass@proxy.local:3128").unwrap();
        assert_eq!(config.host, "proxy.local");
        assert_eq!(config.port, 3128);
        assert_eq!(config.username.as_deref(), Some("user"));
        assert_eq!(config.password.as_deref(), Some("pass"));
    }

    #[test]
    fn test_parse_socks5_proxy() {
        let config = parse_proxy_url("socks5://socks.local:1080").unwrap();
        assert_eq!(config.protocol, ProxyProtocol::Socks5);
        assert_eq!(config.host, "socks.local");
        assert_eq!(config.port, 1080);
    }

    #[test]
    fn test_parse_no_scheme() {
        let config = parse_proxy_url("proxy.local:8080").unwrap();
        assert_eq!(config.protocol, ProxyProtocol::Http);
        assert_eq!(config.host, "proxy.local");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_detect_from_env_https() {
        let mut env = HashMap::new();
        env.insert(
            "HTTPS_PROXY".to_string(),
            "http://proxy.corp:8080".to_string(),
        );
        env.insert(
            "NO_PROXY".to_string(),
            "localhost,.internal.corp".to_string(),
        );

        let detection = detect_from_env_map(&env);
        let proxy = detection.proxy.unwrap();
        assert_eq!(proxy.host, "proxy.corp");
        assert_eq!(proxy.port, 8080);
        assert_eq!(
            detection.source,
            Some(ProxySource::Environment("HTTPS_PROXY".to_string()))
        );
        assert_eq!(proxy.no_proxy, vec!["localhost", ".internal.corp"]);
    }

    #[test]
    fn test_detect_no_proxy() {
        let env: HashMap<String, String> = HashMap::new();
        let detection = detect_from_env_map(&env);
        assert!(detection.proxy.is_none());
    }

    #[test]
    fn test_should_bypass() {
        let no_proxy = vec![
            "localhost".to_string(),
            ".internal.corp".to_string(),
            "10.0.0.1".to_string(),
        ];

        assert!(should_bypass("localhost", &no_proxy));
        assert!(should_bypass("api.internal.corp", &no_proxy));
        assert!(should_bypass("10.0.0.1", &no_proxy));
        assert!(!should_bypass("external.example.com", &no_proxy));
    }

    #[test]
    fn test_bypass_wildcard() {
        let no_proxy = vec!["*".to_string()];
        assert!(should_bypass("anything.example.com", &no_proxy));
    }

    #[test]
    fn test_default_ports() {
        let http = parse_proxy_url("http://proxy.local").unwrap();
        assert_eq!(http.port, 8080);

        let https = parse_proxy_url("https://proxy.local").unwrap();
        assert_eq!(https.port, 8443);

        let socks = parse_proxy_url("socks5://proxy.local").unwrap();
        assert_eq!(socks.port, 1080);
    }

    #[tokio::test]
    async fn test_probe_proxy_tcp_reachable() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        // Simulate a proxy that responds with 200
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read the CONNECT request
            let mut buf = [0u8; 256];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            // Respond with 200
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
        });

        let result = probe_proxy(addr, "example.com", 443, Duration::from_secs(5)).await;
        assert!(result.tcp_reachable);
        assert!(result.connect_supported);
        assert_eq!(result.status_code, Some(200));
        assert!(result.tcp_rtt.is_some());
        assert!(!result.auth_required);
    }

    #[tokio::test]
    async fn test_probe_proxy_auth_required() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 256];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            stream
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .await
                .unwrap();
        });

        let result = probe_proxy(addr, "example.com", 443, Duration::from_secs(5)).await;
        assert!(result.tcp_reachable);
        assert!(!result.connect_supported);
        assert_eq!(result.status_code, Some(407));
        assert!(result.auth_required);
    }

    #[tokio::test]
    async fn test_probe_proxy_unreachable() {
        // Non-listening address
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let result = probe_proxy(addr, "example.com", 443, Duration::from_millis(100)).await;
        assert!(!result.tcp_reachable);
        assert!(!result.connect_supported);
        assert!(result.error.is_some());
    }
}
