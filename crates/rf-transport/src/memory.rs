//! In-memory transport driver for testing.
//!
//! Uses `tokio::io::duplex` to create paired streams.
//! The "dial" end connects to a "listen" end via a shared channel.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::DuplexStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Shared state for connecting in-memory peers.
#[derive(Clone)]
pub struct MemoryBroker {
    listeners: Arc<Mutex<HashMap<String, mpsc::Sender<DuplexStream>>>>,
}

impl MemoryBroker {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for MemoryBroker {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory transport driver.
pub struct MemoryDriver {
    broker: MemoryBroker,
}

impl MemoryDriver {
    pub fn new(broker: MemoryBroker) -> Self {
        Self { broker }
    }
}

#[async_trait::async_trait]
impl Driver for MemoryDriver {
    fn name(&self) -> &str {
        "memory"
    }

    fn available(&self) -> bool {
        true
    }

    async fn dial(
        &self,
        target: &Target,
        _config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let listeners = self.broker.listeners.lock().await;
        let sender = listeners
            .get(&target.agent_id)
            .ok_or_else(|| {
                TransportError::Connection(format!("no listener for agent: {}", target.agent_id))
            })?
            .clone();
        drop(listeners);

        let (client_stream, server_stream) = tokio::io::duplex(65536);

        sender
            .send(server_stream)
            .await
            .map_err(|_| TransportError::Connection("listener closed".into()))?;

        Ok(Box::new(client_stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let (tx, rx) = mpsc::channel(32);

        let mut listeners = self.broker.listeners.lock().await;
        listeners.insert(addr.to_string(), tx);

        Ok(Box::new(MemoryListener { rx: Mutex::new(rx) }))
    }
}

struct MemoryListener {
    rx: Mutex<mpsc::Receiver<DuplexStream>>,
}

#[async_trait::async_trait]
impl Listener for MemoryListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let mut rx = self.rx.lock().await;
        let stream = rx
            .recv()
            .await
            .ok_or_else(|| TransportError::Connection("broker shutdown".into()))?;
        Ok(Box::new(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_memory_driver_connect() {
        let broker = MemoryBroker::new();
        let driver = MemoryDriver::new(broker);

        // Start listener
        let listener = driver.listen("test-agent").await.unwrap();

        // Dial from a client
        let target = Target {
            agent_id: "test-agent".into(),
            relay_url: None,
            meet_token: None,
        };

        let dial_handle = tokio::spawn({
            let broker_clone = driver.broker.clone();
            let target_clone = target.clone();
            async move {
                let d = MemoryDriver::new(broker_clone);
                d.dial(&target_clone, &HashMap::new()).await
            }
        });

        let mut server_stream = listener.accept().await.unwrap();
        let mut client_stream = dial_handle.await.unwrap().unwrap();

        // Write from client, read on server
        client_stream.write_all(b"hello server").await.unwrap();
        let mut buf = [0u8; 12];
        server_stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello server");

        // Write from server, read on client
        server_stream.write_all(b"hello client").await.unwrap();
        let mut buf = [0u8; 12];
        client_stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello client");
    }

    #[tokio::test]
    async fn test_memory_driver_no_listener() {
        let broker = MemoryBroker::new();
        let driver = MemoryDriver::new(broker);

        let target = Target {
            agent_id: "nonexistent".into(),
            relay_url: None,
            meet_token: None,
        };

        let result = driver.dial(&target, &HashMap::new()).await;
        assert!(result.is_err());
    }
}
