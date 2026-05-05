# Testing

## Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p rf-crypto
cargo test -p rf-transport
cargo test -p rf-policy

# Run a specific test
cargo test -p rf-policy test_capability_check

# Run with output
cargo test -- --nocapture
```

## Test Coverage

| Crate | Tests | Coverage Focus |
|-------|-------|---------------|
| `rf-crypto` | 25 | Noise handshake, key management, resumption, PQ KEM |
| `rf-transport` | 121 | NAT traversal, mesh, proxy, gossip, overlays, WireGuard, platforms |
| `rf-rpc` | 61 | Codec roundtrips, DTN queuing, routing, SOCKS5, controller |
| `rf-policy` | 31 | RBAC, capabilities, deny-by-default, distributed policy |
| `rf-executor` | 48 | Command execution, streaming, PTY, log tailing, plugins |
| `rf-bootstrap` | 11 | OTP enrollment, TrustStore |
| Integration | 2 | End-to-end relay + agent + client |

**Total: 336 tests**

## Writing Tests

### Unit Tests

Place unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deny_by_default() {
        let policy = Policy::empty();
        assert!(policy.check("rm -rf /").is_denied());
    }
}
```

### Async Tests

Use `#[tokio::test]` for async tests:

```rust
#[tokio::test]
async fn test_secure_channel() {
    let (client, server) = tokio::io::duplex(8192);
    // ... test over simulated connection
}
```

### Security Tests

Security-critical paths need both positive AND negative tests:

```rust
#[test]
fn test_policy_allows_valid_command() {
    let policy = load_policy("allow systemctl status");
    assert!(policy.check("systemctl status nginx").is_allowed());
}

#[test]
fn test_policy_denies_invalid_command() {
    let policy = load_policy("allow systemctl status");
    assert!(policy.check("rm -rf /").is_denied());
}
```

## CI

Tests run automatically on every push via GitHub Actions:
- All platforms: Linux, macOS, Windows
- `cargo test`, `cargo clippy`, `cargo fmt --check`
