//! File-descriptor passing over UNIX domain sockets (SCM_RIGHTS).
//!
//! Enables zero-copy handoff of pre-authenticated connections between processes.
//! A parent process (e.g., `rf-agent`) can accept and authenticate a connection,
//! then pass the raw file descriptor to a child worker process without re-handshaking.
//!
//! Security: The receiving process inherits the authenticated channel state.
//! Only pass FDs to trusted child processes. Peer credentials are verified before send.

use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

use tokio::net::UnixStream;

use crate::error::TransportError;

/// Maximum number of file descriptors that can be passed in a single message.
const MAX_FDS: usize = 8;

/// Send file descriptors over a UNIX domain socket using SCM_RIGHTS.
///
/// The `fds` slice contains the raw file descriptors to pass to the peer.
/// A single data byte is sent alongside (required by the protocol).
///
/// # Safety
///
/// The caller must ensure `fds` contains valid, open file descriptors.
/// After sending, the FDs remain open in the sender — close them if no longer needed.
pub async fn send_fds(socket: &UnixStream, fds: &[RawFd]) -> Result<(), TransportError> {
    if fds.is_empty() {
        return Err(TransportError::Connection(
            "fd_passing: no file descriptors to send".into(),
        ));
    }
    if fds.len() > MAX_FDS {
        return Err(TransportError::Connection(format!(
            "fd_passing: too many fds ({}, max {})",
            fds.len(),
            MAX_FDS
        )));
    }

    let raw_fd = socket.as_raw_fd();

    socket
        .writable()
        .await
        .map_err(|e| TransportError::Connection(format!("fd_passing: socket not writable: {e}")))?;

    let fds_owned = fds.to_vec();
    let result = socket.try_io(tokio::io::Interest::WRITABLE, || {
        send_fds_blocking(raw_fd, &fds_owned)
    });

    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(TransportError::Connection(format!(
            "fd_passing: sendmsg failed: {e}"
        ))),
    }
}

/// Receive file descriptors from a UNIX domain socket using SCM_RIGHTS.
///
/// Returns the received file descriptors. The caller takes ownership and
/// is responsible for closing them.
pub async fn recv_fds(socket: &UnixStream) -> Result<Vec<RawFd>, TransportError> {
    let raw_fd = socket.as_raw_fd();

    socket
        .readable()
        .await
        .map_err(|e| TransportError::Connection(format!("fd_passing: socket not readable: {e}")))?;

    let result = socket.try_io(tokio::io::Interest::READABLE, || recv_fds_blocking(raw_fd));

    match result {
        Ok(fds) => Ok(fds),
        Err(e) => Err(TransportError::Connection(format!(
            "fd_passing: recvmsg failed: {e}"
        ))),
    }
}

/// Convert a received raw FD into a tokio `UnixStream` for async I/O.
///
/// # Safety
///
/// The caller must ensure `fd` is a valid, connected UNIX stream socket
/// that was received via `recv_fds`.
pub unsafe fn fd_to_unix_stream(fd: RawFd) -> Result<UnixStream, TransportError> {
    let std_stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
    std_stream.set_nonblocking(true).map_err(|e| {
        TransportError::Connection(format!("fd_passing: set_nonblocking failed: {e}"))
    })?;
    UnixStream::from_std(std_stream)
        .map_err(|e| TransportError::Connection(format!("fd_passing: from_std failed: {e}")))
}

