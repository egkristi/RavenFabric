# RavenFabric — AI Agent Instructions

## Language

All code, comments, documentation, commit messages, and plan files must be written in **English only**.

## Writing Style

- Be conservative with emoji use. A project icon in the title is fine; emoji walls are not.
- Write in a clear, professional tone. Let the content speak for itself.

## Project Overview

RavenFabric is a secure remote execution and mesh networking agent written in Rust.
It replaces Tailscale, Headscale, Ansible, Salt, NetBird, ZeroTier, and similar tools with a single,
cryptographically verified binary. Noise XX mutual authentication, deny-by-default policy,
and structured audit logging are non-negotiable foundations.

## Architecture

Cargo workspace with 10 crates:

| Crate | Purpose | Status |
|---|---|---|
| `rf-crypto` | Noise XX handshake, SecureChannel (encrypted frames), key management | **Done** (~500 LOC, 7 tests) |
| `rf-transport` | Driver trait, WebSocket + Memory backends | **Done** (~250 LOC, 2 tests) |
| `rf-rpc` | Request/Response types, msgpack codec, RPC session, yamux multiplexing | **Done** (~430 LOC, 5 tests) |
| `rf-audit` | Structured JSON-lines audit logging (every action logged) | **Done** (53 LOC, 0 tests) |
| `rf-policy` | YAML policy loading, command/path/resource enforcement, deny-by-default | **Done** (281 LOC, 4 tests) |
| `rf-executor` | Command execution + streaming under policy control with timeout and output limiting | **Done** (~600 LOC, 12 tests) |
| `rf-bootstrap` | OTP enrollment flow, TrustStore, relay pairing | **Done** (~380 LOC, 11 tests) |
| `rf-relay` | Stateless encrypted relay broker (binary) | **Done** |
| `rf-agent` | Agent binary (connects to relay, executes RPC) | **Done** |
| `rf-cli` | CLI client `rf` (exec, dev, status, completions) | **Done** |
| `rf-integration-tests` | End-to-end integration tests | **Done** (2 tests) |

**Total: ~4,000 LOC, 53 tests, 0 clippy warnings.**

## Dependency Flow

```
rf-crypto  (no internal deps)
  ↑
rf-transport (depends on rf-crypto)
rf-bootstrap (depends on rf-crypto)
  ↑
rf-rpc (depends on rf-crypto, rf-transport)
rf-audit (no internal deps)
rf-policy (depends on rf-audit)
  ↑
rf-executor (depends on rf-policy, rf-rpc, rf-audit)
  ↑
rf-relay   (depends on rf-transport)
rf-agent   (depends on rf-crypto, rf-transport, rf-rpc, rf-executor, rf-policy, rf-audit, rf-bootstrap)
rf-cli     (depends on rf-crypto, rf-transport, rf-rpc)
```

## Platform Targets

RavenFabric runs **everywhere**. The agent must compile and operate on any device that can run code:

| Tier | Platforms | Notes |
|------|-----------|-------|
| **Tier 1** (CI-tested, fully supported) | Linux amd64, Linux arm64, macOS amd64, macOS arm64, Windows amd64 | Static musl binaries for Linux |
| **Tier 2** (compiles, best-effort) | Linux armv7 (RPi), Linux riscv64, FreeBSD, Android (aarch64/armv7), iOS (aarch64) | May need reduced feature set |
| **Tier 3** (planned/experimental) | WASM/WASI, OpenWrt (MIPS/ARM), ESPHome/ESP32 (via esp-idf), bare-metal ARM (no_std subset) | Minimal agent profile |

**Design constraints for universal deployment:**
- No libc dependency on Linux (musl static linking)
- No hard dependency on filesystem (embedded/WASM may lack it)
- Async runtime must support single-threaded mode (IoT, constrained devices)
- Agent memory footprint < 10 MB idle (Raspberry Pi Zero, Android background service)
- Binary size < 15 MB stripped (network-constrained deployment)
- All OS-specific code behind `#[cfg()]` — no `#[cfg(unix)]` without a Windows/WASM alternative
- Feature flags for heavy dependencies: `full` (default), `minimal` (no TUN, no sysinfo, no QUIC)

**Mobile considerations:**
- Android: agent runs as foreground service, NDK cross-compile, no JNI in core (optional thin JNI wrapper)
- iOS: agent as Network Extension, no background restrictions if using proper entitlements
- Both: respect OS power management, suspend/resume reconnect cycle

## Coding Standards

