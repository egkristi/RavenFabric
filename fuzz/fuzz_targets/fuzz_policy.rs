//! Fuzz target for the policy YAML parser.
//! Tests that arbitrary YAML-like input never causes panics.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to string (policy is text-based YAML)
    if let Ok(yaml_str) = std::str::from_utf8(data) {
        // Try to parse as a policy document — must not panic
        let _ = rf_policy::rpc_policy::RpcPolicy::from_yaml(yaml_str);
    }
});
