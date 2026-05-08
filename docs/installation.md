# Installation

## Download Pre-built Binaries

Pre-built binaries are available on the [GitHub Releases](https://github.com/egkristi/RavenFabric/releases) page:

| Platform | Binary |
|----------|--------|
| Linux x86_64 (static musl) | `ravenfabric-linux-amd64-musl-{agent,relay,cli}` |
| Linux ARM64 (static musl) | `ravenfabric-linux-arm64-musl-{agent,relay,cli}` |
| Linux armv7 (static musl) | `ravenfabric-linux-armv7-musl-{agent,relay,cli}` |
| macOS x86_64 | `ravenfabric-darwin-amd64-{agent,relay,cli}` |
| macOS ARM64 (Apple Silicon) | `ravenfabric-darwin-arm64-{agent,relay,cli}` |
| Windows x86_64 | `ravenfabric-windows-amd64-{agent,relay,cli}.exe` |

```bash
# Download CLI
curl -LO https://github.com/egkristi/RavenFabric/releases/latest/download/ravenfabric-linux-amd64-musl-cli
chmod +x ravenfabric-linux-amd64-musl-cli
sudo mv ravenfabric-linux-amd64-musl-cli /usr/local/bin/rf
```

## Install Script

```bash
curl -fsSL https://raw.githubusercontent.com/egkristi/RavenFabric/main/deploy/install.sh | sh
```

## Package Managers

```bash
# macOS/Linux (Homebrew) — https://github.com/egkristi/homebrew-tap
brew install egkristi/tap/ravenfabric
# Or tap first, then install by name:
brew tap egkristi/tap
brew install ravenfabric
# Upgrade:
brew update && brew upgrade ravenfabric

# Arch Linux (AUR)
yay -S ravenfabric

# Cargo
cargo install ravenfabric
```

## Build from Source

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release
```

## Docker

```bash
docker pull ghcr.io/egkristi/ravenfabric:latest
docker run -d -p 9090:9090 ghcr.io/egkristi/ravenfabric:latest relay
```

## Requirements

- Rust 1.88+ (MSRV — build only)
- No runtime dependencies (fully static binary)
- Binary size: < 15 MB stripped (typically 8-12 MB)

## Full Installation Guide

See [ravenfabric.io/docs/getting-started/installation.html](https://ravenfabric.io/docs/getting-started/installation.html) for complete installation instructions including all package managers, Docker, and cross-compilation.
