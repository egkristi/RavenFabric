//! MCP server core — handles JSON-RPC messages and dispatches to tools.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::Timelike;

use rf_audit::logger::{AuditLogger, FileAuditLogger};
use rf_audit::types::AuditEntry;
use rf_executor::command::Executor;
use rf_policy::anomaly::{AnomalyConfig, AnomalyResponse, IdentityBaseline};
use rf_policy::rpc_policy::RpcPolicy;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::protocol::{INTERNAL_ERROR, JsonRpcRequest, JsonRpcResponse, MCP_VERSION};
use crate::tools;

/// Default maximum requests per minute for rate limiting.
const DEFAULT_MAX_REQUESTS_PER_MINUTE: u32 = 60;

/// Session rate limiter using a sliding window.
struct RateLimiter {
    /// Maximum requests allowed per window.
    max_per_window: u32,
    /// Window duration.
    window: std::time::Duration,
    /// Timestamps of recent requests.
    timestamps: std::collections::VecDeque<std::time::Instant>,
}

impl RateLimiter {
    fn new(max_per_minute: u32) -> Self {
        Self {
            max_per_window: max_per_minute,
            window: std::time::Duration::from_secs(60),
            timestamps: std::collections::VecDeque::new(),
        }
    }

    /// Check if a request is allowed. Returns Ok(()) or Err with remaining wait time.
    fn check(&mut self) -> Result<(), std::time::Duration> {
        let now = std::time::Instant::now();

        // Remove expired timestamps
        while let Some(front) = self.timestamps.front() {
            if now.duration_since(*front) > self.window {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }

        if self.timestamps.len() >= self.max_per_window as usize {
            // Rate limited — calculate how long until the oldest entry expires
            // Safety: timestamps is non-empty since len >= max_per_window > 0
            let oldest = self
                .timestamps
                .front()
                .expect("timestamps non-empty when len >= max_per_window");
            let wait = self.window - now.duration_since(*oldest);
            Err(wait)
        } else {
            self.timestamps.push_back(now);
            Ok(())
        }
    }

    /// Get current request count in the window.
    fn current_count(&self) -> u32 {
        self.timestamps.len() as u32
    }
}

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
    /// Per-session rate limiter.
    rate_limiter: Arc<tokio::sync::Mutex<RateLimiter>>,
    /// Per-session behavioral anomaly tracker.
    anomaly_tracker: Arc<tokio::sync::Mutex<IdentityBaseline>>,
    /// Optional webhook URL for anomaly/security alerts.
    alert_webhook: Option<String>,
}

/// Status of an approval request.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
}

