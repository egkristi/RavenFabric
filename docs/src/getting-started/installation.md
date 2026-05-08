# Installation

## Pre-built Binaries (Recommended)

Download the latest release from [GitHub Releases](https://github.com/egkristi/RavenFabric-Published/releases):

| Platform | Binary |
|----------|--------|
| Linux x86_64 (static) | [ravenfabric-linux-amd64-musl-cli](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-amd64-musl-cli) |
| Linux ARM64 (static) | [ravenfabric-linux-arm64-musl-cli](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-arm64-musl-cli) |
| Linux armv7 (static) | [ravenfabric-linux-armv7-musl-cli](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-armv7-musl-cli) |
| macOS x86_64 | [ravenfabric-darwin-amd64-cli](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-darwin-amd64-cli) |
| macOS ARM64 (Apple Silicon) | [ravenfabric-darwin-arm64-cli](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-darwin-arm64-cli) |
| Windows x86_64 | [ravenfabric-windows-amd64-cli.exe](https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-windows-amd64-cli.exe) |

```bash
# Example: Linux x86_64
curl -Lo rf https://github.com/egkristi/RavenFabric-Published/releases/latest/download/ravenfabric-linux-amd64-musl-cli
chmod +x rf
sudo mv rf /usr/local/bin/
```

## Install Script

```bash
curl -fsSL https://get.ravenfabric.io | sh
```

## Package Managers

### macOS

```bash
# Homebrew — https://github.com/egkristi/homebrew-tap
brew install egkristi/tap/ravenfabric
# Or tap first, then install by name:
brew tap egkristi/tap
brew install ravenfabric
# Upgrade:
brew update && brew upgrade ravenfabric
```

### Linux

```bash
# Arch Linux (AUR)
yay -S ravenfabric

# Snap
sudo snap install ravenfabric

# Nix
nix profile install ravenfabric
```

### Windows

```powershell
# winget
winget install RavenFabric.RavenFabric

# Scoop
scoop bucket add extras
scoop install ravenfabric

# Chocolatey
choco install ravenfabric
```

### Cargo

```bash
cargo install ravenfabric
```

## From Source

RavenFabric requires Rust 1.88+ (Edition 2024).

```bash
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build --release

# Binaries are in target/release/
ls target/release/rf target/release/rf-agent target/release/rf-relay target/release/rf-mcp-server
```

## Docker

```bash
docker pull ghcr.io/egkristi/ravenfabric:latest

# Or build locally
docker build --target agent -t ravenfabric-agent .
docker build --target relay -t ravenfabric-relay .
```

## Static Binary (Linux)

For a fully static binary with no libc dependency:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| Linux x86_64 | Tier 1 (CI-tested) | Static musl binary |
| Linux ARM64 | Tier 1 (CI-tested) | Static musl binary |
| macOS x86_64 | Tier 1 (CI-tested) | Universal binary |
| macOS ARM64 | Tier 1 (CI-tested) | Apple Silicon native |
| Windows x86_64 | Tier 1 (CI-tested) | Static CRT |
| Linux armv7 | Tier 2 (CI cross-checked) | Raspberry Pi 3/4/Zero 2W |
| Linux riscv64 | Tier 2 (CI cross-checked) | RISC-V boards |
| FreeBSD x86_64 | Tier 2 (CI cross-checked) | BSD servers |
| Android (aarch64) | Tier 3 (planned) | NDK cross-compile |
| iOS (aarch64) | Tier 3 (planned) | Network Extension |

## Verify Installation

```bash
rf --version
rf-agent --help
rf-relay --help
rf-mcp-server --help
```
