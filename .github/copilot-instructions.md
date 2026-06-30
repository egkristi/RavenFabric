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

Cargo workspace with 14 crates:

| Crate | Purpose | Status |
|---|---|---|
| `rf-crypto` | Noise XX handshake, SecureChannel (encrypted frames), key management, PQ hybrid KEM, no_std frame_codec, HSM/PKCS#11 key provider, TPM 2.0 key sealing | **Done** (~3,547 LOC, 61 tests) |
| `rf-transport` | Driver trait, WebSocket + QUIC + Memory + UNIX socket + Stdio backends, NAT traversal, path selection, exotic transports, LoRa, BLE, AX.25, satellite, mixnet, MASQUE, ECH | **Done** (~22,391 LOC, 557 tests) |
| `rf-rpc` | Request/Response types, msgpack codec, RPC session, yamux multiplexing, controller API, agent registry with relay tracking | **Done** (~6,500 LOC, 122 tests) |
| `rf-audit` | Structured JSON-lines audit logging (every action logged), real-time alert rules with deduplication, alert webhook destinations, Syslog RFC 5424, CEF format, LEEF, OCSF, Splunk HEC, Elasticsearch, Datadog, buffered collector | **Done** (~2,415 LOC, 71 tests) |
| `rf-policy` | YAML policy loading, command/path/network/HTTP/resource enforcement, deny-by-default, CRDT convergence, RBAC, templates, injection detection, header injection/stripping | **Done** (~5,500 LOC, 140 tests) |
| `rf-executor` | Command execution + streaming under policy control with timeout and output limiting, desired-state convergence, event triggers, result parsing, grains | **Done** (~10,700 LOC, 175 tests) |
| `rf-bootstrap` | OTP enrollment flow, TrustStore, relay pairing | **Done** (~430 LOC, 19 tests) |
| `rf-relay` | Stateless encrypted relay broker (binary) with per-IP rate limiting, cross-region forwarding | **Done** (~672 LOC, 15 tests) |
| `rf-agent` | Agent binary (connects to relay or listens directly, executes RPC, reconnect with backoff + relay HA failover, single-threaded mode) | **Done** (~755 LOC) |
| `rf-cli` | CLI client `rf` (exec, dev, status, policy, cp, proxy, completions, direct connect) | **Done** (~2,000 LOC) |
| `rf-mcp-server` | MCP server binary (AI agent integration, stdio + HTTP+SSE transport) | **Done** (~3,400 LOC, 61 tests) |
| `rf-mcp-client` | MCP client SDK (Rust library for building MCP-aware applications) | **Done** (~720 LOC, 14 tests) |
| `rf-ingress` | HTTP ingress gateway (axum), reverse-proxy routing table, API key auth, per-IP rate limiting | **Done** (~580 LOC, 11 tests) |
| `rf-integration-tests` | End-to-end integration tests | **Done** (~2,050 LOC, 50 tests) |

**Total: ~75,139 LOC, 1,429 tests, 0 clippy warnings.**

## Dependency Flow

