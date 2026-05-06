//! Abstract namespace Unix socket driver.
//!
//! Linux abstract namespace sockets live in kernel memory (not the filesystem),
//! so they disappear automatically when the last reference closes.
//! No file cleanup needed, no permission issues with socket files.
//!
//! # Platform Support
//!
//! - Linux only (abstract namespace is Linux-specific)
//! - On other platforms, dial/listen returns an error
//!
//! # Addressing
//!
//! Abstract sockets use a NUL byte prefix: `\0ravenfabric`
//! The default name is `\0ravenfabric-agent`.

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default abstract socket name (without the NUL prefix).
pub const DEFAULT_ABSTRACT_NAME: &str = "ravenfabric-agent";

/// Abstract namespace socket driver.
pub struct AbstractNsDriver {
    /// Socket name (NUL prefix added automatically).
    name: String,
}

impl AbstractNsDriver {
    /// Create with default socket name.
    pub fn new() -> Self {
        Self {
            name: DEFAULT_ABSTRACT_NAME.into(),
        }
    }

    /// Create with a custom socket name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    fn resolve_name(&self, config: &DriverConfig) -> String {
        config
            .get("abstract_name")
            .cloned()
            .unwrap_or_else(|| self.name.clone())
    }
}

impl Default for AbstractNsDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// Bidirectional stream over an abstract namespace socket.
pub struct AbstractNsStream {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
}

impl AbstractNsStream {
    /// Create from reader/writer (for testing).
    pub fn from_parts(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }

    /// Create from a tokio duplex (for testing).
    #[cfg(test)]
    pub fn from_duplex(stream: tokio::io::DuplexStream) -> Self {
        let (read, write) = tokio::io::split(stream);
        Self::from_parts(read, write)
    }
}

impl AsyncRead for AbstractNsStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for AbstractNsStream {
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

#[async_trait::async_trait]
impl Driver for AbstractNsDriver {
    fn name(&self) -> &str {
        "abstract-ns"
    }

    fn available(&self) -> bool {
        cfg!(target_os = "linux")
    }

    async fn dial(
        &self,
        _target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let sock_name = self.resolve_name(config);

        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::FromRawFd;

            // Use raw libc for abstract namespace socket connection (portable across Linux targets)
            let name_bytes = sock_name.as_bytes().to_vec();
            let std_stream = tokio::task::spawn_blocking(move || {
                unsafe {
                    let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
                    if fd < 0 {
                        return Err(std::io::Error::last_os_error());
                    }

                    // Build abstract socket address: first byte is NUL, then the name
                    let mut addr: libc::sockaddr_un = std::mem::zeroed();
                    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
                    // sun_path[0] = 0 (abstract namespace indicator)
                    // sun_path[1..] = name bytes
                    let max_len = addr.sun_path.len() - 1;
                    if name_bytes.len() > max_len {
                        libc::close(fd);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "abstract socket name too long",
                        ));
                    }
                    for (i, &b) in name_bytes.iter().enumerate() {
                        addr.sun_path[i + 1] = b as libc::c_char;
                    }

                    let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name_bytes.len())
                        as libc::socklen_t;

                    let ret = libc::connect(
                        fd,
                        &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                        addr_len,
                    );
                    if ret < 0 {
                        let err = std::io::Error::last_os_error();
                        libc::close(fd);
                        return Err(err);
                    }

                    Ok(std::os::unix::net::UnixStream::from_raw_fd(fd))
                }
            })
            .await
            .map_err(|e| {
                TransportError::Connection(format!("abstract-ns: spawn_blocking failed: {e}"))
            })?
            .map_err(|e| {
                TransportError::Connection(format!(
                    "abstract-ns: connect to @{sock_name} failed: {e}"
                ))
            })?;

            std_stream.set_nonblocking(true).map_err(|e| {
                TransportError::Connection(format!("abstract-ns: set_nonblocking failed: {e}"))
            })?;
            let stream = tokio::net::UnixStream::from_std(std_stream).map_err(|e| {
                TransportError::Connection(format!("abstract-ns: tokio wrap failed: {e}"))
            })?;

            let (read, write) = tokio::io::split(stream);
            Ok(Box::new(AbstractNsStream::from_parts(read, write)))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(TransportError::Connection(format!(
                "abstract-ns: not available on this platform (name: @{sock_name})"
            )))
        }
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let sock_name = if addr.is_empty() {
            self.name.clone()
        } else {
            addr.to_string()
        };

        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::FromRawFd;

            // Use raw libc for abstract namespace socket binding (portable across Linux targets)
            let name_bytes = sock_name.as_bytes().to_vec();
            let std_listener = unsafe {
                let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0);
                if fd < 0 {
                    return Err(TransportError::Connection(format!(
                        "abstract-ns: socket creation failed: {}",
                        std::io::Error::last_os_error()
                    )));
                }

                let mut addr_un: libc::sockaddr_un = std::mem::zeroed();
                addr_un.sun_family = libc::AF_UNIX as libc::sa_family_t;
                let max_len = addr_un.sun_path.len() - 1;
                if name_bytes.len() > max_len {
                    libc::close(fd);
                    return Err(TransportError::Connection(
                        "abstract-ns: socket name too long".into(),
                    ));
                }
                for (i, &b) in name_bytes.iter().enumerate() {
                    addr_un.sun_path[i + 1] = b as libc::c_char;
                }

                let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name_bytes.len())
                    as libc::socklen_t;

                let ret = libc::bind(
                    fd,
                    &addr_un as *const libc::sockaddr_un as *const libc::sockaddr,
                    addr_len,
                );
                if ret < 0 {
                    let err = std::io::Error::last_os_error();
                    libc::close(fd);
                    return Err(TransportError::Connection(format!(
                        "abstract-ns: bind to @{sock_name} failed: {err}"
                    )));
                }

                let ret = libc::listen(fd, 128);
                if ret < 0 {
                    let err = std::io::Error::last_os_error();
                    libc::close(fd);
                    return Err(TransportError::Connection(format!(
                        "abstract-ns: listen failed: {err}"
                    )));
                }

                std::os::unix::net::UnixListener::from_raw_fd(fd)
            };

            std_listener.set_nonblocking(true).map_err(|e| {
                TransportError::Connection(format!("abstract-ns: set_nonblocking failed: {e}"))
            })?;
            let listener = tokio::net::UnixListener::from_std(std_listener).map_err(|e| {
                TransportError::Connection(format!("abstract-ns: tokio wrap failed: {e}"))
            })?;

            Ok(Box::new(AbstractNsListener { listener }))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(TransportError::Connection(format!(
                "abstract-ns: listener not available on this platform (name: @{sock_name})"
            )))
        }
    }
}

