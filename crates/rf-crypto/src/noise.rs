use snow::{Builder, Error as SnowError, HandshakeState, StatelessTransportState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{Duration, timeout};
use tracing::{debug, trace, warn};

use crate::error::CryptoError;
use crate::keys::StaticKey;

/// Timeout for the complete Noise XX handshake (3 messages each direction).
/// Prevents hanging indefinitely on relay connections with cross-platform issues.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Noise protocol pattern used throughout RavenFabric.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Maximum plaintext payload per frame (before encryption).
/// snow's MAXMSGLEN = 65535 includes the 16-byte MAC tag, so plaintext is capped at 65519.
pub const MAX_FRAME_PAYLOAD: usize = 65535 - FRAME_OVERHEAD; // 65519

/// Overhead per encrypted frame (16-byte MAC).
pub const FRAME_OVERHEAD: usize = 16;

/// Wire protocol magic bytes.
pub const WIRE_MAGIC: &[u8; 4] = b"RVNF";

/// Current wire protocol version.
pub const WIRE_VERSION: u8 = 1;

/// Perform Noise XX handshake over a raw transport stream.
///
/// Sends wire magic + version before the Noise handshake begins.
/// Returns the negotiated `StatelessTransportState` (for SecureChannel) and the peer's static public key.
///
/// When `compat_mode` is true, adds a small yield between handshake messages
/// to work around cross-platform relay timing issues (macOS→Linux via snow-0.10.0).
pub async fn handshake<T>(
    transport: &mut T,
    is_initiator: bool,
    static_key: &StaticKey,
) -> Result<(StatelessTransportState, [u8; 32]), CryptoError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    handshake_with_compat(transport, is_initiator, static_key, false).await
}

