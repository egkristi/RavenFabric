//! Stdio transport for communicating with MCP servers.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::error::McpError;
use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Stdio-based MCP transport.
///
/// Spawns an MCP server process and communicates via stdin/stdout
/// using newline-delimited JSON-RPC messages.
pub struct StdioTransport {
    child: Mutex<Child>,
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
}

impl StdioTransport {
    /// Spawn an MCP server process with the given command and arguments.
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError::Transport(format!("failed to spawn {command}: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Transport("failed to capture stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("failed to capture stdout".into()))?;

        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
        })
    }

    /// Send a JSON-RPC request and receive the response.
    pub(crate) async fn request(&self, req: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let mut line = serde_json::to_string(req)?;
        line.push('\n');

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| McpError::Transport(format!("write failed: {e}")))?;
            stdin
                .flush()
                .await
                .map_err(|e| McpError::Transport(format!("flush failed: {e}")))?;
        }

        // Read response
        let mut response_line = String::new();
        {
            let mut stdout = self.stdout.lock().await;
            stdout
                .read_line(&mut response_line)
                .await
                .map_err(|e| McpError::Transport(format!("read failed: {e}")))?;
        }

        if response_line.is_empty() {
            return Err(McpError::ProcessExited("no response received".into()));
        }

        let resp: JsonRpcResponse = serde_json::from_str(response_line.trim())?;
        Ok(resp)
    }

    /// Check if the server process is still running.
    pub async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }

    /// Kill the server process.
    pub async fn shutdown(&self) -> Result<(), McpError> {
        let mut child = self.child.lock().await;
        child
            .kill()
            .await
            .map_err(|e| McpError::Transport(format!("kill failed: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_nonexistent_command() {
        let err = StdioTransport::spawn("nonexistent-rf-binary-12345", &[]).err();
        assert!(err.is_some());
        assert!(matches!(err.unwrap(), McpError::Transport(_)));
    }

    #[tokio::test]
    async fn test_spawn_echo_process() {
        // Use 'cat' as a simple echo server for transport testing
        let transport = StdioTransport::spawn("cat", &[]).unwrap();
        assert!(transport.is_alive().await);
        transport.shutdown().await.unwrap();
    }
}
