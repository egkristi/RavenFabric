//! Log tailing framework.
//!
//! Provides types for configuring log collection from files,
//! journald, and structured log parsing (JSON, logfmt, regex/grok).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Source of logs to tail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogSource {
    /// Tail a file by path (supports glob patterns).
    File {
        /// Glob pattern for log files (e.g., "/var/log/app/*.log").
        pattern: String,
        /// Follow rotation (detect when file is rotated and reopen).
        follow_rotation: bool,
    },
    /// Read from systemd journald.
    Journald {
        /// Filter by systemd unit name.
        unit: Option<String>,
        /// Filter by syslog identifier.
        identifier: Option<String>,
        /// Only entries since this boot.
        current_boot: bool,
    },
    /// Read from a command's stdout/stderr.
    Command {
        /// Command to run for log output.
        command: String,
    },
}

/// Log parsing format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum LogFormat {
    /// Raw lines, no parsing.
    Raw,
    /// JSON structured logs (one object per line).
    Json,
    /// Logfmt key=value pairs.
    Logfmt,
    /// Custom regex with named capture groups.
    Regex { pattern: String },
    /// Grok pattern (e.g., "%{TIMESTAMP_ISO8601:ts} %{LOGLEVEL:level} %{GREEDYDATA:msg}").
    Grok { pattern: String },
}

/// A parsed log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Raw line content.
    pub raw: String,
    /// Parsed timestamp (ISO 8601) if extracted.
    pub timestamp: Option<String>,
    /// Log level if extracted.
    pub level: Option<LogLevel>,
    /// Structured fields extracted by the parser.
    pub fields: std::collections::HashMap<String, String>,
    /// Source identifier (file path, unit name, etc.).
    pub source: String,
    /// Hostname/agent ID.
    pub agent_id: String,
}

/// Log severity level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

/// Configuration for a log tailing job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailConfig {
    /// Unique name for this tail job.
    pub name: String,
    /// Source to tail.
    pub source: LogSource,
    /// How to parse the log lines.
    pub format: LogFormat,
    /// Minimum level to forward (filter out lower levels).
    pub min_level: Option<LogLevel>,
    /// Maximum lines per second to forward (rate limiting).
    pub rate_limit: Option<u32>,
    /// Include filter regex (only forward matching lines).
    pub include_pattern: Option<String>,
    /// Exclude filter regex (drop matching lines).
    pub exclude_pattern: Option<String>,
}

/// State of a log tailing job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailState {
    /// Not started yet.
    Idle,
    /// Actively tailing.
    Running,
    /// Paused (e.g., rate limited).
    Paused,
    /// Source not found or unreadable.
    Error,
    /// Completed (non-follow mode or source removed).
    Stopped,
}

/// Statistics for a log tailing job.
#[derive(Debug, Clone, Default)]
pub struct TailStats {
    /// Total lines read.
    pub lines_read: u64,
    /// Lines forwarded (after filtering).
    pub lines_forwarded: u64,
    /// Lines dropped by rate limiting.
    pub lines_rate_limited: u64,
    /// Lines dropped by level/pattern filter.
    pub lines_filtered: u64,
    /// Bytes read total.
    pub bytes_read: u64,
    /// Number of file rotations detected.
    pub rotations: u32,
}

impl TailStats {
    /// Record a line that was read and forwarded.
    pub fn record_forwarded(&mut self, bytes: u64) {
        self.lines_read += 1;
        self.lines_forwarded += 1;
        self.bytes_read += bytes;
    }

    /// Record a line that was filtered out.
    pub fn record_filtered(&mut self, bytes: u64) {
        self.lines_read += 1;
        self.lines_filtered += 1;
        self.bytes_read += bytes;
    }

    /// Record a line that was rate-limited.
    pub fn record_rate_limited(&mut self, bytes: u64) {
        self.lines_read += 1;
        self.lines_rate_limited += 1;
        self.bytes_read += bytes;
    }

    /// Record a file rotation.
    pub fn record_rotation(&mut self) {
        self.rotations += 1;
    }
}

/// Parse a logfmt line into key-value pairs.
pub fn parse_logfmt(line: &str) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();
    let mut chars = line.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek() == Some(&' ') {
            chars.next();
        }

        // Read key
        let key: String = chars.by_ref().take_while(|c| *c != '=').collect();
        if key.is_empty() {
            break;
        }

        // Read value
        let value = if chars.peek() == Some(&'"') {
            chars.next(); // consume opening quote
            let v: String = chars.by_ref().take_while(|c| *c != '"').collect();
            v
        } else {
            chars.by_ref().take_while(|c| *c != ' ').collect()
        };

        fields.insert(key, value);
    }

    fields
}

