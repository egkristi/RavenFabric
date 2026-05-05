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
}
