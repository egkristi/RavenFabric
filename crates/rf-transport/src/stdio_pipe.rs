//! Stdio pipe transport driver for parent-child process communication.
//!
//! Provides a transport over stdin/stdout for scenarios where the agent
//! runs as a child process. Used for:
//! - MCP stdio transport (Claude Desktop, IDE extensions)
//! - Embedded agents spawned by a parent process
//! - Subprocess-based isolation (policy sandbox)
//!
//! The driver uses stdin for reading and stdout for writing, making it
//! compatible with any process launcher that pipes stdio.
//!
//! Security: Same Noise XX handshake applies over stdio. The pipe is just
//! a byte transport — authentication and encryption happen above.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// A bidirectional stream built from a pair of unidirectional channels.
///
/// Combines a reader (e.g., child stdout or parent stdin) with a writer
/// (e.g., child stdin or parent stdout) into a single `AsyncStream`.
pub struct StdioPipe {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
}

impl StdioPipe {
    /// Create a new stdio pipe from separate reader and writer.
    pub fn new(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }

    /// Create a pipe from process stdin/stdout (parent side).
    ///
    /// Reader = child's stdout (what the child writes, parent reads)
    /// Writer = child's stdin (what parent writes, child reads)
    pub fn from_child(stdout: ChildStdout, stdin: ChildStdin) -> Self {
        Self {
            reader: Box::new(stdout),
            writer: Box::new(stdin),
        }
    }

    /// Create a pipe from the current process's stdin/stdout (child side).
    ///
    /// Reader = own stdin (what the parent writes)
    /// Writer = own stdout (what this process sends to parent)
    pub fn from_own_stdio() -> Self {
        Self {
            reader: Box::new(io::stdin()),
            writer: Box::new(io::stdout()),
        }
    }
}

impl AsyncRead for StdioPipe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for StdioPipe {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

/// Stdio transport driver.
///
/// On `dial()`, spawns a child process and returns a pipe over its stdin/stdout.
/// On `listen()`, wraps the current process's stdin/stdout as a listener that
/// yields a single connection (since stdio is inherently single-session).
pub struct StdioDriver {
    /// The command to spawn when dialing (e.g., "rf-agent --stdio").
    command: Option<String>,
}

impl StdioDriver {
    /// Create a driver that will use the current process's stdio (child/server mode).
    pub fn new() -> Self {
        Self { command: None }
    }

    /// Create a driver that spawns a subprocess on dial (parent/client mode).
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: Some(command.into()),
        }
    }

    /// Spawn a child process and return the pipe + child handle.
    async fn spawn_child(&self, cmd: &str) -> Result<(StdioPipe, Child), TransportError> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(TransportError::Connection(
                "stdio driver: empty command".into(),
            ));
        }

        let mut command = Command::new(parts[0]);
        if parts.len() > 1 {
            command.args(&parts[1..]);
        }

        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        let mut child = command.spawn().map_err(|e| {
            TransportError::Connection(format!("stdio driver: failed to spawn '{cmd}': {e}"))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            TransportError::Connection("stdio driver: child stdin not available".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            TransportError::Connection("stdio driver: child stdout not available".into())
        })?;

        Ok((StdioPipe::from_child(stdout, stdin), child))
    }
}

impl Default for StdioDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for StdioDriver {
    fn name(&self) -> &str {
        "stdio"
    }

    fn available(&self) -> bool {
        true
    }

    async fn dial(
        &self,
        _target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Command can come from config or the driver's preset command
        let cmd = config
            .get("stdio_command")
            .map(|s| s.as_str())
            .or(self.command.as_deref())
            .ok_or_else(|| {
                TransportError::Connection(
                    "stdio driver: no command configured (set 'stdio_command' in config or use StdioDriver::with_command)".into(),
                )
            })?;

        let (pipe, _child) = self.spawn_child(cmd).await?;
        // Note: child handle is intentionally leaked here — it will be cleaned up
        // when the pipe is dropped and the child's stdin closes, causing it to exit.
        // For production use, wrap in a ChildGuard that kills on drop.
        Ok(Box::new(pipe))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        Ok(Box::new(StdioListener::new()))
    }
}

