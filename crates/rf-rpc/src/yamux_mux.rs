//! Yamux-based multiplexing over SecureChannel.
//!
//! Provides concurrent RPC streams over a single encrypted connection.
//! Each RPC request/response pair uses a separate yamux stream.
//!
//! Yamux uses `futures_io` traits while tokio uses its own. We bridge
//! them using `tokio_util::compat`.
//!
//! Both client and server spawn a background driver task that continuously
//! polls the yamux `Connection` to process I/O. Without this driver, stream
//! reads/writes would deadlock since yamux frames only flow when the
//! connection is polled.

use futures_util::future::poll_fn;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use yamux::{Config, Connection, Mode, Stream};

use crate::codec;
use crate::error::RpcError;
use crate::types::{Request, Response};

/// A multiplexed RPC connection (client-side).
///
/// Opens a new yamux stream for each RPC request, enabling concurrent requests.
/// Internally spawns a driver task to keep the connection alive.
pub struct MuxClient {
    stream_request_tx: mpsc::Sender<oneshot::Sender<Result<Stream, RpcError>>>,
}

/// A multiplexed RPC connection (server-side).
///
/// Accepts incoming yamux streams, each carrying one RPC request.
/// Internally spawns a driver task that delivers accepted streams.
pub struct MuxServer {
    stream_rx: mpsc::Receiver<Stream>,
}

impl MuxClient {
    /// Create a new multiplexed client from a tokio async stream.
    ///
    /// Spawns a background task that drives the yamux connection.
    pub fn new<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: T) -> Self {
        let compat = TokioAsyncReadCompatExt::compat(stream);
        let mut config = Config::default();
        config.set_max_num_streams(256);
        let mut connection = Connection::new(compat, config, Mode::Client);

        let (tx, mut rx) = mpsc::channel::<oneshot::Sender<Result<Stream, RpcError>>>(16);

        tokio::spawn(async move {
            loop {
                let done = poll_fn(|cx| {
                    // Check for stream-open requests from the client API
                    if let std::task::Poll::Ready(msg) = rx.poll_recv(cx) {
                        match msg {
                            Some(reply_tx) => {
                                // poll_new_outbound allocates a stream ID immediately
                                let result = match connection.poll_new_outbound(cx) {
                                    std::task::Poll::Ready(Ok(s)) => Ok(s),
                                    std::task::Poll::Ready(Err(e)) => {
                                        Err(RpcError::Io(format!("yamux: {e}")))
                                    }
                                    std::task::Poll::Pending => {
                                        Err(RpcError::Io("yamux: too many streams".into()))
                                    }
                                };
                                let _ = reply_tx.send(result);
                            }
                            None => return std::task::Poll::Ready(true), // channel closed
                        }
                    }

                    // Drive the connection (flushes writes, reads socket, etc.)
                    match connection.poll_next_inbound(cx) {
                        std::task::Poll::Ready(None) => {
                            return std::task::Poll::Ready(true);
                        }
                        std::task::Poll::Ready(Some(Err(_))) => {
                            return std::task::Poll::Ready(true);
                        }
                        std::task::Poll::Ready(Some(Ok(_))) => {
                            // Unexpected inbound stream on client side, ignore
                        }
                        std::task::Poll::Pending => {}
                    }

                    std::task::Poll::Pending
                })
                .await;

                if done {
                    break;
                }
            }
        });

        Self {
            stream_request_tx: tx,
        }
    }

    /// Open a new yamux stream.
    async fn open_stream(&self) -> Result<Stream, RpcError> {
        let (tx, rx) = oneshot::channel();
        self.stream_request_tx
            .send(tx)
            .await
            .map_err(|_| RpcError::Io("connection closed".into()))?;
        rx.await
            .map_err(|_| RpcError::Io("connection closed".into()))?
    }

    /// Send an RPC request and receive the response over a new multiplexed stream.
    pub async fn request(&self, req: &Request) -> Result<Response, RpcError> {
        let stream = self.open_stream().await?;
        let mut tokio_stream = FuturesAsyncReadCompatExt::compat(stream);

        let req_data = codec::encode(req)?;
        write_frame(&mut tokio_stream, &req_data).await?;

        let resp_data = read_frame(&mut tokio_stream).await?;
        codec::decode(&resp_data)
    }
}

