# Testing

## Running Tests

```bash
# All tests
cargo test --all

# Specific crate
cargo test -p rf-crypto

# With output
cargo test --all -- --nocapture

# Single test
cargo test -p rf-policy test_allowed_command
```

## Test Architecture

### Unit Tests

Each crate contains unit tests in `#[cfg(test)]` modules. Tests use:

- **`tokio::io::duplex`**: Simulated bidirectional connections for transport/crypto tests
- **In-memory policy**: YAML strings parsed directly (no file I/O in tests)
- **`tempfile`**: Temporary directories for key file tests

### Integration Tests

Integration tests verify end-to-end behavior:

- Full Noise XX handshake over duplex streams
- Policy enforcement with various command patterns
- OTP generation, validation, and expiry
- SecureChannel send/recv with encryption verification

### Property Tests

`proptest` for invariant verification:

- Any command not matching allow rules is denied
- Encrypted frames decrypt to original plaintext
- OTP tokens are never reusable

## Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out html --skip-clean --exclude-files "crates/rf-cli/*" "crates/rf-agent/*" "crates/rf-relay/*"
```

Coverage threshold: 60% (enforced in CI).

## Benchmarks

```bash
cargo bench
```

Key metrics to track:
- Noise XX handshake latency
- SecureChannel throughput (frames/sec)
- Policy evaluation time per command
- Executor overhead vs raw `sh -c`
