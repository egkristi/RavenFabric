//! Vsock transport driver for VM-to-hypervisor communication.
//!
//! Uses Linux `AF_VSOCK` for high-performance IPC between virtual machines
//! and their hypervisors (firecracker, cloud-hypervisor, QEMU).
//!
//! # Platform Support
//!
//! - Linux only (AF_VSOCK requires kernel support)
//! - Requires `vhost-vsock` kernel module loaded
//! - Host CID is always 2, guest CIDs are assigned by hypervisor
//!
//! # Use Cases
//!
//! - Firecracker microVM ↔ host agent communication
//! - Cloud-hypervisor guest agent management
//! - QEMU/KVM VM provisioning

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default vsock port for RavenFabric.
pub const DEFAULT_VSOCK_PORT: u32 = 9473; // "RF" in phone keypad (73)

/// Well-known CID for the host.
pub const HOST_CID: u32 = 2;

/// Vsock transport driver for VM-to-hypervisor communication.
pub struct VsockDriver {
    /// Default CID to connect to (2 = host, >2 = guest).
    default_cid: u32,
    /// Default port.
    default_port: u32,
}

impl VsockDriver {
    /// Create a vsock driver connecting to host (CID 2).
    pub fn new() -> Self {
        Self {
            default_cid: HOST_CID,
            default_port: DEFAULT_VSOCK_PORT,
        }
    }

    /// Create a vsock driver with custom CID and port.
    pub fn with_target(cid: u32, port: u32) -> Self {
        Self {
            default_cid: cid,
            default_port: port,
        }
    }

    fn resolve_cid(&self, config: &DriverConfig) -> u32 {
        config
            .get("vsock_cid")
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.default_cid)
    }

    fn resolve_port(&self, config: &DriverConfig) -> u32 {
        config
            .get("vsock_port")
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.default_port)
    }
}

impl Default for VsockDriver {
    fn default() -> Self {
        Self::new()
    }
}

/// A bidirectional stream over a vsock connection.
pub struct VsockStream {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
}

impl VsockStream {
    /// Create from reader/writer (for testing or wrapping OS primitives).
    pub fn from_parts(
        reader: impl AsyncRead + Unpin + Send + 'static,
        writer: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }
}

impl AsyncRead for VsockStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for VsockStream {
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
impl Driver for VsockDriver {
    fn name(&self) -> &str {
        "vsock"
    }

    fn available(&self) -> bool {
        // vsock is only available on Linux with kernel support
        cfg!(target_os = "linux")
    }

    async fn dial(
        &self,
        _target: &Target,
        config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        let cid = self.resolve_cid(config);
        let port = self.resolve_port(config);

        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::FromRawFd;

            // Create AF_VSOCK socket
            let fd =
                unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
            if fd < 0 {
                return Err(TransportError::Connection(
                    "vsock: failed to create socket".into(),
                ));
            }

            let addr = libc::sockaddr_vm {
                svm_family: libc::AF_VSOCK as u16,
                svm_reserved1: 0,
                svm_port: port,
                svm_cid: cid,
                svm_zero: [0u8; 4],
            };

            let ret = unsafe {
                libc::connect(
                    fd,
                    &addr as *const libc::sockaddr_vm as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
                )
            };

            if ret < 0 {
                unsafe { libc::close(fd) };
                return Err(TransportError::Connection(format!(
                    "vsock: connect to CID {cid} port {port} failed: {}",
                    std::io::Error::last_os_error()
                )));
            }

            // Convert to tokio async stream
            let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
            std_stream.set_nonblocking(true).map_err(|e| {
                TransportError::Connection(format!("vsock: set_nonblocking failed: {e}"))
            })?;
            let tokio_stream = tokio::net::TcpStream::from_std(std_stream).map_err(|e| {
                TransportError::Connection(format!("vsock: tokio wrap failed: {e}"))
            })?;
            let (read, write) = tokio::io::split(tokio_stream);
            Ok(Box::new(VsockStream::from_parts(read, write)))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(TransportError::Connection(format!(
                "vsock: not available on this platform (CID {cid}, port {port})"
            )))
        }
    }

