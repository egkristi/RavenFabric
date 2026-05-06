//! Fuzz target for the MCP server JSON-RPC protocol parsing.
//! Tests that arbitrary bytes never cause panics during deserialization.

#![no_main]
use libfuzzer_sys::fuzz_target;

// Directly test serde_json deserialization of JSON-RPC request format
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

fuzz_target!(|data: &[u8]| {
    // Try to parse arbitrary bytes as a JSON-RPC request — must not panic
    let _ = serde_json::from_slice::<JsonRpcRequest>(data);

    // Also try as a generic JSON value — must not panic
    let _ = serde_json::from_slice::<serde_json::Value>(data);

    // Try as UTF-8 string then parse — covers different code paths
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<JsonRpcRequest>(s);
    }
});
