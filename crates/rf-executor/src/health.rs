//! Health check probes — HTTP/TCP/UDP endpoint monitoring.
//!
//! Provides types and logic for checking the health of endpoints,
//! services, and the agent itself.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Type of health check to perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    /// TCP connection test (connect + optional banner read).
    Tcp { host: String, port: u16 },
    /// HTTP(S) GET with expected status code.
    Http { url: String, expected_status: u16 },
    /// Process alive check (by name or PID).
    Process { name: String },
    /// Command execution (exit code 0 = healthy).
    Command { cmd: String },
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheck {
    /// Unique name for this check.
    pub name: String,
    /// Type of probe.
    pub probe: ProbeType,
    /// How often to run this check.
    pub interval: Duration,
    /// Timeout for each probe attempt.
    pub timeout: Duration,
    /// How many consecutive failures before marking unhealthy.
    pub failure_threshold: u32,
    /// How many consecutive successes to recover from unhealthy.
    pub success_threshold: u32,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            name: String::new(),
            probe: ProbeType::Tcp {
                host: "localhost".into(),
                port: 80,
            },
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 1,
        }
    }
}

/// Current health status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Check has not run yet.
    Unknown,
    /// Endpoint is healthy.
    Healthy,
    /// Endpoint is degraded (some failures but below threshold).
    Degraded,
    /// Endpoint is unhealthy (failure threshold exceeded).
    Unhealthy,
}

/// Result of a single probe execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeResult {
    /// Whether the probe succeeded.
    pub success: bool,
    /// Response time in milliseconds.
    pub latency_ms: u64,
    /// Error message if failed.
    pub error: Option<String>,
    /// Timestamp (ms since epoch).
    pub timestamp_ms: u64,
}

/// State tracker for a health check (tracks consecutive successes/failures).
#[derive(Debug, Clone)]
pub struct HealthTracker {
    check: HealthCheck,
    status: HealthStatus,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_result: Option<ProbeResult>,
    total_checks: u64,
    total_failures: u64,
}

impl HealthTracker {
    /// Create a new health tracker.
    pub fn new(check: HealthCheck) -> Self {
        Self {
            check,
            status: HealthStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_result: None,
            total_checks: 0,
            total_failures: 0,
        }
    }

    /// Record a probe result and update health status.
    pub fn record(&mut self, result: ProbeResult) {
        self.total_checks += 1;

        if result.success {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;

            if self.consecutive_successes >= self.check.success_threshold {
                self.status = HealthStatus::Healthy;
            }
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
            self.total_failures += 1;

            if self.consecutive_failures >= self.check.failure_threshold {
                self.status = HealthStatus::Unhealthy;
            } else if self.consecutive_failures > 0 {
                self.status = HealthStatus::Degraded;
            }
        }

        self.last_result = Some(result);
    }

    /// Current health status.
    pub fn status(&self) -> HealthStatus {
        self.status
    }

    /// The health check configuration.
    pub fn check(&self) -> &HealthCheck {
        &self.check
    }

    /// Last probe result.
    pub fn last_result(&self) -> Option<&ProbeResult> {
        self.last_result.as_ref()
    }

    /// Availability percentage (0.0 - 1.0).
    pub fn availability(&self) -> f64 {
        if self.total_checks == 0 {
            return 1.0;
        }
        1.0 - (self.total_failures as f64 / self.total_checks as f64)
    }

    /// Whether the check is currently healthy.
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy || self.status == HealthStatus::Unknown
    }
}

/// Execute a probe and return the result.
pub async fn execute_probe(probe: &ProbeType, timeout: Duration) -> ProbeResult {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let start = std::time::Instant::now();

    match probe {
        ProbeType::Tcp { host, port } => {
            let addr = format!("{host}:{port}");
            match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await {
                Ok(Ok(_stream)) => ProbeResult {
                    success: true,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error: None,
                    timestamp_ms: ts,
                },
                Ok(Err(e)) => ProbeResult {
                    success: false,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("TCP connect to {addr}: {e}")),
                    timestamp_ms: ts,
                },
                Err(_) => ProbeResult {
                    success: false,
                    latency_ms: timeout.as_millis() as u64,
                    error: Some(format!("TCP connect to {addr}: timeout")),
                    timestamp_ms: ts,
                },
            }
        }
        ProbeType::Http {
            url,
            expected_status,
        } => {
            // Basic HTTP GET using raw TCP — no HTTP client dependency needed.
            // Parse URL to extract host, port, path.
            let result = http_probe(url, *expected_status, timeout).await;
            ProbeResult {
                success: result.is_ok(),
                latency_ms: start.elapsed().as_millis() as u64,
                error: result.err(),
                timestamp_ms: ts,
            }
        }
        ProbeType::Process { name } => {
            let found = check_process_alive(name);
            ProbeResult {
                success: found,
                latency_ms: start.elapsed().as_millis() as u64,
                error: if found {
                    None
                } else {
                    Some(format!("process '{name}' not found"))
                },
                timestamp_ms: ts,
            }
        }
        ProbeType::Command { cmd } => {
            match tokio::time::timeout(
                timeout,
                tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status(),
            )
            .await
            {
                Ok(Ok(status)) => ProbeResult {
                    success: status.success(),
                    latency_ms: start.elapsed().as_millis() as u64,
                    error: if status.success() {
                        None
                    } else {
                        Some(format!("exit code: {}", status.code().unwrap_or(-1)))
                    },
                    timestamp_ms: ts,
                },
                Ok(Err(e)) => ProbeResult {
                    success: false,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("command failed: {e}")),
                    timestamp_ms: ts,
                },
                Err(_) => ProbeResult {
                    success: false,
                    latency_ms: timeout.as_millis() as u64,
                    error: Some("command timed out".into()),
                    timestamp_ms: ts,
                },
            }
        }
    }
}

