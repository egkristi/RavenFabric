# Multi-Distro Linux Demo

Verify that RavenFabric's static musl binaries work on every major Linux distribution — no runtime dependencies, no compilation, no package manager integration required.

## Architecture

```
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│  Ubuntu   │ │  Debian  │ │  Fedora  │ │  Rocky   │ │ Manjaro  │
│  24.04    │ │  12      │ │  41      │ │  9       │ │  (Arch)  │
│ apt/deb   │ │ apt/deb  │ │ dnf/rpm  │ │ dnf/rpm  │ │ pacman   │
└─────┬─────┘ └─────┬────┘ └─────┬────┘ └─────┬────┘ └─────┬────┘
      │             │            │             │            │
      └──────┬──────┴─────┬──────┴──────┬──────┴─────┬──────┘
             │            │             │            │
┌──────────┐ │ ┌──────────┐ ┌──────────┐ │ ┌──────────┐
│ openSUSE │ │ │  Alpine  │ │  Amazon  │ │ │   Void   │
│Tumbleweed│ │ │  3.20    │ │  2023    │ │ │  latest  │
│zypper/rpm│ │ │ apk/musl │ │ dnf/rpm  │ │ │  xbps    │
└─────┬────┘ │ └─────┬────┘ └─────┬────┘ │ └─────┬────┘
      │      │       │            │      │       │
      └──────┴───────┴─────┬──────┴──────┴───────┘
                           │
                  ┌────────┴────────┐
                  │  rf-relay-ubuntu │
                  │  Ubuntu 24.04    │
                  │  :9092 (host)    │
                  └────────┬────────┘
                           │ port 9092
                           │
                    ┌──────┴──────┐
                    │   rf CLI    │
                    │  (your Mac) │
                    └─────────────┘
```

All agents run the same static musl binary — zero runtime dependencies regardless of distro.

## Distributions Covered

| Container | Image | Package Manager | Init System | libc | Token |
|-----------|-------|-----------------|-------------|------|-------|
| `rf-ubuntu` | `ubuntu:24.04` | apt (deb) | systemd | glibc | `ubuntu` |
| `rf-debian` | `debian:12-slim` | apt (deb) | systemd | glibc | `debian` |
| `rf-fedora` | `fedora:41` | dnf (rpm) | systemd | glibc | `fedora` |
| `rf-rocky` | `rockylinux:9` | dnf (rpm) | systemd | glibc | `rocky` |
| `rf-manjaro` | `manjarolinux/base` | pacman | systemd | glibc | `manjaro` |
| `rf-opensuse` | `opensuse/tumbleweed` | zypper (rpm) | systemd | glibc | `opensuse` |
| `rf-alpine` | `alpine:3.20` | apk | OpenRC | musl | `alpine` |
| `rf-amazon` | `amazonlinux:2023` | dnf (rpm) | systemd | glibc | `amazon` |
| `rf-void` | `void-linux/void-glibc-full` | xbps | runit | glibc | `void` |

This covers all major Linux packaging ecosystems: **deb**, **rpm**, **pacman**, **apk**, **xbps** — and both **glibc** and **musl** libc variants.

## Prerequisites

- Docker
- The `rf` CLI binary (build with `cargo build --release -p rf-cli` or install via `brew install egkristi/tap/ravenfabric`)

## Quick Start

```bash
cd demos/multi-distro-linux
./setup.sh

# Execute on any distro
rf --relay ws://127.0.0.1:9092 exec --token ubuntu 'cat /etc/os-release | head -3'
rf --relay ws://127.0.0.1:9092 exec --token alpine 'cat /etc/os-release | head -3'
rf --relay ws://127.0.0.1:9092 exec --token fedora 'cat /etc/os-release | head -3'

# Verify all agents
./setup.sh verify

# Teardown
./setup.sh teardown
```

## What This Proves

### 1. Static binary universality
The same `rf-agent` binary runs on Ubuntu, Debian, Fedora, Rocky, Manjaro (Arch-based), openSUSE, Alpine, Amazon Linux, and Void Linux without modification. No shared libraries, no runtime dependencies.

### 2. musl-on-glibc compatibility
The agent is compiled with musl for static linking. It runs on both glibc-based distros (Ubuntu, Fedora, etc.) and musl-native distros (Alpine) without issues.

### 3. Package manager independence
RavenFabric doesn't need apt, dnf, pacman, or any package manager to install. A single `curl` + `chmod +x` is sufficient on any Linux distribution.

### 4. Distro-agnostic operation
Commands execute identically regardless of the underlying distribution. The policy engine, audit logging, and Noise XX encryption work the same everywhere.

## Commands

```bash
# Setup all containers
./setup.sh

# Teardown all containers
./setup.sh teardown

# Show container status
./setup.sh status

# Verify all agents respond
./setup.sh verify
```

## Example: Cross-Distro Fleet Query

```bash
RELAY="ws://127.0.0.1:9092"

# Collect OS info from every distro
for distro in ubuntu debian fedora rocky manjaro opensuse alpine amazon void; do
    echo "=== $distro ==="
    rf --relay $RELAY exec --token $distro 'cat /etc/os-release | grep -E "^(PRETTY_NAME|ID)="' 2>/dev/null | grep -v "^2"
    sleep 6
done
```

## Example: Package Manager Detection

