//! MCP protocol types (client-side representations).

use serde::{Deserialize, Serialize};

/// Result of an `rf_exec` tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// Command output (stdout).
    pub output: String,
    /// Standard error output.
    pub stderr: String,
    /// Exit code (0 = success).
    pub exit_code: i32,
    /// Whether the command was allowed by policy.
    pub allowed: bool,
}

/// Result of an `rf_query_policy` tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Whether the command would be allowed.
    pub allowed: bool,
    /// The policy rule that matched.
    pub matched_rule: String,
    /// Human-readable reason.
    pub reason: String,
}

/// File content from `rf_file_read`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    /// File path.
    pub path: String,
    /// File content (text).
    pub content: String,
}

/// A tool capability exposed by the MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCapability {
    /// Tool name (e.g., "rf_exec").
    pub name: String,
    /// Tool description.
    pub description: String,
}

/// JSON-RPC 2.0 request (client-side, serializable).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response (client-side, deserializable).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JsonRpcResponse {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[allow(dead_code)]
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[allow(dead_code)]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = JsonRpcRequest::new(
            1,
            "tools/call",
            Some(serde_json::json!({"name": "rf_exec"})),
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"tools/call\""));
    }

    #[test]
    fn test_response_deserialization_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_response_deserialization_error() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_exec_result() {
        let result = ExecResult {
            output: "hello".into(),
            stderr: String::new(),
            exit_code: 0,
            allowed: true,
        };
        assert_eq!(result.output, "hello");
        assert!(result.allowed);
    }

    #[test]
    fn test_policy_decision() {
        let decision = PolicyDecision {
            allowed: false,
            matched_rule: "deny: .*rm.*-rf.*".into(),
            reason: "matches deny rule".into(),
        };
        assert!(!decision.allowed);
    }
}
