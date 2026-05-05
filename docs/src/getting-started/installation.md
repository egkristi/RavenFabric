# Installation

## From Source (Recommended)

RavenFabric requires Rust 1.85+ (Edition 2024).

```bash
# Clone the repository
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric

# Build all binaries (release mode)
cargo build --release

# Binaries are in target/release/
ls target/release/rf target/release/rf-agent target/release/rf-relay
```

## Static Binary (Linux)

For Linux deployments, build with musl for a fully static binary:

```bash
# Install musl target
rustup target add x86_64-unknown-linux-musl

# Build static binary
cargo build --release --target x86_64-unknown-linux-musl
```

## Verify Installation

```bash
# Check CLI
./target/release/rf --version

# Check agent
./target/release/rf-agent --help

# Check relay
./target/release/rf-relay --help
```

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| Linux x86_64 | Fully supported | Static musl binaries |
| Linux aarch64 | Fully supported | Static musl binaries |
| macOS x86_64 | Fully supported | |
| macOS aarch64 | Fully supported | Apple Silicon |
| Windows x86_64 | Fully supported | |
| Linux armv7 | Best effort | Raspberry Pi |
| FreeBSD | Best effort | |
| Android | Planned | NDK cross-compile |
| iOS | Planned | Network Extension |