/// Internal handshake with optional compat mode for cross-platform relay issues.
pub async fn handshake_with_compat<T>(
    transport: &mut T,
    is_initiator: bool,
    static_key: &StaticKey,
    compat_mode: bool,
) -> Result<(StatelessTransportState, [u8; 32]), CryptoError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    // Wire protocol: send magic + version
    transport
        .write_all(WIRE_MAGIC)
        .await
        .map_err(|_| CryptoError::Disconnected)?;
    transport
        .write_all(&[WIRE_VERSION])
        .await
        .map_err(|_| CryptoError::Disconnected)?;

    // Wire protocol: receive and validate magic + version
    let mut magic = [0u8; 4];
    transport
        .read_exact(&mut magic)
        .await
        .map_err(|_| CryptoError::Disconnected)?;
    if &magic != WIRE_MAGIC {
        return Err(CryptoError::Handshake(format!(
            "invalid wire magic: expected RVNF, got {magic:?}"
        )));
    }

    let mut version = [0u8; 1];
    transport
        .read_exact(&mut version)
        .await
        .map_err(|_| CryptoError::Disconnected)?;
    if version[0] != WIRE_VERSION {
        return Err(CryptoError::Handshake(format!(
            "unsupported wire version: {}",
            version[0]
        )));
    }

    let builder = Builder::new(
        NOISE_PATTERN
            .parse()
            .expect("static noise pattern is always valid"),
    )
    .local_private_key(static_key.private_bytes())
    .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    let mut noise = if is_initiator {
        builder.build_initiator()
    } else {
        builder.build_responder()
    }
    .map_err(|e| CryptoError::Handshake(e.to_string()))?;

    // Use a larger buffer for the handshake to accommodate any platform-specific
    // message size variations (snow-0.10.0 macOS→Linux relay issue).
    // The +256 accounts for potential Noise XX message expansion on different
    // platform curve25519 implementations.
    let mut buf = vec![0u8; 65535 + 256];

    // Wrap the entire handshake in a timeout to prevent hanging on
    // cross-platform relay connections (snow-0.10.0 macOS→Linux issue).
    let handshake_result = timeout(HANDSHAKE_TIMEOUT, async {
        if is_initiator {
            // → msg1: e
            send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
            if compat_mode {
                tokio::task::yield_now().await;
            }
            // ← msg2: e, ee, s, es
            recv_handshake_msg(transport, &mut noise, &mut buf).await?;
            if compat_mode {
                tokio::task::yield_now().await;
            }
            // → msg3: s, se
            send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
        } else {
            // ← msg1: e
            recv_handshake_msg(transport, &mut noise, &mut buf).await?;
            if compat_mode {
                tokio::task::yield_now().await;
            }
            // → msg2: e, ee, s, es
            send_handshake_msg(transport, &mut noise, &[], &mut buf).await?;
            if compat_mode {
                tokio::task::yield_now().await;
            }
            // ← msg3: s, se
            recv_handshake_msg(transport, &mut noise, &mut buf).await?;
        }
        Ok::<_, CryptoError>(())
    })
    .await;

    match handshake_result {
        Ok(Ok(())) => debug!("Noise XX handshake complete"),
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            warn!(
                "Noise XX handshake timed out after {}s — possible cross-platform relay issue",
                HANDSHAKE_TIMEOUT.as_secs()
            );
            return Err(CryptoError::HandshakeInput(
                "handshake timed out — relay may have cross-platform compatibility issue".into(),
            ));
        }
    }

    let peer_key = noise
        .get_remote_static()
        .ok_or_else(|| CryptoError::Handshake("no remote static key".into()))?;

    let mut peer_key_arr = [0u8; 32];
    peer_key_arr.copy_from_slice(peer_key);

    let transport_state = noise
        .into_stateless_transport_mode()
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
    let len = noise.write_message(payload, buf).map_err(|e| {
        if matches!(e, SnowError::Input) {
            warn!(
                "Noise write_message Error::Input — buf={}, payload={}, finished={}, my_turn={}",
                buf.len(),
                payload.len(),
                noise.is_handshake_finished(),
                noise.is_my_turn(),
            );
            CryptoError::HandshakeInput(format!(
                "write_message buffer too small: buf={}, payload={}",
                buf.len(),
                payload.len(),
            ))
        } else {
            CryptoError::Handshake(e.to_string())
        }
    })?;

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

    // Use a payload buffer sized to the maximum possible decrypted output:
    // the ciphertext length plus the MAC overhead (16 bytes per encrypted token).
    // snow 0.10.0 can return Error::Input if the output buffer is too small.
    let mut payload_buf = vec![0u8; 65535 + 256];
    noise
        .read_message(&buf[..len], &mut payload_buf)
        .map_err(|e| {
            if matches!(e, SnowError::Input) {
                warn!(
                    "Noise read_message Error::Input — msg_len={}, buf={}, finished={}, my_turn={}",
                    len,
                    buf.len(),
                    noise.is_handshake_finished(),
                    noise.is_my_turn(),
                );
                CryptoError::HandshakeInput(format!(
                    "read_message buffer too small: msg_len={}, buf={}",
                    len,
                    buf.len(),
                ))
            } else {
                CryptoError::Handshake(e.to_string())
            }
        })?;

    trace!("recv handshake msg: {} bytes", len);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::SecureChannel;
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

    #[tokio::test]
    async fn test_handshake_rejects_bad_magic() {
        let key_a = StaticKey::generate();

        let (mut client, mut server) = duplex(65536);

        // Write bad magic from "server" side
        let bad_side = async move {
            server.write_all(b"BAAD").await.unwrap();
            server.write_all(&[WIRE_VERSION]).await.unwrap();
            // Read client's magic+version
            let mut buf = [0u8; 5];
            server.read_exact(&mut buf).await.unwrap();
        };

        let client_side = handshake(&mut client, true, &key_a);

        let (_, result) = tokio::join!(bad_side, client_side);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid wire magic"));
    }

    #[tokio::test]
    async fn test_secure_channel_roundtrip() {
        let key_a = StaticKey::generate();
        let key_b = StaticKey::generate();

        let (mut client, mut server) = duplex(65536);

        let (result_a, result_b) = tokio::join!(
            handshake(&mut client, true, &key_a),
            handshake(&mut server, false, &key_b),
        );

        let (state_a, peer_a) = result_a.unwrap();
        let (state_b, peer_b) = result_b.unwrap();

        // Split the duplex into read/write halves for each channel
        let (client_read, client_write) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::duplex(65536);

        // Channel A writes to server_write, reads from client_read
        // Channel B writes to client_write, reads from server_read
        let chan_a = SecureChannel::new(server_read, client_write, state_a, peer_a);
        let chan_b = SecureChannel::new(client_read, server_write, state_b, peer_b);

        // A sends to B
        chan_a.send(b"hello from A").await.unwrap();
        let received = chan_b.recv().await.unwrap();
        assert_eq!(received, b"hello from A");

        // B sends to A
        chan_b.send(b"hello from B").await.unwrap();
        let received = chan_a.recv().await.unwrap();
        assert_eq!(received, b"hello from B");
    }

    #[tokio::test]
    async fn test_close_notify() {
        let key_a = StaticKey::generate();
        let key_b = StaticKey::generate();

        let (mut client, mut server) = duplex(65536);

        let (result_a, result_b) = tokio::join!(
            handshake(&mut client, true, &key_a),
            handshake(&mut server, false, &key_b),
        );

        let (state_a, peer_a) = result_a.unwrap();
        let (state_b, peer_b) = result_b.unwrap();

        let (client_read, client_write) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::duplex(65536);

        let chan_a = SecureChannel::new(server_read, client_write, state_a, peer_a);
        let chan_b = SecureChannel::new(client_read, server_write, state_b, peer_b);

        // A sends close-notify
        chan_a.close_notify().await.unwrap();

        // B receives empty payload (the close-notify signal)
        let received = chan_b.recv().await.unwrap();
        assert!(
            received.is_empty(),
            "close-notify should produce empty payload"
        );
    }
}
