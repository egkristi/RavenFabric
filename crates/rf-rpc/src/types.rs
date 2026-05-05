use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RPC request sent from client/orchestrator to agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
}

/// The action to perform on the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        };
        let bytes = codec::encode(&req).unwrap();
        let decoded: Request = codec::decode(&bytes).unwrap();
        assert_eq!(req, decoded);
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
        ];
        for action in actions {
            let req = Request {
                id: "test".into(),
                action,
                timeout_ms: None,
            };
            let bytes = codec::encode(&req).unwrap();
            let decoded: Request = codec::decode(&bytes).unwrap();
            assert_eq!(req, decoded);
        }
    }
}