```text
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
rf-mcp-server (depends on rf-executor, rf-policy, rf-audit)
rf-mcp-client (no internal deps — standalone SDK)
rf-ingress (depends on rf-crypto, rf-transport, rf-rpc, rf-audit)
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

- **Edition**: Rust 2024, MSRV 1.88
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

## Manual Tasks

Some tasks require human action (account creation, secret provisioning, external submissions).
These are tracked in [`MANUAL-TASKS-TODO.md`](../MANUAL-TASKS-TODO.md) at the repo root.

**Before implementing a feature that depends on a secret or external service**, check that file to see if the prerequisite is marked done. Do not attempt to automate tasks listed there — they require human credentials or UI interaction.

## Published Binaries Repository

Pre-built binaries are published to [`egkristi/RavenFabric-Published`](https://github.com/egkristi/RavenFabric-Published).

**Under NO circumstances should anything but compiled binaries be published to that repository.** No source code, no configuration files, no documentation, no Cargo.toml, no .rs files — only executable binaries and checksums. The release workflow includes a safety check that aborts if any source files are detected in the artifacts.

## Known Technical Debt

All previously tracked debt items have been resolved:

- ~~Audit write errors silently swallowed~~ — `log()` returns `Result<(), AuditError>`
- ~~`Box<dyn Error>` return type~~ — replaced with typed `PolicyError`
- ~~No reconnect loop~~ — exponential backoff + jitter implemented
- ~~No config file support~~ — `raven.toml` loading implemented
- ~~No rate limiting~~ — per-IP sliding window rate limiter (20 conn/min)
- ~~No yamux multiplexing~~ — `MuxClient`/`MuxServer` in `rf-rpc`
- ~~No `rf dev` mode~~ — relay + agent in one process

**Current technical debt** is tracked in [ROADMAP.md](../ROADMAP.md#technical-debt) — see the v0.25.3 release checklist for audit findings that need resolution.

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
- **Issue tracking**: When you discover work that should be done but is out of scope for the current task, create a GitHub Issue for it rather than ignoring it

### Push Procedure (MANDATORY)

The repository is **private** by default. Every push must follow this exact sequence:

1. **Make repo public** before pushing:

   ```bash
   gh repo edit egkristi/RavenFabric --visibility public --accept-visibility-change-consequences
   ```

2. **Push commit and version tag** (every version bump commit MUST be tagged to trigger the release pipeline):

   ```bash
   git add -A && git commit -m "<message>" && git push
   git tag -a vX.Y.Z <commit-sha> -m "Release vX.Y.Z — <short description>"
   git push origin vX.Y.Z
   ```

3. **Wait for ALL GitHub Actions pipelines to complete successfully** (Check, Test, Clippy, Format, MSRV, Cross-compile, Coverage, CodeQL, Release, Docker, and any other triggered workflows). Monitor with:

   ```bash
   gh run list --branch main --limit 8
   ```

4. **If any pipeline fails**: Diagnose and fix immediately. Create a GitHub Issue for each distinct problem. Push the fix (repo is still public).
5. **Make repo private** only after all pipelines are green:

   ```bash
   gh repo edit egkristi/RavenFabric --visibility private --accept-visibility-change-consequences
   ```

**Important:** Do not leave the repo public longer than necessary. Make it private again as soon as all pipelines finish. Never push without completing this full cycle.

**Critical:** Every version bump commit **must** be followed by a `git tag` + `git push origin vX.Y.Z`. Without the tag, the Release workflow does not run, and no binaries are published to GitHub Releases or `RavenFabric-Published`.

## GitHub Actions Minutes

The repository uses the public-for-push workflow above because private repos on the GitHub Free plan have limited Actions minutes (2,000/month). Public repos get unlimited free Actions minutes.

## Versioning (Semantic Versioning)

Follow [SemVer 2.0.0](https://semver.org/). The version is defined in `[workspace.package].version` in the root `Cargo.toml` and inherited by all crates.

| Change type | Version bump | Example |
|---|---|---|
| **Breaking API/wire-protocol change** | **Major** (`X.0.0`) | Remove RPC action, change wire format |
| **New feature** (backward-compatible) | **Minor** (`0.X.0`) | New transport driver, new CLI command, new policy capability |
| **Bug fix, docs, refactor** (no new functionality) | **Patch** (`0.0.X`) | Test fix, doc accuracy, clippy cleanup |

When bumping version:

1. Update `version` in root `Cargo.toml` (`[workspace.package]`)
2. Update all inter-crate dependency version strings (`find crates -name Cargo.toml -exec sed ...`)
3. Update `deploy/helm/ravenfabric/Chart.yaml` (`version` + `appVersion`)
4. Add a dated section header in `CHANGELOG.md` (e.g. `## [0.2.0] — 2026-05-08`)
5. Commit with `chore: bump version to X.Y.Z`
6. **Push an annotated tag** — the Release workflow only fires on tag pushes; without it no binaries are published:

   ```bash
   git tag -a vX.Y.Z <commit-sha> -m "Release vX.Y.Z — <short description>"
   git push origin vX.Y.Z
   ```

**Always bump at least patch on every commit that changes code or fixes bugs.** Consider minor bump when new features land.

## Feature Completion Checklist

Every new feature **must** include the following before it is considered done:

1. **Changelog updated** — add entry to `CHANGELOG.md` describing the feature
2. **Relevant documentation updated** — update `README.md`, `ROADMAP.md`, and any other affected docs to reflect the new functionality
3. **ROADMAP.md checklist updated** — mark the corresponding item as `[x]` in the appropriate release checklist section

## Version & Metric Consistency (Mandatory)

Every version bump **must** update all of the following locations atomically. No location may be left behind:

| Location | What to update |
|---|---|
| `Cargo.toml` (`[workspace.package]`) | `version` field |
| All `crates/*/Cargo.toml` inter-crate deps | version strings |
| `CHANGELOG.md` | new dated section header |
| `ROADMAP.md` | Current Status version + stats (LOC, tests) |
| `ARCHITECTURE.md` (root) | stats line + example Cargo.toml snippet |
| `README.md` | status line, version badge, stats header |
| `docs/` (any files with version/stats) | version and stats references |
| `website/index.html` | hero eyebrow, architecture stats, status section release label + all item version badges |
| `website-promotion-ai/index.html` | LOC stat, test stat, and all prose references |
| `sdks/typescript/package.json` + `package-lock.json` | `version` field |
| `sdks/python/pyproject.toml` | `version` field |
| `deploy/helm/ravenfabric/Chart.yaml` | `version` + `appVersion` |
| `deploy/snap/snapcraft.yaml` | `version` |
| `deploy/fdroid/metadata/*.yml` | `versionName` |
| `.github/copilot-instructions.md` | Total LOC + test count in the crate table footer |

**LOC and test counts** must be recalculated from source before every version bump:

```bash
find crates -name "*.rs" | xargs wc -l | tail -1          # total LOC
cargo test --workspace 2>&1 | grep "test result" | awk '{sum += $4} END {print sum}'  # total tests
```

**Rule:** If a commit changes code, every metric-bearing document in this list must reflect the new numbers before the commit is pushed. Stale counts in any document are a defect.

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
  network:
    allow:
      - cidr: "10.0.0.0/8"
        ports: ["80", "443", "8080-8090"]
      - hostname: "*.internal.com"
        ports: ["443"]
    deny:
      - cidr: "192.168.0.0/16"
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

All planned features through v1.3 are **implemented**. The project is in alpha at v0.25.1.

**Current priority:** Fix audit findings from the 26-day rpi5 soak test. See [ROADMAP.md](../ROADMAP.md#release-checklist-v0253--remaining-audit-fixes) for the v0.25.3 release checklist.

**Next priorities (v1.0.0-beta.1 — Beta Readiness):**

1. Fix relay mode — add `--relay` flag to agent systemd config
2. Fix cross-platform Noise XX handshake (snow-0.10.0 `input error` on macOS→Linux)
3. Deploy MCP server as systemd service
4. Add policy rules for non-exec actions (shell, forward, proxy, background)
5. Add RavenFabric-specific Prometheus metrics
6. Restrict `bash` in policy allow list

## Website (ravenfabric.io)

The project landing page is at [ravenfabric.io](https://ravenfabric.io), served via Cloudflare Pages.

### Stack

- **Static HTML/CSS** — single `index.html` with inlined CSS, zero JS dependencies
- **Cloudflare Pages** — builds directly from GitHub (no Actions workflow needed)
- **Custom domain** — `ravenfabric.io` (DNS configured in Cloudflare dashboard)

### Structure

```text
website/
├── index.html              # Single-page landing (all CSS inlined)
├── _headers                # Security headers (Cloudflare Pages native support)
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

Auto-deploys on push to `main` via Cloudflare Pages (connected to GitHub repo):

```text
git push origin main → Cloudflare Pages builds from repo → live at ravenfabric.io (~1-2 min)
```

### Maintenance Rules

- **No build step** — edit `website/index.html` directly, no bundlers or frameworks
- **No JavaScript** — keep the site static HTML/CSS only
- **No localhost references** — CI validates no `localhost` or `127.0.0.1` in HTML
- **Required files** — `index.html`, `assets/favicon.svg`, `assets/og-image.png` must exist (CI validates)
- **Test locally** with `python3 -m http.server 8000` in the `website/` directory
- **og-image.png** must be 1200×630 for proper Open Graph social card rendering
- **Security headers** — defined in `website/_headers`, served natively by Cloudflare Pages
