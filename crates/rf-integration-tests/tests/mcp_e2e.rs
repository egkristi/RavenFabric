//! MCP Server integration tests
//!
//! Tests the full MCP JSON-RPC protocol flow:
//!   client → JSON-RPC request → MCP server → policy check → execute → response

use rf_mcp_server::{JsonRpcRequest, McpServer};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_test_server() -> McpServer {
    let policy_yaml = r#"
spec:
  commands:
    allow:
      - pattern: "^echo .*"
      - pattern: "^uname.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /tmp
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
"#;

    let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = std::env::temp_dir().join(format!("rf-mcp-test-{}-{}", std::process::id(), id));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let policy_path = tmp_dir.join("policy.yaml");
    std::fs::write(&policy_path, policy_yaml).unwrap();
    let audit_path = tmp_dir.join("audit.jsonl");

    McpServer::new(
        Some(&policy_path),
        Some(&audit_path),
        "test-caller-key",
        Some("test-token".to_string()),
        Some(60),
        None,
        vec![],
    )
    .expect("failed to create MCP server")
}

fn make_request(method: &str, params: serde_json::Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: method.to_string(),
        params,
    }
}

/// Authenticate the server by sending a valid initialize request.
async fn authenticate(server: &McpServer) {
    let request = make_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            },
            "apiToken": "test-token"
        }),
    );
    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "authenticate should succeed: {:?}",
        response.error
    );
}

#[tokio::test]
async fn test_mcp_initialize() {
    let server = create_test_server();

    let request = make_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            },
            "apiToken": "test-token"
        }),
    );

    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "initialize should succeed: {:?}",
        response.error
    );
    let result = response.result.unwrap();
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["serverInfo"]["name"].is_string());
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request("tools/list", json!({}));
    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "tools/list should succeed: {:?}",
        response.error
    );

    let result = response.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert!(
        tools.len() >= 7,
        "should have at least 7 tools, got {}",
        tools.len()
    );

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"rf_exec"), "should have rf_exec tool");
    assert!(
        tool_names.contains(&"rf_query_policy"),
        "should have rf_query_policy tool"
    );
    assert!(
        tool_names.contains(&"rf_file_read"),
        "should have rf_file_read tool"
    );
    assert!(
        tool_names.contains(&"rf_list_my_capabilities"),
        "should have rf_list_my_capabilities tool"
    );
}

#[tokio::test]
async fn test_mcp_exec_allowed() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request(
        "tools/call",
        json!({
            "name": "rf_exec",
            "arguments": {
                "command": "echo integration-test"
            }
        }),
    );

    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "exec should succeed for allowed command: {:?}",
        response.error
    );

    let result = response.result.unwrap();
    let content = result["content"].as_array().unwrap();
    assert!(!content.is_empty(), "should have output content");

    let text = content[0]["text"].as_str().unwrap();
    assert!(
        text.contains("integration-test"),
        "output should contain 'integration-test', got: {text}"
    );
}

#[tokio::test]
async fn test_mcp_exec_denied() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request(
        "tools/call",
        json!({
            "name": "rf_exec",
            "arguments": {
                "command": "rm -rf /"
            }
        }),
    );

    let response = server.handle_request(&request).await;
    // Denied commands should return an error or isError content
    if let Some(result) = &response.result {
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_error {
            let content = result["content"].as_array().unwrap();
            let text = content[0]["text"].as_str().unwrap_or("");
            assert!(
                text.to_lowercase().contains("denied") || text.to_lowercase().contains("policy"),
                "denied command should mention policy denial, got: {text}"
            );
        }
    }
    // If there's an error field, that's also acceptable for denied commands
}

#[tokio::test]
async fn test_mcp_query_policy() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request(
        "tools/call",
        json!({
            "name": "rf_query_policy",
            "arguments": {
                "command": "echo hello"
            }
        }),
    );

    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "query_policy should succeed: {:?}",
        response.error
    );

    let result = response.result.unwrap();
    let content = result["content"].as_array().unwrap();
    let text = content[0]["text"].as_str().unwrap();
    assert!(
        text.to_lowercase().contains("allow"),
        "echo should be allowed by policy, got: {text}"
    );
}

#[tokio::test]
async fn test_mcp_list_capabilities() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request(
        "tools/call",
        json!({
            "name": "rf_list_my_capabilities",
            "arguments": {}
        }),
    );

    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_none(),
        "list_capabilities should succeed: {:?}",
        response.error
    );

    let result = response.result.unwrap();
    let content = result["content"].as_array().unwrap();
    assert!(!content.is_empty(), "capabilities should have content");
}

#[tokio::test]
async fn test_mcp_invalid_method() {
    let server = create_test_server();
    authenticate(&server).await;

    let request = make_request("nonexistent/method", json!({}));
    let response = server.handle_request(&request).await;
    assert!(
        response.error.is_some(),
        "invalid method should return error"
    );

    let error = response.error.unwrap();
    assert_eq!(error.code, -32601, "should be method not found error");
}

#[tokio::test]
async fn test_mcp_rate_limiting() {
    let tmp_dir = std::env::temp_dir().join("rf-mcp-rate-test");
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let policy_path = tmp_dir.join("policy.yaml");
    std::fs::write(
        &policy_path,
        "spec:\n  commands:\n    allow:\n      - pattern: \".*\"\n  resources:\n    maxOutputBytes: 1048576\n    timeoutSeconds: 30\n",
    )
    .unwrap();
    let audit_path = tmp_dir.join("audit.jsonl");

    let server = McpServer::new(
        Some(policy_path.as_path()),
        Some(audit_path.as_path()),
        "rate-test-caller",
        Some("rate-test-token".to_string()),
        Some(5), // 5 requests per minute
        None,
        vec![],
    )
    .expect("failed to create MCP server");

    // Authenticate first
    let init_request = make_request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" },
            "apiToken": "rate-test-token"
        }),
    );
    let resp = server.handle_request(&init_request).await;
    assert!(
        resp.error.is_none(),
        "rate-limit auth failed: {:?}",
        resp.error
    );

    // Send requests rapidly — should eventually get rate-limited
    let mut rate_limited = false;
    for i in 0..10 {
        let request = make_request(
            "tools/call",
            json!({
                "name": "rf_exec",
                "arguments": {
                    "command": format!("echo test{i}")
                }
            }),
        );
        let response = server.handle_request(&request).await;
        if let Some(error) = &response.error {
            if error.code == -32000 || error.message.to_lowercase().contains("rate") {
                rate_limited = true;
                break;
            }
        }
    }
    assert!(rate_limited, "should hit rate limit after rapid requests");
}
