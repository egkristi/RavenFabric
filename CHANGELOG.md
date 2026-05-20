# Changelog

All notable changes to RavenFabric will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.0] — 2026-05-20

### Added

- **LEEF (Log Event Extended Format) audit formatter** — new `LeefAuditLogger<L>` in `rf-audit::leef`. Generic wrapper around any `AuditLogger`. Supports LEEF 1.0 and 2.0 via `LeefVersion` enum (default: V2). LEEF 1.0 header: `LEEF:1.0|Vendor|Product|Version|EventID|attrs`; LEEF 2.0 adds a Label field: `LEEF:2.0|Vendor|Product|Version|EventID|Label|attrs`. Tab-delimited attributes: `devTime`, `requestId`, `act`, `outcome`, `sev`, `src`, `duration`, `matchedRule`, and optionally `cmd`, `exitCode`, `reason`. Proper LEEF escaping (`|`, `\`, `=`) in both header and attribute values. `new()` constructor (defaults to V2) and `with_version()` to select format. IBM QRadar compatible. 9 new tests.
- **OCSF (Open Cybersecurity Schema Framework) audit formatter** — new `OcsfAuditLogger<L>` in `rf-audit::ocsf`. Converts each `AuditEntry` to OCSF 1.1.0 schema, class_uid 6003 (Application Activity). Types: `OcsfMetadata`, `OcsfProduct`, `OcsfActor`, `OcsfUser`, `OcsfEvent`. Maps audit decisions to OCSF severity codes (0–6) and status codes (0=Unknown, 1=Success, 2=Failure). Public functions: `to_ocsf_event()` and `format_ocsf()`. Serialization errors logged at `warn` level, never surfaced as errors. 12 new tests.
- **Splunk HEC audit destination** — new `SplunkHecAuditLogger` in `rf-audit::splunk`. Sends audit entries to a Splunk HTTP Event Collector endpoint with token-based authentication. `SplunkHecConfig`: configurable `url`, `token`, `index` (default `"main"`), `batch_size` (default 1). `HecPayload`: `time` (f64 Unix epoch), `source` (`"ravenfabric"`), `sourcetype` (`"rf:audit"`), `index`, `event`. Batch queue (`Arc<Mutex<Vec<String>>>`): accumulates serialized events and flushes at `batch_size`. `Drop` impl: flushes any remaining buffered events on drop; send failures logged at `warn`. 8 new tests.

### Changed

- **Version**: Bumped to 0.11.0 across all crates and deploy manifests
- **Total tests**: 1,232 (up from 1,203) — +29 rf-audit (9 leef + 12 ocsf + 8 splunk)
- **LOC**: ~60,548 (up from ~59,448)
- `rf-audit::lib` now exports `leef`, `ocsf`, and `splunk` modules

## [0.10.0] — 2026-05-20

### Added

- **Syslog RFC 5424 audit destination** — new `SyslogAuditLogger` in `rf-audit::syslog`. Implements `AuditLogger` and sends each entry as an RFC 5424 syslog message to a remote server via UDP or TCP. UDP variant reuses a single bound socket (fire-and-forget, no error propagation). TCP variant maintains a persistent connection with 5-second timeouts and reconnects on drop; uses RFC 6587 octet-counting framing. Facility (`SyslogFacility`: Kernel, User, Daemon, Auth, Local0–Local7) and severity (`SyslogSeverity`: Emergency through Debug) are configurable. Priority = `facility * 8 + severity`. Structured data includes `request_id`, `decision`, `matched_rule`, `caller_key`, `duration_ms`. Delivery failures are logged at `warn` level, never surfaced as errors. New constructors: `SyslogAuditLogger::udp()` and `SyslogAuditLogger::tcp()`. 6 new tests.
- **CEF (Common Event Format) audit wrapper** — new `CefAuditLogger<L>` in `rf-audit::cef`. Generic wrapper around any `AuditLogger`. Converts each `AuditEntry` to a CEF-formatted line (`CEF:0|RavenFabric|RavenFabric|version|class_id|name|severity|extension`) before forwarding to the inner logger. CEF severity maps from audit decision (`denied→8`, `error→7`, `allowed→3`, other→`5`). Extension fields include `rt` (epoch ms), `requestId`, `act`, `outcome`, `dvcpid`, `reason`, `duration`, `cs1Label=matchedRule`, `cs1`, `exitCode`, `suser`. Proper CEF escaping: `|` and `\` in header fields; `=`, `\`, `\n`, `\r` in extension values. Compatible with ArcSight, Splunk, IBM QRadar, and other SIEM systems. 8 new tests.
- **Rotation audit trail** — `TrustStore` now maintains an in-memory append-only rotation log (`Vec<RotationEvent>`). New types: `RotationEventType` (`Rotate` / `Revoke`) and `RotationEvent` (`timestamp`, `agent_id`, `event_type`, `old_key_hash`, `new_key_hash`, `version`). `rotate_key()` appends a `Rotate` event with the hex digest of both old and new keys, plus the new version number. `revoke_immediate()` appends a `Revoke` event with the old key hash and current version. New `TrustStore::rotation_history()` method returns the full audit trail. 3 new tests: `test_rotation_audit_trail_rotate`, `test_rotation_audit_trail_revoke`, `test_rotation_audit_trail_multiple_events`.

### Changed

- **Version**: Bumped to 0.10.0 across all crates and deploy manifests
- **Total tests**: 1,203 (up from 1,186) — +14 rf-audit (syslog + CEF), +3 rf-bootstrap (rotation trail)
- **LOC**: ~59,448 (up from ~58,611)
- `rf-audit::lib` now exports `cef` and `syslog` modules

## [0.9.0] — 2026-05-20

### Added

- **zstd compression for file transfer** — `FilePush` and `FilePull` RPC actions now accept an optional `compress: bool` field. When enabled, the client sends compressed chunks (FilePush) or the agent returns compressed chunks (FilePull) using zstd level-3 compression. The `FileChunk` response includes a `compressed` flag so the receiver knows whether to decompress. Transparent and policy-neutral — compression is negotiated per-request. 2 new executor tests: `test_file_push_compress`, `test_file_pull_compress`.
- **Secret versioning in TrustStore** — `TrustedAgent` now tracks `version: u32` (starts at 1, incremented on each key rotation), `key_history: Vec<String>` (all previous public keys, oldest first), `revoked: bool`, and `revoked_at: Option<String>` (RFC 3339 timestamp). New `rotate_key(old, new)` method atomically increments version and archives the old key.
- **Emergency revocation** — new `TrustStore::revoke_immediate(public_key)` method immediately marks an agent as revoked (`revoked=true`, `revoked_at=<now>`). `is_trusted()` returns `false` for revoked agents instantly without removing the entry from the store (preserved for audit). 4 new bootstrap tests: `test_is_trusted_false_when_revoked`, `test_revoke_immediate_sets_fields`, `test_revoke_immediate_unknown_key`, `test_rotate_key`, `test_key_history_after_rotation`.

### Changed

- `TrustStore::is_trusted()` now returns `false` for revoked agents (previously only checked key presence).

## [0.8.0] — 2026-05-20

### Added

- **Generic webhook audit log forwarding** — new `WebhookAuditLogger` in `rf-audit::logger`. Wraps any existing `AuditLogger` (e.g., `FileAuditLogger`) and forwards each audit entry to a remote HTTP endpoint via an asynchronous fire-and-forget HTTP POST. Payload is a JSON-serialized `AuditEntry` (same format as JSON-lines files). Supports `http://host:port/path` scheme. Connection and write failures are logged at `warn` level and never surface as errors to the caller. 5 new tests: `test_webhook_audit_logger_delivers_entry` (TCP listener verifies HTTP POST), `test_webhook_audit_logger_delegates_to_inner` (file logger also receives entry), `test_parse_webhook_url_with_port_and_path`, `test_parse_webhook_url_default_port`, `test_parse_webhook_url_invalid`.

### Changed

- **Version**: Bumped to 0.8.0 across all crates and deploy manifests
- **Total tests**: 1,179 (up from 1,174)
- **rf-audit**: +5 tests (32 total); `WebhookAuditLogger`, `parse_webhook_url()`, `post_audit_webhook()` added to `logger` module

## [0.7.0] — 2026-05-20

### Added

- **Alert webhook destinations** — `AlertRule::with_webhook(url)` builder method. When an alert fires (and is not suppressed by deduplication), an asynchronous HTTP POST is dispatched to the configured URL with a JSON payload containing `rule`, `action`, `decision`, `request_id`, `matched_rule`, `command`, and `timestamp`. Delivery is fire-and-forget; failures are logged at `warn` level. Supports `http://host:port/path` scheme. 4 new tests in rf-audit: `test_webhook_url_configured`, `test_no_webhook_by_default`, `test_webhook_delivered_on_alert` (binds a TCP listener and verifies the HTTP POST arrives), `test_webhook_not_fired_when_deduped` (verifies suppressed alerts produce no network traffic).

### Changed

- **Version**: Bumped to 0.7.0 across all crates and deploy manifests
- **Total tests**: 1,174 (up from 1,170)
- **rf-audit**: +4 tests (27 total); `AlertRule` gains `webhook_url` field and `with_webhook()` builder

## [0.6.0] — 2026-05-20

### Added

- **HTTP rate limiting per destination** — `maxHttpRequestsPerWindow` and `httpRateLimitWindowSecs` fields in policy `resources` section. When set, limits the number of `HttpForward` requests to each upstream target within the configured window. Tracked per-destination in a sliding-window timestamp queue (`VecDeque<Instant>`) stored in the executor. Returns `RpcResult::Denied` with `rule: resources.maxHttpRequestsPerWindow` when exceeded. 0 (default) means unlimited. 2 new tests in rf-executor, 2 new tests in rf-policy.
- **Bandwidth throttling for file transfer** — `maxTransferBytesPerSec` field in policy `resources` section (default 0 = unlimited). Enforced on both `FilePush` (per chunk, after write) and `FilePull` (per chunk, after read). Uses elapsed-time pacing: if chunk transfer was faster than the configured rate, sleeps the remainder of the expected duration. 2 new tests in rf-policy, 2 new tests in rf-executor.

### Changed

- **Version**: Bumped to 0.6.0 across all crates and deploy manifests
- **Total tests**: 1,170 (up from 1,164)
- **rf-policy**: +2 tests (140 total); 3 new fields in `RpcPolicy`/`ResourceSpec`
- **rf-executor**: +4 tests (173 total); rate-limit state in `Executor`, throttle logic in `handle_file_push`/`handle_file_pull`

## [0.5.0] — 2026-05-20

### Added
- **Header injection/stripping policy** — `http.headers.require` and `http.headers.forbid` lists in policy YAML. Required headers must be present on every HTTP request; forbidden headers must not appear. Enforced by `check_http_headers()` in rf-policy, called from `handle_http_forward()` in rf-executor before connecting to the upstream. Case-insensitive header name matching. 6 new tests in rf-policy.
- **Real-time alert rules** — `AlertRule` and `AlertEngine` in new `rf-audit::alert` module. Rules match audit entries by pattern (regex applied to `action`, `decision`, and `command`). Matching rules emit `tracing::warn!` structured alerts. Alert deduplication: same rule+action pair suppressed within a configurable window (`dedup_window_secs`). 9 new tests in rf-audit.
- **Glob pattern expansion for `rf cp`** — `rf cp "/var/log/*.gz" agent:/backup/` expands glob patterns locally before uploading. Multiple matched files are pushed individually; destination directory is derived per-file. Powered by the `glob` crate. Invalid patterns and zero-match globs return a clear error.

### Changed
- **Version**: Bumped to 0.5.0 across all crates and deploy manifests
- **Total tests**: 1,164 (up from 1,149)
- **rf-audit**: +9 tests (23 total); added `glob` dependency to workspace

## [0.4.0] — 2026-05-16

### Added
- **MCP tool: `rf_http_request`** — AI agents can call private APIs through RavenFabric with full policy enforcement. Supports GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS. Structured JSON response with `status_code`, `headers`, `body` (auto-parsed as JSON when `Content-Type: application/json`), and `latency_ms`. Method validated against allowlist. 4 new tests in rf-mcp-server.
- **File transfer size limits** — `maxFileSizeBytes` field in policy `resources` section (default 100 MB). Enforced on both `FilePush` (checked per chunk: `offset + chunk_size`) and `FilePull` (checked against file metadata before reading). Returns `RpcResult::Denied` with `rule: resources.maxFileSizeBytes` when exceeded. 2 new tests in rf-executor, 2 new tests in rf-policy.

### Changed
- **Version**: Bumped to 0.4.0 across all crates and deploy manifests
- **Total tests**: 1,149 (up from 1,141)

## [0.3.0] — 2026-05-14

### Added
- **HTTP-aware proxy mode** — `rf proxy --http` enables per-request HTTP policy enforcement. Agent parses method, path, headers via `httparse` before forwarding to upstream. New `HttpForward` RPC action and `HttpResponse` result type. HTTP policy rules in YAML (`http.allow`/`http.deny` by method + path regex). Request/response body size limits (`maxRequestBodyBytes`/`maxResponseBodyBytes`). Per-request audit logging with method, path, status code, latency. Header injection detection prevents CRLF attacks. 10 new tests.
- **Proxy idle timeout + max duration** — Configurable per-connection limits prevent resource exhaustion from abandoned tunnels. Policy defaults (`proxyIdleTimeoutSeconds: 300`, `proxyMaxDurationSeconds: 3600`) with per-request override via `--idle-timeout` and `--max-duration` CLI flags. Effective values returned in `ProxyConnected` response. 2 new tests.
- **Network policy rules for proxy targets** — `check_network_target()` enforces CIDR/hostname/port allow/deny rules on TCP proxy connections. Immutable deny blocks link-local/metadata addresses (169.254.0.0/16, fe80::/10). IPv4, IPv6 (bracket notation), port ranges, hostname globs (`*.internal.com`), deny-by-default. 14 new tests.
- **MCP tool: `rf_file_transfer`** — AI agents can copy files on-agent with full policy enforcement, integrity verification (SHA-256), and audit logging. 9th MCP tool.
- **Recursive directory transfer** — `rf cp -r` flag for uploading entire directory trees to agents with automatic file enumeration
- **Bulk file transfer** — `FilePush` and `FilePull` RPC actions with chunked transfer, SHA-256 integrity verification, atomic writes (temp + rename), resumable offset tracking, and full policy enforcement
- **TCP proxy tunneling** — `Proxy` RPC action with policy-checked TCP connect, `rf proxy` CLI command for local listener tunneling through agent to remote targets
- **`rf cp` CLI command** — familiar scp-like syntax (`rf cp local agent:/path` and `rf cp agent:/path local`) with progress reporting and checksum verification
- **Release: rf-mcp-server in binaries** — MCP server now included in release artifacts for all platforms (was previously source-only). Install script downloads all 4 binaries: rf-agent, rf-relay, rf, rf-mcp-server
- **v0.3.0 features shipped** — QUIC, WireGuard direct, STUN hole-punching, interactive shell, port-forwarding, playbooks, full mesh VPN, MagicDNS, sealed secrets, Reticulum, Tor, serial drivers — all confirmed code-complete and tested

### Fixed
- **install.sh**: Fixed "unbound variable" error when piped to bash (`curl | bash`). The EXIT trap referenced a local variable (`tmpdir`) that was out of scope after `main()` returned. Moved to global cleanup function
- **Release workflow**: Added `rf-mcp-server` build step and packaging — previously only rf-agent, rf-relay, and rf (CLI) were included in release artifacts

### Changed
- **Version**: Bumped to 0.3.0 across all crates, deploy manifests, SDKs, and documentation

## [Unreleased]

### Changed
- **website**: Migrated hosting from GitHub Pages to Cloudflare Pages (builds directly from GitHub). Security headers (`_headers`) now served natively. HSTS preload active. Removed `website/CNAME` and `.github/workflows/pages.yml`

### Added
- **Security audit tests**: 17 new integration tests covering key zeroization, OTP replay prevention, policy bypass, wire protocol rejection, and codec stability (`security_audit.rs`)
- **Wire protocol stability doc**: Formal stability guarantees for wire format, handshake sequence, and RPC serialization (`docs/src/reference/wire-protocol-stability.md`)
- **API stability markers**: `#[non_exhaustive]` on core public enums (`CryptoError`, `RpcError`, `PolicyError`, `Action`, `RpcResult`) — prevents downstream breakage when new variants are added

### Security
- **wasmtime**: Upgraded from v29 to v36.0.9, resolving all 16 security advisories (GHSA-q8hx-mm92-4wbg, GHSA-4w5q-m7x3-bxgf, GHSA-v39r-r8gw-p945, GHSA-5wvc-xrjx-h2xq, GHSA-5jmc-43q8-x28q, GHSA-7mpv-9xg9-5jx4, GHSA-75hq-h9g9-4gjr, GHSA-5wgq-hcmq-3rf7, GHSA-5j3r-j6x2-23x2, GHSA-cx96-5vf6-8x3f, GHSA-34ch-7c68-q6x6, GHSA-rj3g-829c-8jpc, GHSA-pp24-53gm-jr4j, GHSA-4x44-w425-m2p3, GHSA-jcr4-92f4-r3jm, GHSA-w2mj-m73j-q22c). v36.0.9 is optimal: MSRV-compatible (requires Rust 1.86, project uses 1.88) and covers all backported fix ranges

### Fixed
- **deps**: Migrated rf-executor, rf-policy, rf-rpc from `sha2 = "0.10"` to workspace `sha2 = "0.11"` — eliminates direct dependency version mismatch (#99)
- **code quality**: Replaced 2 production `unwrap()` calls in rf-transport (overlay.rs, quic.rs) with `expect()` + justification
- **docs**: Fixed inaccurate test counts in README (rf-transport 551→542, rf-rpc 120→112, rf-mcp-client 15→14) and copilot-instructions

### Changed
- **ci**: Replaced `cargo-tarpaulin` with `cargo-llvm-cov` for more accurate coverage measurement
- **deps**: Bumped `criterion` from 0.5.1 to 0.8.2
- **deps**: Bumped `toml` from 0.8.23 to 1.1.2
- **deps**: Bumped `rusqlite` from 0.35.0 to 0.39.0
- **ci**: Bumped `docker/login-action` from 3 to 4
- **ci**: Bumped `docker/build-push-action` from 6 to 7
- **ci**: Bumped `docker/setup-qemu-action` from 3 to 4
- **ci**: Bumped `actions/upload-pages-artifact` from 3 to 5
- **ci**: Bumped `actions/deploy-pages` from 4 to 5
- **security**: Updated SECURITY.md supported versions (0.3.x is now supported, 0.2.x deprecated)

### Added
- **Direct connection mode**: Agent `--listen` flag starts a WebSocket server for point-to-point connections (like sshd). CLI `--connect` flag dials the agent directly, bypassing the relay. Same Noise XX encryption, policy enforcement, and audit logging as relay mode
- **Demos**: Direct connection demo (`demos/direct-connection/`) — 4 scenarios: direct exec, system info, policy denial, audit trail
- **Demos**: MCP/AI Agent demo (`demos/mcp-agent/`) — 6 scenarios: policy discovery, command execution, policy denial, human approval, audit trail, file operations
- **Demos**: Resilience demo (`demos/resilience/`) — 5 scenarios: agent reconnect, relay restart recovery, network partition, graceful degradation, exponential backoff visualization (4 Docker containers)
- **Demos**: Controller/Web UI demo (`demos/controller/`) — 5 scenarios: agent list, health check, remote execution via HTTP API, fleet dashboard, policy view (3 Docker containers)

### Changed
- **Docs**: README demos section expanded from 6 to 9 demos with subsections for each
- **Docs**: ROADMAP demo consolidation items resolved — multi-distro-linux and kubernetes-cnpg scenarios confirmed to contain environment-specific content (not duplicates)

## [0.2.0] — 2026-05-10

### Added
- **Web UI**: HTTP server for the embedded dashboard — binds `TcpListener`, serves dashboard HTML at `/`, routes `/api/*` through `ApiDispatcher`, bearer token auth, security headers (X-Frame-Options, X-Content-Type-Options, Cache-Control), request size limiting (1 MB), 6 tests
- **WASM Plugins**: Plugin execution runtime via `wasmtime` behind `wasm-plugins` feature flag — `WasmRuntime` loads and executes WASM modules with fuel metering, memory isolation, and the `alloc/process/result_len` host interface. Stub runtime without feature returns clear error. 2 new tests
- **SPIFFE Identity**: Full workload identity implementation — `SpiffeIdentity::new()`, `parse()`, `validate()`, `path_matches()` with wildcard support, `TrustBundle` with domain verification, `SpiffeError` type, 8 new tests
- **Controller**: HTTP server serves controller API endpoints (agent list, health check) over the network — previously in-memory only

### Changed
- **Versions**: All packaging manifests, SDKs, website, demos, and docs updated to v0.2.0
- **Stats**: 13 crates, ~53,900 LOC, 1,094 tests, 0 clippy warnings

## [0.1.6] — 2026-05-10

### Added
- **CI**: Binary size gate — new CI job builds release binaries and fails if any exceed 15 MB, enforcing the deployment size constraint documented in the architecture

### Changed
- **Versions**: All packaging manifests, SDKs, website, demos, and docs updated from v0.1.5 to v0.1.6

## [0.1.5] — 2026-05-10

### Added
- **Security**: Policy enforcement for all RPC handlers — shell open, port forwarding (local/remote/SOCKS5), health check (command probes), and log tail now go through deny-by-default policy checks
- **Security**: Audit logging for all RPC actions — every handler (metrics, status, read, write, list, signal, background exec, shell open/input/close, port forward start/close, remote forward, SOCKS5 forward, health check, tail log) now produces a structured audit entry with request ID, action, decision, matched rule, and duration
- **Security**: Policy check for `signal` action — `kill -<signal> <pid>` is now checked against command policy before execution
- **Security**: Shell session recording — every PTY session records all input/output in asciicast v2 format (NDJSON) and emits the full recording to the audit log on session close for replay-grade compliance traceability
- **Tests**: Shell session recording test — verifies open/input/close lifecycle, audit entries for each phase, asciicast v2 recording in audit with header validation
- **Tests**: Audit JSON round-trip test — verifies all audit entries serialize to valid JSON with required fields (timestamp, request_id, action, decision, matched_rule, caller_key, duration_ms)
- **Tests**: Strengthened audit assertions — existing tests now verify `action`, `command`, `request_id`, `caller_key`, and `matched_rule` fields (previously only checked `decision`)
- **Demos**: Desired-State Convergence demo — 7 scenarios covering drift detection, auto-remediation, report-only mode, grains-based targeting, event triggers, and version constraints (`demos/desired-state/`)
- **Tests**: 18 desired-state showcase integration tests — YAML parsing, 4 resource types (packages/files/services/sysctl), convergence modes, grains label matching, event bus triggers, full lifecycle (`crates/rf-integration-tests/tests/desired_state_showcase.rs`)
- **Demos**: Transport Showcase demo — 5 transports (WebSocket, QUIC, UNIX Socket, Stdio Pipe, Memory) with end-to-end encrypted command execution over each, proving transport interchangeability (`demos/transport-showcase/`)
- **Tests**: 5 transport showcase integration tests — each performs Noise XX handshake + SecureChannel + RPC execution over a different transport driver (`crates/rf-integration-tests/tests/transport_showcase.rs`)
- **Demos**: Multi-Node Ubuntu demo — 2-agent Docker setup with relay, setup/teardown scripts, 11 scenario scripts (`demos/multi-node-ubuntu/`)
- **Demos**: Multi-Distro Linux demo — 9 Linux distributions (Ubuntu, Debian, Fedora, Rocky, Manjaro, openSUSE, Alpine, Amazon Linux, Void) with setup/verify/teardown (`demos/multi-distro-linux/`)
- **Demos**: Kubernetes + CloudNativePG demo — 2-instance CNPG PostgreSQL cluster with rf-agent sidecar, Gatekeeper exemption handling, auto-detect host IP (`demos/kubernetes-cnpg/`)
- **Demos**: Asciinema recording scripts and animated SVG exports for all 3 demos (`demos/recordings/`)
- **Demos**: Policy Denial scenario for all 3 demos — restrictive policy, allowed/denied command tests, audit log inspection (`scenarios/12-policy-denial.sh`, `scenarios/policy-denial.sh`)
- **Demos**: Audit Trail scenario for all 3 demos — structured JSON-lines audit log inspection, per-agent entry counts, cross-distro format consistency (`scenarios/13-audit-trail.sh`, `scenarios/audit-trail.sh`)
- **Demos**: Port Forwarding scenario for all 3 demos — local/reverse/SOCKS5 forwarding through encrypted tunnels, cross-distro tunneling, K8s PostgreSQL port forwarding (`scenarios/14-port-forwarding.sh`, `scenarios/port-forwarding.sh`)
- **Demos**: Dev Mode (Zero-Setup) scenario for all 3 demos — single-command dev environment, relay + agent in one process, zero config (`scenarios/15-dev-mode.sh`, `scenarios/dev-mode.sh`)
- **Demos**: Fleet Orchestration scenario for all 3 demos — multi-agent playbooks with parallel/sequential/rolling/canary strategies, automatic rollback (`scenarios/16-fleet-orchestration.sh`, `scenarios/fleet-orchestration.sh`)
- **Demos**: Human Approval scenario for all 3 demos — human-in-the-loop approval gate for AI-controlled agents via MCP, operator approve/deny workflow, defense in depth (`scenarios/17-human-approval.sh`, `scenarios/human-approval.sh`)
- **MCP Server**: Approval enforcement in `tool_exec` — commands matching `--approval-pattern` regex patterns are blocked until a human operator approves via `approve()`/`deny()` API
- **MCP Server**: `--require-approval` mandatory mode — when enabled, ALL mutating operations (`rf_exec`, `rf_file_write`) require a valid approval, making bypass impossible regardless of patterns
- **MCP Server**: Approval enforcement in `tool_file_write` — file write operations require approval when `--require-approval` is set, using `write:<path>` as the command binding
- **MCP Server**: SHA-256 command hash verification — approved command is cryptographically bound to the approval, preventing command substitution attacks
- **MCP Server**: One-time-use enforcement — each approval can only be consumed once, subsequent attempts return DENIED
- **MCP Server**: RBAC enforcement fix — executor now shares the same policy Arc as the server, so caller profile policies are actually enforced during command execution (previously executor used a separate, never-updated policy)
- **MCP Server**: 30-minute TTL on approvals — expired approvals automatically return DENIED
- **MCP Server**: `approval_id` parameter added to `rf_exec` tool for passing approved approval IDs
- **Website**: Live demos page at `/demos/` with animated terminal recordings, architecture diagrams, setup instructions
- **Website**: Policy Denial section added to each demo on the website demos page
- **Website**: "Demos" navigation link added to main site, blog pages, and footer
- **Blog**: "Demo 1: Multi-Node Ubuntu" post — walkthrough of all 17 scenarios, from remote execution to human approval for AI agents
- **CLI**: `--stream` and `--background` execution flags for streaming and fire-and-forget modes

### Fixed
- **Agent/Relay/CLI**: `RUST_LOG` environment variable now correctly controls log filtering — `RUST_LOG=warn` properly suppresses INFO/DEBUG lines (#95)
- **CLI**: Added `close_notify()` after `exec` and `status` commands — agent now detects session end and reconnects cleanly instead of hanging indefinitely
- **K8s Demo**: Deployment uses `strategy: Recreate` with `terminationGracePeriodSeconds: 3` and SIGTERM→SIGINT trap to prevent dual-pod relay pairing race condition
- **Versions**: All packaging manifests, SDKs, website, Web UI, and docs updated from v0.1.4 to v0.1.5
- **Docs**: Python SDK documentation URL now points to public `ravenfabric.io/docs/` instead of private repo
- **Docs**: Contributing guide SECURITY.md link changed from absolute GitHub URL to relative path
- **Transport**: QUIC test no longer flaky — uses OS-assigned port directly instead of rebinding, eliminating address-already-in-use race condition (#96)

## [0.1.4] — 2026-05-08

### Added
- **Packaging**: macOS `.pkg` installer build script (`deploy/macos/build-pkg.sh`) — universal binary support, launchd integration, pre/post install scripts
- **Packaging**: openSUSE OBS spec file (`deploy/obs/ravenfabric.spec`) — RPM packaging for zypper/OBS, systemd integration, dedicated user/group
- **Packaging**: F-Droid metadata (`deploy/fdroid/`) — full app listing with descriptions, changelog, build recipe for aarch64 Android
- **CLI**: Added `--version` flag to `rf` CLI (reads version from Cargo.toml via clap)
- **Homebrew**: Fixed formula to use pre-built binaries from `RavenFabric-Published` with real SHA256 hashes

### Fixed
- **Website**: Removed all links to private `egkristi/RavenFabric` repo from public website
- **Website**: Hero CTA changed from "View on GitHub" to "Download Latest Release" (→ RavenFabric-Published)
- **Website**: Badges now use static shields.io (version, language, license) + Latest Release from Published repo
- **Website**: Removed GitHub card sidebar, nav GitHub button, footer repo links
- **Website**: FAQ updated — source access pending legal review, available on request
- **Docs**: Removed GitHub repo/edit links from mdBook (`book.toml`) — no more broken edit buttons
- **Docs**: Fixed `security.txt` — removed private repo advisory/policy URLs
- **Docs**: Replaced GitHub link in docs landing page with releases link
- **Docs**: Fixed troubleshooting page — issues link replaced with email contact
- **Docs**: Blog pages — GitHub links → RavenFabric-Published releases
- **Docs**: Compliance docs version updated from `v0.5-dev` to `v0.1.4` with correct stats (50k LOC, 1,037 tests)
- **Docs**: Fixed README post-quantum wording from "(planned)" to reflect actual implementation (`HybridKemContext` + `PqxdhRatchet`)
- **Docs**: Updated `MANUAL-TASKS-TODO.md` — marked transport drivers section as completed, updated issue #89 reference
- **Docs**: Updated ROADMAP — pkg installer, zypper, F-Droid changed from `[ ] Planned` to `[x]` with packaging files
- **Docs**: README CI badge replaced with static version badge (private repo badge returns 404)
- **Versions**: OBS `_service` revision updated from `v0.1.3` to `v0.1.4`
- **Versions**: Web UI footer version updated from `v0.1.0` to `v0.1.4`
- **Release workflow**: Fixed Homebrew formula test assertion (`--help` instead of `--version`)

## [0.1.3] — 2026-05-08

### Added
- **Transport**: I2P driver (`i2p.rs`) — SAM bridge protocol v3.1 (TCP 7656), stream connect/accept, destination validation, session management (15 tests)
- **Transport**: Veilid driver (`veilid.rs`) — JSON-RPC API transport via Veilid daemon, DHT route-based addressing, app_call protocol, route validation (15 tests)
- **Transport**: Reticulum Network Stack driver (`reticulum.rs`) — shared instance TCP, 2-byte framed protocol, hex destination hash validation, FNV-1a hashing (18 tests)
- **Transport**: BLE driver (`ble.rs`) — Nordic UART Service GATT proxy, MAC address validation, MTU-based fragmentation/reassembly (17 tests)
- **Transport**: Wi-Fi Direct driver (`wifi_direct.rs`) — wpa_supplicant ctrl, P2P device address validation, peer info parsing (12 tests)
- **Transport**: Audio modem driver (`audio_modem.rs`) — 2-FSK modulation, near-ultrasonic 18/19kHz, zero-crossing detection, CRC-16/CCITT framing (15 tests)
- **Transport**: QR-stream driver (`qr_stream.rs`) — QR frame sequencing, fragment/reassemble, ECC levels, bitrate estimation (15 tests)
- **Transport**: LoRa/Meshtastic driver (`lora.rs`) — Meshtastic serial/TCP protocol, magic-byte framing, node ID validation, spreading factor airtime (17 tests)
- **Transport**: AX.25 packet radio driver (`ax25.rs`) — KISS TNC framing, callsign/SSID parsing, UI frames (19 tests)
- **Transport**: HF radio/Winlink driver (`hf_radio.rs`) — VARA HF modem TCP interface, CONNECT/MYCALL commands, message framing (16 tests)
- **Transport**: Satellite link driver (`satellite.rs`) — Iridium SBD AT commands, IMEI validation, SBD checksum, orbital pass windows (17 tests)
- **Transport**: Mixnet driver (`mixnet.rs`) — Sphinx packet format, multi-hop routing, SURB anonymous replies, latency estimation (20 tests)
- **CI**: Created GitHub issue #92 tracking Actions 0-step workflow failures (suspected exhausted minutes)

### Fixed
- **CI**: Fixed branch protection MSRV check name from "MSRV (1.85)" to "MSRV (1.88)" to match actual Rust MSRV
- **Docs**: Updated LOC counts to match actual codebase (~50,000 total, rf-transport ~21,900)
- **Docs**: Updated test counts to match actual test suite (1,037 Rust tests, rf-transport 542, rf-mcp-client 15)
- **Docs**: Updated `docs/src/architecture/overview.md` with accurate per-crate stats
- **Docs**: Updated `MANUAL-TASKS-TODO.md` with detailed diagnosis of 0-step workflow failures

## [0.1.2] — 2026-05-08

### Added
- **Transport**: Tor hidden service driver (`tor.rs`) — full SOCKS5 CONNECT via local Tor proxy, .onion validation, 8 tests
- **Transport**: Yggdrasil overlay driver (`yggdrasil.rs`) — TCP over Yggdrasil IPv6 mesh (200::/7), listen support, 7 tests
- **Packaging**: Snap package manifest (`deploy/snap/snapcraft.yaml`) — daemon support, strict confinement, amd64+arm64
- **Packaging**: WiX MSI installer manifest (`deploy/wix/ravenfabric.wxs`) — Windows service, PATH, feature tree
- **Packaging**: NSIS EXE installer script (`deploy/nsis/ravenfabric.nsi`) — GUI installer, service install, Start Menu
- **CI**: Publish binaries to [egkristi/RavenFabric-Published](https://github.com/egkristi/RavenFabric-Published) on release (versioned directories + GitHub Release + SHA256SUMS)
- **CI**: Automated crates.io publish job in release workflow (dependency-ordered, skip-on-error)

### Fixed
- **CI**: Fixed `cargo fmt` check failure — expanded struct initializations in `named_pipe.rs` to multi-line format
- **Docker**: Fixed `latest` tag not being applied to container images (tag-triggered workflows don't match `is_default_branch`)
- **Packaging**: Updated `flake.nix` version from 0.1.0 to 0.1.2
- **Packaging**: Bumped version to 0.1.2 in all packaging manifests (AUR, Alpine, Chocolatey, Scoop, WinGet, Python SDK, TypeScript SDK)
- **Docs**: Updated install URLs from raw GitHub to `get.ravenfabric.io` across README, ROADMAP, install docs, and deploy script

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
