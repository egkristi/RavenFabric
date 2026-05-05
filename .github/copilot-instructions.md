# RavenFabric — AI Agent Instructions

## Language

All code, comments, documentation, commit messages, and plan files must be written in **English only**.

## Writing Style

- Be conservative with emoji use. A project icon in the title is fine; emoji walls are not.
- Write in a clear, professional tone. Let the content speak for itself.

## Project Overview

RavenFabric is a secure remote execution and mesh networking agent written in Rust.
It replaces Tailscale, Headscale, Ansible, Salt, NetBird, ZeroTier, and similar tools with a single,
cryptographically verified binary. Noise XX mutual authentication, deny-by-default policy,
and structured audit logging are non-negotiable foundations.

## Architecture

Cargo workspace with 10 crates:

| Crate | Purpose |
|---|---|
| `rf-crypto` | Noise XX handshake, SecureChannel (encrypted frames), key management |
| `rf-transport` | Driver trait, WebSocket/QUIC/WireGuard backends |
| `rf-rpc` | Request/Response types, msgpack codec, yamux multiplexing |
| `rf-audit` | Structured JSON-lines audit logging (every action logged) |
| `rf-policy` | YAML policy loading, command/path/resource enforcement, deny-by-default |
| `rf-executor` | Command execution under policy control with timeout and output limiting |
| `rf-bootstrap` | OTP enrollment flow, relay pairing |
| `rf-relay` | Stateless encrypted relay broker (binary) |
| `rf-agent` | Agent binary (connects to relay, executes RPC) |
| `rf-cli` | CLI client `rf` (exec, dev, status) |

## Dependency Flow

```
rf-crypto  (no internal deps)
  ↑
rf-transport (depends on rf-crypto)
rf-bootstrap (depends on rf-crypto)
  ↑
rf-rpc (depends on rf-crypto, rf-transport)
rf-audit (no internal deps)
rf-policy (depends on rf-audit)
  ↑
rf-executor (depends on rf-policy, rf-rpc, rf-audit)
  ↑
rf-relay   (depends on rf-transport)
rf-agent   (depends on rf-crypto, rf-transport, rf-rpc, rf-executor, rf-policy, rf-audit, rf-bootstrap)
rf-cli     (depends on rf-crypto, rf-transport, rf-rpc)
```

## Coding Standards

- **Edition**: Rust 2024, MSRV 1.85
- **Async runtime**: Tokio (full features)
- **Error handling**: `thiserror` for library errors, `anyhow` only in binaries (agent, relay, cli)
- **Traits**: Use `async-trait` for async trait methods. All traits must be `Send + Sync`
- **Serialization**: `rmp-serde` (msgpack) for wire protocol, `serde_yaml` for config/policy, `serde_json` for audit logs
- **Crypto**: `snow` crate for Noise XX. No TLS. No certificates. Mutual key authentication only
- **HTTP**: None in core. WebSocket via `tokio-tungstenite` for relay transport
- **Logging**: `tracing` crate. Use `info!`, `warn!`, `error!` — never `println!` in libraries
- **Tests**: Unit tests in each crate. Integration tests use `tokio::io::duplex` for simulated connections

## Key Design Principles

1. **Thread-safe by default**: All public types must be `Send + Sync`
2. **No unwrap in library code**: Use `?` and proper error types
3. **Deny-by-default**: Policy engine denies anything not explicitly allowed
4. **Zero-trust networking**: Every connection mutually authenticated via Noise XX
5. **Audit everything**: Every RPC action produces a structured audit entry
6. **No plaintext secrets on disk**: Private keys file-permission protected, zeroed on drop
7. **Batch operations**: Where applicable (multi-agent exec, file transfers)
8. **Builder pattern**: For complex types (SecureChannel, Executor, etc.)
9. **Feature flags**: Optional transports behind cargo features (quic, wireguard)
10. **Single static binary**: Agent deploys as one file, no runtime dependencies

## Wire Protocol

- Magic: `RVNF` (4 bytes)
- Version: 1 byte
- Handshake: Noise_XX_25519_ChaChaPoly_BLAKE2s
- Frames: `[length: 4 bytes BE][ciphertext + 16-byte MAC]`
- Multiplexing: yamux over SecureChannel
- RPC encoding: msgpack (rmp-serde)

## Security Invariants

1. No command executes without policy check
2. No connection accepted without completed Noise handshake
3. Audit log append-only (no delete/truncate operations)
4. Private keys zeroed from memory on drop
5. OTP tokens single-use, hash-stored, TTL-enforced
6. Symlink resolution before path policy checks (prevent traversal)
7. Output size bounded (prevent memory exhaustion)
8. Execution timeout enforced (prevent hanging)
9. No shell injection — commands run via `sh -c` with policy-checked string
10. Relay never decrypts payload (end-to-end between agent and client)

## Build & Test

```bash
cargo build              # Debug build
cargo build --release    # Release build (LTO, stripped)
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt --check        # Format check
```

## Git Workflow

- **Commit and push for each completed feature or resolved issue** — do not batch unrelated changes
- All planned changes tracked as GitHub Issues before work begins
- Commit messages: `feat: <description>`, `fix: <description>`, `refactor: <description>`
- Reference GitHub Issues in commits (e.g. `feat: add QUIC transport driver #5`)
- Always run `cargo test` and `cargo clippy` before pushing
- Format: `git add -A && git commit -m "<message>" && git push`
- **After every push**: Check GitHub Actions for pipeline failures. If any workflow fails, diagnose and fix immediately
- **If pipeline fails**: Create a GitHub Issue for each distinct problem so nothing is forgotten, then fix it
- **Issue tracking**: When you discover work that should be done but is out of scope for the current task, create a GitHub Issue for it rather than ignoring it

## Policy YAML Format

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Configuration (raven.toml)

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0  # infinite

[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"
```
