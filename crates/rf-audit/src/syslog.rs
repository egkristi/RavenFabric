//! Syslog (RFC 5424) audit log destination.
//!
//! Sends each `AuditEntry` as an RFC 5424 syslog message to a remote server
//! over UDP or TCP.  Delivery failures are logged at `warn` level and never
//! surfaced as errors to the caller (fire-and-forget for UDP; best-effort for
//! TCP).

use std::{
    io::Write,
    net::{TcpStream, ToSocketAddrs, UdpSocket},
    sync::Mutex,
    time::Duration,
};

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

/// RFC 5424 syslog facility codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyslogFacility {
    /// `kern` (0)
    Kernel = 0,
    /// `user` (1) — default for user-space applications
    User = 1,
    /// `daemon` (3)
    Daemon = 3,
    /// `auth` (4) — security/authorization
    Auth = 4,
    /// `local0`–`local7` (16–23)
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

/// RFC 5424 syslog severity codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyslogSeverity {
    /// Emergency (0) — system is unusable
    Emergency = 0,
    /// Alert (1) — action must be taken immediately
    Alert = 1,
    /// Critical (2) — critical conditions
    Critical = 2,
    /// Error (3) — error conditions
    Error = 3,
    /// Warning (4) — warning conditions
    Warning = 4,
    /// Notice (5) — normal but significant condition
    Notice = 5,
    /// Informational (6) — informational messages
    Informational = 6,
    /// Debug (7) — debug-level messages
    Debug = 7,
}

fn rfc5424_priority(facility: SyslogFacility, severity: SyslogSeverity) -> u8 {
    (facility as u8) * 8 + (severity as u8)
}

/// Format an `AuditEntry` as an RFC 5424 syslog message.
///
/// Format: `<PRI>VERSION SP TIMESTAMP SP HOSTNAME SP APPNAME SP PROCID SP MSGID SP STRUCTURED-DATA SP MSG`
pub fn format_rfc5424(
    entry: &AuditEntry,
    facility: SyslogFacility,
    severity: SyslogSeverity,
    hostname: &str,
    app_name: &str,
) -> String {
    let pri = rfc5424_priority(facility, severity);
    let ts = entry.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ");
    let proc_id = std::process::id();
    // MSGID = action type; STRUCTURED-DATA = decision and caller fields
    let msg_id = &entry.action;
    let structured = format!(
        "[rf@ravenfabric request_id=\"{}\" decision=\"{}\" matched_rule=\"{}\" caller=\"{}\" duration_ms=\"{}\"]",
        entry.request_id, entry.decision, entry.matched_rule, entry.caller_key, entry.duration_ms,
    );
    let msg = match &entry.command {
        Some(cmd) => format!("{} {}", entry.action, cmd),
        None => entry.action.clone(),
    };
    format!("<{pri}>1 {ts} {hostname} {app_name} {proc_id} {msg_id} {structured} {msg}")
}

// ── Transport ─────────────────────────────────────────────────────────────────

/// Transport protocol for syslog delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyslogTransport {
    /// UDP (connectionless, fire-and-forget)
    Udp,
    /// TCP (connection-oriented, octet-counting framing as per RFC 6587)
    Tcp,
}

// ── SyslogAuditLogger ─────────────────────────────────────────────────────────

/// Audit logger that forwards entries to a syslog server via UDP or TCP.
///
/// - **UDP**: A single bound socket is reused for all sends. No connection
///   state; delivery is not guaranteed.
/// - **TCP**: A persistent connection is maintained with a 5-second connect/
///   write timeout. If the connection drops, the next `log()` call will attempt
///   to reconnect.
pub struct SyslogAuditLogger {
    target: String,
    transport: SyslogTransport,
    facility: SyslogFacility,
    severity: SyslogSeverity,
    hostname: String,
    app_name: String,
    /// UDP socket (reused across calls)
    udp_socket: Option<UdpSocket>,
    /// TCP stream protected by mutex for concurrent log() calls
    tcp_stream: Mutex<Option<TcpStream>>,
}

impl SyslogAuditLogger {
    /// Create a new UDP syslog logger.
    ///
    /// `target` is `host:port` of the syslog server (e.g. `"127.0.0.1:514"`).
    pub fn udp(
        target: impl Into<String>,
        facility: SyslogFacility,
        severity: SyslogSeverity,
        hostname: impl Into<String>,
        app_name: impl Into<String>,
    ) -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        Ok(Self {
            target: target.into(),
            transport: SyslogTransport::Udp,
            facility,
            severity,
            hostname: hostname.into(),
            app_name: app_name.into(),
            udp_socket: Some(socket),
            tcp_stream: Mutex::new(None),
        })
    }

    /// Create a new TCP syslog logger.
    ///
    /// `target` is `host:port` of the syslog server.  The connection is
    /// established lazily on the first `log()` call.
    pub fn tcp(
        target: impl Into<String>,
        facility: SyslogFacility,
        severity: SyslogSeverity,
        hostname: impl Into<String>,
        app_name: impl Into<String>,
    ) -> Self {
        Self {
            target: target.into(),
            transport: SyslogTransport::Tcp,
            facility,
            severity,
            hostname: hostname.into(),
            app_name: app_name.into(),
            udp_socket: None,
            tcp_stream: Mutex::new(None),
        }
    }

    fn send_udp(&self, msg: &str) -> Result<(), std::io::Error> {
        let socket = self
            .udp_socket
            .as_ref()
            .expect("udp socket not initialized");
        let addr: std::net::SocketAddr = self
            .target
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no address"))?;
        socket.send_to(msg.as_bytes(), addr)?;
        Ok(())
    }

    fn send_tcp(&self, msg: &str) -> Result<(), std::io::Error> {
        let mut guard = self.tcp_stream.lock().unwrap_or_else(|p| p.into_inner());

        // Connect or reconnect if necessary
        if guard.is_none() {
            let stream = TcpStream::connect_timeout(
                &self.target.to_socket_addrs()?.next().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "no address")
                })?,
                Duration::from_secs(5),
            )?;
            stream.set_write_timeout(Some(Duration::from_secs(5)))?;
            *guard = Some(stream);
        }

        // RFC 6587 §3.4.1 octet-counting: "<LEN> <MSG>\n"
        let framed = format!("{} {}\n", msg.len(), msg);
        if let Some(ref mut stream) = *guard {
            if let Err(e) = stream.write_all(framed.as_bytes()) {
                // Drop the connection so the next call reconnects
                *guard = None;
                return Err(e);
            }
        }
        Ok(())
    }
}

