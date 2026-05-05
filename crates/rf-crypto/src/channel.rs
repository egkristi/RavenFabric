use snow::TransportState;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::error::CryptoError;
use crate::noise::MAX_FRAME_PAYLOAD;

/// An established encrypted channel after Noise XX handshake.
///
/// Provides send/recv of encrypted frames over the underlying transport.
/// Each direction has independent encryption state (nonces).
///
/// Thread-safe: send and recv can be called concurrently from different tasks.
pub struct SecureChannel<T> {
    reader: Mutex<ChannelReader<T>>,
    writer: Mutex<ChannelWriter<T>>,
    peer_key: [u8; 32],
}

struct ChannelReader<T> {
    transport: T,
    state: TransportState,
    buf: Vec<u8>,
}

struct ChannelWriter<T> {
    transport: T,
    state: TransportState,
    buf: Vec<u8>,
}

impl<T> SecureChannel<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Create a new SecureChannel from a completed handshake.
    ///
    /// The transport is split into two halves for concurrent read/write.
    /// `transport_state` comes from `noise::handshake()`.
    pub fn new(
        read_half: T,
        write_half: T,
        read_state: TransportState,
        write_state: TransportState,
        peer_key: [u8; 32],
    ) -> Self {
        Self {
            reader: Mutex::new(ChannelReader {
                transport: read_half,
                state: read_state,
                buf: vec![0u8; MAX_FRAME_PAYLOAD + 16 + 4],
            }),
            writer: Mutex::new(ChannelWriter {
                transport: write_half,
                state: write_state,
                buf: vec![0u8; MAX_FRAME_PAYLOAD + 16 + 4],
            }),
            peer_key,
        }
    }

    /// Remote party's verified Noise static public key.
    pub fn peer_key(&self) -> &[u8; 32] {
        &self.peer_key
    }

    /// Send an encrypted frame.
    ///
    /// Wire format: [length: 4 bytes big-endian] [ciphertext + 16-byte MAC]
    /// Maximum plaintext: 65535 bytes.
    pub async fn send(&self, plaintext: &[u8]) -> Result<(), CryptoError> {
        if plaintext.len() > MAX_FRAME_PAYLOAD {
            return Err(CryptoError::FrameTooLarge {
                size: plaintext.len(),
                max: MAX_FRAME_PAYLOAD,
            });
        }

        let mut writer = self.writer.lock().await;

        let ChannelWriter {
            transport,
            state,
            buf,
        } = &mut *writer;

        let len = state
            .write_message(plaintext, buf)
            .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

        transport
            .write_all(&(len as u32).to_be_bytes())
            .await
            .map_err(|_| CryptoError::Disconnected)?;
        transport
            .write_all(&buf[..len])
            .await
            .map_err(|_| CryptoError::Disconnected)?;

        Ok(())
    }

    /// Receive and decrypt one frame.
    ///
    /// Returns the decrypted plaintext.
    pub async fn recv(&self) -> Result<Vec<u8>, CryptoError> {
        let mut reader = self.reader.lock().await;

        let mut len_buf = [0u8; 4];
        reader
            .transport
            .read_exact(&mut len_buf)
            .await
            .map_err(|_| CryptoError::Disconnected)?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > reader.buf.len() {
            return Err(CryptoError::FrameTooLarge {
                size: len,
                max: reader.buf.len(),
            });
        }

        let ChannelReader {
            transport,
            state,
            buf,
        } = &mut *reader;

        transport
            .read_exact(&mut buf[..len])
            .await
            .map_err(|_| CryptoError::Disconnected)?;

        let mut plaintext = vec![0u8; MAX_FRAME_PAYLOAD];
        let plaintext_len = state
            .read_message(&buf[..len], &mut plaintext)
            .map_err(|e| CryptoError::Decrypt(e.to_string()))?;

        plaintext.truncate(plaintext_len);
        Ok(plaintext)
    }
}
