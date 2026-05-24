# Building from Source

## Prerequisites

- Rust 1.88+ (Edition 2024)
- Git
- C compiler (for `ring` crate — `cc`, `gcc`, or `clang`)

## Build

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release
```

Binaries are in `target/release/`:

- `rf` — CLI client
- `rf-agent` — Agent daemon
- `rf-relay` — Relay broker
- `rf-mcp-server` — MCP server for AI agents

## Feature Flags

The workspace supports feature flags for different build profiles:

| Feature | Crate | Description | Default |
|---------|-------|-------------|---------|
| `full` | rf-agent | All transports + sysinfo metrics | Yes |
| `minimal` | rf-agent | Core only (no QUIC, no sysinfo) | No |
| `quic` | rf-agent | QUIC transport via quinn | Included in `full` |
| `sysinfo` | rf-agent | System metrics collection | Included in `full` |
| `http-sse` | rf-mcp-server | HTTP+SSE multi-user transport | No |

```bash
# Minimal agent build (smaller binary, fewer dependencies)
cargo build --release -p rf-agent --no-default-features --features minimal

# MCP server with HTTP+SSE support
cargo build --release -p rf-mcp-server --features http-sse
```

## Cross-Compilation

### Static Linux Binary (musl)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### ARM (Raspberry Pi)

```bash
# Using cross (Docker-based cross-compilation)
cargo install cross --git https://github.com/cross-rs/cross
cross check --target armv7-unknown-linux-musleabihf -p rf-agent
```

### CI Cross-Compile Targets

The following targets are verified in CI:

| Target | Status |
|--------|--------|
| `x86_64-unknown-linux-gnu` | Tier 1 |
| `x86_64-unknown-linux-musl` | Tier 1 |
| `aarch64-unknown-linux-gnu` | Tier 1 |
| `aarch64-unknown-linux-musl` | Tier 1 |
| `x86_64-apple-darwin` | Tier 1 |
| `aarch64-apple-darwin` | Tier 1 |
| `x86_64-pc-windows-msvc` | Tier 1 |
| `armv7-unknown-linux-musleabihf` | Tier 2 (cross-check) |
| `riscv64gc-unknown-linux-gnu` | Tier 2 (cross-check) |
| `x86_64-unknown-freebsd` | Tier 2 (cross-check) |

## Docker

```bash
# Multi-stage build
docker build --target agent -t ravenfabric-agent .
docker build --target relay -t ravenfabric-relay .
```

## Verification

```bash
# Lint
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --all --check

# Run all tests
cargo test --all

# Fuzz testing (requires nightly)
cd crates/rf-rpc && cargo +nightly fuzz run fuzz_codec
cd crates/rf-policy && cargo +nightly fuzz run fuzz_policy
cd crates/rf-transport && cargo +nightly fuzz run fuzz_frame
```