/// Parse a JSON log line, extracting common fields.
pub fn parse_json_line(line: &str) -> Option<LogEntry> {
    let obj: serde_json::Value = serde_json::from_str(line).ok()?;
    let map = obj.as_object()?;

    let mut fields = std::collections::HashMap::new();
    for (k, v) in map {
        if let Some(s) = v.as_str() {
            fields.insert(k.clone(), s.to_string());
        } else {
            fields.insert(k.clone(), v.to_string());
        }
    }

    let timestamp = fields
        .remove("timestamp")
        .or_else(|| fields.remove("ts"))
        .or_else(|| fields.remove("time"));

    let level = fields
        .remove("level")
        .or_else(|| fields.remove("severity"))
        .and_then(|l| parse_level(&l));

    Some(LogEntry {
        raw: line.to_string(),
        timestamp,
        level,
        fields,
        source: String::new(),
        agent_id: String::new(),
    })
}

/// Parse a log level string.
pub fn parse_level(s: &str) -> Option<LogLevel> {
    match s.to_lowercase().as_str() {
        "trace" => Some(LogLevel::Trace),
        "debug" => Some(LogLevel::Debug),
        "info" | "information" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" | "err" => Some(LogLevel::Error),
        "fatal" | "critical" | "crit" => Some(LogLevel::Fatal),
        _ => None,
    }
}

/// Resolve glob pattern to matching file paths.
pub fn resolve_glob(pattern: &str) -> Vec<PathBuf> {
    // In production this would use the `glob` crate.
    // For now, if the pattern contains no wildcards, treat as literal path.
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        // Placeholder: return empty for glob patterns (needs glob crate)
        Vec::new()
    } else {
        let path = PathBuf::from(pattern);
        if path.exists() {
            vec![path]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_logfmt() {
        let fields =
            parse_logfmt("ts=2024-01-15T10:30:00Z level=info msg=\"hello world\" service=api");
        assert_eq!(fields.get("ts").unwrap(), "2024-01-15T10:30:00Z");
        assert_eq!(fields.get("level").unwrap(), "info");
        assert_eq!(fields.get("msg").unwrap(), "hello world");
        assert_eq!(fields.get("service").unwrap(), "api");
    }

    #[test]
    fn test_parse_json_line() {
        let line = r#"{"timestamp":"2024-01-15T10:30:00Z","level":"error","msg":"connection failed","host":"web-01"}"#;
        let entry = parse_json_line(line).unwrap();
        assert_eq!(entry.timestamp.as_deref(), Some("2024-01-15T10:30:00Z"));
        assert_eq!(entry.level, Some(LogLevel::Error));
        assert_eq!(entry.fields.get("msg").unwrap(), "connection failed");
    }

    #[test]
    fn test_parse_level() {
        assert_eq!(parse_level("info"), Some(LogLevel::Info));
        assert_eq!(parse_level("WARNING"), Some(LogLevel::Warn));
        assert_eq!(parse_level("FATAL"), Some(LogLevel::Fatal));
        assert_eq!(parse_level("critical"), Some(LogLevel::Fatal));
        assert_eq!(parse_level("unknown"), None);
    }

    #[test]
    fn test_tail_stats() {
        let mut stats = TailStats::default();
        stats.record_forwarded(100);
        stats.record_forwarded(50);
        stats.record_filtered(30);
        stats.record_rate_limited(20);
        stats.record_rotation();

        assert_eq!(stats.lines_read, 4);
        assert_eq!(stats.lines_forwarded, 2);
        assert_eq!(stats.lines_filtered, 1);
        assert_eq!(stats.lines_rate_limited, 1);
        assert_eq!(stats.bytes_read, 200);
        assert_eq!(stats.rotations, 1);
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Fatal);
    }

    #[test]
    fn test_tail_config_serde() {
        let config = TailConfig {
            name: "app-logs".into(),
            source: LogSource::File {
                pattern: "/var/log/app/*.log".into(),
                follow_rotation: true,
            },
            format: LogFormat::Json,
            min_level: Some(LogLevel::Warn),
            rate_limit: Some(1000),
            include_pattern: None,
            exclude_pattern: Some("healthcheck".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: TailConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "app-logs");
        assert_eq!(parsed.min_level, Some(LogLevel::Warn));
    }

    #[test]
    fn test_resolve_glob_literal_nonexistent() {
        let paths = resolve_glob("/nonexistent/path/foo.log");
        assert!(paths.is_empty());
    }
}
