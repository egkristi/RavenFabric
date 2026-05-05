# Security

## Threat Model

RavenFabric assumes:

- **Network is hostile**: All traffic is encrypted. No plaintext ever.
- **Relay is untrusted**: Relay cannot decrypt payloads (end-to-end encryption).
- **Agents are targeted**: Policy engine denies everything not explicitly allowed.
- **Keys can be stolen**: Keys are permission-protected, zeroed on drop.

## Security Invariants

These must hold at all times. Any violation is a critical bug:

1. **No command executes without policy check** — Every `Execute` action goes through `RpcPolicy::check_command()`
2. **No connection accepted without completed Noise handshake** — The `SecureChannel` is only constructed after successful Noise XX
3. **Audit log is append-only** — `FileAuditLogger` opens with `O_APPEND`, no delete/truncate
4. **Private keys zeroed from memory on drop** — `StaticKey` implements `Drop` with `zeroize`
5. **OTP tokens are single-use** — `OtpStore::validate_and_consume()` marks used immediately
6. **Symlinks resolved before path checks** — `RpcPolicy::check_path()` calls `canonicalize()`
7. **Output size bounded** — Executor truncates at `maxOutputBytes`
8. **Execution timeout enforced** — Executor wraps commands in `tokio::time::timeout`
9. **No shell injection** — Commands are policy-checked strings, not user-interpolated
10. **Relay never decrypts** — Relay forwards opaque bytes between paired connections

## Hardening

### Binary hardening (release profile)

```toml
[profile.release]
lto = true           # Link-Time Optimization
codegen-units = 1    # Single codegen unit for better optimization
strip = true         # Strip debug symbols
panic = "abort"      # No stack unwinding (smaller binary, no info leak)
```

### Rust safety

- `unsafe_code = "forbid"` at workspace level
- No `unwrap()` in library code
- All public types are `Send + Sync`

### Network hardening

- No HTTP endpoints on the agent (attack surface reduction)
- WebSocket over TLS for relay transport (defense in depth)
- Reconnect with exponential backoff (no DoS amplification)
- Connection idle timeout on relay

## Vulnerability Reporting

See [SECURITY.md](../SECURITY.md) for responsible disclosure process.
