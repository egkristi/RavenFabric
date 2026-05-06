# Changelog

All notable changes to RavenFabric will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- **MCP Server**: `rf-mcp-server` binary with JSON-RPC 2.0 over stdio (7 tools: exec, query_policy, file_read, file_write, list_capabilities, audit_query, request_approval)
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
