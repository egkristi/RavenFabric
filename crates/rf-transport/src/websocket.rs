//! WebSocket transport driver.
//!
//! Connects to a relay server over WebSocket, providing an
//! AsyncRead + AsyncWrite stream for the Noise handshake and RPC.
//!
//! Uses a spawned bridge task to convert between the WebSocket message
//! stream and a byte-oriented `DuplexStream`.

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// WebSocket transport driver.
pub struct WebSocketDriver;

impl WebSocketDriver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSocketDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for WebSocketDriver {
    fn name(&self) -> &str {
        "websocket"
    }

    fn available(&self) -> bool {
        true
    }

    async fn dial(
        &self,
        target: &Target,
        _config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let url = target
            .relay_url
            .as_ref()
            .ok_or_else(|| TransportError::Connection("no relay_url in target".into()))?;

        let (mut ws_stream, _) = connect_async(url)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        // Send meet token as the first WS message before bridging.
        // This ensures it arrives as a separate frame (relay protocol requirement).
        if let Some(token) = &target.meet_token {
            ws_stream
                .send(Message::Binary(token.as_bytes().to_vec().into()))
                .await
                .map_err(|e| TransportError::Connection(e.to_string()))?;
        }

        let stream = bridge_ws(ws_stream);
        Ok(Box::new(stream))
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        Ok(Box::new(WsListener { listener }))
    }
}

struct WsListener {
    listener: TcpListener,
}

#[async_trait::async_trait]
impl Listener for WsListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let (tcp_stream, _addr) = self
            .listener
            .accept()
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        let ws_stream = tokio_tungstenite::accept_async(tcp_stream)
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))?;

        let stream = bridge_ws(ws_stream);
        Ok(Box::new(stream))
    }
}

/// Bridge a WebSocket stream into a DuplexStream for byte-oriented I/O.
///
/// Spawns two tasks:
/// - Reader: WS messages → write to duplex
/// - Writer: read from duplex → WS messages
///
/// Uses a 64 KB duplex buffer. Noise XX handshake messages are ~200 bytes,
/// so 64 KB provides ample headroom to prevent deadlocks during asymmetric
/// relay latency without wasting memory.
fn bridge_ws<S>(ws: S) -> DuplexStream
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
        + Send
        + Unpin
        + 'static,
{
    let (app_stream, bridge_stream) = tokio::io::duplex(64 * 1024); // 64 KB buffer
    let (mut bridge_read, mut bridge_write) = tokio::io::split(bridge_stream);
    let (mut ws_sink, mut ws_source) = ws.split();

    // WS → app: read messages from WS, write bytes to app
    tokio::spawn(async move {
        while let Some(msg) = ws_source.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if bridge_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {} // skip ping/pong/text
            }
        }
    });

    // app → WS: read bytes from app, send as WS binary messages
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            match bridge_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let msg = Message::Binary(buf[..n].to_vec().into());
                    if ws_sink.send(msg).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = ws_sink.close().await;
    });

    app_stream
}