/// Basic HTTP GET probe using raw TCP (no HTTP client dependency).
async fn http_probe(url: &str, expected_status: u16, timeout: Duration) -> Result<(), String> {
    // Parse URL: http://host:port/path or https://host:port/path
    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = url
        .split_once('/')
        .map(|(h, p)| (h, format!("/{p}")))
        .unwrap_or((url, "/".into()));

    let addr = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:80")
    };

    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .map_err(|_| "connection timeout".to_string())?
        .map_err(|e| format!("connect: {e}"))?;

    let host = host_port.split(':').next().unwrap_or(host_port);
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = stream;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("write: {e}"))?;

    let mut buf = vec![0u8; 1024];
    let n = tokio::time::timeout(timeout, stream.read(&mut buf))
        .await
        .map_err(|_| "read timeout".to_string())?
        .map_err(|e| format!("read: {e}"))?;

    let response = String::from_utf8_lossy(&buf[..n]);
    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = response.lines().next().unwrap_or("");
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if status_code == expected_status {
        Ok(())
    } else {
        Err(format!(
            "expected status {expected_status}, got {status_code}"
        ))
    }
}

/// Check if a process with the given name is alive (using sysinfo if available).
fn check_process_alive(name: &str) -> bool {
    #[cfg(feature = "sysinfo")]
    {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        sys.processes()
            .values()
            .any(|p| p.name().to_string_lossy().contains(name))
    }
    #[cfg(not(feature = "sysinfo"))]
    {
        let _ = name;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_check() -> HealthCheck {
        HealthCheck {
            name: "web-server".into(),
            probe: ProbeType::Http {
                url: "http://localhost:8080/health".into(),
                expected_status: 200,
            },
            interval: Duration::from_secs(10),
            timeout: Duration::from_secs(3),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }

    fn success_result() -> ProbeResult {
        ProbeResult {
            success: true,
            latency_ms: 15,
            error: None,
            timestamp_ms: 1_700_000_000_000,
        }
    }

    fn failure_result() -> ProbeResult {
        ProbeResult {
            success: false,
            latency_ms: 3000,
            error: Some("connection refused".into()),
            timestamp_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn test_initial_state() {
        let tracker = HealthTracker::new(test_check());
        assert_eq!(tracker.status(), HealthStatus::Unknown);
        assert!(tracker.is_healthy()); // Unknown counts as healthy
    }

    #[test]
    fn test_becomes_healthy_after_threshold() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Unknown); // Need 2 successes

        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_becomes_unhealthy_after_threshold() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_recovery_resets_failures() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(failure_result());
        tracker.record(failure_result());
        assert_eq!(tracker.status(), HealthStatus::Degraded);

        // One success resets failure counter
        tracker.record(success_result());
        tracker.record(success_result());
        assert_eq!(tracker.status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_availability() {
        let mut tracker = HealthTracker::new(test_check());
        tracker.record(success_result());
        tracker.record(success_result());
        tracker.record(success_result());
        tracker.record(failure_result());

        assert!((tracker.availability() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_probe_types_serialize() {
        let tcp = ProbeType::Tcp {
            host: "db.local".into(),
            port: 5432,
        };
        let json = serde_json::to_string(&tcp).unwrap();
        let parsed: ProbeType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tcp);
    }

    #[tokio::test]
    async fn test_tcp_probe_refused() {
        // Port 1 is almost certainly not listening
        let probe = ProbeType::Tcp {
            host: "127.0.0.1".into(),
            port: 1,
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_tcp_probe_success() {
        // Start a listener, then probe it
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let probe = ProbeType::Tcp {
            host: "127.0.0.1".into(),
            port,
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_command_probe_success() {
        let probe = ProbeType::Command {
            cmd: "true".into(),
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_command_probe_failure() {
        let probe = ProbeType::Command {
            cmd: "false".into(),
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_http_probe_success() {
        // Start a minimal HTTP server
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let probe = ProbeType::Http {
            url: format!("http://127.0.0.1:{port}/health"),
            expected_status: 200,
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(result.success, "error: {:?}", result.error);
    }

    #[tokio::test]
    async fn test_http_probe_wrong_status() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let response = "HTTP/1.1 503 Service Unavailable\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let probe = ProbeType::Http {
            url: format!("http://127.0.0.1:{port}/health"),
            expected_status: 200,
        };
        let result = execute_probe(&probe, Duration::from_secs(2)).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("503"));
    }
}
