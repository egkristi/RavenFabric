# Installation

RavenFabric is distributed as a single static binary with zero runtime dependencies.
Binaries are typically 6–12 MB stripped. All include the Noise XX handshake, policy engine, and audit logger.

## Quick Start — One-Line Installer

```bash
curl -fsSL https://ravenfabric.io/install.sh | sh
```

This downloads `rf` (CLI), `rf-agent`, and `rf-relay` to `/usr/local/bin`.
Auto-detects OS and architecture.

## GitHub Releases (Pre-built Binaries)

The primary distribution channel.
[github.com/egkristi/RavenFabric-Published](https://github.com/egkristi/RavenFabric-Published)

| Platform | Arch | Binary | Agent | Relay |
|----------|------|--------|-------|-------|
| Linux (musl) | amd64 | ✅ | ✅ | ✅ |
| Linux (musl) | arm64 | ✅ | ✅ | ✅ |
| Linux (musl) | armv7 | ✅ | ✅ | ✅ |
| macOS | amd64 | ✅ | ✅ | ✅ |
| macOS | arm64 | ✅ | ✅ | ✅ |
| Windows | amd64 | ✅ | ✅ | ✅ |

```bash
# Linux x86_64
curl -LO https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-amd64-musl-cli
chmod +x ravenfabric-linux-amd64-musl-cli
sudo mv ravenfabric-linux-amd64-musl-cli /usr/local/bin/rf

# Verify checksum
curl -LO https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-amd64-musl-cli.sha256
sha256sum -c ravenfabric-linux-amd64-musl-cli.sha256
```

## Homebrew (macOS & Linux)

Tested and working:

```bash
brew install egkristi/tap/ravenfabric
```

Upgrade: `brew update && brew upgrade ravenfabric`

[Homebrew tap](https://github.com/egkristi/homebrew-tap)

## Cargo (crates.io)

All 13 crates published on [crates.io](https://crates.io/search?q=rf-). Requires Rust 1.88+.

```bash
cargo install rf-cli         # CLI
cargo install rf-agent       # Agent
cargo install rf-relay       # Relay broker
cargo install rf-mcp-server  # MCP server for AI agents
```

| Crate | Purpose |
|-------|---------|
| [`rf-cli`](https://crates.io/crates/rf-cli) | CLI client (`rf`) |
| [`rf-agent`](https://crates.io/crates/rf-agent) | Agent daemon |
| [`rf-relay`](https://crates.io/crates/rf-relay) | Relay broker |
| [`rf-mcp-server`](https://crates.io/crates/rf-mcp-server) | MCP server |
| [`rf-crypto`](https://crates.io/crates/rf-crypto) | Noise XX, key management |
| [`rf-transport`](https://crates.io/crates/rf-transport) | 30+ transport drivers |
| [`rf-rpc`](https://crates.io/crates/rf-rpc) | RPC types + yamux |
| [`rf-audit`](https://crates.io/crates/rf-audit) | Audit logging |
| [`rf-policy`](https://crates.io/crates/rf-policy) | Policy engine |
| [`rf-executor`](https://crates.io/crates/rf-executor) | Execution engine |
| [`rf-bootstrap`](https://crates.io/crates/rf-bootstrap) | OTP enrollment |
| [`rf-ingress`](https://crates.io/crates/rf-ingress) | HTTP ingress |
| [`rf-mcp-client`](https://crates.io/crates/rf-mcp-client) | MCP client SDK |

## Build from Source

Requires Rust 1.88+ (Edition 2024). No runtime dependencies.

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release

# Binaries: target/release/{rf, rf-agent, rf-relay, rf-mcp-server}
```

Static musl build (Linux):

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

Cross-compile for Raspberry Pi:

```bash
rustup target add armv7-unknown-linux-musleabihf
cargo build --release --target armv7-unknown-linux-musleabihf
```

## Docker Compose

Local relay + agent stack:

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
docker compose up -d
```

## Systemd Services (Linux)

```bash
sudo cp deploy/rf-agent.service /etc/systemd/system/
sudo cp deploy/rf-relay.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now rf-agent
```

## Planned Package Managers (Not Yet Published)

| Manager | Status |
|---------|--------|
| Snap | 🔜 `snap/snapcraft.yaml` ready |
| AUR (Arch) | 🔜 `deploy/aur/PKGBUILD` ready |
| Alpine (apk) | 🔜 `deploy/alpine/APKBUILD` ready |
| Flatpak | 🔜 `deploy/flatpak/` ready |
| Winget / Choco / Scoop | 🔜 `deploy/winget/`, `deploy/chocolatey/`, `deploy/scoop/` ready |
| Docker image (ghcr) | 🔜 `Dockerfile` ready |
| Helm | 🔜 `deploy/helm/` ready |
| Android / iOS / WASM | 🔜 Roadmap |

## Platform Support

| Tier | Platforms | Status |
|------|-----------|--------|
| Tier 1 (CI) | Linux amd64/arm64, macOS amd64/arm64, Windows amd64 | ✅ |
| Tier 2 | Linux armv7/riscv64, FreeBSD | ⚠️ |
| Tier 3 (Planned) | Android, iOS, WASM, OpenWrt | 🔜 |

## Verify Installation

```bash
rf --version
rf-agent --help
rf-relay --help
rf-mcp-server --help
```