impl MuxServer {
    /// Create a new multiplexed server from a tokio async stream.
    ///
    /// Spawns a background task that drives the yamux connection and delivers
    /// accepted streams through an internal channel.
    pub fn new<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(stream: T) -> Self {
        let compat = TokioAsyncReadCompatExt::compat(stream);
        let mut config = Config::default();
        config.set_max_num_streams(256);
        let mut connection = Connection::new(compat, config, Mode::Server);

        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            loop {
                match poll_fn(|cx| connection.poll_next_inbound(cx)).await {
                    Some(Ok(stream)) => {
                        if tx.send(stream).await.is_err() {
                            break; // receiver dropped
                        }
                    }
                    Some(Err(_)) => break,
                    None => break,
                }
            }
        });

        Self { stream_rx: rx }
    }

    /// Accept the next inbound stream (one RPC request).
    /// Returns None when the connection is closed.
    pub async fn accept(&mut self) -> Result<Option<Stream>, RpcError> {
        Ok(self.stream_rx.recv().await)
    }
}

/// Read a length-prefixed frame from a tokio-compatible stream.
async fn read_frame<S: tokio::io::AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, RpcError> {
    use tokio::io::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| RpcError::Io(format!("read frame length: {e}")))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 16 * 1024 * 1024 {
        return Err(RpcError::Io("frame too large".into()));
    }

    let mut data = vec![0u8; len];
    stream
        .read_exact(&mut data)
        .await
        .map_err(|e| RpcError::Io(format!("read frame data: {e}")))?;
    Ok(data)
}

/// Write a length-prefixed frame to a tokio-compatible stream.
async fn write_frame<S: tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    data: &[u8],
) -> Result<(), RpcError> {
    use tokio::io::AsyncWriteExt;

    let len = (data.len() as u32).to_be_bytes();
    stream
        .write_all(&len)
        .await
        .map_err(|e| RpcError::Io(format!("write frame length: {e}")))?;
    stream
        .write_all(data)
        .await
        .map_err(|e| RpcError::Io(format!("write frame data: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| RpcError::Io(format!("flush frame: {e}")))?;
    Ok(())
}

/// Helper: read a request from a yamux stream (converts to tokio internally).
pub async fn read_request(stream: Stream) -> Result<(Request, Stream), RpcError> {
    let mut compat = FuturesAsyncReadCompatExt::compat(stream);
    let data = read_frame(&mut compat).await?;
    let req = codec::decode(&data)?;
    Ok((req, compat.into_inner()))
}

/// Helper: write a response to a yamux stream (converts to tokio internally).
pub async fn write_response(stream: Stream, resp: &Response) -> Result<(), RpcError> {
    let mut compat = FuturesAsyncReadCompatExt::compat(stream);
    let data = codec::encode(resp)?;
    write_frame(&mut compat, &data).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, RpcResult};
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_yamux_mux_request_response() {
        let (client_stream, server_stream) = duplex(65536);

        let client = MuxClient::new(client_stream);
        let mut server = MuxServer::new(server_stream);

        let req = Request {
            id: "mux-test-1".into(),
            action: Action::Status,
            timeout_ms: Some(5000),
            reason: None,
        };

        // Server task: accept stream, read request, send response
        let server_handle = tokio::spawn(async move {
            let stream = server.accept().await.unwrap().unwrap();
            let (received, stream) = read_request(stream).await.unwrap();
            assert_eq!(received.id, "mux-test-1");

            let resp = Response {
                id: received.id,
                result: RpcResult::StatusInfo {
                    agent_id: "test-agent".into(),
                    version: "0.1.0".into(),
                    uptime_seconds: 42,
                    region: None,
                },
            };
            write_response(stream, &resp).await.unwrap();
        });

        // Client: send request and get response
        let response = client.request(&req).await.unwrap();
        assert_eq!(response.id, "mux-test-1");
        if let RpcResult::StatusInfo { agent_id, .. } = &response.result {
            assert_eq!(agent_id, "test-agent");
        } else {
            panic!("unexpected result type");
        }

        server_handle.await.unwrap();
    }
}