- **Edition**: Rust 2024, MSRV 1.85
- **Async runtime**: Tokio (full features for server/desktop; `tokio` with `rt` feature only for constrained)
- **Error handling**: `thiserror` for library errors, `anyhow` only in binaries (agent, relay, cli)
- **Traits**: Use `async-trait` for async trait methods. All traits must be `Send + Sync`
- **Serialization**: `rmp-serde` (msgpack) for wire protocol, `serde_yaml` for config/policy, `serde_json` for audit logs
- **Crypto**: `snow` crate for Noise XX. No TLS. No certificates. Mutual key authentication only
- **HTTP**: None in core. WebSocket via `tokio-tungstenite` for relay transport
- **Logging**: `tracing` crate. Use `info!`, `warn!`, `error!` — never `println!` in libraries
- **Tests**: Unit tests in each crate. Integration tests use `tokio::io::duplex` for simulated connections
- **Platform portability**: Never assume Unix. Use `std::path::Path`, `#[cfg(target_os)]`, and feature gates for OS-specific code

## Key Design Principles

1. **Thread-safe by default**: All public types must be `Send + Sync`
2. **No unwrap in library code**: Use `?` and proper error types. Use `expect()` only for truly impossible failures with an explanation
3. **Deny-by-default**: Policy engine denies anything not explicitly allowed
4. **Zero-trust networking**: Every connection mutually authenticated via Noise XX
5. **Audit everything**: Every RPC action produces a structured audit entry
6. **No plaintext secrets on disk**: Private keys file-permission protected, zeroed on drop
7. **Batch operations**: Where applicable (multi-agent exec, file transfers)
8. **Builder pattern**: For complex types (SecureChannel, Executor, etc.)
9. **Feature flags**: Optional transports behind cargo features (quic, wireguard)
10. **Single static binary**: Agent deploys as one file, no runtime dependencies
11. **Tests for security-critical code**: Every policy check, every executor path, every crypto operation must have test coverage
12. **Propagate errors, never swallow**: Audit writes, file I/O, lock acquisition — failures must be reported, not ignored
13. **Run anywhere**: The agent compiles for any target that supports Rust. No platform is excluded by design. If it has a CPU, it can be a node
14. **Reachable by any means**: Any byte-moving channel is a valid transport. Protocol diversity is a security and resilience property, not a luxury
15. **Adaptive under attack**: If tampering, injection, or interference is detected on a transport, the agent autonomously migrates to an alternative path without dropping the session
16. **Observable everywhere**: Connection health metrics and monitoring data propagate through the same fabric — including mesh hops and DTN store-carry-forward paths. No node is a monitoring blind spot

## Wire Protocol

- Magic: `RVNF` (4 bytes)
- Version: 1 byte
- Handshake: Noise_XX_25519_ChaChaPoly_BLAKE2s
- Frames: `[length: 4 bytes BE][ciphertext + 16-byte MAC]`
- Multiplexing: yamux over SecureChannel
- RPC encoding: msgpack (rmp-serde)

## Security Philosophy

- **Security is always the top priority.** Never trade security for convenience or speed.
- **Security must always be implemented**, not deferred. Every feature ships with its security controls in place.
- **Remember and enforce security policies.** Every code path must respect the deny-by-default policy engine.
- **Airtight policy → execution within its bounds only.** No capability exists outside what the policy explicitly permits. If the policy does not allow it, it does not happen.

## Security Invariants

1. No command executes without policy check
2. No connection accepted without completed Noise handshake
3. Audit log append-only (no delete/truncate operations)
4. Private keys zeroed from memory on drop
5. OTP tokens single-use, hash-stored, TTL-enforced
6. Symlink resolution before path policy checks (prevent traversal)
7. Output size bounded (prevent memory exhaustion)
8. Execution timeout enforced (prevent hanging)
9. No shell injection — commands run via `sh -c` with policy-checked string
10. Relay never decrypts payload (end-to-end between agent and client)
11. Wire protocol magic (`RVNF`) and version byte validated on every connection
12. `RwLock`/`Mutex` poisoning handled gracefully (no panics on poisoned locks)
13. Tamper detection triggers automatic transport migration — compromised paths are abandoned immediately
14. Connection metrics propagate even over DTN/mesh — no blind spots regardless of topology

## Known Technical Debt

The following issues are known and should be addressed:

