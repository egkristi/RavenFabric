# Building from Source

## Prerequisites

- Rust 1.85+ (Edition 2024)
- Git

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify version
rustc --version  # Must be 1.85.0 or later
```

## Clone and Build

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric

# Debug build
cargo build

# Release build (optimized, LTO, stripped)
cargo build --release
```

## Cross-Compilation

### Linux Static Binary (musl)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### Linux ARM64

```bash
rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

### Linux ARMv7 (Raspberry Pi)

```bash
rustup target add armv7-unknown-linux-musleabihf
cargo build --release --target armv7-unknown-linux-musleabihf
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `full` | Yes | All features enabled |
| `minimal` | No | No TUN, no sysinfo, no QUIC |
| `websocket` | Yes | WebSocket transport |
| `quic` | No | QUIC transport |

```bash
# Minimal build (smaller binary)
cargo build --release --no-default-features --features minimal

# With QUIC
cargo build --release --features quic
```

## Verify Build

```bash
# Run all tests
cargo test

# Lint
cargo clippy

# Format check
cargo fmt --check
```