/// A listener that yields exactly one connection from the current process's stdio.
///
/// Stdio is inherently single-session: once accepted, subsequent calls return an error.
struct StdioListener {
    accepted: std::sync::atomic::AtomicBool,
}

impl StdioListener {
    fn new() -> Self {
        Self {
            accepted: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl Listener for StdioListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Only one connection per stdio session
        if self
            .accepted
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(TransportError::Connection(
                "stdio listener: already accepted (stdio is single-session)".into(),
            ));
        }

        Ok(Box::new(StdioPipe::from_own_stdio()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_stdio_driver_name() {
        let driver = StdioDriver::new();
        assert_eq!(driver.name(), "stdio");
    }

    #[test]
    fn test_stdio_driver_available() {
        let driver = StdioDriver::new();
        assert!(driver.available());
    }

    #[tokio::test]
    async fn test_stdio_pipe_from_duplex() {
        // Use tokio duplex as a simulated pipe pair
        let (client_stream, server_stream) = tokio::io::duplex(1024);
        let (server_read, server_write) = tokio::io::split(server_stream);
        let (client_read, client_write) = tokio::io::split(client_stream);

        let mut server_pipe = StdioPipe::new(server_read, server_write);
        let mut client_pipe = StdioPipe::new(client_read, client_write);

        // Client writes, server reads
        client_pipe.write_all(b"hello from client").await.unwrap();
        let mut buf = [0u8; 64];
        let n = server_pipe.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello from client");

        // Server writes, client reads
        server_pipe.write_all(b"hello from server").await.unwrap();
        let n = client_pipe.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello from server");
    }

    #[tokio::test]
    async fn test_stdio_driver_dial_spawns_process() {
        // Use 'echo' as a simple child process
        let driver = StdioDriver::with_command("echo hello");
        let config = HashMap::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let result = driver.dial(&target, &config).await;
        assert!(result.is_ok(), "Should spawn echo successfully");

        let mut stream = result.unwrap();
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        // echo outputs "hello\n"
        assert_eq!(&buf[..n], b"hello\n");
    }

    #[tokio::test]
    async fn test_stdio_driver_dial_with_config_override() {
        let driver = StdioDriver::new();
        let mut config = HashMap::new();
        config.insert("stdio_command".to_string(), "echo override".to_string());
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let result = driver.dial(&target, &config).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"override\n");
    }

    #[tokio::test]
    async fn test_stdio_driver_dial_no_command_fails() {
        let driver = StdioDriver::new();
        let config = HashMap::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("no command configured"));
    }

    #[tokio::test]
    async fn test_stdio_driver_dial_invalid_command_fails() {
        let driver = StdioDriver::with_command("__nonexistent_binary_that_does_not_exist_12345__");
        let config = HashMap::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("failed to spawn"));
    }

    #[tokio::test]
    async fn test_stdio_listener_single_session() {
        let listener = StdioListener::new();
        // First accept should work (but we can't actually use real stdin in tests,
        // so we just verify the second call fails)
        assert!(!listener.accepted.load(std::sync::atomic::Ordering::SeqCst));
        listener
            .accepted
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let result = listener.accept().await;
        assert!(result.is_err());
        let err = result.err().unwrap().to_string();
        assert!(err.contains("already accepted"));
    }

    #[tokio::test]
    async fn test_stdio_bidirectional_with_cat() {
        // Use 'cat' as an echo server (reads stdin, writes to stdout)
        let driver = StdioDriver::with_command("cat");
        let config = HashMap::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };

        let mut stream = driver.dial(&target, &config).await.unwrap();

        // Write to cat's stdin
        stream.write_all(b"ping").await.unwrap();
        stream.flush().await.unwrap();

        // Read back from cat's stdout
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");
    }

    use std::collections::HashMap;
}
