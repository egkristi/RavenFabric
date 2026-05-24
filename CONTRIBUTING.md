# Contributing

Contributions to RavenFabric are welcome. This guide covers the development workflow.

## Prerequisites

- Rust 1.85+ (MSRV)
- Git

## Getting Started

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
git config core.hooksPath .githooks
cargo build
cargo test
```

## Development Workflow

1. Check [GitHub Issues](https://github.com/egkristi/RavenFabric/issues) for open work
2. Create a branch from `main`
3. Implement your changes
4. Run the full check suite:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings -A clippy::unwrap_used
cargo test --all
```

1. Commit with a conventional message
2. Open a Pull Request

## Commit Messages

Use conventional commits:

```
feat: add QUIC transport driver #5
fix: handle reconnect timeout correctly closes #8
refactor: extract frame codec into module
docs: add wire protocol documentation
test: add property tests for policy engine
perf: reduce handshake allocations
```

## Code Standards

- **Edition**: Rust 2024
- **Async runtime**: Tokio (full features)
- **Error handling**: `thiserror` for library errors, `anyhow` only in binaries
- **Traits**: Use `async-trait`; all traits must be `Send + Sync`
- **Serialization**: `rmp-serde` (wire), `serde_yaml` (config), `serde_json` (audit)
- **Crypto**: `snow` for Noise XX. No TLS, no certificates
- **Logging**: `tracing` crate (`info!`, `warn!`, `error!` — never `println!` in libraries)
- **No `unwrap()` in library code**: Use `?` and proper error types
- **Tests**: Unit tests in each crate. Use `tokio::io::duplex` for transport tests

## Project Structure

```
RavenFabric/
├── Cargo.toml           # Workspace root
├── crates/
│   ├── rf-crypto/       # Noise XX, SecureChannel, keys
│   ├── rf-transport/    # Driver trait, WebSocket/QUIC
│   ├── rf-rpc/          # Message types, codec
│   ├── rf-audit/        # JSON-lines audit logging
│   ├── rf-policy/       # YAML policy enforcement
│   ├── rf-executor/     # Command execution
│   ├── rf-bootstrap/    # OTP enrollment
│   ├── rf-relay/        # Relay broker binary
│   ├── rf-agent/        # Agent binary
│   └── rf-cli/          # CLI binary (rf)
├── .github/workflows/   # CI/CD
└── docs/                # Documentation
```

## Security

RavenFabric enforces strict security invariants. Any code change must preserve:

1. No command executes without policy check
2. No connection accepted without completed Noise handshake
3. Audit log is append-only
4. Private keys zeroed on drop
5. OTP tokens single-use

See [SECURITY.md](SECURITY.md) for vulnerability reporting.
