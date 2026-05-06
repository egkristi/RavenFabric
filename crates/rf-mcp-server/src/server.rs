//! MCP server core — handles JSON-RPC messages and dispatches to tools.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rf_audit::logger::{AuditLogger, FileAuditLogger};
use rf_audit::types::AuditEntry;
use rf_executor::command::Executor;
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::protocol::{INTERNAL_ERROR, JsonRpcRequest, JsonRpcResponse, MCP_VERSION};
use crate::tools;

/// The MCP server instance.
pub struct McpServer {
    executor: Arc<Executor>,
    policy: Arc<RwLock<RpcPolicy>>,
    audit: Arc<dyn AuditLogger>,
    session_id: String,
    /// Audit entries recorded in this session (for rf_audit_query).
    session_log: Arc<tokio::sync::Mutex<Vec<AuditEntry>>>,
    /// Pending approval requests.
    approvals: Arc<tokio::sync::Mutex<HashMap<String, ApprovalRequest>>>,
    /// Optional API token for authentication. If set, `initialize` must include it.
    api_token: Option<String>,
    /// Whether the session has been authenticated.
    authenticated: Arc<tokio::sync::Mutex<bool>>,
}

/// A pending human approval request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ApprovalRequest {
    id: String,
    operation: String,
    command: String,
    reason: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl McpServer {
    /// Create a new MCP server with the given policy and audit logger.
    ///
    /// If `api_token` is `Some`, the client must provide a matching token
    /// in the `initialize` request params (`{"apiToken": "..."}`) before
    /// any tool calls are accepted.
    pub fn new(
        policy_path: Option<&Path>,
        audit_path: Option<&Path>,
        caller_key: &str,
        api_token: Option<String>,
    ) -> anyhow::Result<Self> {
        let policy = if let Some(path) = policy_path {
            RpcPolicy::load(path)?
        } else {
            // Default deny-all policy if none specified
            RpcPolicy::from_yaml(
                "spec:\n  commands:\n    allow: []\n    deny:\n      - pattern: \".*\"\n  resources:\n    maxOutputBytes: 10485760\n    timeoutSeconds: 300\n",
            )?
        };

        let policy = Arc::new(RwLock::new(policy));

        let audit: Arc<dyn AuditLogger> = if let Some(path) = audit_path {
            Arc::new(FileAuditLogger::new(path.to_path_buf())?)
        } else {
            Arc::new(rf_audit::logger::NullAuditLogger)
        };

        let session_id = uuid::Uuid::new_v4().to_string();
        let session_log = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        let executor_policy = Arc::new(RwLock::new(if let Some(path) = policy_path {
            RpcPolicy::load(path)?
        } else {
            RpcPolicy::from_yaml(
                "spec:\n  commands:\n    allow: []\n    deny:\n      - pattern: \".*\"\n  resources:\n    maxOutputBytes: 10485760\n    timeoutSeconds: 300\n",
            )?
        }));

        let executor = Executor::new(executor_policy, audit.clone(), caller_key.to_string());

        // If no token is configured, session starts authenticated
        let authenticated = Arc::new(tokio::sync::Mutex::new(api_token.is_none()));

        Ok(Self {
            executor: Arc::new(executor),
            policy,
            audit,
            session_id,
            session_log,
            approvals: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            api_token,
            authenticated,
        })
    }

    /// Run the MCP server over stdio (stdin/stdout).
    pub async fn run_stdio(&self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        info!(session_id = %self.session_id, "MCP server started (stdio mode)");

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                info!("stdin closed, shutting down");
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            debug!(message = %trimmed, "received JSON-RPC message");

            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse::error(
                        None,
                        crate::protocol::PARSE_ERROR,
                        format!("Parse error: {e}"),
                    );
                    self.write_response(&mut stdout, &resp).await?;
                    continue;
                }
            };

            // Notifications (no id) don't get responses
            if request.id.is_none() {
                self.handle_notification(&request);
                continue;
            }

            let response = self.handle_request(&request).await;
            self.write_response(&mut stdout, &response).await?;
        }

        Ok(())
    }

    /// Write a JSON-RPC response to the output stream.
    async fn write_response(
        &self,
        writer: &mut tokio::io::Stdout,
        response: &JsonRpcResponse,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(response)?;
        debug!(response = %json, "sending response");
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    /// Handle a notification (no response expected).
    #[allow(clippy::unused_self)]
    fn handle_notification(&self, request: &JsonRpcRequest) {
        match request.method.as_str() {
            "notifications/initialized" => {
                info!("client initialized");
            }
            "notifications/cancelled" => {
                debug!("request cancelled by client");
            }
            other => {
                debug!(method = %other, "unknown notification");
            }
        }
    }

    /// Dispatch a JSON-RPC request to the appropriate handler.
    async fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        // `initialize` is always allowed (it's where auth happens)
        if request.method == "initialize" {
            return self.handle_initialize(id, &request.params).await;
        }

        // All other methods require authentication
        if !*self.authenticated.lock().await {
            warn!(method = %request.method, "rejected unauthenticated request");
            return JsonRpcResponse::error(
                id,
                INTERNAL_ERROR,
                "Authentication required. Provide apiToken in initialize params.",
            );
        }

        match request.method.as_str() {
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, &request.params).await,
            "resources/list" => self.handle_resources_list(id),
            "prompts/list" => self.handle_prompts_list(id),
            _ => JsonRpcResponse::method_not_found(id, &request.method),
        }
    }

    /// Handle MCP `initialize` — return server capabilities and validate API token.
    async fn handle_initialize(&self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        // Validate API token if one is configured
        if let Some(ref expected_token) = self.api_token {
            let provided = params.get("apiToken").and_then(|t| t.as_str());
            match provided {
                Some(token) if constant_time_eq(token.as_bytes(), expected_token.as_bytes()) => {
                    *self.authenticated.lock().await = true;
                    info!(session_id = %self.session_id, "API token validated");
                }
                Some(_) => {
                    warn!(session_id = %self.session_id, "invalid API token provided");
                    return JsonRpcResponse::error(id, INTERNAL_ERROR, "Invalid API token");
                }
                None => {
                    warn!(session_id = %self.session_id, "API token required but not provided");
                    return JsonRpcResponse::error(
                        id,
                        INTERNAL_ERROR,
                        "API token required. Include \"apiToken\" in initialize params.",
                    );
                }
            }
        }

        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": MCP_VERSION,
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "rf-mcp-server",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    /// Handle MCP `tools/list` — return available tools.
    #[allow(clippy::unused_self)]
    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, tools::list_tools())
    }

    /// Handle MCP `tools/call` — dispatch to the appropriate tool handler.
    async fn handle_tools_call(&self, id: Option<Value>, params: &Value) -> JsonRpcResponse {
        let Some(tool_name) = params.get("name").and_then(|n| n.as_str()) else {
            return JsonRpcResponse::invalid_params(id, "missing 'name' in params");
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let result = match tool_name {
            "rf_exec" => self.tool_exec(&arguments).await,
            "rf_query_policy" => self.tool_query_policy(&arguments).await,
            "rf_file_read" => self.tool_file_read(&arguments).await,
            "rf_file_write" => self.tool_file_write(&arguments).await,
            "rf_list_my_capabilities" => self.tool_list_capabilities().await,
            "rf_audit_query" => self.tool_audit_query(&arguments).await,
            "rf_request_approval" => self.tool_request_approval(&arguments).await,
            _ => Err(format!("Unknown tool: {tool_name}")),
        };

        match result {
            Ok(content) => JsonRpcResponse::success(id, content),
            Err(msg) => JsonRpcResponse::error(id, INTERNAL_ERROR, msg),
        }
    }

    /// Handle MCP `resources/list` — we expose no resources.
    #[allow(clippy::unused_self)]
    fn handle_resources_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, json!({ "resources": [] }))
    }

    /// Handle MCP `prompts/list` — we expose no prompts.
    #[allow(clippy::unused_self)]
    fn handle_prompts_list(&self, id: Option<Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(id, json!({ "prompts": [] }))
    }

    // --- Tool implementations ---

    /// Execute a command through the RavenFabric executor (policy-checked).
    async fn tool_exec(&self, args: &Value) -> Result<Value, String> {
        let command = args
            .get("command")
            .and_then(|c| c.as_str())
            .ok_or("missing 'command' argument")?
            .to_string();

        let workdir = args
            .get("workdir")
            .and_then(|w| w.as_str())
            .map(String::from);
        let reason = args
            .get("reason")
            .and_then(|r| r.as_str())
            .map(String::from);
        let timeout_ms = args.get("timeout_ms").and_then(Value::as_u64);

        let request = Request {
            id: uuid::Uuid::new_v4().to_string(),
            action: Action::Execute {
                command: command.clone(),
                env: HashMap::new(),
                workdir,
            },
            timeout_ms,
            reason: reason.clone(),
        };

        let response = self.executor.handle(request).await;
        self.record_audit(&command, &response, reason.as_deref())
            .await;

        match response.result {
            RpcResult::Success {
                stdout,
                stderr,
                exit_code,
                duration_ms,
            } => {
                let mut output = String::new();
                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr] ");
                    output.push_str(&stderr);
                }
                if output.is_empty() {
                    output = format!("(exit code: {exit_code}, {duration_ms}ms)");
                }
                Ok(tools::text_content(output))
            }
            RpcResult::Denied { reason, rule } => Ok(tools::error_content(format!(
                "DENIED: {reason}\nMatched rule: {rule}"
            ))),
            RpcResult::Error { message } => Ok(tools::error_content(format!("ERROR: {message}"))),
            _ => Ok(tools::error_content("unexpected response type")),
        }
    }

    /// Pre-flight policy check — does not execute.
    async fn tool_query_policy(&self, args: &Value) -> Result<Value, String> {
        let command = args
            .get("command")
            .and_then(|c| c.as_str())
            .ok_or("missing 'command' argument")?;

        let policy = self.policy.read().await;
        let decision = policy.check_command(command);

        let result = json!({
            "command": command,
            "decision": format!("{:?}", decision),
            "allowed": decision.allowed,
        });

        Ok(tools::text_content(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        ))
    }

    /// Read a file subject to path policy.
    async fn tool_file_read(&self, args: &Value) -> Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or("missing 'path' argument")?;

        let request = Request {
            id: uuid::Uuid::new_v4().to_string(),
            action: Action::Read {
                path: path.to_string(),
            },
            timeout_ms: None,
            reason: Some("MCP file read".into()),
        };

        let response = self.executor.handle(request).await;
        self.record_audit(&format!("read:{path}"), &response, Some("MCP file read"))
            .await;

        match response.result {
            RpcResult::Success { stdout, .. } => Ok(tools::text_content(stdout)),
            RpcResult::Denied { reason, rule } => Ok(tools::error_content(format!(
                "DENIED: {reason}\nRule: {rule}"
            ))),
            RpcResult::Error { message } => Ok(tools::error_content(message)),
            _ => Ok(tools::error_content("unexpected response type")),
        }
    }

    /// Write a file subject to path policy.
    async fn tool_file_write(&self, args: &Value) -> Result<Value, String> {
        let path = args
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or("missing 'path' argument")?;
        let content = args
            .get("content")
            .and_then(|c| c.as_str())
            .ok_or("missing 'content' argument")?;
        let mode = args.get("mode").and_then(Value::as_u64).map(|m| m as u32);

        let request = Request {
            id: uuid::Uuid::new_v4().to_string(),
            action: Action::Write {
                path: path.to_string(),
                data: content.as_bytes().to_vec(),
                mode,
            },
            timeout_ms: None,
            reason: Some("MCP file write".into()),
        };

        let response = self.executor.handle(request).await;
        self.record_audit(&format!("write:{path}"), &response, Some("MCP file write"))
            .await;

        match response.result {
            RpcResult::Success { .. } => Ok(tools::text_content(format!("Written to {path}"))),
            RpcResult::Denied { reason, rule } => Ok(tools::error_content(format!(
                "DENIED: {reason}\nRule: {rule}"
            ))),
            RpcResult::Error { message } => Ok(tools::error_content(message)),
            _ => Ok(tools::error_content("unexpected response type")),
        }
    }

    /// List capabilities allowed by the current policy.
    async fn tool_list_capabilities(&self) -> Result<Value, String> {
        let policy = self.policy.read().await;
        let info = json!({
            "max_output_bytes": policy.max_output_bytes,
            "timeout_seconds": policy.timeout_seconds,
            "session_id": self.session_id,
            "note": "Use rf_query_policy to check specific commands. All operations are deny-by-default."
        });
        Ok(tools::text_content(
            serde_json::to_string_pretty(&info).unwrap_or_default(),
        ))
    }

    /// Query recent audit entries from this session.
    async fn tool_audit_query(&self, args: &Value) -> Result<Value, String> {
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
        let action_filter = args.get("action_filter").and_then(|f| f.as_str());

        let log = self.session_log.lock().await;
        let entries: Vec<&AuditEntry> = log
            .iter()
            .rev()
            .filter(|e| action_filter.is_none_or(|f| e.action.contains(f)))
            .take(limit)
            .collect();

        let output = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into());
        Ok(tools::text_content(output))
    }

    /// Request human approval (writes to stderr for operator visibility).
    async fn tool_request_approval(&self, args: &Value) -> Result<Value, String> {
        let operation = args
            .get("operation")
            .and_then(|o| o.as_str())
            .ok_or("missing 'operation' argument")?;
        let command = args
            .get("command")
            .and_then(|c| c.as_str())
            .ok_or("missing 'command' argument")?;
        let reason = args
            .get("reason")
            .and_then(|r| r.as_str())
            .ok_or("missing 'reason' argument")?;

        let approval_id = uuid::Uuid::new_v4().to_string();

        let approval = ApprovalRequest {
            id: approval_id.clone(),
            operation: operation.to_string(),
            command: command.to_string(),
            reason: reason.to_string(),
            timestamp: chrono::Utc::now(),
        };

        // Store the pending approval
        self.approvals
            .lock()
            .await
            .insert(approval_id.clone(), approval);

        // Write to stderr so the operator can see it (stderr is not part of MCP protocol)
        warn!(
            approval_id = %approval_id,
            operation = %operation,
            command = %command,
            reason = %reason,
            "APPROVAL REQUIRED"
        );

        // In a full implementation, this would block until the operator approves/denies.
        // For now, return a pending status that the AI agent can poll.
        Ok(tools::text_content(format!(
            "Approval requested (id: {approval_id}).\n\
             Operation: {operation}\n\
             Command: {command}\n\
             Reason: {reason}\n\n\
             Status: PENDING — waiting for human operator approval.\n\
             The operator has been notified via stderr/logging."
        )))
    }

    /// Record an audit entry for this session.
    async fn record_audit(&self, command: &str, response: &Response, reason: Option<&str>) {
        let (decision, matched_rule, exit_code) = match &response.result {
            RpcResult::Success { exit_code, .. } => {
                ("allowed".to_string(), String::new(), Some(*exit_code))
            }
            RpcResult::Denied { reason, rule } => (format!("denied: {reason}"), rule.clone(), None),
            RpcResult::Error { message } => (format!("error: {message}"), String::new(), None),
            _ => ("unknown".to_string(), String::new(), None),
        };

        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            request_id: response.id.clone(),
            action: "mcp_tool_call".to_string(),
            command: Some(command.to_string()),
            decision,
            matched_rule,
            exit_code,
            duration_ms: 0,
            caller_key: self.session_id.clone(),
            reason: reason.map(String::from),
        };

        // Store in session log
        self.session_log.lock().await.push(entry.clone());

        // Write to persistent audit
        if let Err(e) = self.audit.log(entry) {
            error!(error = %e, "failed to write audit entry");
        }
    }
}

