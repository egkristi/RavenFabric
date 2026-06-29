//! Lightweight Prometheus metrics HTTP endpoint.
//!
//! Serves `/metrics` on a configurable TCP port. Uses raw tokio TCP —
//! no HTTP framework dependency needed for a single read-only endpoint.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::metrics::{
    MetricCollector, RavenFabricMetricsCollector, SystemMetricsCollector, to_prometheus,
};

/// Configuration for the metrics server.
#[derive(Debug, Clone)]
pub struct MetricsServerConfig {
    /// Address to bind (e.g., "127.0.0.1:9100").
    pub bind_addr: String,
}

impl Default for MetricsServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:9100".into(),
        }
    }
}

/// Start the Prometheus metrics HTTP server with system and RavenFabric metrics.
///
/// This spawns a Tokio task that listens for HTTP GET /metrics requests
/// and responds with the current metrics in Prometheus exposition format.
///
/// `rf_collector` — an optional pre-configured RavenFabric metrics collector.
/// When `None`, a fresh collector with independent counters is created (useful
/// for testing).  When `Some(collector)`, the collector's counters are shared
/// with the executor so that `/metrics` reflects real-time activity.
///
/// Returns the JoinHandle for the server task.
pub async fn start_metrics_server(
    config: MetricsServerConfig,
    rf_collector: Option<RavenFabricMetricsCollector>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(&config.bind_addr).await?;
    info!(
        "prometheus metrics endpoint listening on {}",
        config.bind_addr
    );

    let system_collector = Arc::new(Mutex::new(SystemMetricsCollector::new(
        Duration::from_secs(15),
    )));
    let rf_collector = Arc::new(Mutex::new(
        rf_collector.unwrap_or_else(|| RavenFabricMetricsCollector::new_with_counters(
            Duration::from_secs(15),
        )),
    ));

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, _addr)) => {
                    let system_collector = system_collector.clone();
                    let rf_collector = rf_collector.clone();
                    tokio::spawn(async move {
                        // Read the HTTP request (we only need to know it's a GET)
                        let mut buf = vec![0u8; 2048];
                        let n = match stream.read(&mut buf).await {
                            Ok(n) => n,
                            Err(_) => return,
                        };

                        let request = String::from_utf8_lossy(&buf[..n]);

                        // Only serve GET /metrics
                        if request.starts_with("GET /metrics") {
                            let mut all_points = Vec::new();

                            // Collect system metrics
                            {
                                let mut coll = system_collector.lock().await;
                                all_points.extend(coll.collect());
                            }

                            // Collect RavenFabric metrics
                            {
                                let mut coll = rf_collector.lock().await;
                                all_points.extend(coll.collect());
                            }

                            let body = to_prometheus(&all_points);

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
                        } else if request.starts_with("GET /health") || request.starts_with("GET /")
                        {
                            let body = "ok\n";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\n\
                                 Content-Type: text/plain\r\n\
                                 Content-Length: {}\r\n\
                                 Connection: close\r\n\
                                 \r\n\
                                 {}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\n\
                                            Content-Length: 0\r\n\
                                            Connection: close\r\n\
                                            \r\n";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    });
                }
                Err(e) => {
                    warn!("metrics server accept error: {}", e);
                }
            }
        }
    });

    Ok(handle)
}

/// Type alias for the 6-tuple of shared atomic counters used by
/// [`RavenFabricMetricsCollector`].
pub type RfCounters = (
    Arc<std::sync::atomic::AtomicU64>,
    Arc<std::sync::atomic::AtomicU64>,
    Arc<std::sync::atomic::AtomicU64>,
    Arc<std::sync::atomic::AtomicI64>,
    Arc<std::sync::atomic::AtomicU64>,
    Arc<std::sync::atomic::AtomicU64>,
);

/// Create a [`RavenFabricMetricsCollector`] and return it together with its
/// shared counter Arcs so callers can wire them into the executor.
///
/// Usage in agent main:
/// ```ignore
/// let (rf_collector, counters) = new_rf_collector_with_counters();
/// let executor = Executor::new(...)
///     .with_counters(
///         Some(counters.0),
///         Some(counters.1),
///         Some(counters.2),
///         Some(counters.3),
///         Some(counters.4),
///         Some(counters.5),
///     );
/// start_metrics_server(config, Some(rf_collector)).await?;
/// ```
#[allow(clippy::type_complexity)]
pub fn new_rf_collector_with_counters() -> (RavenFabricMetricsCollector, RfCounters) {
    let collector = RavenFabricMetricsCollector::new_with_counters(Duration::from_secs(15));
    let (ca, cd, ae, ac, hc, hl) = collector.counters();
    let counters = (
        Arc::clone(ca),
        Arc::clone(cd),
        Arc::clone(ae),
        Arc::clone(ac),
        Arc::clone(hc),
        Arc::clone(hl),
    );
    (collector, counters)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_server_responds() {
        // Bind to port 0 to get a random available port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config = MetricsServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
        };

        let _handle = start_metrics_server(config, None).await.unwrap();

        // Give the server time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Make a request
        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: text/plain; version=0.0.4"));
        assert!(response.contains("system_cpu_usage_percent"));
    }

    #[tokio::test]
    async fn test_metrics_server_404_on_unknown_path() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let config = MetricsServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
        };

        let _handle = start_metrics_server(config, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        stream
            .write_all(b"POST /something HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("404 Not Found"));
    }
}
