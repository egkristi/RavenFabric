# Testing

## Running Tests

```bash
# All tests
cargo test --all

# Single crate
cargo test -p rf-executor

# Specific test
cargo test -p rf-policy test_deny_wins
```

## Test Coverage by Crate

| Crate | Tests | Focus |
|-------|-------|-------|
| `rf-crypto` | 35 | Handshake, SecureChannel, key management |
| `rf-transport` | 248 | Drivers, NAT traversal, connection management, discovery |
| `rf-rpc` | 98 | Codec, yamux mux, controller, types |
| `rf-audit` | 3 | Logger write, error handling |
| `rf-policy` | 55 | Allow/deny, RBAC, CRDT, capability tokens |
| `rf-executor` | 105 | Execution, streaming, PTY, metrics, health probes |
| `rf-bootstrap` | 11 | OTP, enrollment, trust store |
| `rf-relay` | 7 | Pairing, rate limiting, forwarding |
| `rf-integration-tests` | 2 | Full E2E pipeline |
| **Total** | **564** | |

## Test Patterns

### Unit Tests

Each crate has unit tests in `#[cfg(test)]` modules within the source files:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_denies_rm_rf() {
        let policy = RpcPolicy::from_yaml(STRICT_POLICY).unwrap();
        let decision = policy.check_command("rm -rf /");
        assert!(decision.is_denied());
    }
}
```

### Async Tests

Transport and RPC tests use `tokio::io::duplex` for in-memory connections:

```rust
#[tokio::test]
async fn test_rpc_roundtrip() {
    let (client, server) = tokio::io::duplex(8192);
    // Test RPC over simulated connection without real network
}
```

### Security Tests

Security-critical paths require both positive AND negative tests:

```rust
#[test]
fn test_symlink_traversal_blocked() {
    // Verify symlinks to denied paths are caught
}

#[test]
fn test_output_limit_enforced() {
    // Verify output truncation at configured limit
}
```

### Integration Tests

The `rf-integration-tests` crate runs full E2E flows:
- Client → Relay → Agent → Policy → Execute → Response
- Policy denial verification

```bash
cargo test -p rf-integration-tests
```

## Fuzz Testing

Three fuzz targets exercise parser edge cases:

```bash
# Requires nightly Rust
rustup install nightly

# RPC codec (malformed msgpack)
cd crates/rf-rpc && cargo +nightly fuzz run fuzz_codec

# Policy parser (malformed YAML)
cd crates/rf-policy && cargo +nightly fuzz run fuzz_policy

# Transport frames (malformed frames)
cd crates/rf-transport && cargo +nightly fuzz run fuzz_frame
```

## CI Pipeline

The CI runs on every push to `main`:

| Job | What |
|-----|------|
| **Check** | `cargo check --all-targets` |
| **Clippy** | `cargo clippy --all-targets -- -D warnings` |
| **Format** | `cargo fmt --all --check` |
| **Test** | `cargo test --all` |
| **MSRV** | `cargo check` with Rust 1.88 |
| **Cross-compile** | armv7, riscv64, FreeBSD via `cross` |
| **Coverage** | cargo-tarpaulin → Codecov |
| **CodeQL** | Static analysis for security issues |

## Performance Benchmarks

Criterion benchmarks for hot paths:

```bash
# Crypto benchmarks (handshake, encrypt/decrypt)
cargo bench -p rf-crypto

# Codec benchmarks (serialize/deserialize)
cargo bench -p rf-rpc
```