/// Constant-time byte comparison to prevent timing attacks on token validation.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_policy() -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(
            f,
            r#"spec:
  commands:
    allow:
      - pattern: "^echo .*"
      - pattern: "^ls.*"
    deny:
      - pattern: "^rm.*"
  filesystem:
    allow:
      - path: /tmp
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30"#
        )
        .unwrap();
        f
    }

    #[tokio::test]
    async fn test_server_creation() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None);
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({"protocolVersion": "2024-11-05"}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert_eq!(result["protocolVersion"], MCP_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: json!({}),
        };

        let response = server.handle_request(&request).await;
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
    }

    #[tokio::test]
    async fn test_tool_query_policy_allowed() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let result = server
            .tool_query_policy(&json!({"command": "echo hello"}))
            .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("true") || text.contains("Allowed"));
    }

    #[tokio::test]
    async fn test_tool_query_policy_denied() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let result = server
            .tool_query_policy(&json!({"command": "rm -rf /"}))
            .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("false") || text.contains("Denied"));
    }

    #[tokio::test]
    async fn test_tool_exec_denied_command() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let result = server
            .tool_exec(&json!({"command": "rm -rf /tmp/important"}))
            .await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("DENIED"));
    }

    #[tokio::test]
    async fn test_tool_audit_query_empty() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let result = server.tool_audit_query(&json!({})).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("[]"));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let policy_file = create_test_policy();
        let server = McpServer::new(Some(policy_file.path()), None, "test-caller", None).unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(99)),
            method: "nonexistent/method".into(),
            params: json!({}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_api_token_valid() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("secret-token-123".into()),
        )
        .unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({"apiToken": "secret-token-123"}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_api_token_invalid() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("secret-token-123".into()),
        )
        .unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({"apiToken": "wrong-token"}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert!(response.error.unwrap().message.contains("Invalid"));
    }

    #[tokio::test]
    async fn test_api_token_missing() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("secret-token-123".into()),
        )
        .unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert!(response.error.unwrap().message.contains("required"));
    }

    #[tokio::test]
    async fn test_unauthenticated_tool_call_rejected() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("secret-token-123".into()),
        )
        .unwrap();

        // Try to call a tool without initializing first
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: json!({}),
        };

        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert!(
            response
                .error
                .unwrap()
                .message
                .contains("Authentication required")
        );
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(constant_time_eq(b"", b""));
    }
}
