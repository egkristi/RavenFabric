//! PTY session types for interactive terminal access.
//!
//! Defines types for PTY allocation, terminal sessions, and session recording
//! in asciinema v2 format. The actual PTY spawn is platform-specific and
//! requires `#[cfg(unix)]` for real implementation.

use serde::{Deserialize, Serialize};

/// Terminal size (rows x cols).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// PTY session configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyConfig {
    /// Shell to spawn (default: user's shell or /bin/sh).
    pub shell: Option<String>,
    /// Initial terminal size.
    pub size: TerminalSize,
    /// Working directory.
    pub cwd: Option<String>,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
    /// Session timeout (seconds). 0 = no timeout.
    pub timeout_secs: u64,
    /// Whether to record the session.
    pub record: bool,
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            shell: None,
            size: TerminalSize::default(),
            cwd: None,
            env: Vec::new(),
            timeout_secs: 3600, // 1 hour default
            record: false,
        }
    }
}

/// State of a PTY session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session requested, not yet spawned.
    Pending,
    /// PTY spawned and active.
    Active,
    /// Session suspended (e.g., during migration).
    Suspended,
    /// Session ended normally (exit code available).
    Exited,
    /// Session killed by timeout or policy.
    Killed,
    /// Session failed to start.
    Failed,
}

/// Metadata for a PTY session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Unique session ID.
    pub session_id: String,
    /// Agent the session is running on.
    pub agent_id: String,
    /// Current state.
    pub state: SessionState,
    /// Terminal size.
    pub size: TerminalSize,
    /// Shell command running.
    pub shell: String,
    /// When the session started (Unix timestamp ms).
    pub started_at_ms: u64,
    /// Duration in milliseconds (0 if still active).
    pub duration_ms: u64,
    /// Exit code if exited.
    pub exit_code: Option<i32>,
}

/// Input event to a PTY session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PtyInput {
    /// Raw bytes to write to the PTY.
    Data { data: Vec<u8> },
    /// Resize the terminal.
    Resize { size: TerminalSize },
    /// Send a signal to the PTY process.
    Signal { signal: PtySignal },
}

/// Output event from a PTY session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PtyOutput {
    /// Data read from PTY stdout.
    Data { data: Vec<u8> },
    /// Session has exited.
    Exit { code: i32 },
    /// Session error.
    Error { message: String },
}

/// Signal to send to a PTY process.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum PtySignal {
    Sigint,
    Sigterm,
    Sigkill,
    Sighup,
    Sigwinch,
}

/// Asciinema v2 recording header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsciicastHeader {
    pub version: u8,
    pub width: u16,
    pub height: u16,
    pub timestamp: Option<u64>,
    pub title: Option<String>,
    pub env: Option<AsciicastEnv>,
}

/// Environment recorded in asciicast header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsciicastEnv {
    #[serde(rename = "SHELL")]
    pub shell: Option<String>,
    #[serde(rename = "TERM")]
    pub term: Option<String>,
}

/// A single asciicast event (timestamp, type, data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsciicastEvent {
    /// Seconds since recording start (float).
    pub time: f64,
    /// Event type: "o" (output) or "i" (input).
    pub event_type: String,
    /// Data (UTF-8 encoded terminal output/input).
    pub data: String,
}

impl AsciicastEvent {
    /// Create an output event.
    pub fn output(time: f64, data: String) -> Self {
        Self {
            time,
            event_type: "o".to_string(),
            data,
        }
    }

    /// Create an input event.
    pub fn input(time: f64, data: String) -> Self {
        Self {
            time,
            event_type: "i".to_string(),
            data,
        }
    }
}

/// Session recorder that accumulates asciicast events.
#[derive(Debug)]
pub struct SessionRecorder {
    header: AsciicastHeader,
    events: Vec<AsciicastEvent>,
    start_time_ms: u64,
}

impl SessionRecorder {
    pub fn new(width: u16, height: u16, start_time_ms: u64) -> Self {
        Self {
            header: AsciicastHeader {
                version: 2,
                width,
                height,
                timestamp: Some(start_time_ms / 1000),
                title: None,
                env: None,
            },
            events: Vec::new(),
            start_time_ms,
        }
    }

    /// Record output data at the given timestamp.
    pub fn record_output(&mut self, data: &str, timestamp_ms: u64) {
        let elapsed = (timestamp_ms - self.start_time_ms) as f64 / 1000.0;
        self.events
            .push(AsciicastEvent::output(elapsed, data.to_string()));
    }

