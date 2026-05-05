use snow::{Builder, HandshakeState, TransportState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace};

use crate::error::CryptoError;
use crate::keys::StaticKey;

/// Noise protocol pattern used throughout RavenFabric.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Maximum plaintext payload per frame (before encryption).
pub const MAX_FRAME_PAYLOAD: usize = 65535;

/// Overhead per encrypted frame (16-byte MAC).
pub const FRAME_OVERHEAD: usize = 16;

/// Wire protocol magic bytes.
pub const WIRE_MAGIC: &[u8; 4] = b"RVNF";

/// Current wire protocol version.
pub const WIRE_VERSION: u8 = 1;

/// Perform Noise XX handshake over a raw transport stream.
///
/// Returns the negotiated TransportState (for SecureChannel) and the peer's static public key.
pub async fn handshake<T>(
    transport: &mut T,
    is_initiator: bool,
    static_key: &StaticKey,
) -> Result<(TransportState, [u8; 32]), CryptoError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let builder = Builder::new(NOISE_PATTERN.parse().unwrap())
        .local_private_key(static_key.private_bytes());

    let mut noise = if is_initiator {
        builder.build_initiator()
    } else {
        builder.build_responder()
    }
    .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    let mut buf = vec![0u8; 65535];

    if is_initiator {
        // → msg1: e
        send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
        // ← msg2: e, ee, s, es
        recv_handshake_msg(transport, &mut noise, &mut buf).await?;
        // → msg3: s, se
        send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
    } else {
        // ← msg1: e
        recv_handshake_msg(transport, &mut noise, &mut buf).await?;
        // → msg2: e, ee, s, es
        send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
        // ← msg3: s, se
        recv_handshake_msg(transport, &mut noise, &mut buf).await?;
    }

    debug!("Noise XX handshake complete");

    let peer_key = noise
        .get_remote_static()
        .ok_or_else(|| CryptoError::Handshake("no remote static key".into()))?;

    let mut peer_key_arr = [0u8; 32];
    peer_key_arr.copy_from_slice(peer_key);

    let transport_state = noise
        .into_transport_mode()
        .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    Ok((transport_state, peer_key_arr))
}

/// Send a handshake message (length-prefixed).
async fn send_handshake_msg<T>(
    transport: &mut T,
    noise: &mut HandshakeState,
    payload: &[u8],
    buf: &mut [u8],
) -> Result<(), CryptoError>
where
    T: AsyncWrite + Unpin,
{
    let len = noise
        .write_message(payload, buf)
        .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    transport
        .write_all(&(len as u16).to_be_bytes())
        .await
        .map_err(|_| CryptoError::Disconnected)?;
    transport
        .write_all(&buf[..len])
        .await
        .map_err(|_| CryptoError::Disconnected)?;

    trace!("sent handshake msg: {} bytes", len);
    Ok(())
}

/// Receive a handshake message (length-prefixed).
async fn recv_handshake_msg<T>(
    transport: &mut T,
    noise: &mut HandshakeState,
    buf: &mut [u8],
) -> Result<(), CryptoError>
where
    T: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 2];
    transport
        .read_exact(&mut len_buf)
        .await
        .map_err(|_| CryptoError::Disconnected)?;
    let len = u16::from_be_bytes(len_buf) as usize;

    if len > buf.len() {
        return Err(CryptoError::FrameTooLarge {
            size: len,
            max: buf.len(),
        });
    }

    transport
        .read_exact(&mut buf[..len])
        .await
        .map_err(|_| CryptoError::Disconnected)?;

    let mut payload_buf = vec![0u8; 65535];
    noise
        .read_message(&buf[..len], &mut payload_buf)
        .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    trace!("recv handshake msg: {} bytes", len);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn test_handshake_succeeds() {
        let key_a = StaticKey::generate();
        let key_b = StaticKey::generate();

        let (mut client, mut server) = duplex(65536);

        let (result_a, result_b) = tokio::join!(
            handshake(&mut client, true, &key_a),
            handshake(&mut server, false, &key_b),
        );

        let (_, peer_key_a) = result_a.unwrap();
        let (_, peer_key_b) = result_b.unwrap();

        // Each side sees the other's public key
        assert_eq!(peer_key_a, key_b.public);
        assert_eq!(peer_key_b, key_a.public);
    }
}
