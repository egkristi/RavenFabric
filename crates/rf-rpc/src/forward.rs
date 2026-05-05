//! Port forwarding types — local/remote/dynamic forward definitions.
//!
//! Implements the equivalent of ssh -L, ssh -R, and ssh -D through
//! the RavenFabric mesh.

use serde::{Deserialize, Serialize};

/// Direction of port forwarding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForwardDirection {
    /// Local forward (ssh -L): listen locally, forward to remote agent.
    Local,
    /// Remote forward (ssh -R): listen on remote agent, forward back to client.
    Remote,
}

/// A port forwarding rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortForward {
    /// Direction of the forward.
    pub direction: ForwardDirection,
    /// Local bind address (host:port for local, bind address for remote listener).
    pub bind_addr: String,
    /// Target address to forward to (host:port on the remote/local side).
    pub target_addr: String,
    /// Optional unique ID for this forward rule.
    pub id: Option<String>,
}

/// State of an active port forward.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForwardState {
    /// Listener bound, waiting for connections.
    Listening,
    /// Forward active (at least one connection being forwarded).
    Active,
    /// Forward closed.
    Closed,
    /// Error occurred (listener bind failed, target unreachable).
    Error,
}

/// Statistics for an active port forward.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForwardStats {
    /// Total connections accepted.
    pub connections_total: u64,
    /// Currently active connections.
    pub connections_active: u32,
    /// Total bytes forwarded (both directions).
    pub bytes_forwarded: u64,
}

/// Manages a set of active port forwards.
#[derive(Debug, Default)]
pub struct ForwardManager {
    forwards: Vec<(PortForward, ForwardState, ForwardStats)>,
}

impl ForwardManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new port forward rule.
    pub fn add(&mut self, forward: PortForward) -> usize {
        let idx = self.forwards.len();
        self.forwards
            .push((forward, ForwardState::Listening, ForwardStats::default()));
        idx
    }

    /// Remove a port forward by index.
    pub fn remove(&mut self, idx: usize) -> Option<PortForward> {
        if idx < self.forwards.len() {
            let (fwd, _, _) = self.forwards.remove(idx);
            Some(fwd)
        } else {
            None
        }
    }

    /// Get current state of a forward.
    pub fn state(&self, idx: usize) -> Option<ForwardState> {
        self.forwards.get(idx).map(|(_, state, _)| *state)
    }

    /// Update state of a forward.
    pub fn set_state(&mut self, idx: usize, state: ForwardState) {
        if let Some((_, s, _)) = self.forwards.get_mut(idx) {
            *s = state;
        }
    }

    /// Record a new connection on a forward.
    pub fn record_connection(&mut self, idx: usize) {
        if let Some((_, state, stats)) = self.forwards.get_mut(idx) {
            stats.connections_total += 1;
            stats.connections_active += 1;
            *state = ForwardState::Active;
        }
    }

    /// Record a connection close on a forward.
    pub fn record_disconnect(&mut self, idx: usize) {
        if let Some((_, state, stats)) = self.forwards.get_mut(idx) {
            stats.connections_active = stats.connections_active.saturating_sub(1);
            if stats.connections_active == 0 {
                *state = ForwardState::Listening;
            }
        }
    }

    /// Record bytes forwarded.
    pub fn record_bytes(&mut self, idx: usize, bytes: u64) {
        if let Some((_, _, stats)) = self.forwards.get_mut(idx) {
            stats.bytes_forwarded += bytes;
        }
    }

    /// List all active forwards.
    pub fn list(&self) -> Vec<(&PortForward, ForwardState, &ForwardStats)> {
        self.forwards
            .iter()
            .map(|(fwd, state, stats)| (fwd, *state, stats))
            .collect()
    }

    /// Number of active forwards.
    pub fn len(&self) -> usize {
        self.forwards.len()
    }

    /// Whether the manager has no forwards.
    pub fn is_empty(&self) -> bool {
        self.forwards.is_empty()
    }
}

/// Start a local TCP port forward: listen on `bind_addr`, forward each
/// connection to `target_addr`. Runs until `cancel` is notified.
///
/// Returns immediately after the listener is bound. Each accepted connection
/// is handled in a spawned Tokio task that copies data bidirectionally.
pub async fn start_local_forward(
    bind_addr: &str,
    target_addr: String,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((inbound, _addr)) => {
                            let target = target_addr.clone();
                            tokio::spawn(async move {
                                if let Err(e) = forward_connection(inbound, &target).await {
                                    tracing::warn!("forward to {target} failed: {e}");
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("accept failed: {e}");
                            break;
                        }
                    }
                }
                _ = cancel_wait(&cancel) => {
                    tracing::info!("port forward cancelled");
                    break;
                }
            }
        }
    });
    Ok(handle)
}