#[cfg(target_os = "linux")]
struct AbstractNsListener {
    listener: tokio::net::UnixListener,
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl Listener for AbstractNsListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let (stream, _) =
            self.listener.accept().await.map_err(|e| {
                TransportError::Connection(format!("abstract-ns: accept failed: {e}"))
            })?;
        let (read, write) = tokio::io::split(stream);
        Ok(Box::new(AbstractNsStream::from_parts(read, write)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = AbstractNsDriver::new();
        assert_eq!(driver.name(), "abstract-ns");
    }

    #[test]
    fn test_default_name() {
        let driver = AbstractNsDriver::new();
        assert_eq!(driver.name, DEFAULT_ABSTRACT_NAME);
    }

    #[test]
    fn test_custom_name() {
        let driver = AbstractNsDriver::with_name("my-agent");
        assert_eq!(driver.name, "my-agent");
    }

    #[test]
    fn test_resolve_name_from_config() {
        let driver = AbstractNsDriver::new();
        let mut config = DriverConfig::new();
        config.insert("abstract_name".into(), "custom-sock".into());
        assert_eq!(driver.resolve_name(&config), "custom-sock");
    }

    #[test]
    fn test_resolve_name_default() {
        let driver = AbstractNsDriver::new();
        let config = DriverConfig::new();
        assert_eq!(driver.resolve_name(&config), DEFAULT_ABSTRACT_NAME);
    }

    #[test]
    fn test_available_platform() {
        let driver = AbstractNsDriver::new();
        if cfg!(target_os = "linux") {
            assert!(driver.available());
        } else {
            assert!(!driver.available());
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn test_dial_unavailable() {
        let driver = AbstractNsDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.err().unwrap());
        assert!(err_msg.contains("not available"));
    }

    #[tokio::test]
    async fn test_stream_from_duplex() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (client, server) = tokio::io::duplex(1024);
        let mut stream = AbstractNsStream::from_duplex(server);
        let mut client = client;

        client.write_all(b"abstract ns test").await.unwrap();
        drop(client);

        let mut buf = vec![0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"abstract ns test");
    }
}