    /// Record input data at the given timestamp.
    pub fn record_input(&mut self, data: &str, timestamp_ms: u64) {
        let elapsed = (timestamp_ms - self.start_time_ms) as f64 / 1000.0;
        self.events
            .push(AsciicastEvent::input(elapsed, data.to_string()));
    }

    /// Get the number of recorded events.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Serialize to asciicast v2 format (NDJSON).
    pub fn to_asciicast(&self) -> String {
        let mut output = serde_json::to_string(&self.header).unwrap_or_default();
        output.push('\n');
        for event in &self.events {
            // Asciicast v2 format: [time, type, data]
            let line = format!(
                "[{:.6}, \"{}\", {}]",
                event.time,
                event.event_type,
                serde_json::to_string(&event.data).unwrap_or_default()
            );
            output.push_str(&line);
            output.push('\n');
        }
        output
    }
}

/// A live PTY session on Unix.
///
/// Spawns a shell process attached to a pseudo-terminal and provides
/// async read/write access to it via Tokio.
#[cfg(unix)]
pub struct PtySession {
    /// The master file descriptor (owned).
    master_fd: std::os::unix::io::OwnedFd,
    /// The child process.
    child: tokio::process::Child,
    /// Session configuration.
    config: PtyConfig,
}

#[cfg(unix)]
impl PtySession {
    /// Spawn a new PTY session with the given config.
    ///
    /// Uses `openpty()` to create a pseudo-terminal pair, then forks
    /// the shell process with the slave end as its controlling terminal.
    pub fn spawn(config: PtyConfig) -> std::io::Result<Self> {
        use std::os::unix::io::{FromRawFd, OwnedFd};

        // Allocate a PTY pair
        let mut master: libc::c_int = 0;
        let mut slave: libc::c_int = 0;
        let ret = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }

        // Wrap in OwnedFd for safety
        let master_fd = unsafe { OwnedFd::from_raw_fd(master) };
        let slave_fd = unsafe { OwnedFd::from_raw_fd(slave) };

        // Set initial terminal size
        let winsize = libc::winsize {
            ws_row: config.size.rows,
            ws_col: config.size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(master, libc::TIOCSWINSZ, &winsize);
        }