| Priority | Issue | Location | Fix |
|----------|-------|----------|-----|
| Minor | Audit write errors silently swallowed | `rf-audit/src/logger.rs` | Return `Result` from `log()` or use `tracing::error!` |
| Minor | `Box<dyn Error>` return type | `rf-policy/src/rpc_policy.rs` | Replace with typed `PolicyError` |
| Minor | No reconnect loop | `rf-agent/src/main.rs` | Add exponential backoff + jitter reconnect |
| Minor | No config file support | `rf-agent` | Implement raven.toml loading |
| Minor | No rate limiting | `rf-relay/src/main.rs` | Add per-IP rate limiting |
| Enhancement | No yamux multiplexing | `rf-rpc` | Add yamux for concurrent RPC requests |
| Enhancement | No `rf dev` mode | `rf-cli` | Start relay + agent in one process |

## Testing Requirements

When implementing new code or fixing bugs:

- **Every public function** in library crates must have at least one test
- **Security-critical paths** (policy check, key validation, OTP validation) need positive AND negative tests
- **Serialization types** need roundtrip tests (serialize → deserialize → assert equality)
- **Error paths** must be tested — not just the happy path
- **Use `tokio::io::duplex`** for transport/channel tests (no real network)
- **Use `tempfile`** for filesystem tests (no test pollution)
- Integration tests go in `tests/` directories within crates

## GitHub Security & Quality Checks

Periodically verify the following are in good standing on the GitHub repository:

- **Security policy** — `SECURITY.md` is present and up-to-date
- **Security advisories** — no unresolved advisories
- **Vulnerability reporting** — private reporting enabled
- **Dependabot alerts** — no open critical/high alerts; review and resolve promptly
- **Code scanning alerts** — CodeQL has no open findings
- **Secret scanning alerts** — no exposed secrets; resolve immediately if any appear

## Build & Test

```bash
cargo build              # Debug build
cargo build --release    # Release build (LTO, stripped)
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt --check        # Format check
```

## Git Workflow

- **Commit and push for each completed feature or resolved issue** — do not batch unrelated changes
- All planned changes tracked as GitHub Issues before work begins
- Commit messages: `feat: <description>`, `fix: <description>`, `refactor: <description>`
- Reference GitHub Issues in commits (e.g. `feat: add QUIC transport driver #5`)
- Always run `cargo test` and `cargo clippy` before pushing
- Format: `git add -A && git commit -m "<message>" && git push`
- **After every push**: Check GitHub Actions for pipeline failures. If any workflow fails, diagnose and fix immediately
- **If pipeline fails**: Create a GitHub Issue for each distinct problem so nothing is forgotten, then fix it
- **Issue tracking**: When you discover work that should be done but is out of scope for the current task, create a GitHub Issue for it rather than ignoring it

## Policy YAML Format

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Configuration (raven.toml)

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0  # infinite

[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"
```

## Implementation Priority (What to Build Next)

The critical path to a working demo (`rf exec --token <token> "cmd"`) is **COMPLETE**.
v0.1 foundation is done: all crates implemented, 35 tests passing, integration tests working.

**Next priorities (v0.2):**
1. QUIC transport driver
2. yamux multiplexing for concurrent RPC
3. Hot-reload policy via SIGHUP
4. Streaming stdout/stderr via mux stream
5. Agent enrollment flow (OTP → key exchange)
6. Per-IP rate limiting on relay

## Website (ravenfabric.io)

The project landing page is at [ravenfabric.io](https://ravenfabric.io), served via GitHub Pages.

### Stack

- **Static HTML/CSS** — single `index.html` with inlined CSS, zero JS dependencies
- **GitHub Pages** — hosting via `.github/workflows/pages.yml`
- **Custom domain** — `ravenfabric.io` (CNAME file in `website/`)

### Structure

```
website/
├── index.html              # Single-page landing (all CSS inlined)
├── CNAME                   # Custom domain for GitHub Pages
├── robots.txt              # SEO crawler directives
├── sitemap.xml             # Sitemap for search engines
├── .well-known/
│   └── security.txt        # RFC 9116 security contact
└── assets/
    ├── favicon.svg         # SVG favicon
    ├── og-image.svg        # Open Graph source
    └── og-image.png        # Open Graph rendered (1200×630)
```

### Deployment

Auto-deploys on push to `main` when files under `website/` change:
```
git push origin main → GitHub Actions → live at ravenfabric.io (~1-2 min)
```

### Maintenance Rules

- **No build step** — edit `website/index.html` directly, no bundlers or frameworks
- **No JavaScript** — keep the site static HTML/CSS only
- **No localhost references** — CI validates no `localhost` or `127.0.0.1` in HTML
- **Required files** — `index.html`, `CNAME`, `assets/favicon.svg`, `assets/og-image.png` must exist (CI validates)
- **CNAME must stay** — removing it breaks the custom domain
- **Test locally** with `python3 -m http.server 8000` in the `website/` directory
- **og-image.png** must be 1200×630 for proper Open Graph social card rendering
