# Installation

## Pre-built Binaries (Recommended)

Download the latest release from [GitHub Releases](https://github.com/egkristi/RavenFabric/releases):

| Platform | Binary |
|----------|--------|
| Linux x86_64 (static) | `ravenfabric-linux-amd64-musl-cli` |
| Linux ARM64 (static) | `ravenfabric-linux-arm64-musl-cli` |
| macOS x86_64 | `ravenfabric-darwin-amd64-cli` |
| macOS ARM64 (Apple Silicon) | `ravenfabric-darwin-arm64-cli` |
| Windows x86_64 | `ravenfabric-windows-amd64-cli.exe` |

```bash
# Example: Linux x86_64
curl -Lo rf https://github.com/egkristi/RavenFabric/releases/latest/download/ravenfabric-linux-amd64-musl-cli
chmod +x rf
sudo mv rf /usr/local/bin/
```

## Homebrew (macOS / Linux)

```bash
brew install egkristi/tap/ravenfabric
```

## From Source

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
rf --version

# Check agent
rf-agent --help

# Check relay
rf-relay --help
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
