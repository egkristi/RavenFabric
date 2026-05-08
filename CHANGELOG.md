# Changelog

All notable changes to RavenFabric will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] — 2026-05-08

### Fixed
- **CI**: Fixed `cargo fmt` check failure — expanded struct initializations in `named_pipe.rs` to multi-line format
- **Docker**: Fixed `latest` tag not being applied to container images (tag-triggered workflows don't match `is_default_branch`)

### Changed
- **Release**: Release pipeline now produces all 8 platform binaries (verified: linux-amd64, linux-arm64, linux-amd64-musl, linux-arm64-musl, linux-armv7-musl, darwin-arm64, darwin-amd64, windows-amd64)

## [0.1.1] — 2026-05-08

### Changed
- **Docs**: README.md — comprehensive accuracy audit removing fabricated YAML formats (reduced 1,449 → 1,099 lines, 24% reduction), fixed policy/security/transport/bootstrap/grains sections to match actual codebase, consolidated redundant comparison sections, corrected license badge to AGPL-3.0-or-later
- **Docs**: README.md — updated LOC counts to match actual codebase (~43,800 total, rf-rpc ~5,900, rf-agent ~380)
- **Docs**: ROADMAP.md — cleaned up stale closed-issue references in Distribution & Packaging section, replaced `#N` refs with descriptive status text
- **Docs**: copilot-instructions.md — updated total LOC (~43,800) and rf-integration-tests LOC (~580)
- **Deploy**: Helm Chart.yaml — fixed maintainers YAML format (structured `name:` / `email:` keys)
- **Docs**: cloudnativepg.md — fixed CLI syntax (`rf tunnel` → `rf forward`, added `--token` flags), corrected sealed secrets status (Planned → Done), updated PostgreSQL image tag (18 → 17)
- **Docs**: ai-agent-skill.md — fixed `rf tunnel` → `rf forward` and `rf shell` syntax

### Fixed
- **Tests**: MCP E2E integration tests — fixed race condition where parallel tests shared same temp directory, causing intermittent "missing field `spec`" YAML parse failures in CI

### Added
- **Crypto**: Noise XX handshake (Noise_XX_25519_ChaChaPoly_BLAKE2s) with wire protocol
- **Crypto**: SecureChannel with concurrent read/write via split Mutex pattern
- **Crypto**: StaticKey management (generate, load, save, zeroed on drop)
- **Transport**: Driver trait with AsyncStream abstraction
- **RPC**: Request/Response types with msgpack serialization
- **Audit**: Structured JSON-lines audit logging (FileAuditLogger)
- **Policy**: YAML policy loading with deny-by-default enforcement
- **Policy**: Command pattern matching (regex) and filesystem path checks with symlink resolution
- **Executor**: Command execution with policy enforcement, timeout, output limiting
- **Bootstrap**: OTP generation and validation (single-use, hash-stored, TTL-enforced)
- **CLI**: Skeleton with exec/dev/status subcommands
- **CI**: GitHub Actions (check, fmt, clippy, test, coverage, MSRV)
- **CI**: Cross-platform release workflow (Linux, macOS, ARM64)
- **CI**: CodeQL security scanning
- **CI**: Dependabot for Cargo and GitHub Actions
- **MCP Server**: `rf-mcp-server` binary with JSON-RPC 2.0 over stdio (8 tools: exec, query_policy, file_read, file_write, list_capabilities, audit_query, request_approval, check_approval)
- **Transport**: Named pipe driver for Windows local IPC (`\\.\pipe\ravenfabric`)
- **Transport**: Vsock driver for VM-to-hypervisor communication (Firecracker, QEMU, cloud-hypervisor)
- **Transport**: Abstract namespace socket driver (Linux-only, kernel-managed, no filesystem cleanup)
- **Transport**: Auto-select driver (probes available transports, selects best by priority)
- **Transport**: Socket activation support (systemd-style LISTEN_FDS protocol)
- **Policy**: Behavioral anomaly detection — velocity, novelty, timing, and escalation scoring per identity with automatic capability reduction
- **Audit**: AI compliance reporting — EU AI Act risk classification, NIST AI RMF mapping, human oversight tracking, report generation (JSON/CSV export)
- **RPC**: Embedded Web UI dashboard — real-time agent metrics, connected agents table, activity feed (self-contained HTML/CSS/JS, no external dependencies)
- **MCP Server**: API token authentication — `--api-token` / `RF_API_TOKEN`, constant-time validation
- **MCP Server**: Per-session rate limiting — sliding window throttle (`--rate-limit` / `RF_RATE_LIMIT`, default 60/min)
- **MCP Server**: Session isolation — unique session ID, process-level sandbox, session ID exposed in initialize response
- **Docs**: Claude Code integration guide (`docs/src/integrations/claude-code.md`)
- **Docs**: Cursor integration guide (`docs/src/integrations/cursor.md`)
- **Docs**: Aider integration guide (`docs/src/integrations/aider.md`)
- **Docs**: Claude Desktop integration guide (`docs/src/integrations/claude-desktop.md`)
- **Docs**: AI Agent Quick Start tutorial (`docs/src/getting-started/ai-quickstart.md`)
- **MCP Server**: Anomaly-audit integration — behavioral events written to audit log with baseline comparison
- **MCP Server**: `rf_check_approval` tool — poll approval status (PENDING/APPROVED/DENIED)
- **MCP Server**: `approve()` / `deny()` API for operator approval control
- **MCP Server**: Token rotation — comma-separated tokens for grace period, `--api-token-file` for external rotation
- **MCP Server**: Alert routing — `--alert-webhook` / `RF_ALERT_WEBHOOK` sends anomaly events to HTTP endpoint
- **MCP Server**: RBAC per caller — `--callers` TOML config maps tokens to per-caller policy profiles
- **MCP Server**: Per-session cryptographic identity — short-lived Curve25519 keypair generated per session, public key in `initialize` response and capabilities
- **MCP Server**: HTTP+SSE transport — `--http-listen` for multi-user server deployment (requires `http-sse` feature), per-session isolation, SSE streaming, health endpoint
- **Executor**: Desired-state convergence engine — declarative resource management (packages, files, services, sysctl) with drift detection, remediation mode, version constraints, `ConvergenceReport` (18 tests)
- **Executor**: Event system — trigger-based execution with Cron, FileWatch, ProcessExit, Webhook, Timer triggers, broadcast-based `EventBus`, `TimerScheduler` (12 tests)
- **Executor**: Result parsing and assertions — multi-format parser (JSON, YAML, CSV, key-value, lines, regex) with assertion engine (Eq, Ne, Contains, Matches, Gt/Lt/Gte/Lte, Exists) (18 tests)
- **Executor**: Grains auto-collection — Salt-like system facts (OS, arch, hostname, env) with label selector matching for agent targeting (10 tests)
- **SDK**: Python MCP client (`sdks/python/`) — pip-installable package with async + sync API, StdioTransport, JSON-RPC 2.0, LangChain + CrewAI + OpenAI + Anthropic + AutoGen integrations, typed dataclasses, 41 tests
- **SDK**: TypeScript MCP client (`sdks/typescript/`) — npm package with fully typed async API, StdioTransport, Promise-based JSON-RPC, 12 tests
- **SDK**: Agent framework benchmark suite (`sdks/python/benchmarks/`)
- **Crypto**: `no_std` feature gate — `rf-crypto --no-default-features` compiles without std, exposes `frame_codec` module (ChaCha20-Poly1305 encrypt/decrypt, 7 tests)
- **Crypto**: WASM target support — `rf-crypto` compiles for `wasm32-wasip1`
- **CI**: `no_std + WASM` job validates both compilation targets
- **Deploy**: OpenWrt package (`deploy/openwrt/`) — Makefile + procd init script
- **Deploy**: macOS DMG build script (`deploy/macos/build-dmg.sh`)
- **Deploy**: Alpine APKBUILD (`deploy/alpine/`) with OpenRC init scripts
- **Deploy**: Android NDK cross-compile config (`deploy/android/`) + AndroidManifest.xml
- **Deploy**: iOS Network Extension guide (`deploy/ios/`) + cargo config
- **Docs**: Asciinema demo recording script (`docs/demo/demo-record.sh`)
- **Docs**: `no_std` evaluation for bare-metal ARM (`docs/evaluations/no-std-evaluation.md`)
- **Transport**: MASQUE proxy transport (HTTP/3 CONNECT-UDP tunneling)
- **Transport**: ECH (Encrypted Client Hello) transport for censorship resistance
- **MCP Client**: Rust MCP client SDK (`rf-mcp-client`) — stdio transport, typed tool wrappers (720 LOC, 14 tests)
- **MCP Server**: Fuzz target `fuzz_mcp_protocol` for protocol fuzzing
- **Transport**: File-descriptor passing via SCM_RIGHTS
- **Deploy**: Docker multi-stage build — 4 targets (agent, relay, cli, mcp-server) from scratch/alpine, Rust 1.88 musl static
- **Deploy**: Docker Compose local demo — relay + agent + CLI containers with shared network
- **Deploy**: Helm chart — relay Deployment, agent DaemonSet, ConfigMap, Ingress, NOTES.txt, full values.yaml
- **CI**: Docker workflow — build and push 4 images (multi-arch amd64+arm64) on tag via QEMU
- **Tests**: MCP server E2E integration tests — 8 tests covering initialize/auth, tools/list, exec (allow/deny), policy query, capabilities, invalid method, rate limiting
