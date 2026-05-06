//! Msgpack codec for RPC messages.
//!
//! Wire format: [length: 4 bytes BE] [msgpack payload]

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::RpcError;

/// Maximum RPC message size (8 MB).
const MAX_MSG_SIZE: usize = 8 * 1024 * 1024;

/// Encode a message to msgpack bytes.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, RpcError> {
    rmp_serde::to_vec_named(msg).map_err(|e| RpcError::Codec(e.to_string()))
}

/// Decode a message from msgpack bytes.
pub fn decode<'a, T: Deserialize<'a>>(data: &'a [u8]) -> Result<T, RpcError> {
    rmp_serde::from_slice(data).map_err(|e| RpcError::Codec(e.to_string()))
}

/// Write a length-prefixed msgpack message to a stream.
pub async fn write_msg<T: Serialize, W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &T,
) -> Result<(), RpcError> {
    let data = encode(msg)?;
    if data.len() > MAX_MSG_SIZE {
        return Err(RpcError::MessageTooLarge {
            size: data.len(),
            max: MAX_MSG_SIZE,
        });
    }
    writer
        .write_all(&(data.len() as u32).to_be_bytes())
        .await
        .map_err(|e| RpcError::Io(e.to_string()))?;
    writer
        .write_all(&data)
        .await
        .map_err(|e| RpcError::Io(e.to_string()))?;
    Ok(())
}

/// Read a length-prefixed msgpack message from a stream.
pub async fn read_msg<T: for<'a> Deserialize<'a>, R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<T, RpcError> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| RpcError::Io(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_MSG_SIZE {
        return Err(RpcError::MessageTooLarge {
            size: len,
            max: MAX_MSG_SIZE,
        });
    }

    let mut data = vec![0u8; len];
    reader
        .read_exact(&mut data)
        .await
        .map_err(|e| RpcError::Io(e.to_string()))?;

    decode(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, Request, Response, RpcResult};

    #[test]
    fn test_roundtrip_request() {
        let req = Request {
            id: "test-123".into(),
            action: Action::Execute {
                command: "echo hello".into(),
                env: Default::default(),
                workdir: None,
            },
            timeout_ms: Some(5000),
            reason: None,
        };

        let encoded = encode(&req).unwrap();
        let decoded: Request = decode(&encoded).unwrap();

        assert_eq!(decoded.id, "test-123");
        assert_eq!(decoded.timeout_ms, Some(5000));
        if let Action::Execute { command, .. } = &decoded.action {
            assert_eq!(command, "echo hello");
        } else {
            panic!("wrong action type");
        }
    }

    #[test]
    fn test_roundtrip_response() {
        let resp = Response {
            id: "test-456".into(),
            result: RpcResult::Success {
                stdout: "hello world\n".into(),
                stderr: String::new(),
                exit_code: 0,
                duration_ms: 42,
            },
        };

        let encoded = encode(&resp).unwrap();
        let decoded: Response = decode(&encoded).unwrap();

        assert_eq!(decoded.id, "test-456");
        if let RpcResult::Success {
            stdout, exit_code, ..
        } = &decoded.result
        {
            assert_eq!(stdout, "hello world\n");
            assert_eq!(*exit_code, 0);
        } else {
            panic!("wrong result type");
        }
    }

    #[tokio::test]
    async fn test_stream_write_read() {
        let req = Request {
            id: "stream-test".into(),
            action: Action::Metrics,
            timeout_ms: None,
            reason: None,
        };

        let (mut client, mut server) = tokio::io::duplex(65536);
        write_msg(&mut client, &req).await.unwrap();

        let decoded: Request = read_msg(&mut server).await.unwrap();
        assert_eq!(decoded.id, "stream-test");
    }
}
