use std::sync::Arc;

use snow::StatelessTransportState;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use crate::error::CryptoError;
use crate::noise::MAX_FRAME_PAYLOAD;

/// An established encrypted channel after Noise XX handshake.
///
/// Provides send/recv of encrypted frames over the underlying transport.
/// Each direction has independent nonce counters for encryption/decryption.
///
/// Thread-safe: send and recv can be called concurrently from different tasks.
pub struct SecureChannel<R, W> {
    reader: Mutex<ChannelReader<R>>,
    writer: Mutex<ChannelWriter<W>>,
    peer_key: [u8; 32],
}

struct ChannelReader<R> {
    transport: R,
    state: Arc<StatelessTransportState>,
    nonce: u64,
    buf: Vec<u8>,
}

struct ChannelWriter<W> {
    transport: W,
    state: Arc<StatelessTransportState>,
    nonce: u64,
    buf: Vec<u8>,
}

impl<R, W> SecureChannel<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Create a new SecureChannel from a completed handshake.
    ///
    /// Takes separate read and write halves for concurrent I/O.
    /// `state` is the `StatelessTransportState` from `noise::handshake()`.
    pub fn new(
        read_half: R,
        write_half: W,
        state: StatelessTransportState,
        peer_key: [u8; 32],
    ) -> Self {
        let state = Arc::new(state);
        Self {
            reader: Mutex::new(ChannelReader {
                transport: read_half,
                state: Arc::clone(&state),
                nonce: 0,
                buf: vec![0u8; MAX_FRAME_PAYLOAD + 16 + 4],
            }),
            writer: Mutex::new(ChannelWriter {
                transport: write_half,
                state,
                nonce: 0,
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
            nonce,
            buf,
        } = &mut *writer;

        let len = state
            .write_message(*nonce, plaintext, buf)
            .map_err(|e| CryptoError::Encrypt(e.to_string()))?;
        *nonce += 1;

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

        // Minimum valid frame is 16 bytes (empty plaintext + 16-byte MAC/tag).
        // Anything smaller is frame injection / protocol violation.
        if len < 16 {
            return Err(CryptoError::FrameInjection);
        }

        if len > reader.buf.len() {
            return Err(CryptoError::FrameTooLarge {
                size: len,
                max: reader.buf.len(),
            });
        }

        let ChannelReader {
            transport,
            state,
            nonce,
            buf,
        } = &mut *reader;

        transport
            .read_exact(&mut buf[..len])
            .await
            .map_err(|_| CryptoError::Disconnected)?;

        let mut plaintext = vec![0u8; MAX_FRAME_PAYLOAD];
        let plaintext_len = state
            .read_message(*nonce, &buf[..len], &mut plaintext)
            .map_err(|e| {
                let msg = e.to_string();
                // snow returns "Decrypt" error when MAC verification fails
                if msg.contains("Decrypt") || msg.contains("decrypt") || msg.contains("AEAD") {
                    CryptoError::TamperDetected
                } else {
                    CryptoError::Decrypt(msg)
                }
            })?;
        *nonce += 1;

        plaintext.truncate(plaintext_len);
        Ok(plaintext)
    }

    /// Send a close-notify frame and flush the transport.
    ///
    /// A close-notify is an encrypted empty payload (0 bytes plaintext).
    /// After calling this, the channel should not be used for sending.
    /// The peer will receive a zero-length decrypted result from `recv()`.
    pub async fn close_notify(&self) -> Result<(), CryptoError> {
        self.send(&[]).await?;
        self.flush().await
    }

    /// Flush the underlying transport writer.
    ///
    /// Ensures all buffered data is actually transmitted. This is important
    /// before dropping the channel after sending the last data frame, as
    /// some transport layers (e.g., WebSocket, TLS) may buffer writes.
    pub async fn flush(&self) -> Result<(), CryptoError> {
        let mut writer = self.writer.lock().await;
        writer
            .transport
            .flush()
            .await
            .map_err(|_| CryptoError::Disconnected)
    }
}
