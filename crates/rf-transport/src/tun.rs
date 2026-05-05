//! TUN device creation and management for mesh VPN.
//!
//! Implements cross-platform TUN device creation for Layer 3 mesh networking.
//! On Linux, uses /dev/net/tun with ioctl. On macOS, uses utun via socket().
//! On unsupported platforms, provides a no-op stub.

use std::io;
use std::net::Ipv6Addr;

/// Configuration for a TUN device.
#[derive(Debug, Clone)]
pub struct TunConfig {
    /// Device name (Linux: "rvnf0", macOS: auto-assigned utunN).
    pub name: String,
    /// IPv6 address to assign to the interface.
    pub address: Ipv6Addr,
    /// Prefix length (e.g., 32 for /32 mesh).
    pub prefix_len: u8,
    /// MTU (default: 1400 to account for encryption overhead).
    pub mtu: u16,
    /// Whether to bring the interface up immediately.
    pub up: bool,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            name: "rvnf0".to_string(),
            address: Ipv6Addr::UNSPECIFIED,
            prefix_len: 32,
            mtu: 1400,
            up: true,
        }
    }
}

/// A handle to an open TUN device.
pub struct TunDevice {
    /// File descriptor (Linux/macOS).
    #[cfg(unix)]
    fd: std::os::unix::io::RawFd,
    /// Device name as assigned by the kernel.
    pub name: String,
    /// Configured MTU.
    pub mtu: u16,
}

