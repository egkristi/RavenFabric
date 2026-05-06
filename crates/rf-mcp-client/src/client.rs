//! High-level MCP client for interacting with RavenFabric MCP servers.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::json;

use crate::error::McpError;
use crate::protocol::{ExecResult, FileContent, JsonRpcRequest, PolicyDecision, ToolCapability};
use crate::transport::StdioTransport;

/// MCP client for RavenFabric.
///
/// Provides type-safe methods for all RavenFabric MCP tools.
pub struct McpClient {
    transport: StdioTransport,
    request_id: AtomicU64,
    initialized: AtomicBool,
    timeout: Duration,
}

impl McpClient {
    /// Create a new MCP client wrapping a transport.
    pub fn new(transport: StdioTransport) -> Self {
        Self {
            transport,
            request_id: AtomicU64::new(1),
            initialized: AtomicBool::new(false),
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Initialize the MCP session (required before making tool calls).
    pub async fn initialize(&self) -> Result<(), McpError> {
        let req = self.make_request(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "rf-mcp-client",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let resp = tokio::time::timeout(self.timeout, self.transport.request(&req))
            .await
            .map_err(|_| McpError::Timeout(self.timeout.as_millis() as u64))??;

        if let Some(err) = resp.error {
            return Err(McpError::ServerError {
                code: err.code,
                message: err.message,
            });
        }

        self.initialized.store(true, Ordering::Release);

        // Send initialized notification
        let notif = self.make_request("notifications/initialized", None);
        // Notification — don't wait for response (fire and forget via write)
        let _ = self.transport.request(&notif).await;

        Ok(())
    }

    /// Execute a command on the target system.
    pub async fn exec(
        &self,
        command: &str,
        workdir: Option<&str>,
        reason: Option<&str>,
    ) -> Result<ExecResult, McpError> {
        self.check_initialized()?;

        let mut args = json!({"command": command});
        if let Some(wd) = workdir {
            args["workdir"] = json!(wd);
        }
        if let Some(r) = reason {
            args["reason"] = json!(r);
        }

        let result = self.call_tool("rf_exec", args).await?;
        parse_exec_result(&result)
    }

    /// Query whether a command would be allowed by policy (without executing).
    pub async fn query_policy(&self, command: &str) -> Result<PolicyDecision, McpError> {
        self.check_initialized()?;

        let result = self
            .call_tool("rf_query_policy", json!({"command": command}))
            .await?;
        parse_policy_decision(&result)
    }

    /// Read a file from the target filesystem.
    pub async fn file_read(&self, path: &str) -> Result<FileContent, McpError> {
        self.check_initialized()?;

        let result = self
            .call_tool("rf_file_read", json!({"path": path}))
            .await?;
        parse_file_content(&result, path)
    }

    /// Write content to a file on the target filesystem.
    pub async fn file_write(&self, path: &str, content: &str) -> Result<(), McpError> {
        self.check_initialized()?;

        let result = self
            .call_tool("rf_file_write", json!({"path": path, "content": content}))
            .await?;

        // Check for policy denial in result
        if let Some(text) = extract_text_content(&result) {
            if text.contains("denied") || text.contains("DENIED") {
                return Err(McpError::PolicyDenied(text));
            }
        }
        Ok(())
    }

    /// List capabilities available to this session.
    pub async fn list_capabilities(&self) -> Result<Vec<ToolCapability>, McpError> {
        self.check_initialized()?;

        let result = self.call_tool("rf_list_my_capabilities", json!({})).await?;
        parse_capabilities(&result)
    }

    /// Request human approval for a sensitive operation.
    pub async fn request_approval(&self, command: &str, reason: &str) -> Result<String, McpError> {
        self.check_initialized()?;

        let result = self
            .call_tool(
                "rf_request_approval",
                json!({"command": command, "reason": reason}),
            )
            .await?;

        Ok(extract_text_content(&result).unwrap_or_default())
    }

    /// List available tools from the server.
    pub async fn list_tools(&self) -> Result<Vec<ToolCapability>, McpError> {
        let req = self.make_request("tools/list", None);
        let resp = tokio::time::timeout(self.timeout, self.transport.request(&req))
            .await
            .map_err(|_| McpError::Timeout(self.timeout.as_millis() as u64))??;

        if let Some(err) = resp.error {
            return Err(McpError::ServerError {
                code: err.code,
                message: err.message,
            });
        }

        let result = resp.result.unwrap_or_default();
        let tools = result["tools"].as_array().cloned().unwrap_or_default();
        Ok(tools
            .iter()
            .filter_map(|t| {
                Some(ToolCapability {
                    name: t["name"].as_str()?.to_string(),
                    description: t["description"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    /// Call a tool by name with the given arguments.
    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let req = self.make_request(
            "tools/call",
            Some(json!({
                "name": tool_name,
                "arguments": arguments
            })),
        );

        let resp = tokio::time::timeout(self.timeout, self.transport.request(&req))
            .await
            .map_err(|_| McpError::Timeout(self.timeout.as_millis() as u64))??;

        if let Some(err) = resp.error {
            return Err(McpError::ServerError {
                code: err.code,
                message: err.message,
            });
        }

        Ok(resp.result.unwrap_or_default())
    }

    fn make_request(&self, method: &str, params: Option<serde_json::Value>) -> JsonRpcRequest {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        JsonRpcRequest::new(id, method, params)
    }

    fn check_initialized(&self) -> Result<(), McpError> {
        if !self.initialized.load(Ordering::Acquire) {
            return Err(McpError::NotInitialized);
        }
        Ok(())
    }
}

// --- Response parsers ---

fn extract_text_content(result: &serde_json::Value) -> Option<String> {
    let content = result["content"].as_array()?;
    let text_item = content.iter().find(|c| c["type"] == "text")?;
    text_item["text"].as_str().map(String::from)
}

fn parse_exec_result(result: &serde_json::Value) -> Result<ExecResult, McpError> {
    let text = extract_text_content(result)
        .ok_or_else(|| McpError::Protocol("no text content in response".into()))?;

    // Check for policy denial
    if text.contains("DENIED") || text.contains("denied by policy") {
        return Ok(ExecResult {
            output: String::new(),
            stderr: text.clone(),
            exit_code: -1,
            allowed: false,
        });
    }

    // Parse successful execution result
    // The server returns output as text content
    let is_error = result["isError"].as_bool().unwrap_or(false);
    Ok(ExecResult {
        output: if is_error {
            String::new()
        } else {
            text.clone()
        },
        stderr: if is_error { text } else { String::new() },
        exit_code: if is_error { 1 } else { 0 },
        allowed: true,
    })
}

fn parse_policy_decision(result: &serde_json::Value) -> Result<PolicyDecision, McpError> {
    let text = extract_text_content(result)
        .ok_or_else(|| McpError::Protocol("no text content in response".into()))?;

    let allowed = text.contains("allowed") || text.contains("ALLOWED");
    Ok(PolicyDecision {
        allowed,
        matched_rule: text.clone(),
        reason: text,
    })
}

fn parse_file_content(result: &serde_json::Value, path: &str) -> Result<FileContent, McpError> {
    let text = extract_text_content(result)
        .ok_or_else(|| McpError::Protocol("no text content in response".into()))?;

    if text.contains("DENIED") || text.contains("denied") {
        return Err(McpError::PolicyDenied(text));
    }

    Ok(FileContent {
        path: path.to_string(),
        content: text,
    })
}

fn parse_capabilities(result: &serde_json::Value) -> Result<Vec<ToolCapability>, McpError> {
    let text = extract_text_content(result)
        .ok_or_else(|| McpError::Protocol("no text content in response".into()))?;

    // Capabilities are returned as text — parse tool names from the list
    Ok(text
        .lines()
        .filter(|line| line.starts_with("- ") || line.starts_with("* "))
        .map(|line| {
            let name = line.trim_start_matches("- ").trim_start_matches("* ");
            ToolCapability {
                name: name.to_string(),
                description: String::new(),
            }
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exec_result_success() {
        let result = json!({
            "content": [{"type": "text", "text": "hello world\n"}]
        });
        let exec = parse_exec_result(&result).unwrap();
        assert!(exec.allowed);
        assert_eq!(exec.output, "hello world\n");
        assert_eq!(exec.exit_code, 0);
    }

    #[test]
    fn test_parse_exec_result_denied() {
        let result = json!({
            "content": [{"type": "text", "text": "DENIED by policy: command matches deny rule"}],
            "isError": true
        });
        let exec = parse_exec_result(&result).unwrap();
        assert!(!exec.allowed);
        assert_eq!(exec.exit_code, -1);
    }

    #[test]
    fn test_parse_policy_decision_allowed() {
        let result = json!({
            "content": [{"type": "text", "text": "allowed: matches rule ^ls.*"}]
        });
        let decision = parse_policy_decision(&result).unwrap();
        assert!(decision.allowed);
    }

    #[test]
    fn test_parse_policy_decision_denied() {
        let result = json!({
            "content": [{"type": "text", "text": "denied: matches deny rule .*rm.*-rf.*"}]
        });
        let decision = parse_policy_decision(&result).unwrap();
        assert!(!decision.allowed);
    }

    #[test]
    fn test_parse_file_content() {
        let result = json!({
            "content": [{"type": "text", "text": "file contents here"}]
        });
        let file = parse_file_content(&result, "/tmp/test.txt").unwrap();
        assert_eq!(file.path, "/tmp/test.txt");
        assert_eq!(file.content, "file contents here");
    }

    #[test]
    fn test_parse_file_content_denied() {
        let result = json!({
            "content": [{"type": "text", "text": "DENIED: path not in allowed list"}]
        });
        let err = parse_file_content(&result, "/etc/shadow").unwrap_err();
        assert!(matches!(err, McpError::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn test_client_not_initialized() {
        let transport = StdioTransport::spawn("cat", &[]).unwrap();
        let client = McpClient::new(transport);
        let result = client.exec("ls", None, None).await;
        assert!(matches!(result, Err(McpError::NotInitialized)));
    }
}
