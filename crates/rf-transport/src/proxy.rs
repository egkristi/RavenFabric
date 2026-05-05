//! Corporate proxy detection and environment resolution.
//!
//! Detects HTTP/HTTPS/SOCKS proxy settings from environment variables
//! and system configuration, producing connection parameters for
//! upstream transport drivers.

use std::collections::HashMap;
use std::env;

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
        let with_dot = format!(".{}", pattern);
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
}
