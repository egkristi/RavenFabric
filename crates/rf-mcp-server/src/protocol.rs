//! JSON-RPC 2.0 protocol types for MCP communication.

use serde::{Deserialize, Serialize};

/// MCP protocol version supported by this server.
pub const MCP_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 notification (no id, no response expected).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn method_not_found(id: Option<serde_json::Value>, method: &str) -> Self {
        Self::error(id, -32601, format!("Method not found: {method}"))
    }

    pub fn invalid_params(id: Option<serde_json::Value>, msg: impl Into<String>) -> Self {
        Self::error(id, -32602, msg)
    }
}

impl JsonRpcNotification {
    #[allow(dead_code)]
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        }
    }
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i64 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i64 = -32600;
#[allow(dead_code)]
pub const METHOD_NOT_FOUND: i64 = -32601;
#[allow(dead_code)]
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_success_response() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::Value::Number(1.into())),
            serde_json::json!({"content": [{"type": "text", "text": "hello"}]}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_error_response() {
        let resp = JsonRpcResponse::error(Some(serde_json::json!(1)), -32601, "Method not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_deserialize_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"rf_exec","arguments":{"command":"ls"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
        assert_eq!(req.params["name"], "rf_exec");
    }

    #[test]
    fn test_notification() {
        let notif = JsonRpcNotification::new("notifications/initialized", None);
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains("\"method\":\"notifications/initialized\""));
        assert!(!json.contains("\"params\""));
    }
}
