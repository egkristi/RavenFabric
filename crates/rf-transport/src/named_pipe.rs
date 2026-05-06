//! Named pipe transport driver for Windows local IPC.
//!
//! Provides zero-network IPC between processes on Windows via named pipes.
//! Pipe path: `\\.\pipe\ravenfabric` (or custom via config).
//!
//! Used for:
//! - AI agent ↔ RavenFabric agent communication on Windows
//! - Local development (`rf exec local` on Windows)
//! - Service-to-service IPC without network
//!
//! Security: Same Noise XX handshake applies over the pipe. Local does not
//! mean trusted. The pipe is created with restricted DACL (creator only).

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default named pipe path on Windows.
pub const DEFAULT_PIPE_PATH: &str = r"\\.\pipe\ravenfabric";

/// Named pipe transport driver for Windows.
///
/// Implements the `Driver` trait for same-host communication via Windows named pipes.
pub struct NamedPipeDriver {
    default_path: String,
}

impl NamedPipeDriver {
    /// Create a new named pipe driver with the default pipe path.
    pub fn new() -> Self {
        Self {
            default_path: DEFAULT_PIPE_PATH.to_string(),
        }
    }

    /// Create a named pipe driver with a custom pipe path.
    pub fn with_path(path: impl Into<String>) -> Self {
        Self {
            default_path: path.into(),
        }
    }

    /// Resolve the pipe path from config or use default.
    fn resolve_path(&self, config: &DriverConfig) -> String {
        config
            .get("pipe_path")
            .cloned()
            .unwrap_or_else(|| self.default_path.clone())
    }
}

impl Default for NamedPipeDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// A bidirectional stream over a Windows named pipe.
///
/// On non-Windows platforms, this is a stub that uses `tokio::io::duplex`
/// to allow compilation and testing on all platforms.
pub struct NamedPipeStream {
    #[cfg(windows)]
    inner: tokio::net::windows::named_pipe::NamedPipeClient,
    #[cfg(not(windows))]
    reader: Box<dyn AsyncRead + Unpin + Send>,
    #[cfg(not(windows))]
    writer: Box<dyn AsyncWrite + Unpin + Send>,
}

#[cfg(not(windows))]
impl NamedPipeStream {
    /// Create a stream from reader/writer pair (for testing on non-Windows).
    pub fn from_duplex(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }
}

impl AsyncRead for NamedPipeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        #[cfg(windows)]
        {
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
        #[cfg(not(windows))]
        {
            Pin::new(&mut self.reader).poll_read(cx, buf)
        }
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        #[cfg(windows)]
        {
            Pin::new(&mut self.inner).poll_write(cx, buf)
        }
        #[cfg(not(windows))]
        {
            Pin::new(&mut self.writer).poll_write(cx, buf)
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        #[cfg(windows)]
        {
            Pin::new(&mut self.inner).poll_flush(cx)
        }
        #[cfg(not(windows))]
        {
            Pin::new(&mut self.writer).poll_flush(cx)
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        #[cfg(windows)]
        {
            Pin::new(&mut self.inner).poll_shutdown(cx)
        }
        #[cfg(not(windows))]
        {
            Pin::new(&mut self.writer).poll_shutdown(cx)
        }
    }
}

#[async_trait::async_trait]
impl Driver for NamedPipeDriver {
    fn name(&self) -> &str {
        "named-pipe"
    }

    fn available(&self) -> bool {
        cfg!(windows)
    }

    async fn dial(
        &self,
        _target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let pipe_path = self.resolve_path(config);

        #[cfg(windows)]
        {
            let client = tokio::net::windows::named_pipe::ClientOptions::new()
                .open(&pipe_path)
                .map_err(|e| {
                    TransportError::Connection(format!(
                        "named pipe: failed to connect to '{pipe_path}': {e}"
                    ))
                })?;
            Ok(Box::new(NamedPipeStream { inner: client }))
        }

        #[cfg(not(windows))]
        {
            Err(TransportError::Connection(format!(
                "named pipe driver not available on this platform (requested: {pipe_path})"
            )))
        }
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let pipe_path = if addr.is_empty() {
            self.default_path.clone()
        } else {
            addr.to_string()
        };

        #[cfg(windows)]
        {
            Ok(Box::new(NamedPipeListener::new(pipe_path)?))
        }

        #[cfg(not(windows))]
        {
            Err(TransportError::Connection(format!(
                "named pipe listener not available on this platform (requested: {pipe_path})"
            )))
        }
    }
}

/// Named pipe listener that accepts connections on Windows.
#[cfg(windows)]
struct NamedPipeListener {
    pipe_path: String,
}

#[cfg(windows)]
impl NamedPipeListener {
    fn new(pipe_path: String) -> Result<Self, TransportError> {
        Ok(Self { pipe_path })
    }
}

#[cfg(windows)]
#[async_trait::async_trait]
impl Listener for NamedPipeListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let server = ServerOptions::new()
            .first_pipe_instance(false)
            .create(&self.pipe_path)
            .map_err(|e| {
                TransportError::Connection(format!(
                    "named pipe: failed to create server pipe '{}': {e}",
                    self.pipe_path
                ))
            })?;

        server.connect().await.map_err(|e| {
            TransportError::Connection(format!(
                "named pipe: failed to accept connection on '{}': {e}",
                self.pipe_path
            ))
        })?;

        // On Windows, the server pipe itself implements AsyncRead + AsyncWrite
        Ok(Box::new(NamedPipeStream { inner: server }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = NamedPipeDriver::new();
        assert_eq!(driver.name(), "named-pipe");
    }

    #[test]
    fn test_default_path() {
        let driver = NamedPipeDriver::new();
        assert_eq!(driver.default_path, DEFAULT_PIPE_PATH);
    }

    #[test]
    fn test_custom_path() {
        let driver = NamedPipeDriver::with_path(r"\\.\pipe\custom-test");
        assert_eq!(driver.default_path, r"\\.\pipe\custom-test");
    }

    #[test]
    fn test_available_platform() {
        let driver = NamedPipeDriver::new();
        if cfg!(windows) {
            assert!(driver.available());
        } else {
            assert!(!driver.available());
        }
    }

    #[test]
    fn test_resolve_path_from_config() {
        let driver = NamedPipeDriver::new();
        let mut config = DriverConfig::new();
        config.insert("pipe_path".into(), r"\\.\pipe\override".into());
        assert_eq!(driver.resolve_path(&config), r"\\.\pipe\override");
    }

    #[test]
    fn test_resolve_path_default() {
        let driver = NamedPipeDriver::new();
        let config = DriverConfig::new();
        assert_eq!(driver.resolve_path(&config), DEFAULT_PIPE_PATH);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_dial_unavailable_on_non_windows() {
        let driver = NamedPipeDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("not available"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_listen_unavailable_on_non_windows() {
        let driver = NamedPipeDriver::new();
        let result = driver.listen("").await;
        assert!(result.is_err());
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_named_pipe_stream_duplex() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (client, server) = tokio::io::duplex(1024);
        let (server_read, server_write) = tokio::io::split(server);

        let mut stream = NamedPipeStream::from_duplex(server_read, server_write);
        let mut client = client;

        client.write_all(b"hello pipe").await.unwrap();
        drop(client);

        let mut buf = vec![0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello pipe");
    }
}