/// Wait until the cancel signal is received.
async fn cancel_wait(cancel: &tokio::sync::watch::Receiver<bool>) {
    let mut rx = cancel.clone();
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Forward data bidirectionally between an inbound stream and a target address.
async fn forward_connection(
    mut inbound: tokio::net::TcpStream,
    target_addr: &str,
) -> std::io::Result<()> {
    let mut outbound = tokio::net::TcpStream::connect(target_addr).await?;
    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = tokio::io::copy(&mut ri, &mut wo);
    let server_to_client = tokio::io::copy(&mut ro, &mut wi);

    tokio::select! {
        result = client_to_server => {
            result?;
        }
        result = server_to_client => {
            result?;
        }
    }
    Ok(())
}

/// Start a remote TCP port forward (ssh -R equivalent): agent listens on
/// `bind_addr` and for each accepted connection, forwards to `target_addr`
/// which is resolved on the agent side (the remote end).
///
/// In RavenFabric context: the agent opens a listener, and when a client
/// connects to it, the agent connects to the target and relays bidirectionally.
/// This is useful for exposing a service through the agent.
pub async fn start_remote_forward(
    bind_addr: &str,
    target_addr: String,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> std::io::Result<(tokio::task::JoinHandle<()>, std::net::SocketAddr)> {
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let bound_addr = listener.local_addr()?;
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((inbound, peer)) => {
                            tracing::info!("remote forward: accepted from {peer}");
                            let target = target_addr.clone();
                            tokio::spawn(async move {
                                if let Err(e) = forward_connection(inbound, &target).await {
                                    tracing::warn!("remote forward to {target} failed: {e}");
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("remote forward accept failed: {e}");
                            break;
                        }
                    }
                }
                _ = cancel_wait(&cancel) => {
                    tracing::info!("remote forward cancelled");
                    break;
                }
            }
        }
    });
    Ok((handle, bound_addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_forward() -> PortForward {
        PortForward {
            direction: ForwardDirection::Local,
            bind_addr: "127.0.0.1:8080".into(),
            target_addr: "db-server:5432".into(),
            id: Some("db-tunnel".into()),
        }
    }

    fn remote_forward() -> PortForward {
        PortForward {
            direction: ForwardDirection::Remote,
            bind_addr: "0.0.0.0:9090".into(),
            target_addr: "localhost:3000".into(),
            id: None,
        }
    }

    #[test]
    fn test_add_and_list() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        mgr.add(remote_forward());

        assert_eq!(mgr.len(), 2);
        let list = mgr.list();
        assert_eq!(list[0].0.bind_addr, "127.0.0.1:8080");
        assert_eq!(list[1].0.bind_addr, "0.0.0.0:9090");
    }

    #[test]
    fn test_initial_state_listening() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        assert_eq!(mgr.state(0), Some(ForwardState::Listening));
    }

    #[test]
    fn test_connection_tracking() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());

        mgr.record_connection(0);
        assert_eq!(mgr.state(0), Some(ForwardState::Active));

        mgr.record_connection(0);
        let list = mgr.list();
        assert_eq!(list[0].2.connections_active, 2);
        assert_eq!(list[0].2.connections_total, 2);

        mgr.record_disconnect(0);
        let list = mgr.list();
        assert_eq!(list[0].2.connections_active, 1);

        mgr.record_disconnect(0);
        assert_eq!(mgr.state(0), Some(ForwardState::Listening));
    }

    #[test]
    fn test_bytes_tracking() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());

        mgr.record_bytes(0, 1024);
        mgr.record_bytes(0, 2048);

        let list = mgr.list();
        assert_eq!(list[0].2.bytes_forwarded, 3072);
    }

    #[test]
    fn test_remove() {
        let mut mgr = ForwardManager::new();
        mgr.add(local_forward());
        mgr.add(remote_forward());

        let removed = mgr.remove(0).unwrap();
        assert_eq!(removed.bind_addr, "127.0.0.1:8080");
        assert_eq!(mgr.len(), 1);
    }

    #[test]
    fn test_serialization() {
        let fwd = local_forward();
        let json = serde_json::to_string(&fwd).unwrap();
        let parsed: PortForward = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, fwd);
    }

    #[tokio::test]
    async fn test_local_forward_data_flow() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Start an echo server (simulates the "target" service)
        let echo_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo_listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = echo_listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 1024];
                    loop {
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        stream.write_all(&buf[..n]).await.unwrap();
                    }
                });
            }
        });

        // Start the port forwarder
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let handle =
            start_local_forward("127.0.0.1:0", format!("127.0.0.1:{echo_port}"), cancel_rx).await;

        // We need the actual port — re-bind to get it. Use a different approach:
        // Bind first, get port, then connect.
        // Actually, start_local_forward already bound. We need the port.
        // Let's use a known port approach instead:
        drop(handle);
        drop(cancel_tx);

        // Re-do with a known bind port
        let temp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let forward_port = temp_listener.local_addr().unwrap().port();
        drop(temp_listener);

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let _handle = start_local_forward(
            &format!("127.0.0.1:{forward_port}"),
            format!("127.0.0.1:{echo_port}"),
            cancel_rx,
        )
        .await
        .unwrap();

        // Give the listener a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect through the forwarder
        let mut client = tokio::net::TcpStream::connect(format!("127.0.0.1:{forward_port}"))
            .await
            .unwrap();
        client.write_all(b"hello forward").await.unwrap();

        let mut buf = vec![0u8; 64];
        let n = client.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello forward");

        // Cancel the forwarder
        cancel_tx.send(true).unwrap();
    }
}
