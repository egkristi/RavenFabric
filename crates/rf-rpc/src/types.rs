use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RPC request sent from client/orchestrator to agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
    /// Optional reasoning from AI agent explaining why this action is needed.
    /// Recorded in audit log for compliance and forensics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The action to perform on the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Action {
    Execute {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    /// Like Execute, but streams stdout/stderr incrementally over the connection.
    StreamExecute {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    Read {
        path: String,
    },
    Write {
        path: String,
        data: Vec<u8>,
        mode: Option<u32>,
    },
    List {
        path: String,
    },
    Metrics,
    Signal {
        pid: u32,
        signal: i32,
    },
    /// Ping/status check — agent responds with its version and uptime.
    Status,
    /// Execute a command in the background. Returns a job ID immediately.
    BackgroundExec {
        command: String,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    /// Query the status of a background job by its ID.
    JobQuery {
        job_id: String,
    },
    /// Wait for a background job to complete and return its output.
    JobWait {
        job_id: String,
    },
    /// Lightweight heartbeat ping — agent responds with Pong immediately.
    /// Used for liveness detection. No policy check required.
    Ping,
    /// Open an interactive shell session via PTY on the agent.
    Shell {
        shell: Option<String>,
        rows: u16,
        cols: u16,
        env: HashMap<String, String>,
    },
    /// Send data (input) to an active shell session.
    ShellInput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Resize an active shell session.
    ShellResize {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    /// Close an active shell session.
    ShellClose {
        session_id: String,
    },
    /// Start a local-to-remote port forward on the agent.
    PortForward {
        bind_addr: String,
        target_addr: String,
    },
    /// Stop a port forward by ID.
    PortForwardClose {
        forward_id: String,
    },
    /// Remote port forward (ssh -R equivalent): agent listens on bind_addr,
    /// and for each accepted connection, data is forwarded back to the client
    /// which connects to target_addr locally.
    RemoteForward {
        bind_addr: String,
        target_addr: String,
    },
    /// Start a SOCKS5 dynamic forward proxy on the agent.
    Socks5Forward {
        bind_addr: String,
    },
    /// Stop a SOCKS5 forward by ID.
    Socks5Close {
        forward_id: String,
    },
    /// Run a health check probe.
    HealthCheck {
        probe_type: String,
        target: String,
        timeout_ms: u64,
    },
    /// Tail a log file on the agent.
    TailLog {
        path: String,
        lines: Option<u32>,
    },
    /// Push a file chunk from client to agent (upload).
    /// Chunked transfer: client sends multiple FilePush requests for large files.
    FilePush {
        /// Destination path on agent filesystem
        path: String,
        /// Byte offset within the file (for resumable transfers)
        offset: u64,
        /// File data chunk
        data: Vec<u8>,
        /// If true, this is the final chunk — agent should finalize the file
        done: bool,
        /// Expected SHA-256 of the complete file (sent with done=true for verification)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// File mode (permissions) to set on the final file (Unix octal, e.g. 0o644)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
        /// If true, data is zstd-compressed; agent decompresses before writing.
        #[serde(default)]
        compress: bool,
    },
    /// Pull a file chunk from agent to client (download).
    /// Client specifies offset and max chunk size; agent responds with data.
    FilePull {
        /// Source path on agent filesystem
        path: String,
        /// Byte offset to start reading from
        offset: u64,
        /// Maximum bytes to return in this chunk
        max_chunk: u32,
        /// If true, agent compresses the response chunk with zstd.
        #[serde(default)]
        compress: bool,
    },
    /// Open a TCP proxy connection through the agent to a target.
    /// Agent connects to target and bridges traffic over yamux stream.
    Proxy {
        /// Target address (host:port) the agent should connect to
        target: String,
        /// Idle timeout in seconds (no data flowing = connection closed). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_timeout_secs: Option<u32>,
        /// Maximum connection duration in seconds (hard cap). None = use policy default.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_duration_secs: Option<u32>,
    },
    /// Forward an HTTP request through the agent to an upstream target.
    /// Agent inspects method + path against HTTP policy rules before forwarding.
    HttpForward {
        /// Target base URL (e.g., "localhost:8080")
        target: String,
        /// HTTP method (GET, POST, PUT, DELETE, etc.)
        method: String,
        /// Request path (e.g., "/api/v1/users")
        path: String,
        /// Request headers
        headers: HashMap<String, String>,
        /// Request body (empty for GET/HEAD/DELETE)
        #[serde(default)]
        body: Vec<u8>,
    },
}

/// Identifies which output stream a chunk belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamType {
    Stdout,
    Stderr,
}

/// RPC response sent from agent back to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub result: RpcResult,
}

/// The result of an RPC action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RpcResult {
    Success {
        stdout: String,
        stderr: String,
        exit_code: i32,
        duration_ms: u64,
    },
    Denied {
        reason: String,
        rule: String,
    },
    Error {
        message: String,
    },
    /// Response to a Status action.
    StatusInfo {
        agent_id: String,
        version: String,
        uptime_seconds: u64,
    },
    /// Incremental output chunk from a streaming execution.
    StreamChunk {
        stream: StreamType,
        data: Vec<u8>,
    },
    /// Final message from a streaming execution indicating process completion.
    StreamEnd {
        exit_code: i32,
        duration_ms: u64,
    },
    /// Background job was started successfully.
    JobStarted {
        job_id: String,
        pid: u32,
    },
    /// Status of a background job.
    JobStatus {
        job_id: String,
        running: bool,
        exit_code: Option<i32>,
        stdout: Option<String>,
        stderr: Option<String>,
    },
    /// Response to a Ping action — heartbeat acknowledgment.
    Pong {
        timestamp_ms: u64,
    },
    /// Shell session opened successfully.
    ShellOpened {
        session_id: String,
    },
    /// Output data from a shell session.
    ShellOutput {
        session_id: String,
        data: Vec<u8>,
    },
    /// Shell session has exited.
    ShellExited {
        session_id: String,
        exit_code: i32,
    },
    /// Port forward started successfully.
    ForwardStarted {
        forward_id: String,
        bind_addr: String,
    },
    /// Port forward stopped.
    ForwardStopped {
        forward_id: String,
    },
    /// Health check result.
    HealthCheckResult {
        success: bool,
        latency_ms: u64,
        error: Option<String>,
    },
    /// Log tail output.
    TailOutput {
        lines: Vec<String>,
        path: String,
    },
    /// Response to a FilePush action — chunk accepted.
    FileChunkAck {
        /// Byte offset after this chunk (next expected offset)
        offset: u64,
        /// True if file is finalized (all chunks received, checksum verified)
        finalized: bool,
    },
    /// Response to a FilePull action — file data chunk.
    FileChunk {
        /// Byte offset this chunk starts at
        offset: u64,
        /// File data
        data: Vec<u8>,
        /// Total file size in bytes
        total_size: u64,
        /// SHA-256 checksum of the entire file (sent with last chunk)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checksum: Option<String>,
        /// True if data is zstd-compressed (matches compress flag in FilePull request).
        #[serde(default)]
        compressed: bool,
    },
    /// Response to a Proxy action — connection established.
    ProxyConnected {
        /// Unique proxy session ID for this connection
        proxy_id: String,
        /// Effective idle timeout in seconds (applied by client)
        idle_timeout_secs: u32,
        /// Effective max duration in seconds (applied by client)
        max_duration_secs: u32,
    },
    /// Response to an HttpForward action — upstream HTTP response.
    HttpResponse {
        /// HTTP status code (e.g., 200, 404, 500)
        status_code: u16,
        /// Response headers
        headers: HashMap<String, String>,
        /// Response body
        body: Vec<u8>,
        /// Latency in milliseconds (time to receive full response from upstream)
        latency_ms: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;

    #[test]
    fn roundtrip_execute_request() {
        let req = Request {
            id: "req-42".into(),
            action: Action::Execute {
                command: "ls -la /tmp".into(),
                env: [("FOO".into(), "bar".into())].into_iter().collect(),
                workdir: Some("/home".into()),
            },
            timeout_ms: Some(30000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_request_with_reason() {
        let req = Request {
            id: "ai-req-1".into(),
            action: Action::Execute {
                command: "cargo test".into(),
                env: Default::default(),
                workdir: Some("/project".into()),
            },
            timeout_ms: Some(60000),
            reason: Some("Running tests to verify the refactoring didn't break anything".into()),
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
        assert_eq!(
            decoded.reason.unwrap(),
            "Running tests to verify the refactoring didn't break anything"
        );
    }

    #[test]
    fn roundtrip_stream_execute_request() {
        let req = Request {
            id: "stream-1".into(),
            action: Action::StreamExecute {
                command: "tail -f /var/log/syslog".into(),
                env: HashMap::new(),
                workdir: None,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_success_response() {
        let resp = Response {
            id: "resp-1".into(),
            result: RpcResult::Success {
                stdout: "hello world\n".into(),
                stderr: String::new(),
                exit_code: 0,
                duration_ms: 42,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_denied_response() {
        let resp = Response {
            id: "resp-2".into(),
            result: RpcResult::Denied {
                reason: "command not allowed".into(),
                rule: "deny: .*rm.*".into(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_stream_chunk() {
        let resp = Response {
            id: "stream-2".into(),
            result: RpcResult::StreamChunk {
                stream: StreamType::Stderr,
                data: b"error output\n".to_vec(),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_stream_end() {
        let resp = Response {
            id: "stream-3".into(),
            result: RpcResult::StreamEnd {
                exit_code: 1,
                duration_ms: 5000,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_status_info() {
        let resp = Response {
            id: "status-1".into(),
            result: RpcResult::StatusInfo {
                agent_id: "web-01".into(),
                version: "0.1.0".into(),
                uptime_seconds: 86400,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_all_actions() {
        let actions = vec![
            Action::Read {
                path: "/etc/hosts".into(),
            },
            Action::Write {
                path: "/tmp/test".into(),
                data: vec![1, 2, 3],
                mode: Some(0o644),
            },
            Action::List {
                path: "/var/log".into(),
            },
            Action::Metrics,
            Action::Signal {
                pid: 1234,
                signal: 15,
            },
            Action::Status,
            Action::BackgroundExec {
                command: "sleep 10".into(),
                env: HashMap::new(),
                workdir: Some("/tmp".into()),
            },
            Action::JobQuery {
                job_id: "job-123".into(),
            },
            Action::JobWait {
                job_id: "job-456".into(),
            },
            Action::Ping,
            Action::Shell {
                shell: Some("/bin/bash".into()),
                rows: 24,
                cols: 80,
                env: HashMap::new(),
            },
            Action::ShellInput {
                session_id: "sess-1".into(),
                data: b"ls\n".to_vec(),
            },
            Action::ShellResize {
                session_id: "sess-1".into(),
                rows: 50,
                cols: 120,
            },
            Action::ShellClose {
                session_id: "sess-1".into(),
            },
            Action::PortForward {
                bind_addr: "127.0.0.1:8080".into(),
                target_addr: "db:5432".into(),
            },
            Action::PortForwardClose {
                forward_id: "fwd-1".into(),
            },
            Action::RemoteForward {
                bind_addr: "0.0.0.0:2222".into(),
                target_addr: "127.0.0.1:22".into(),
            },
            Action::Socks5Forward {
                bind_addr: "127.0.0.1:1080".into(),
            },
            Action::Socks5Close {
                forward_id: "socks-1".into(),
            },
            Action::HealthCheck {
                probe_type: "tcp".into(),
                target: "127.0.0.1:80".into(),
                timeout_ms: 5000,
            },
            Action::TailLog {
                path: "/var/log/app.log".into(),
                lines: Some(100),
            },
        ];
        for action in actions {
            let req = Request {
                id: "test".into(),
                action,
                timeout_ms: None,
                reason: None,
            };
            let bytes = codec::encode(&req).unwrap();
            let decoded: Request = codec::decode(&bytes).unwrap();
            assert_eq!(req, decoded);
        }
    }

    #[test]
    fn roundtrip_job_started() {
        let resp = Response {
            id: "r-1".into(),
            result: RpcResult::JobStarted {
                job_id: "job-abc".into(),
                pid: 42,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_job_status() {
        let resp = Response {
            id: "r-2".into(),
            result: RpcResult::JobStatus {
                job_id: "job-xyz".into(),
                running: false,
                exit_code: Some(0),
                stdout: Some("output".into()),
                stderr: Some(String::new()),
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_pong() {
        let resp = Response {
            id: "ping-1".into(),
            result: RpcResult::Pong {
                timestamp_ms: 1714900000000,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_push() {
        let req = Request {
            id: "fp-1".into(),
            action: Action::FilePush {
                path: "/tmp/test.bin".into(),
                offset: 0,
                data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                done: false,
                checksum: None,
                mode: Some(0o644),
                compress: false,
            },
            timeout_ms: Some(30000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_pull() {
        let req = Request {
            id: "fp-2".into(),
            action: Action::FilePull {
                path: "/var/log/syslog".into(),
                offset: 1024,
                max_chunk: 65536,
                compress: false,
            },
            timeout_ms: None,
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn roundtrip_file_chunk_ack() {
        let resp = Response {
            id: "fp-1".into(),
            result: RpcResult::FileChunkAck {
                offset: 4,
                finalized: true,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_file_chunk() {
        let resp = Response {
            id: "fp-2".into(),
            result: RpcResult::FileChunk {
                offset: 0,
                data: vec![1, 2, 3, 4, 5],
                total_size: 100,
                checksum: Some("abc123".into()),
                compressed: false,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn roundtrip_proxy() {
        let req = Request {
            id: "px-1".into(),
            action: Action::Proxy {
                target: "10.0.0.5:5432".into(),
                idle_timeout_secs: Some(60),
                max_duration_secs: None,
            },
            timeout_ms: Some(10000),
            reason: None,
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);

        let resp = Response {
            id: "px-1".into(),
            result: RpcResult::ProxyConnected {
                proxy_id: "proxy-px-1".into(),
                idle_timeout_secs: 60,
                max_duration_secs: 3600,
            },
        };
        let bytes = codec::encode(&resp).unwrap();
        let decoded: Response = codec::decode(&bytes).unwrap();
        assert_eq!(resp, decoded);
    }
}
