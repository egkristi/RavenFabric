//! RPC session over an encrypted SecureChannel.
//!
//! Provides request/response semantics over the frame-based SecureChannel.
//! Each frame carries one complete msgpack-encoded RPC message.

use rf_crypto::channel::SecureChannel;
use rf_crypto::error::CryptoError;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::codec;
use crate::error::RpcError;
use crate::types::{Request, Response};

/// An RPC session over a SecureChannel.
///
/// Client-side: call `request()` to send a request and await the response.
/// Server-side: call `recv_request()` then `send_response()`.
pub struct RpcSession<R, W> {
    channel: SecureChannel<R, W>,
}

impl<R, W> RpcSession<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Create a new RPC session from an established SecureChannel.
    pub fn new(channel: SecureChannel<R, W>) -> Self {
        Self { channel }
    }

    /// Get the peer's public key.
    pub fn peer_key(&self) -> &[u8; 32] {
        self.channel.peer_key()
    }

    /// Client: send a request and wait for the response.
    pub async fn request(&self, req: &Request) -> Result<Response, RpcError> {
        let data = codec::encode(req)?;
        self.channel.send(&data).await.map_err(crypto_to_rpc)?;

        let resp_data = self.channel.recv().await.map_err(crypto_to_rpc)?;
        codec::decode(&resp_data)
    }

    /// Server: receive the next incoming request.
    pub async fn recv_request(&self) -> Result<Request, RpcError> {
        let data = self.channel.recv().await.map_err(crypto_to_rpc)?;
        codec::decode(&data)
    }

    /// Server: send a response back to the client.
    pub async fn send_response(&self, resp: &Response) -> Result<(), RpcError> {
        let data = codec::encode(resp)?;
        self.channel.send(&data).await.map_err(crypto_to_rpc)
    }
}

fn crypto_to_rpc(e: CryptoError) -> RpcError {
    match e {
        CryptoError::Disconnected => RpcError::SessionClosed,
        other => RpcError::Io(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, RpcResult};
    use rf_crypto::keys::StaticKey;
    use rf_crypto::noise::handshake;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_rpc_session_request_response() {
        let key_a = StaticKey::generate();
        let key_b = StaticKey::generate();

        // Handshake
        let (mut stream_a, mut stream_b) = duplex(65536);
        let (result_a, result_b) = tokio::join!(
            handshake(&mut stream_a, true, &key_a),
            handshake(&mut stream_b, false, &key_b),
        );

        let (state_a, peer_a) = result_a.unwrap();
        let (state_b, peer_b) = result_b.unwrap();

        // Create channels over separate duplex pairs
        let (a_read, b_write) = duplex(65536);
        let (b_read, a_write) = duplex(65536);

        let chan_a = SecureChannel::new(a_read, a_write, state_a, peer_a);
        let chan_b = SecureChannel::new(b_read, b_write, state_b, peer_b);

        let session_client = RpcSession::new(chan_a);
        let session_server = RpcSession::new(chan_b);

        // Run client request and server handler concurrently
        let req = Request {
            id: "test-1".into(),
            action: Action::Execute {
                command: "echo hello".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: Some(5000),
            reason: None,
        };

        let server_handle = tokio::spawn(async move {
            let received = session_server.recv_request().await.unwrap();
            assert_eq!(received.id, "test-1");

            let resp = Response {
                id: received.id,
                result: RpcResult::Success {
                    stdout: "hello\n".into(),
                    stderr: String::new(),
                    exit_code: 0,
                    duration_ms: 10,
                },
            };
            session_server.send_response(&resp).await.unwrap();
        });

        let response = session_client.request(&req).await.unwrap();
        assert_eq!(response.id, "test-1");
        if let RpcResult::Success { stdout, .. } = &response.result {
            assert_eq!(stdout, "hello\n");
        } else {
            panic!("expected success");
        }

        server_handle.await.unwrap();
    }
}