```bash
RELAY="ws://127.0.0.1:9092"

# Detect which package manager is available on each distro
for distro in ubuntu debian fedora rocky manjaro opensuse alpine amazon void; do
    echo -n "$distro: "
    rf --relay $RELAY exec --token $distro \
        'command -v apt 2>/dev/null && echo apt || command -v dnf 2>/dev/null && echo dnf || command -v pacman 2>/dev/null && echo pacman || command -v zypper 2>/dev/null && echo zypper || command -v apk 2>/dev/null && echo apk || command -v xbps-install 2>/dev/null && echo xbps || echo unknown' \
        2>/dev/null | grep -v "^2"
    sleep 6
done
```

## Human Approval for AI Agents Scenario

Same MCP server binary, same approval gate on every distro. AI agents get identical human-in-the-loop protection regardless of distribution.

```bash
# Run the full scenario
./scenarios/human-approval.sh

# Static binary: Ubuntu, Alpine, Fedora — same approval workflow
# RBAC: per-AI-agent policy profiles via --callers config
```

## Fleet Orchestration Scenario

Demonstrates fleet orchestration across heterogeneous Linux distributions. One playbook deploys to Ubuntu, Alpine, Fedora, and more — no per-distro agent packages required.

```bash
# Run the full scenario
./scenarios/fleet-orchestration.sh

# Fleet inventory across distros
# Deploy and verify with the same commands on glibc, musl, rpm, deb
```

## Dev Mode (Zero-Setup) Scenario

Demonstrates that dev mode works identically on every Linux distribution. The statically-linked rf binary has zero dependencies — no package manager, no libraries, just copy and run.

```bash
# Run the full scenario
./scenarios/dev-mode.sh

# Works on any distro:
# Ubuntu (glibc):   rf dev → ready
# Alpine (musl):    rf dev → ready
# Fedora (rpm):     rf dev → ready
# rf exec --token dev 'hostname && uname -r'
```

## Port Forwarding Scenario

Demonstrates local port forwarding across different Linux distributions. Starts web servers on Ubuntu (glibc), Alpine (musl), and Fedora (rpm) agents, then tunnels each through encrypted channels.

```bash
# Run the full scenario
./scenarios/port-forwarding.sh

# Forward to each distro's web server:
# rf --relay ws://127.0.0.1:9092 forward --token ubuntu  -L :8080 -R :8000
# rf --relay ws://127.0.0.1:9092 forward --token alpine  -L :8081 -R :8000
# rf --relay ws://127.0.0.1:9092 forward --token fedora  -L :8082 -R :8000
```

## Audit Trail Scenario

Demonstrates structured audit logging across multiple Linux distributions. Executes commands on 5 distros and inspects the independent audit logs, showing identical JSON format regardless of libc or package manager.

```bash
# Run the full scenario
./scenarios/audit-trail.sh

# What it shows:
# - Identical JSON-lines format on Ubuntu (glibc) and Alpine (musl)
# - Independent append-only audit log per agent
# - Entry count per distro
```

## Policy Denial Scenario

Demonstrates deny-by-default policy enforcement across different distributions. Applies a restrictive policy to Ubuntu (glibc) and Alpine (musl-native), proving the policy engine works identically regardless of libc variant.

```bash
# Run the full scenario
./scenarios/policy-denial.sh

# What it tests:
# - Ubuntu (glibc): hostname allowed, apt/curl/rm denied
# - Alpine (musl): hostname allowed, apk/wget denied
# - Audit log entries for every denial
```

Denied command categories per distro:
- **Ubuntu**: `apt install`, `curl`, `rm -rf`
- **Alpine**: `apk add`, `wget`, `rm -rf`
- **All distros**: `shutdown`, `chmod`, `dnf` (where applicable)

## Port Assignment

This demo uses port **9092** (configurable via `RELAY_PORT` env var) to avoid conflicts with the multi-node-ubuntu demo (port 9091).

```bash
# Use a custom port
RELAY_PORT=9099 ./setup.sh
```

## Container Naming

All containers use the `rf-` prefix:
- Relay: `rf-relay-ubuntu`
- Agents: `rf-ubuntu`, `rf-debian`, `rf-fedora`, `rf-rocky`, `rf-manjaro`, `rf-opensuse`, `rf-alpine`, `rf-amazon`, `rf-void`

## Troubleshooting

### Agent not responding after setup
Agents reconnect with exponential backoff. Wait ~5 seconds between commands to the same agent.

### Image pull fails
Some images may not be available for your architecture (e.g., Void Linux on ARM64). The setup script skips unavailable images gracefully.

### Check agent logs
```bash
docker exec rf-fedora cat /var/log/rf-agent.log
```

### Restart a specific agent
```bash
RELAY_IP=$(docker inspect -f '{{range.NetworkSettings.Networks}}{{.IPAddress}}{{end}}' rf-relay-ubuntu)
docker exec rf-fedora bash -c 'pkill rf-agent'
docker exec -d rf-fedora bash -c "RUST_LOG=info rf-agent \
    --relay ws://${RELAY_IP}:9090 --id rf-fedora --token fedora \
    --policy-path /etc/ravenfabric/policy.yaml \
    --audit-path /var/log/rf-audit.jsonl \
    --key-path /etc/ravenfabric/agent.key \
    > /var/log/rf-agent.log 2>&1"
```