    async fn listen(&self, addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let port: u32 = if addr.is_empty() {
            self.default_port
        } else {
            addr.parse()
                .map_err(|_| TransportError::Connection(format!("vsock: invalid port '{addr}'")))?
        };

        #[cfg(target_os = "linux")]
        {
            let fd =
                unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
            if fd < 0 {
                return Err(TransportError::Connection(
                    "vsock: failed to create listener socket".into(),
                ));
            }

            let addr_vm = libc::sockaddr_vm {
                svm_family: libc::AF_VSOCK as u16,
                svm_reserved1: 0,
                svm_port: port,
                svm_cid: libc::VMADDR_CID_ANY,
                svm_zero: [0u8; 4],
            };

            let ret = unsafe {
                libc::bind(
                    fd,
                    &addr_vm as *const libc::sockaddr_vm as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
                )
            };
            if ret < 0 {
                unsafe { libc::close(fd) };
                return Err(TransportError::Connection(format!(
                    "vsock: bind to port {port} failed: {}",
                    std::io::Error::last_os_error()
                )));
            }

            let ret = unsafe { libc::listen(fd, 128) };
            if ret < 0 {
                unsafe { libc::close(fd) };
                return Err(TransportError::Connection(format!(
                    "vsock: listen failed: {}",
                    std::io::Error::last_os_error()
                )));
            }

            Ok(Box::new(VsockListener { fd, port }))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(TransportError::Connection(format!(
                "vsock: listener not available on this platform (port {port})"
            )))
        }
    }
}

#[cfg(target_os = "linux")]
struct VsockListener {
    fd: i32,
    #[allow(dead_code)]
    port: u32,
}

#[cfg(target_os = "linux")]
impl Drop for VsockListener {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl Listener for VsockListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        use std::os::unix::io::FromRawFd;

        let client_fd = unsafe {
            libc::accept4(
                self.fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };

        if client_fd < 0 {
            return Err(TransportError::Connection(format!(
                "vsock: accept failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        let std_stream = unsafe { std::net::TcpStream::from_raw_fd(client_fd) };
        std_stream.set_nonblocking(true).map_err(|e| {
            TransportError::Connection(format!("vsock: set_nonblocking on accepted: {e}"))
        })?;
        let tokio_stream = tokio::net::TcpStream::from_std(std_stream).map_err(|e| {
            TransportError::Connection(format!("vsock: tokio wrap on accepted: {e}"))
        })?;
        let (read, write) = tokio::io::split(tokio_stream);
        Ok(Box::new(VsockStream::from_parts(read, write)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = VsockDriver::new();
        assert_eq!(driver.name(), "vsock");
    }

    #[test]
    fn test_default_values() {
        let driver = VsockDriver::new();
        assert_eq!(driver.default_cid, HOST_CID);
        assert_eq!(driver.default_port, DEFAULT_VSOCK_PORT);
    }

    #[test]
    fn test_custom_target() {
        let driver = VsockDriver::with_target(100, 8080);
        assert_eq!(driver.default_cid, 100);
        assert_eq!(driver.default_port, 8080);
    }

    #[test]
    fn test_resolve_cid_from_config() {
        let driver = VsockDriver::new();
        let mut config = DriverConfig::new();
        config.insert("vsock_cid".into(), "42".into());
        assert_eq!(driver.resolve_cid(&config), 42);
    }

    #[test]
    fn test_resolve_port_from_config() {
        let driver = VsockDriver::new();
        let mut config = DriverConfig::new();
        config.insert("vsock_port".into(), "1234".into());
        assert_eq!(driver.resolve_port(&config), 1234);
    }

    #[test]
    fn test_available_platform() {
        let driver = VsockDriver::new();
        if cfg!(target_os = "linux") {
            assert!(driver.available());
        } else {
            assert!(!driver.available());
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[tokio::test]
    async fn test_dial_unavailable() {
        let driver = VsockDriver::new();
        let target = Target {
            agent_id: "test".into(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_vsock_stream_from_duplex() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (client, server) = tokio::io::duplex(1024);
        let (read, write) = tokio::io::split(server);
        let mut stream = VsockStream::from_parts(read, write);
        let mut client = client;

        client.write_all(b"vsock test").await.unwrap();
        drop(client);

        let mut buf = vec![0u8; 64];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"vsock test");
    }
}