/// Blocking sendmsg with SCM_RIGHTS ancillary data.
fn send_fds_blocking(socket_fd: RawFd, fds: &[RawFd]) -> io::Result<()> {
    use libc::{
        CMSG_DATA, CMSG_LEN, CMSG_SPACE, SCM_RIGHTS, SOL_SOCKET, c_void, cmsghdr, iovec, msghdr,
        sendmsg,
    };
    use std::mem;
    use std::ptr;

    // Data payload — at least one byte is required
    let data: [u8; 1] = [0x42];
    let mut iov = iovec {
        iov_base: data.as_ptr() as *mut c_void,
        iov_len: 1,
    };

    // Calculate control message buffer size
    let fds_byte_len = std::mem::size_of_val(fds);
    let cmsg_space = unsafe { CMSG_SPACE(fds_byte_len as u32) } as usize;
    let mut cmsg_buf = vec![0u8; cmsg_space];

    let mut msg: msghdr = unsafe { mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut c_void;
    msg.msg_controllen = cmsg_space as _;

    // Fill the control message header
    let cmsg: &mut cmsghdr = unsafe { &mut *(cmsg_buf.as_mut_ptr() as *mut cmsghdr) };
    cmsg.cmsg_level = SOL_SOCKET;
    cmsg.cmsg_type = SCM_RIGHTS;
    cmsg.cmsg_len = unsafe { CMSG_LEN(fds_byte_len as u32) } as _;

    // Copy file descriptors into CMSG_DATA
    unsafe {
        ptr::copy_nonoverlapping(fds.as_ptr() as *const u8, CMSG_DATA(cmsg), fds_byte_len);
    }

    let ret = unsafe { sendmsg(socket_fd, &msg, 0) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Blocking recvmsg with SCM_RIGHTS ancillary data extraction.
fn recv_fds_blocking(socket_fd: RawFd) -> io::Result<Vec<RawFd>> {
    use libc::{
        CMSG_DATA, CMSG_FIRSTHDR, CMSG_LEN, CMSG_NXTHDR, CMSG_SPACE, SCM_RIGHTS, SOL_SOCKET,
        c_void, iovec, msghdr,
    };
    use std::mem;

    let mut data: [u8; 1] = [0];
    let mut iov = iovec {
        iov_base: data.as_mut_ptr() as *mut c_void,
        iov_len: 1,
    };

    let cmsg_space = unsafe { CMSG_SPACE((MAX_FDS * mem::size_of::<RawFd>()) as u32) } as usize;
    let mut cmsg_buf = vec![0u8; cmsg_space];

    let mut msg: msghdr = unsafe { mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut c_void;
    msg.msg_controllen = cmsg_space as _;

    let ret = unsafe { recvmsg_platform(socket_fd, &mut msg) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    if ret == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed"));
    }

    // Extract file descriptors from control messages
    let mut received_fds = Vec::new();
    unsafe {
        let mut cmsg_ptr = CMSG_FIRSTHDR(&msg);
        while !cmsg_ptr.is_null() {
            let cmsg_ref = &*cmsg_ptr;
            if cmsg_ref.cmsg_level == SOL_SOCKET && cmsg_ref.cmsg_type == SCM_RIGHTS {
                let payload_len = cmsg_ref.cmsg_len as usize - CMSG_LEN(0) as usize;
                let fd_count = payload_len / mem::size_of::<RawFd>();
                let fd_ptr = CMSG_DATA(cmsg_ptr) as *const RawFd;
                for i in 0..fd_count {
                    received_fds.push(*fd_ptr.add(i));
                }
            }
            cmsg_ptr = CMSG_NXTHDR(&msg, cmsg_ptr);
        }
    }

    if received_fds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no file descriptors in control message",
        ));
    }

    Ok(received_fds)
}

/// Platform-specific recvmsg: Linux uses MSG_CMSG_CLOEXEC, others use 0.
#[cfg(target_os = "linux")]
unsafe fn recvmsg_platform(socket_fd: RawFd, msg: &mut libc::msghdr) -> isize {
    unsafe { libc::recvmsg(socket_fd, msg, libc::MSG_CMSG_CLOEXEC) }
}

#[cfg(not(target_os = "linux"))]
unsafe fn recvmsg_platform(socket_fd: RawFd, msg: &mut libc::msghdr) -> isize {
    unsafe { libc::recvmsg(socket_fd, msg, 0) }
}

/// Close file descriptors received via `recv_fds` that are no longer needed.
///
/// # Safety
///
/// Only call this on FDs that you own and haven't already converted to a higher-level type.
pub fn close_fds(fds: &[RawFd]) {
    for &fd in fds {
        unsafe {
            libc::close(fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream as TokioUnixStream;

    #[tokio::test]
    async fn test_send_recv_fd() {
        // Create a pair of connected UNIX streams for the control channel
        let (control_a, control_b) = TokioUnixStream::pair().unwrap();

        // Create a pipe whose write-end we'll pass via SCM_RIGHTS
        let (read_pipe, write_pipe) = tokio::net::UnixStream::pair().unwrap();
        let write_fd = write_pipe.as_raw_fd();

        // Send the write_pipe's FD from A to B
        send_fds(&control_a, &[write_fd]).await.unwrap();

        // Receive the FD on B's side
        let received = recv_fds(&control_b).await.unwrap();
        assert_eq!(received.len(), 1);

        // Convert the received FD to a UnixStream and write data through it
        let mut received_stream = unsafe { fd_to_unix_stream(received[0]).unwrap() };
        received_stream.write_all(b"hello via fd").await.unwrap();

        // Read from the original read end
        let mut buf = [0u8; 12];
        let mut read_end = read_pipe;
        read_end.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello via fd");
    }

    #[tokio::test]
    async fn test_send_multiple_fds() {
        let (control_a, control_b) = TokioUnixStream::pair().unwrap();

        // Create two socket pairs
        let (r1, w1) = TokioUnixStream::pair().unwrap();
        let (r2, w2) = TokioUnixStream::pair().unwrap();

        let fds = [w1.as_raw_fd(), w2.as_raw_fd()];
        send_fds(&control_a, &fds).await.unwrap();

        let received = recv_fds(&control_b).await.unwrap();
        assert_eq!(received.len(), 2);

        // Write through first received FD
        let mut stream1 = unsafe { fd_to_unix_stream(received[0]).unwrap() };
        stream1.write_all(b"fd1").await.unwrap();

        let mut buf = [0u8; 3];
        let mut reader1 = r1;
        reader1.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"fd1");

        // Write through second received FD
        let mut stream2 = unsafe { fd_to_unix_stream(received[1]).unwrap() };
        stream2.write_all(b"fd2").await.unwrap();

        let mut buf = [0u8; 3];
        let mut reader2 = r2;
        reader2.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"fd2");
    }

    #[tokio::test]
    async fn test_send_empty_fds_rejected() {
        let (control_a, _control_b) = TokioUnixStream::pair().unwrap();
        let result = send_fds(&control_a, &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_too_many_fds_rejected() {
        let (control_a, _control_b) = TokioUnixStream::pair().unwrap();
        let fds: Vec<RawFd> = (0..20).collect(); // More than MAX_FDS
        let result = send_fds(&control_a, &fds).await;
        assert!(result.is_err());
    }
}
