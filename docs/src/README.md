# RavenFabric

**Secure remote execution and mesh networking — one binary, zero trust, any transport.**

RavenFabric is a universal agent that provides policy-controlled, E2E-encrypted access to any system — regardless of network topology, operating system, or device class. It unifies mesh VPN, remote execution, configuration management, and zero-trust access into a single static binary.

## Core Properties

| Property | Implementation |
|----------|----------------|
| **E2E encrypted** | Noise XX mutual authentication (same crypto as WireGuard) |
| **Deny-by-default** | Policy engine rejects anything not explicitly allowed |
| **Network-agnostic** | WebSocket, QUIC, WireGuard, serial, LoRa, DTN, physical media |
| **Single static binary** | No runtime dependencies, < 15 MB, deploys anywhere |
| **Audit everything** | Structured JSON-lines log for every action |
| **Air-gap capable** | Store-carry-forward delivery over physical media |

## Project Stats

- **11 crates** — layered architecture with strict dependency boundaries
- **~31,000 lines of code** — Rust, Edition 2024
- **564 tests** — unit, integration, and fuzz
- **0 clippy warnings** — enforced in CI

## Quick Links

- [Installation](getting-started/installation.md) — Get the binary
- [Quick Start](getting-started/quickstart.md) — First command in 2 minutes
- [Architecture](architecture/overview.md) — How it works
- [Use Cases](use-cases/cloudnativepg.md) — Real-world deployment patterns
- [CLI Reference](reference/cli.md) — All commands and options
- [Production Deployment](guide/production-deployment.md) — systemd, TLS, monitoring
- [GitHub Repository](https://github.com/egkristi/RavenFabric)