#[cfg(target_os = "linux")]
impl TunDevice {
    /// Create and open a TUN device on Linux via /dev/net/tun.
    pub fn create(config: &TunConfig) -> io::Result<Self> {
        use std::os::unix::io::AsRawFd;

        // Open /dev/net/tun
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/net/tun")?;

        let fd = file.as_raw_fd();

        // Prepare ifreq struct for TUNSETIFF ioctl
        // struct ifreq: 16 bytes name + 2 bytes flags + padding
        let mut ifr = [0u8; 40];
        let name_bytes = config.name.as_bytes();
        let copy_len = name_bytes.len().min(15); // IFNAMSIZ - 1
        ifr[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

        // IFF_TUN | IFF_NO_PI (no packet info header)
        let flags: u16 = 0x0001 | 0x1000;
        ifr[16..18].copy_from_slice(&flags.to_le_bytes());

        // TUNSETIFF = 0x400454CA
        let ret = unsafe { libc::ioctl(fd, 0x4004_54CA, ifr.as_mut_ptr()) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        // Extract assigned name
        let name_end = ifr.iter().take(16).position(|&b| b == 0).unwrap_or(16);
        let assigned_name = String::from_utf8_lossy(&ifr[..name_end]).to_string();

        // Keep the fd alive by leaking the File (we own the fd now)
        let raw_fd = file.as_raw_fd();
        std::mem::forget(file);

        Ok(Self {
            fd: raw_fd,
            name: assigned_name,
            mtu: config.mtu,
        })
    }

    /// Read a packet from the TUN device.
    pub fn read_packet(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    /// Write a packet to the TUN device.
    pub fn write_packet(&self, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(self.fd, buf.as_ptr().cast(), buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
}

#[cfg(target_os = "macos")]
impl TunDevice {
    /// Create and open a TUN device on macOS via utun kernel control socket.
    pub fn create(config: &TunConfig) -> io::Result<Self> {
        // On macOS, TUN devices are created via PF_SYSTEM socket + SYSPROTO_CONTROL
        // The kernel control name is "com.apple.net.utun_control"

        // PF_SYSTEM = 32, SOCK_DGRAM = 2, SYSPROTO_CONTROL = 2
        let fd = unsafe { libc::socket(32, libc::SOCK_DGRAM, 2) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        // struct ctl_info { u_int32_t ctl_id; char ctl_name[96]; }
        let mut ctl_info = [0u8; 100];
        let ctl_name = b"com.apple.net.utun_control";
        ctl_info[4..4 + ctl_name.len()].copy_from_slice(ctl_name);

        // CTLIOCGINFO = 0xC0644E03
        let ret = unsafe { libc::ioctl(fd, 0xC064_4E03u64 as libc::c_ulong, ctl_info.as_mut_ptr()) };
        if ret < 0 {
            unsafe { libc::close(fd) };
            return Err(io::Error::last_os_error());
        }

        let ctl_id = u32::from_ne_bytes([ctl_info[0], ctl_info[1], ctl_info[2], ctl_info[3]]);

        // struct sockaddr_ctl { u8 sc_len, u8 sc_family, u16 ss_sysaddr, u32 sc_id, u32 sc_unit }
        let sc_unit: u32 = 0; // 0 means auto-assign
        let mut sa_ctl = [0u8; 32];
        sa_ctl[0] = 32; // sc_len
        sa_ctl[1] = 32; // AF_SYSTEM = 32
        sa_ctl[2..4].copy_from_slice(&2u16.to_ne_bytes()); // AF_SYS_CONTROL = 2
        sa_ctl[4..8].copy_from_slice(&ctl_id.to_ne_bytes());
        sa_ctl[8..12].copy_from_slice(&sc_unit.to_ne_bytes());

        let ret = unsafe {
            libc::connect(
                fd,
                sa_ctl.as_ptr().cast(),
                sa_ctl.len() as libc::socklen_t,
            )
        };
        if ret < 0 {
            unsafe { libc::close(fd) };
            return Err(io::Error::last_os_error());
        }

        // Get the assigned utun interface name via getsockopt
        let mut ifname = [0u8; 32];
        let mut ifname_len: libc::socklen_t = 32;
        // UTUN_OPT_IFNAME = 2, SYSPROTO_CONTROL = 2
        let ret = unsafe {
            libc::getsockopt(
                fd,
                2, // SYSPROTO_CONTROL
                2, // UTUN_OPT_IFNAME
                ifname.as_mut_ptr().cast(),
                &mut ifname_len,
            )
        };

        let assigned_name = if ret == 0 {
            let name_end = ifname.iter().position(|&b| b == 0).unwrap_or(ifname_len as usize);
            String::from_utf8_lossy(&ifname[..name_end]).to_string()
        } else {
            config.name.clone()
        };

        Ok(Self {
            fd,
            name: assigned_name,
            mtu: config.mtu,
        })
    }

    /// Read a packet from the TUN device.
    /// On macOS, utun prepends a 4-byte protocol header (AF_INET6 = 30).
    pub fn read_packet(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    /// Write a packet to the TUN device.
    /// On macOS, must prepend 4-byte protocol header.
    pub fn write_packet(&self, buf: &[u8]) -> io::Result<usize> {
        // Prepend AF_INET6 header for IPv6 packets
        let mut packet = Vec::with_capacity(4 + buf.len());
        packet.extend_from_slice(&[0, 0, 0, 30]); // AF_INET6 = 30
        packet.extend_from_slice(buf);

        let n = unsafe { libc::write(self.fd, packet.as_ptr().cast(), packet.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok((n as usize).saturating_sub(4))
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl TunDevice {
    /// Stub for unsupported platforms.
    pub fn create(_config: &TunConfig) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TUN devices not supported on this platform",
        ))
    }

    pub fn read_packet(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TUN devices not supported on this platform",
        ))
    }

    pub fn write_packet(&self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TUN devices not supported on this platform",
        ))
    }
}

#[cfg(unix)]
impl Drop for TunDevice {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tun_config_default() {
        let config = TunConfig::default();
        assert_eq!(config.name, "rvnf0");
        assert_eq!(config.mtu, 1400);
        assert_eq!(config.prefix_len, 32);
        assert!(config.up);
    }

    #[test]
    fn test_tun_config_custom() {
        let config = TunConfig {
            name: "mesh0".to_string(),
            address: "fd00:5256::1".parse().unwrap(),
            prefix_len: 64,
            mtu: 1280,
            up: true,
        };
        assert_eq!(config.name, "mesh0");
        assert_eq!(config.mtu, 1280);
    }

    // Note: Actually creating a TUN device requires root/admin privileges.
    // The create() function is tested in integration tests with appropriate permissions.
    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn test_tun_create_without_permission_fails() {
        // Without root, this should fail with permission denied
        let config = TunConfig::default();
        let result = TunDevice::create(&config);
        // We expect this to fail in CI (no root), but it exercises the code path
        assert!(result.is_err());
    }
}