/// A pending human approval request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ApprovalRequest {
    id: String,
    operation: String,
    command: String,
    reason: String,
    status: ApprovalStatus,
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
        max_requests_per_minute: Option<u32>,
        alert_webhook: Option<String>,
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

        let rate_limit = max_requests_per_minute.unwrap_or(DEFAULT_MAX_REQUESTS_PER_MINUTE);
        let rate_limiter = Arc::new(tokio::sync::Mutex::new(RateLimiter::new(rate_limit)));

        let anomaly_tracker = Arc::new(tokio::sync::Mutex::new(IdentityBaseline::new(
            caller_key,
            AnomalyConfig::default(),
        )));

        Ok(Self {
            executor: Arc::new(executor),
            policy,
            audit,
            session_id,
            session_log,
            approvals: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            api_token,
            authenticated,
            rate_limiter,
            anomaly_tracker,
            alert_webhook,
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

        // Rate limit tool calls (not metadata queries like tools/list)
        if request.method == "tools/call" {
            let mut limiter = self.rate_limiter.lock().await;
            if let Err(wait) = limiter.check() {
                warn!(
                    method = %request.method,
                    wait_secs = wait.as_secs(),
                    count = limiter.current_count(),
                    "rate limited"
                );
                return JsonRpcResponse::error(
                    id,
                    INTERNAL_ERROR,
                    format!(
                        "Rate limited. Too many requests. Try again in {} seconds.",
                        wait.as_secs() + 1
                    ),
                );
            }
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
                Some(token) if self.validate_token(token, expected_token) => {
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
                },
                "sessionId": self.session_id
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
            "rf_check_approval" => self.tool_check_approval(&arguments).await,
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
            status: ApprovalStatus::Pending,
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
             The operator has been notified via stderr/logging.\n\
             Use rf_check_approval with this ID to poll status."
        )))
    }

    /// Check the status of a pending approval request.
    async fn tool_check_approval(&self, args: &Value) -> Result<Value, String> {
        let approval_id = args
            .get("approval_id")
            .and_then(|id| id.as_str())
            .ok_or("missing 'approval_id' argument")?;

        let approvals = self.approvals.lock().await;
        match approvals.get(approval_id) {
            Some(approval) => {
                let status = match approval.status {
                    ApprovalStatus::Pending => "PENDING",
                    ApprovalStatus::Approved => "APPROVED",
                    ApprovalStatus::Denied => "DENIED",
                };
                Ok(tools::text_content(format!(
                    "Approval ID: {}\n\
                     Operation: {}\n\
                     Command: {}\n\
                     Status: {status}\n\
                     Requested: {}",
                    approval.id, approval.operation, approval.command, approval.timestamp
                )))
            }
            None => Ok(tools::error_content(format!(
                "No approval found with ID: {approval_id}"
            ))),
        }
    }

    /// Approve a pending request (called via operator mechanism, not by AI).
    #[allow(dead_code)]
    pub async fn approve(&self, approval_id: &str) -> bool {
        let mut approvals = self.approvals.lock().await;
        if let Some(approval) = approvals.get_mut(approval_id) {
            approval.status = ApprovalStatus::Approved;
            info!(approval_id = %approval_id, command = %approval.command, "approval GRANTED");
            true
        } else {
            false
        }
    }

    /// Deny a pending request (called via operator mechanism, not by AI).
    #[allow(dead_code)]
    pub async fn deny(&self, approval_id: &str) -> bool {
        let mut approvals = self.approvals.lock().await;
        if let Some(approval) = approvals.get_mut(approval_id) {
            approval.status = ApprovalStatus::Denied;
            info!(approval_id = %approval_id, command = %approval.command, "approval DENIED");
            true
        } else {
            false
        }
    }

    /// Validate a provided token against expected token(s).
    /// Supports comma-separated tokens for rotation grace period
    /// (e.g., "new-token,old-token" allows both during transition).
    #[allow(clippy::unused_self)]
    fn validate_token(&self, provided: &str, expected: &str) -> bool {
        // Check if expected contains multiple tokens (comma-separated)
        for candidate in expected.split(',') {
            let candidate = candidate.trim();
            if !candidate.is_empty() && constant_time_eq(provided.as_bytes(), candidate.as_bytes())
            {
                return true;
            }
        }
        false
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
            decision: decision.clone(),
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

        // Feed anomaly tracker — record command with denied status
        let denied = decision.starts_with("denied");
        let hour = chrono::Utc::now().hour() as u8;
        let anomalies = self
            .anomaly_tracker
            .lock()
            .await
            .record_command(command, denied, hour);

        // Write anomaly events as audit entries
        for event in &anomalies {
            let anomaly_entry = AuditEntry {
                timestamp: chrono::Utc::now(),
                request_id: uuid::Uuid::new_v4().to_string(),
                action: "anomaly_detected".to_string(),
                command: Some(command.to_string()),
                decision: format!("{:?}", event.anomaly_type),
                matched_rule: format!("score={:.2}", event.score),
                exit_code: None,
                duration_ms: 0,
                caller_key: self.session_id.clone(),
                reason: Some(event.description.clone()),
            };
            self.session_log.lock().await.push(anomaly_entry.clone());
            if let Err(e) = self.audit.log(anomaly_entry) {
                error!(error = %e, "failed to write anomaly audit entry");
            }
        }

        // Send webhook alert for anomaly events
        if !anomalies.is_empty() {
            if let Some(ref webhook_url) = self.alert_webhook {
                let payload = json!({
                    "type": "anomaly_alert",
                    "session_id": self.session_id,
                    "command": command,
                    "anomaly_count": anomalies.len(),
                    "events": anomalies.iter().map(|e| json!({
                        "type": format!("{:?}", e.anomaly_type),
                        "score": e.score,
                        "description": e.description,
                    })).collect::<Vec<_>>(),
                    "cumulative_score": self.anomaly_tracker.lock().await.score(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });
                self.send_webhook_alert(webhook_url, &payload).await;
            }
        }

        // Check if anomaly score warrants session termination
        let response_action = self.anomaly_tracker.lock().await.recommended_response();
        if response_action == AnomalyResponse::TerminateSession {
            warn!(
                session_id = %self.session_id,
                "anomaly score exceeded termination threshold — session should be terminated"
            );
        }
    }

    /// Send a webhook alert (fire-and-forget, errors only logged).
    async fn send_webhook_alert(&self, url: &str, payload: &Value) {
        let body = serde_json::to_vec(payload).unwrap_or_default();
        // Use a simple TCP connection to avoid adding HTTP client dependency.
        // Parse URL and make a basic HTTP POST.
        match Self::http_post(url, &body).await {
            Ok(()) => {
                debug!(url = %url, "webhook alert sent");
            }
            Err(e) => {
                warn!(url = %url, error = %e, "failed to send webhook alert");
            }
        }
    }

    /// Minimal HTTP POST implementation (no external HTTP client dependency).
    async fn http_post(url: &str, body: &[u8]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpStream;

        // Parse URL (supports http://host:port/path)
        let url = url.strip_prefix("http://").unwrap_or(url);
        let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
        let path = format!("/{path}");

        let stream = TcpStream::connect(host_port).await?;
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            path,
            host_port,
            body.len()
        );

        let (_, mut writer) = tokio::io::split(stream);
        writer.write_all(request.as_bytes()).await?;
        writer.write_all(body).await?;
        writer.flush().await?;
        Ok(())
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
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        );
        assert!(server.is_ok());
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

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
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: json!({}),
        };

        let response = server.handle_request(&request).await;
        let result = response.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 8);
    }

    #[tokio::test]
    async fn test_tool_query_policy_allowed() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

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
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

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
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

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
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

        let result = server.tool_audit_query(&json!({})).await;
        assert!(result.is_ok());
        let content = result.unwrap();
        let text = content["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("[]"));
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

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
            None,
            None,
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
    async fn test_api_token_rotation_accepts_old_and_new() {
        let policy_file = create_test_policy();
        // Configure with comma-separated tokens (new,old) for rotation grace period
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("new-token-456,old-token-123".into()),
            None,
            None,
        )
        .unwrap();

        // Old token should still work
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({"apiToken": "old-token-123"}),
        };
        let response = server.handle_request(&request).await;
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_api_token_rotation_accepts_new_token() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            Some("new-token-456,old-token-123".into()),
            None,
            None,
        )
        .unwrap();

        // New token should work
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({"apiToken": "new-token-456"}),
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
            None,
            None,
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
            None,
            None,
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
            None,
            None,
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

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let mut limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check().is_ok());
        }
        assert_eq!(limiter.current_count(), 5);
    }

    #[test]
    fn test_rate_limiter_denies_over_limit() {
        let mut limiter = RateLimiter::new(3);
        for _ in 0..3 {
            assert!(limiter.check().is_ok());
        }
        let result = limiter.check();
        assert!(result.is_err());
        let wait = result.unwrap_err();
        assert!(wait.as_secs() <= 60);
    }

    #[tokio::test]
    async fn test_rate_limit_applied_to_tool_calls() {
        let policy_file = create_test_policy();
        // Rate limit to 2 per minute
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            Some(2),
            None,
        )
        .unwrap();

        // Authenticate first
        let init = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(0)),
            method: "initialize".into(),
            params: json!({"protocolVersion": "2024-11-05"}),
        };
        server.handle_request(&init).await;

        // First two tool calls should succeed (may be denied by policy, but not rate limited)
        for i in 1..=2 {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(i)),
                method: "tools/call".into(),
                params: json!({"name": "rf_exec", "arguments": {"command": "echo hi"}}),
            };
            let response = server.handle_request(&request).await;
            // Should NOT be rate limited (may succeed or fail for policy reasons)
            if let Some(ref err) = response.error {
                assert!(!err.message.contains("Rate limited"));
            }
        }

        // Third tool call should be rate limited
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: json!({"name": "rf_exec", "arguments": {"command": "echo hi"}}),
        };
        let response = server.handle_request(&request).await;
        assert!(response.error.is_some());
        assert!(response.error.unwrap().message.contains("Rate limited"));
    }

    #[tokio::test]
    async fn test_rate_limit_not_applied_to_tools_list() {
        let policy_file = create_test_policy();
        // Very low rate limit
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            Some(1),
            None,
        )
        .unwrap();

        // Authenticate
        let init = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(0)),
            method: "initialize".into(),
            params: json!({"protocolVersion": "2024-11-05"}),
        };
        server.handle_request(&init).await;

        // tools/list should never be rate limited
        for i in 1..=5 {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(i)),
                method: "tools/list".into(),
                params: json!({}),
            };
            let response = server.handle_request(&request).await;
            assert!(response.result.is_some());
            assert!(response.error.is_none());
        }
    }

    #[tokio::test]
    async fn test_approval_workflow() {
        let policy_file = create_test_policy();
        let server = McpServer::new(
            Some(policy_file.path()),
            None,
            "test-caller",
            None,
            None,
            None,
        )
        .unwrap();

        // Authenticate
        let init = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(0)),
            method: "initialize".into(),
            params: json!({"protocolVersion": "2024-11-05"}),
        };
        server.handle_request(&init).await;

        // Request approval
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "tools/call".into(),
            params: json!({
                "name": "rf_request_approval",
                "arguments": {
                    "operation": "deploy",
                    "command": "kubectl apply -f deploy.yaml",
                    "reason": "Deploy new version"
                }
            }),
        };
        let response = server.handle_request(&request).await;
        let result = response.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("PENDING"));
        assert!(text.contains("rf_check_approval"));

        // Extract approval ID from response
        let id_start = text.find("id: ").unwrap() + 4;
        let id_end = text[id_start..].find(')').unwrap() + id_start;
        let approval_id = &text[id_start..id_end];

        // Check approval (should be pending)
        let check = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/call".into(),
            params: json!({
                "name": "rf_check_approval",
                "arguments": {"approval_id": approval_id}
            }),
        };
        let response = server.handle_request(&check).await;
        let result = response.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("PENDING"));

        // Approve it
        assert!(server.approve(approval_id).await);

        // Check again (should be approved)
        let check2 = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: json!({
                "name": "rf_check_approval",
                "arguments": {"approval_id": approval_id}
            }),
        };
        let response = server.handle_request(&check2).await;
        let result = response.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("APPROVED"));
    }
}