impl AuditLogger for SyslogAuditLogger {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let msg = format_rfc5424(
            &entry,
            self.facility,
            self.severity,
            &self.hostname,
            &self.app_name,
        );
        let result = match self.transport {
            SyslogTransport::Udp => self.send_udp(&msg),
            SyslogTransport::Tcp => self.send_tcp(&msg),
        };
        if let Err(e) = result {
            tracing::warn!("syslog delivery failed: {e}");
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::types::AuditEntry;

    fn sample_entry() -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: "test-req-1".into(),
            action: "execute".into(),
            command: Some("ls -la".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 42,
            caller_key: "deadbeef".into(),
            reason: None,
            prev_hash: None,
            hmac: None,
        }
    }

    #[test]
    fn test_format_rfc5424_structure() {
        let entry = sample_entry();
        let msg = format_rfc5424(
            &entry,
            SyslogFacility::User,
            SyslogSeverity::Informational,
            "myhost",
            "ravenfabric",
        );
        // PRI for USER(1) + INFO(6) = 8 + 6 = 14 → "<14>1 ..."
        assert!(
            msg.starts_with("<14>1 "),
            "expected RFC5424 PRI, got: {msg}"
        );
        assert!(msg.contains("myhost"), "expected hostname in message");
        assert!(msg.contains("ravenfabric"), "expected appname in message");
        assert!(msg.contains("execute"), "expected action as MSGID");
        assert!(msg.contains("ls -la"), "expected command in message body");
        assert!(
            msg.contains("allowed"),
            "expected decision in structured data"
        );
    }

    #[test]
    fn test_format_rfc5424_priority_calculation() {
        // LOCAL0(16) + WARN(4) = 128 + 4 = 132
        let entry = sample_entry();
        let msg = format_rfc5424(
            &entry,
            SyslogFacility::Local0,
            SyslogSeverity::Warning,
            "h",
            "a",
        );
        assert!(msg.starts_with("<132>1 "));
    }

    #[test]
    fn test_format_rfc5424_no_command() {
        let mut entry = sample_entry();
        entry.command = None;
        entry.action = "status".into();
        let msg = format_rfc5424(
            &entry,
            SyslogFacility::User,
            SyslogSeverity::Notice,
            "h",
            "a",
        );
        assert!(msg.contains("status"));
    }

    #[test]
    fn test_udp_syslog_delivers_entry() {
        // Bind a UDP listener on a random port, create a SyslogAuditLogger
        // targeting it, log an entry, and verify the data arrives.
        use std::net::UdpSocket;

        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let addr = server.local_addr().unwrap();

        let logger = SyslogAuditLogger::udp(
            addr.to_string(),
            SyslogFacility::User,
            SyslogSeverity::Informational,
            "testhost",
            "ravenfabric",
        )
        .unwrap();

        logger.log(sample_entry()).unwrap();

        let mut buf = vec![0u8; 4096];
        let (len, _) = server
            .recv_from(&mut buf)
            .expect("did not receive UDP packet");
        let received = String::from_utf8_lossy(&buf[..len]);
        assert!(received.contains("<14>1 "), "expected RFC5424 PRI");
        assert!(received.contains("execute"), "expected action in message");
    }

    #[test]
    fn test_tcp_syslog_delivers_entry() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut buf = String::new();
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(&mut stream);
            reader.read_line(&mut buf).unwrap();
            buf
        });

        let logger = SyslogAuditLogger::tcp(
            addr.to_string(),
            SyslogFacility::Local0,
            SyslogSeverity::Notice,
            "testhost",
            "ravenfabric",
        );
        logger.log(sample_entry()).unwrap();

        let received = handle.join().expect("server thread panicked");
        assert!(
            received.contains("execute"),
            "expected action in received TCP message"
        );
        assert!(
            received.contains("allowed"),
            "expected decision in received TCP message"
        );
    }

    #[test]
    fn test_udp_syslog_delivery_failure_is_silent() {
        // Pointing at a port with no listener should not return an error.
        let logger = SyslogAuditLogger::udp(
            "127.0.0.1:1", // port 1 — no listener, but UDP send won't fail immediately
            SyslogFacility::User,
            SyslogSeverity::Informational,
            "h",
            "a",
        )
        .unwrap();
        // Should not propagate I/O errors
        assert!(logger.log(sample_entry()).is_ok());
    }
}
