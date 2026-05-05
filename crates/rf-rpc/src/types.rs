use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RPC request sent from client/orchestrator to agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
}

/// The action to perform on the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Execute {
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

/// RPC response sent from agent back to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub result: RpcResult,
}

/// The result of an RPC action.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
