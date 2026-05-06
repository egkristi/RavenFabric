# Installation

## Download Pre-built Binaries

Pre-built binaries are available for every release on the [GitHub Releases](https://github.com/egkristi/RavenFabric/releases) page:

| Platform | Binary |
|----------|--------|
| Linux x86_64 (static musl) | `ravenfabric-linux-amd64-musl-agent` |
| Linux ARM64 (static musl) | `ravenfabric-linux-arm64-musl-agent` |
| macOS x86_64 | `ravenfabric-darwin-amd64-agent` |
| macOS ARM64 (Apple Silicon) | `ravenfabric-darwin-arm64-agent` |
| Windows x86_64 | `ravenfabric-windows-amd64-agent.exe` |

Each platform provides three binaries: `-agent`, `-relay`, `-cli`.

**Linux / macOS:**

```bash
# Download agent
curl -LO https://github.com/egkristi/RavenFabric/releases/latest/download/ravenfabric-linux-amd64-agent
chmod +x ravenfabric-linux-amd64-agent
sudo mv ravenfabric-linux-amd64-agent /usr/local/bin/rf-agent

# Download CLI
curl -LO https://github.com/egkristi/RavenFabric/releases/latest/download/ravenfabric-linux-amd64-cli
chmod +x ravenfabric-linux-amd64-cli
sudo mv ravenfabric-linux-amd64-cli /usr/local/bin/rf
```

## Build from Source

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release
```

Binaries are in `target/release/`:
- `rf-agent` — Agent daemon
- `rf-relay` — Relay broker
- `rf` — CLI client

## Docker

```bash
# Build agent image
docker build --target agent -t ravenfabric-agent .

# Build relay image
docker build --target relay -t ravenfabric-relay .

# Run relay
docker run -d -p 9090:9090 ravenfabric-relay
```

## Requirements

- Rust 1.85+ (MSRV — build only)
- No runtime dependencies (fully static binary)
- Binary size: < 15 MB stripped (typically 8-12 MB)