        // Determine shell
        let shell = config
            .shell
            .clone()
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));

        // Build command — use the slave fd as stdin/stdout/stderr
        let slave_file: std::fs::File = slave_fd.into();
        let slave_stdout = slave_file.try_clone()?;
        let slave_stderr = slave_file.try_clone()?;

        let mut cmd = tokio::process::Command::new(&shell);
        cmd.stdin(slave_file)
            .stdout(slave_stdout)
            .stderr(slave_stderr);

        if let Some(ref cwd) = config.cwd {
            cmd.current_dir(cwd);
        }

        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        // Create a new session and set controlling terminal
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(0, libc::TIOCSCTTY as _, 0);
                Ok(())
            });
        }

        let child = cmd.spawn()?;

        Ok(Self {
            master_fd,
            child,
            config,
        })
    }

    /// Write data to the PTY (input to the shell).
    pub fn write(&self, data: &[u8]) -> std::io::Result<usize> {
        use std::os::unix::io::AsRawFd;
        let n =
            unsafe { libc::write(self.master_fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
        if n < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    /// Read data from the PTY (output from the shell).
    /// This is a blocking call — use `tokio::task::spawn_blocking` if needed.
    pub fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::os::unix::io::AsRawFd;
        let n = unsafe {
            libc::read(
                self.master_fd.as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                Ok(0)
            } else {
                Err(err)
            }
        } else {
            Ok(n as usize)
        }
    }

    /// Resize the terminal.
    pub fn resize(&self, size: TerminalSize) {
        use std::os::unix::io::AsRawFd;
        let winsize = libc::winsize {
            ws_row: size.rows,
            ws_col: size.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        unsafe {
            libc::ioctl(self.master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize);
        }
    }

    /// Send a signal to the PTY child process.
    pub fn signal(&self, sig: PtySignal) -> std::io::Result<()> {
        if let Some(pid) = self.child.id() {
            let signum = match sig {
                PtySignal::Sigint => libc::SIGINT,
                PtySignal::Sigterm => libc::SIGTERM,
                PtySignal::Sigkill => libc::SIGKILL,
                PtySignal::Sighup => libc::SIGHUP,
                PtySignal::Sigwinch => libc::SIGWINCH,
            };
            let ret = unsafe { libc::kill(pid as libc::pid_t, signum) };
            if ret == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "child process has no PID",
            ))
        }
    }

    /// Wait for the child process to exit, returning the exit code.
    pub async fn wait(&mut self) -> std::io::Result<i32> {
        let status = self.child.wait().await?;
        Ok(status.code().unwrap_or(-1))
    }

    /// Get the PTY configuration.
    pub fn config(&self) -> &PtyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_size_default() {
        let size = TerminalSize::default();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_pty_config_default() {
        let config = PtyConfig::default();
        assert!(config.shell.is_none());
        assert_eq!(config.timeout_secs, 3600);
        assert!(!config.record);
    }

    #[test]
    fn test_session_recorder() {
        let mut recorder = SessionRecorder::new(80, 24, 1000);
        recorder.record_output("hello", 1500);
        recorder.record_input("ls\r\n", 2000);
        recorder.record_output("file1.txt\r\nfile2.txt\r\n", 2100);

        assert_eq!(recorder.event_count(), 3);
    }

    #[test]
    fn test_asciicast_output() {
        let mut recorder = SessionRecorder::new(120, 40, 0);
        recorder.record_output("$ ", 500);
        recorder.record_input("echo hi\r\n", 1000);
        recorder.record_output("hi\r\n$ ", 1200);

        let cast = recorder.to_asciicast();
        assert!(cast.contains("\"version\":2"));
        assert!(cast.contains("\"width\":120"));
        assert!(cast.contains("[0.500000, \"o\""));
        assert!(cast.contains("[1.000000, \"i\""));
    }

    #[test]
    fn test_pty_input_serde() {
        let input = PtyInput::Data {
            data: b"hello".to_vec(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"type\":\"data\""));

        let resize = PtyInput::Resize {
            size: TerminalSize {
                rows: 50,
                cols: 120,
            },
        };
        let json = serde_json::to_string(&resize).unwrap();
        assert!(json.contains("\"type\":\"resize\""));
    }

    #[test]
    fn test_session_states() {
        let info = SessionInfo {
            session_id: "sess-001".into(),
            agent_id: "web-01".into(),
            state: SessionState::Active,
            size: TerminalSize::default(),
            shell: "/bin/bash".into(),
            started_at_ms: 1700000000000,
            duration_ms: 0,
            exit_code: None,
        };
        assert_eq!(info.state, SessionState::Active);
        assert!(info.exit_code.is_none());
    }

    #[test]
    fn test_pty_output_variants() {
        let exit = PtyOutput::Exit { code: 0 };
        let json = serde_json::to_string(&exit).unwrap();
        assert!(json.contains("\"code\":0"));

        let err = PtyOutput::Error {
            message: "spawn failed".into(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("spawn failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_pty_session_spawn_and_exit() {
        use super::PtySession;

        let config = PtyConfig {
            shell: Some("/bin/sh".into()),
            size: TerminalSize { rows: 24, cols: 80 },
            cwd: None,
            env: vec![],
            timeout_secs: 10,
            record: false,
        };

        let mut session = PtySession::spawn(config).unwrap();

        // Give the shell time to start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Send "exit" command
        session.write(b"exit\n").unwrap();

        // Drain output while waiting for exit (shell may block writing prompt)
        let exit_code = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                // Drain output to unblock the shell
                let mut buf = vec![0u8; 4096];
                let _ = session.read(&mut buf);
                // Try wait
                match session.child.try_wait() {
                    Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
                    Ok(None) => {}
                    Err(e) => return Err(e),
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("PTY wait timed out")
        .unwrap();
        assert_eq!(exit_code, 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_pty_session_echo() {
        use super::PtySession;

        let config = PtyConfig {
            shell: Some("/bin/sh".into()),
            size: TerminalSize { rows: 24, cols: 80 },
            cwd: None,
            env: vec![],
            timeout_secs: 10,
            record: false,
        };

        let mut session = PtySession::spawn(config).unwrap();

        // Give the shell time to start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Write a command
        session.write(b"echo PTYTEST123\n").unwrap();

        // Give the shell time to process
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Read output (blocking read in spawn_blocking)
        let mut buf = vec![0u8; 4096];
        let n = session.read(&mut buf).unwrap();
        let output = String::from_utf8_lossy(&buf[..n]);
        assert!(
            output.contains("PTYTEST123"),
            "expected PTYTEST123 in output: {output}"
        );

        session.write(b"exit\n").unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), session.wait()).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_pty_session_resize() {
        use super::PtySession;

        let config = PtyConfig {
            shell: Some("/bin/sh".into()),
            size: TerminalSize { rows: 24, cols: 80 },
            cwd: None,
            env: vec![],
            timeout_secs: 10,
            record: false,
        };

        let mut session = PtySession::spawn(config).unwrap();
        // Resize should not panic or error
        session.resize(TerminalSize {
            rows: 50,
            cols: 120,
        });

        session.write(b"exit\n").unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), session.wait()).await;
    }
}
