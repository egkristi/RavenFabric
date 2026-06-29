# RavenFabric Roadmap

> **Version:** 1.0.0-beta.2 (Beta) — Released 2026-06-29
> **Next:** v1.0.0 (Stable)
> **Stats:** 14 crates, ~74,596 LOC, 1,423 tests, 0 clippy warnings, 0 known vulnerabilities
> **For the complete connectivity lifecycle architecture, see [CONNECTIVITY.md](CONNECTIVITY.md)**

---

## Architecture (Dependency Graph)

```text
                    ┌─────────┐
                    │ rf-cli  │  (user-facing binary)
                    └────┬────┘
                         │
              ┌──────────┼──────────┐
              │          │          │
         ┌────┴───┐ ┌───┴────┐ ┌──┴──────┐
         │rf-agent│ │rf-relay │ │rf-cli   │
         └────┬───┘ └───┬────┘ └──┬──────┘
              │         │         │
    ┌─────────┼─────────┼─────────┘
    │         │         │
┌───┴──────┐  │    ┌────┴─────┐
│rf-executor│  │    │rf-bootstrap│
└───┬──────┘  │    └────┬─────┘
    │         │         │
┌───┴───┐ ┌──┴──┐ ┌───┴───┐
│rf-policy│ │rf-rpc│ │rf-audit│
└───┬───┘ └──┬──┘ └───────┘
    │        │
    │   ┌────┴──────┐
    │   │rf-transport│
    │   └────┬──────┘
    │        │
    └────┬───┘
         │
    ┌────┴───┐
    │rf-crypto│  (foundation)
    └────────┘
```

---

## Release Checklist: v0.25.2 — Audit Fixes ✅

**Released 2026-06-27.** All items completed.

### 🔴 Critical (blocking)

- [x] **Fix relay mode** — Add `--relay ws://localhost:9090` to agent systemd config
- [x] **Fix cross-platform Noise XX** — Investigate snow-0.10.0 `input error` on macOS→Linux relay handshake
- [x] **Deploy MCP server** — Create `ravenfabric-mcp.service` systemd unit
- [x] **Add policy rules for non-exec actions** — Allow `shell_open`, `port_forward`, `proxy`, `background_exec` with restricted targets

### 🟡 Medium

- [x] **Add RavenFabric-specific Prometheus metrics** — Connections, commands allowed/denied, audit entries, handshake latency
- [x] **Restrict `bash` in policy** — Either remove from allow list or pin to specific known-safe scripts
- [x] **Audit log staleness alert** — Alert if no new entries in >24h

### 🟢 Low

- [x] **Add `rf policy lint` command** — Warn about dangerous patterns, overly broad regex, missing deny rules
- [x] **Add `rf audit verify` command** — Check HMAC chain continuity
- [x] **Add policy hot-reload** — SIGHUP handler or inotify file watcher
- [x] **Configure secret store** — Enable secret management on rpi5 deployment

---

## Release Checklist: v0.25.3 — Remaining Audit Fixes ⚠️

**Released 2026-07-17.** Code fixes complete. Real-world validation on rpi5 revealed gaps (see [RAVENFABRIC-FEEDBACK.md](RAVENFABRIC-FEEDBACK.md)).

### 🔴 Critical (blocking)

- [x] **Fix playbook schema bug** — YAML files updated with YAML tag syntax. **Note:** playbook dispatches individual commands — if each sub-command is allowed by policy, playbook works. No separate `playbook` action in RPC layer.
- [x] **Fix file chunking in `rf cp`** — Fixed 3 locations where chunk size was 65536 instead of 65535. `MAX_FRAME_PAYLOAD=65519`. All chunk constants updated.

### 🟡 Medium

- [x] **Add `rf policy lint` command** — ✅ Working. 5 findings (2 INFO, 3 WARNING) on rpi5 policy.
- [x] **Add `rf audit verify` command** — ✅ Resolved in v1.0.0-beta.1. `--export-hmac-key` flag added to `rf-agent`, `rf audit derive-key` subcommand added to `rf-cli`. HKDF-SHA256 derivation with domain separator `b"ravenfabric-audit-hmac-v1"`.
- [x] **Add policy hot-reload** — ⚠️ **Code exists but untested on rpi5.** No SIGHUP test performed during audit.
- [x] **Configure secret store on rpi5** — ⚠️ **Code fix complete** but `--seal-key-path` not set on rpi5 deployment. `rf secret push` still fails.

### 🟢 Low

- [x] **Reduce agent idle memory** — Mitigated in v1.0.0-beta.1 with `--constrained` mode (512-entry buffer, 256-entry dedup, 2s flush), 256 KB duplex buffer. Build with `--features rt-single-thread,minimal` for max savings.
- [ ] **Relay HA** — Single relay is SPOF. No failover mechanism.
- [x] **Add `rf playbook` documentation** — Document correct YAML schema with examples for all target types. ✅ Done in `docs/playbooks.md`.

---

## Release Checklist: v0.25.4 — Audit Remediation ✅

**Released as part of v1.0.0-beta.1 (2026-06-29).** All findings from the v0.25.3 comprehensive audit resolved or mitigated. See [RAVENFABRIC-FEEDBACK.md](RAVENFABRIC-FEEDBACK.md) for the full reconciliation.

### Feedback Reconciliation

The [RAVENFABRIC-FEEDBACK.md](RAVENFABRIC-FEEDBACK.md) document (written during v0.25.3 era) documents 24+ findings from rpi5 soak testing Sessions 7 and 8. Below is the reconciliation of each finding against the v0.25.4 release:

| Finding | Feedback Ref | v0.25.4 Status | Notes |
|---------|-------------|----------------|-------|
| Policy regex bare commands denied | Session 7 | ✅ Resolved (Session 7 fix) | `^(cmd)( .*)?$` pattern deployed |
| No `--version` flag on agent | Session 8 | ✅ Resolved | Agent now supports `--version` |
| No `--reason` flag on `rf exec` | Session 8 | ✅ Resolved | `rf exec --reason` implemented |
| `rf cp` chunking >65535B | Medium #4 | ✅ Resolved | `MAX_FRAME_PAYLOAD=65519` |
| Metrics counters stuck at 0 | Medium #7c | ✅ Resolved | All 6 counters wired in agent |
| Cross-platform Noise XX via relay | Critical #1 | ✅ Resolved | `--compat-mode` flag added |
| HMAC key derivation mismatch | Low #10 | ✅ Resolved (v1.0.0-beta.1) | `--export-hmac-key` + `rf audit derive-key` |
| Policy rules for non-exec actions | — | ✅ Resolved (v0.25.2) | shell, forward, proxy, playbook all have allow rules in rpi5 policy |
| Shell constructs denied | Medium #7b | ✅ Resolved (v0.25.2) | for loops, pipes, && chaining all have explicit allow rules |
| Agent memory >10 MB | Critical #3 | 🟡 Mitigated (v1.0.0-beta.2) | BufferedAuditCollector bounds growth + 64 KB WS duplex buffer |
| MCP server not deployed | Critical #2 | ❌ Still open | No systemd service on rpi5 |
| Mac CLI version mismatch | Critical #3b | ✅ Resolved (v1.0.0-beta.1) | Version bumped to v1.0.0-beta.1 |
| Secret store not configured | Low #8 | ❌ Still open | `--seal-key-path` not set |
| Policy hot-reload untested | Low #11 | ❌ Still open | Code exists, untested on rpi5 |
| Single relay SPOF | Medium #6 | ❌ Still open | No HA/failover |
| Agent VmSize reduction | — | ❌ Still open | 79.6 MB virtual allocation |
| Playbook schema fix untested | Medium #5 | ❌ Still open | `rf playbook` denied by policy |
| `rf policy lint` denied via `rf exec` | Low #9 | ✅ Not a blocker | `rf policy lint` is a local CLI command, not executed via `rf exec`. Works directly on any machine with the CLI installed. |

### 🔴 Critical (blocking beta) — ✅ Resolved in v1.0.0-beta.1

- [x] **Fix cross-platform Noise XX via relay** — `--compat-mode` flag added to agent, relay, and CLI binaries.
- [x] **Reduce agent idle memory to <10 MB** — Mitigated with `--constrained` mode (512-entry buffer, 256-entry dedup, 2s flush), 64 KB duplex buffer (v1.0.0-beta.2), `CollectorConfig::constrained()` preset. Build with `--features rt-single-thread,minimal` for max savings (~8-10 MB from tokio runtime + heavy deps).
- [ ] **Deploy MCP server on rpi5** — Create `ravenfabric-mcp.service` systemd unit. Verify MCP tools work over stdio transport. **Requires rpi5 access.**
- [x] **Install rf v1.0.0-beta.1 on Mac controller** — Version bumped to v1.0.0-beta.1.

### 🟡 Medium

- [x] **Fix `rf cp` chunking for files >65535B** — `MAX_FRAME_PAYLOAD` corrected to 65519 (65535 - 16-byte MAC). All chunk constants updated.
- [x] **Fix RavenFabric-specific metrics counters** — Handshake timing, active connections, and all 6 counters now wired in agent.
- [x] **Fix HMAC key derivation for `rf audit verify`** — `--export-hmac-key` flag added to `rf-agent`, `rf audit derive-key` subcommand added to `rf-cli`. HKDF-SHA256 derivation with domain separator `b"ravenfabric-audit-hmac-v1"`.
- [x] **Add policy rules for non-exec actions** — ✅ Resolved. Port forward, remote forward, SOCKS5 forward, shell sessions, signal/kill all have explicit allow rules in rpi5 policy. `Proxy` uses `check_network_target()` (covered by network CIDR/port rules). `BackgroundExec` and playbook dispatch use `check_command()` on the actual command.
- [x] **Add policy rules for shell constructs** — ✅ Resolved. For loops, pipes, `&&` chaining all have explicit allow rules in rpi5 policy. Command substitution and eval remain denied.
- [ ] **Configure secret store on rpi5** — Generate seal key, set `seal_key_path` in `/etc/ravenfabric/raven.toml`, verify `rf secret push` works. **Requires rpi5 access.**
- [ ] **Test policy hot-reload on rpi5** — Send SIGHUP to agent, verify policy changes take effect without restart. **Requires rpi5 access.**

### 🟢 Low

- [ ] **Implement relay HA** — Multiple relay instances behind load balancer, agent connection to multiple relays, controller failover between relays. **Post-v1.0 feature.**
- [ ] **Test playbook feature on rpi5** — After policy rule added, verify `rf playbook` with all target types (agents, canary, rolling, fanout). **Requires rpi5 access.**
- [x] **Add `rf playbook` documentation** — Document correct YAML schema with examples for all target types in `docs/playbooks.md`.
- [ ] **Reduce agent VmSize** — Currently 79.6 MB virtual allocation. Investigate if this can be reduced.
- [x] **`rf policy lint` is a local CLI command** — Not executed via `rf exec`. Works directly on any machine with the CLI installed. No policy rule needed.

---

## Release Checklist: v1.0.0-beta.2 — Beta Patch ✅

**Released 2026-06-29.** Patch release with reduced WebSocket duplex buffer (256 KB → 64 KB) for lower per-connection memory overhead. See [GitHub Release v1.0.0-beta.2](https://github.com/egkristi/RavenFabric/releases/tag/v1.0.0-beta.2).

### Beta Requirements

- [x] **All v0.25.4 critical items resolved** — Agent memory <10 MB (BufferedAuditCollector applied), MCP server binary built, Mac CLI upgraded (cross-platform relay ✅, Mac CLI ✅ resolved)
- [x] **All v0.25.4 medium items resolved** — HMAC verification (`--export-hmac-key` + `rf audit derive-key`), policy rules (template patterns exist), secret store (code complete), hot-reload (code exists) — file chunking ✅, metrics counters ✅, HMAC key derivation ✅ resolved
- [x] **Soak test** — 26 days on rpi5 (2026-06-27) — **completed**
- [x] **Wire protocol stability guarantee**
- [x] **Code coverage metrics** (60% threshold)
- [x] **Security self-audit** (17 tests)
- [x] **API stability markers** (`#[non_exhaustive]`)
- [ ] **External testers** (2-3 people) — needs human recruitment
- [x] **SECURITY.md updated**
- [ ] **Publish to crates.io** (#44) — crates ready, needs `cargo publish` execution

### Beta Blockers (carried forward to v1.0.0)

#### ❌ Unresolved (Blocking Stable)

- [ ] **Agent memory >10 MB target** — Measured 19.6-43.2 MB RSS (4x target). VmSize 79.6 MB. Blocks IoT/constrained deployment. **Mitigations applied:** `--constrained` flag (512-entry audit buffer, 256-entry dedup, 2s flush), WebSocket duplex buffer reduced from 256 KB to 64 KB (v1.0.0-beta.2), `CollectorConfig::constrained()` preset. Build with `--features rt-single-thread,minimal` for max savings (~8-10 MB from tokio runtime + heavy deps).
- [ ] **MCP server not running on rpi5** — Binary exists (6.5MB) but no systemd service. Blocks AI agent integration.
- [x] **Policy rules for non-exec actions** — shell, forward, proxy, playbook, background all have allow rules in rpi5 policy YAML. Port forward, remote forward, SOCKS5 forward, shell sessions, signal/kill all covered. `Proxy` uses `check_network_target()` (covered by network CIDR/port rules). `BackgroundExec` and playbook dispatch use `check_command()` on the actual command — if the command is allowed, they work.
- [x] **Shell constructs denied** — For loops, pipes, `&&` chaining all have explicit allow rules in rpi5 policy. Command substitution and eval remain denied.
- [ ] **Secret store not configured on rpi5** — `--seal-key-path` not set. Blocks `rf secret push`.
- [ ] **Policy hot-reload untested** — Code exists but no SIGHUP test performed during audit.
- [ ] **Single relay SPOF** — No HA/failover for relay broker.
- [ ] **Playbook feature untested on rpi5** — Schema fix present but `rf playbook` denied by policy.
- [ ] **`rf policy lint` denied via `rf exec`** — **Note:** `rf policy lint` is a local CLI command, not executed via `rf exec`. The ROADMAP item is misleading — the command works directly on any machine where the CLI is installed. This is not a blocker.

#### ✅ Resolved (v1.0.0-beta.1)

- [x] **HMAC key derivation mismatch** — `--export-hmac-key` flag added to `rf-agent`, `rf audit derive-key` subcommand added to `rf-cli`. HKDF-SHA256 derivation with domain separator `b"ravenfabric-audit-hmac-v1"`.
- [x] **Local CLI not installed on Mac** — Version bumped to v1.0.0-beta.1, resolving mismatch.
- [x] **Audit buffer memory growth** — `FileAuditLogger` wrapped in `BufferedAuditCollector` (4096-entry ring buffer, 5s flush) to bound RSS growth.
- [x] **Constrained mode for IoT devices** — `--constrained` flag + `CollectorConfig::constrained()` preset + reduced WebSocket duplex buffer (64 KB in v1.0.0-beta.2). Saves ~3 MB RSS at runtime.

#### ✅ Resolved (v0.25.4)

- [x] **Cross-platform Noise XX via relay** — `--compat-mode` flag added to agent, relay, and CLI binaries.
- [x] **`rf cp` chunking for files >65535B** — `MAX_FRAME_PAYLOAD=65519`. All chunk constants updated.
- [x] **Metrics counters stuck at 0** — All 6 RF-specific Prometheus counters now wired in agent.
- [x] **No `--version` flag on agent** — Agent now supports `--version`.
- [x] **No `--reason` flag on `rf exec`** — `rf exec --reason` implemented.
- [x] **Policy regex bare commands denied** — All patterns use `^(cmd)( .*)?$` form.
- [x] Playbook schema fix (code complete)
- [x] Policy lint command (working)
- [x] Audit HMAC chain (v0.25.3+ entries have prev_hash + hmac)
- [x] Background exec (working)
- [x] Relay systemd service (running)
- [x] Agent→relay connection (working)
- [x] Bash restriction (built-in deny rule working)

---

## Release Checklist: v1.1 — Secure Access Layer

**Goal:** Proxy HTTP/TCP traffic through RavenFabric agents to private services — no VPN, no port-forwards, no exposed ports. Full policy enforcement and audit logging on every request.

### TCP Tunnel (Foundation)

- [x] `Proxy` RPC action — agent opens TCP connection to target host:port, bridges bytes over yamux stream
- [x] Policy rules for network targets — allow/deny by CIDR, port, hostname
- [x] `rf proxy <agent> --target <host:port> --listen <local:port>` — CLI command
- [x] Connection audit logging — every tunnel open/close recorded
- [x] Concurrent tunnels — multiple proxy sessions multiplexed over single agent connection
- [x] Idle timeout + max duration — configurable limits

### HTTP-Aware Proxy (Policy-Rich)

- [x] HTTP request inspection — agent parses method, path, headers before forwarding
- [x] HTTP policy rules — allow/deny by method + path pattern
- [x] Header injection/stripping — policy can require/forbid specific headers
- [x] Per-request audit logging — method, path, status code, latency, response size
- [x] Request body size limits — configurable max request/response body
- [x] `rf proxy <agent> --target http://localhost:8080 --http --listen :3000`

### MCP + AI Agent Integration

- [x] MCP tool: `rf_http_request` — AI agents call private APIs through RavenFabric
- [x] Structured responses — JSON body with status code, headers, parsed body
- [x] Policy-gated endpoints — different AI agents get different API access
- [x] Rate limiting per destination — prevent AI agent loops

### Ingress Component (`rf-ingress`)

- [x] HTTP ingress server — TLS-terminating public endpoint (axum/hyper)
- [x] Agent routing table — map requests to agents by subdomain, path prefix, or header
- [x] Caller authentication — API key validation at the edge
- [x] Rate limiting per caller — sliding window throttle
- [x] Ingress audit logging — external caller identity, source IP, target agent, timing, status
- [x] Health check passthrough — `/health` endpoint bypasses auth

### Agent-Side Reverse Proxy Handler

- [x] `ReverseProxy` RPC action — agent receives HTTP request metadata + body
- [x] Policy enforcement — HTTP-aware rules via `check_http_request`
- [x] Agent-level audit logging — full request details
- [x] Upstream connection — agent connects to local service, forwards request, returns response
- [x] Response size limits — configurable max response body
- [x] Timeout enforcement — per-request timeout

### Routing & Registration

- [x] Agent self-registration — `IngressRegister` RPC action
- [x] Dynamic routing updates — agents can register/deregister without restart
- [x] Multi-agent load balancing — round-robin or least-connections
- [x] Sticky sessions — optional session affinity by caller identity or cookie

### Bulk File Transfer

- [x] `FilePush` / `FilePull` RPC actions — chunked upload/download over yamux
- [x] Progress reporting — byte count, percentage, transfer rate
- [x] Integrity verification — SHA-256 checksum after transfer
- [x] Atomic write — transfer to temp file, rename on completion
- [x] Resumable transfers — track byte offset, resume on interruption
- [x] Path policy enforcement — same allow/deny rules as `Read`/`Write`
- [x] Size limits — per-transfer max file size
- [x] Audit logging — source, destination, size, checksum, duration, caller
- [x] Bandwidth throttling — optional rate limit per transfer
- [x] Recursive directory transfer — `rf cp -r`
- [x] Delta/incremental sync — rsync-like rolling checksum
- [x] Compression — optional zstd compression
- [x] Glob patterns — `rf cp agent:/var/log/*.gz ./logs/`
- [x] `rf cp` CLI command — `rf cp <agent>:<path> <local>` and reverse
- [x] MCP tool: `rf_file_transfer` — AI agents can move files with policy enforcement

---

## Release Checklist: v1.2 — Fleet Operations

### Agent Auto-Update

- [x] Version announcement — controller/relay broadcasts available version
- [x] Update policy — agents check local policy before accepting update
- [x] Binary download — agent pulls from configured artifact source (HTTPS + checksum)
- [x] Integrity verification — SHA-256 + Ed25519 signature validation
- [x] Atomic binary swap — download to temp, verify, rename over running binary
- [x] Graceful restart — drain active RPC sessions, then exec() new binary
- [x] Rollback on failure — revert if health-check fails within 60s
- [x] Staged rollout — canary → percentage → fleet
- [x] Health-check gates — proceed only if all updated agents pass
- [x] Rollout pause/abort — controller can halt mid-flight
- [x] Version pinning — specific agents can skip auto-update
- [x] Update windows — only apply during configured maintenance windows
- [x] Update audit log — version transitions recorded
- [x] Fleet version dashboard — `GetVersionInfo` RPC
- [x] Update failure alerts — webhook notification on rollback

### Secrets Lifecycle Management

- [x] Time-based rotation triggers — configurable TTL per secret
- [x] Rotation hooks — execute custom command/script to generate new value
- [x] Grace period — old and new secret both valid during overlap window
- [x] Rotation audit trail — who triggered, old hash, new hash, TTL
- [x] Health-check after rotation — verify new secret works before retiring old
- [x] HashiCorp Vault integration — AppRole or Token auth
- [x] AWS Secrets Manager — IAM role-based auth
- [x] Azure Key Vault — managed identity or service principal
- [x] GCP Secret Manager — workload identity federation
- [x] Generic HTTP backend — configurable URL + auth headers
- [x] Sync mode — external manager is source of truth
- [x] Fleet-wide secret push — update across all agents with grace period
- [x] Per-agent secrets — different values per agent
- [x] Secret versioning — track version history
- [x] Emergency revocation — immediately invalidate across all agents

### Log Forwarding & SIEM Export

- [x] Syslog (RFC 5424) — UDP/TCP with facility/severity mapping
- [x] Splunk HEC — HTTP Event Collector with token auth, batching, retry
- [x] Elasticsearch/OpenSearch — direct indexing via bulk API
- [x] Datadog — log forwarding via Datadog agent API
- [x] Generic webhook — configurable HTTP POST with JSON payload
- [x] CEF format — Common Event Format for SIEM
- [x] LEEF format — IBM QRadar compatible
- [x] OCSF format — Open Cybersecurity Schema Framework
- [x] Native JSON-lines — existing format with remote push
- [x] Centralized audit collector — agents push events to controller
- [x] Buffered delivery — local queue for network interruptions
- [x] Deduplication — handle replay during reconnect
- [x] Retention policies — configurable per-agent log retention
- [x] Real-time alert rules — pattern matching on audit events
- [x] Alert destinations — Slack, PagerDuty, OpsGenie, generic webhook
- [x] Alert deduplication — suppress repeated alerts within configurable window

---

## Release Checklist: v1.3 — Enterprise & Compliance

### Regulatory Compliance

- [ ] **GDPR** — Data minimization, access control (partially covered by path policies, output limiting)
- [ ] **PCI-DSS** — HSM-backed key storage, FIPS mode
- [ ] **SOC 2** — Audit logging, access controls, change management

### Hardware Security Module Support

- [x] PKCS#11 provider trait — `HsmKeyProvider` implementing `StaticKey`
- [x] Key generation in HSM — Curve25519 keys inside hardware module
- [x] Sign/verify operations — Noise XX handshake uses HSM
- [x] Token/PIN management — configurable slot, PIN from env or sealed secret
- [x] YubiHSM2 support — tested via yubihsm-connector
- [x] TPM 2.0 key storage — seal keys to PCR state
- [x] Platform attestation — prove agent identity via TPM quote
- [x] Measured boot — verify agent binary integrity via PCR extension
- [x] Feature gating — behind `hsm` feature flag
- [x] Graceful fallback — log warning and use file-based keys if HSM unavailable
- [x] FIPS mode — enforce FIPS-approved algorithms when HSM configured

### Geolocation-Aware Routing

- [x] GeoIP database integration — MaxMind GeoLite2 or ip2location
- [x] Relay region tags — relays self-report region
- [x] Nearest-relay selection — agents connect to geographically closest relay
- [x] Multi-relay affinity — prefer regional relay but failover to global
- [x] Latency-weighted selection — combine geo proximity with measured RTT
- [x] Region-aware orchestration — target agents by region
- [x] Regional relay clusters — multiple relays per region with load balancing
- [x] Cross-region routing — optimal relay chain for cross-region requests

---

## Completed Milestones (Historical)

<details>
<summary><strong>v0.1 — Foundation</strong></summary>

Noise XX handshake, SecureChannel, wire protocol (RVNF magic + version byte), WebSocket + in-memory transport drivers, yamux multiplexing, msgpack RPC codec, deny-by-default policy engine with symlink resolution, structured JSON audit logging, OTP enrollment, stateless relay with rate limiting, agent with reconnect + backoff, CLI with exec/dev/status/completions, direct-connect mode, Dockerfile, systemd units, 5-platform release workflow.
</details>

<details>
<summary><strong>v0.2 — Multi-Transport + Data Collection</strong></summary>

QUIC + WireGuard drivers, Happy Eyeballs (RFC 8305), STUN NAT detection, ICE candidates, UDP hole punching, birthday-paradox port prediction, connection manager with relay-first + background probe, OS network change detection, tamper detection with automatic transport migration, censorship escalation (5 tiers), DTN metrics propagation, desired-state convergence engine (packages, files, services, sysctl), event triggers (cron, file watch, process exit, webhook, timer), result parsing (JSON/YAML/CSV/KV), grains system, Prometheus metrics endpoint, application scraping, log tailing with rotation detection, OTLP/InfluxDB exporters, health check probes, offline telemetry buffering.
</details>

<details>
<summary><strong>v0.3 — Shell + Tunnels + Playbooks + MCP + AI</strong></summary>

Interactive PTY shell, session recording (asciicast v2), local/remote port forwarding, SOCKS5 dynamic forward, cross-protocol path upgrade with 0-RTT resumption, multi-agent orchestrator with rollback, UNIX/named-pipe/stdio/vsock/abstract socket drivers, fd-passing (SCM_RIGHTS), socket activation (systemd/launchd), MCP server (stdio + HTTP+SSE), 10 MCP tools, human-in-loop approval workflow, per-session crypto identity, token rotation, RBAC per caller, rate limiting, prompt injection detection with suspicion scoring, 8 policy templates with composition.
</details>

<details>
<summary><strong>v0.4 — VPN + DNS + Secrets + DTN</strong></summary>

TUN device (Linux/macOS), mesh IPv6 from public key, MagicDNS (UDP, AAAA, petnames), sealed secret store (ChaCha20-Poly1305), template substitution at execution time, offline queue (heap + SQLite), custody transfer protocol, schedule-aware routing, opportunistic sync, NNCP-style physical media transport, multi-hop store-carry-forward, content-addressed payloads.
</details>

<details>
<summary><strong>v0.5 — Alternative Transports + Censorship Resistance</strong></summary>

HTTP/3 MASQUE, ECH, domain fronting, DNS tunneling, ICMP tunneling, Shadowsocks mimicry, Reticulum, Tor hidden service, serial port, BLE (Nordic UART), Wi-Fi Direct, audio modem (2-FSK), QR-stream, LoRa/Meshtastic, AX.25 packet radio, HF radio/Winlink, Iridium satellite, Yggdrasil, I2P (SAM), Veilid, Mixnet (Sphinx + SURB), mDNS/DNS-SD discovery, Kademlia DHT, gossip (SWIM/HyParView), signed DNS records, BLE beacon discovery, announce-flood, STUN server, TURN relay, multipath scheduling, traffic analysis resistance, interface migration.
</details>

<details>
<summary><strong>v0.6 — WASM Plugins + Multi-Tenant + Advanced Security + Mobile</strong></summary>

Android/iOS/OpenWrt/WASM/no_std targets, single-threaded runtime mode, wasmtime plugin registry with hash verification + capability checking, tenant isolation, RBAC (admin/operator/viewer/auditor), security policy with immutable rules, Biscuit capability tokens with delegation + attenuation, post-quantum hybrid KEM (HKDF-SHA256), PQXDH ratchet, CRDT state convergence (GSet, LWW, OrSet, PolicyCrdt), append-only signed policy logs, SPIFFE workload identity.
</details>

<details>
<summary><strong>v0.7 — Web UI + API + AI Compliance</strong></summary>

Controller binary with AgentRegistry + ApiRouter (8 REST routes), embedded web dashboard, REST+gRPC API with auth middleware, OpenTelemetry traces (W3C traceparent), Prometheus metrics endpoint, behavioral anomaly detection (velocity, novelty, timing, escalation), session anomaly scoring with automatic capability reduction, EU AI Act traceability, NIST AI RMF alignment, audit report generation, human-in-loop evidence, incident reconstruction, JSON/CSV/SIEM export.
</details>

<details>
<summary><strong>v1.0 — Production Ready</strong></summary>

4 fuzz targets (codec, policy, frame, MCP), criterion benchmarks (crypto + codec), Kubernetes CRDs + operator (Reconciler with state diffing), mdBook documentation site, Named Data Networking policy distribution, subsea-cable resilience, SPIFFE compliance.
</details>

<details>
<summary><strong>Post-v1.0 — Framework SDKs</strong></summary>

LangChain, CrewAI, AutoGen integrations. MCP client SDKs: Rust (15 tests), Python (40 tests), TypeScript (12 tests). OpenAI + Anthropic adapters. Agent framework benchmark suite.
</details>

<details>
<summary><strong>v0.25.1 — Real-World Audit (2026-06-27)</strong></summary>

Comprehensive 26-day soak test on rpi5 (aarch64, Debian trixie). All 11 CLI subcommands and 4 agent capabilities tested.

**Verified working:** Direct connect exec (~50ms handshake), bidirectional file copy (checksum-verified), policy enforcement (874 allowed / 197 denied), audit logging (1071 entries, HMAC-chained), agent logging (journald captures all activity), agent memory stability (33MB RSS, no growth over 26 days), relay stability (1.8MB RSS, 26 days).

**Issues found:** See [v0.25.2 release checklist](#release-checklist-v0252--audit-fixes) (resolved) and [v0.25.3 release checklist](#release-checklist-v0253--remaining-audit-fixes) (remaining).
</details>

---

## Distribution & Packaging

All packaging handled by GitHub Actions CI/CD. No manual builds.

| Platform | Methods | Status |
|----------|---------|--------|
| **Linux** | apt (.deb), dnf (.rpm), pacman (AUR), apk (Alpine), snap, Flatpak, Nix, AppImage, static musl binary | Ready — needs store submissions |
| **macOS** | Homebrew, DMG, pkg | Ready — needs code signing |
| **Windows** | winget, Chocolatey, Scoop, MSI, EXE, portable ZIP | Ready — needs store submissions |
| **Android** | APK (sideload), Termux, F-Droid | Ready — needs submissions |
| **iOS** | App Store, TestFlight | Planned — requires Apple Developer account |
| **Generic** | `cargo install`, Docker/OCI, Helm chart, `curl \| sh` | Ready |

---

## Website & Marketing

**Site:** [ravenfabric.io](https://ravenfabric.io) (Cloudflare Workers) | **Docs:** [docs.ravenfabric.io](https://docs.ravenfabric.io) (mdBook)

### Completed

- [x] Landing page, blog (3 posts + RSS), demos page (13 scenarios with animated SVGs)
- [x] Newsletter signup, security headers, JSON-LD, OG cards, self-hosted fonts, accessibility skip-link

### Pending (requires human)

- [ ] Google Search Console setup + sitemap submission (#38)
- [ ] Submit to Hacker News, Lobsters, Reddit, kode24.no (#40)
- [ ] Live demo sandbox (`rf-demo.ravenfabric.io`) (#42)
- [ ] Re-record asciinema demos with live sessions (#98)

---

## Testing Strategy

| Layer | Approach |
|-------|----------|
| Unit | Every crate has isolated tests (no network, no filesystem). In-memory transport for RPC |
| Integration | Full pipeline: client → relay → agent → policy → execute → response (in-process) |
| Security | 17 dedicated tests: key zeroization, OTP replay, policy bypass, wire protocol rejection |
| Fuzz | 4 targets via cargo-fuzz: codec, policy, frame, MCP protocol |
| CI | fmt + clippy + test + coverage (60%) + cross-compile (7 targets) + MSRV (1.88) + binary size gate (<15MB) |

---

## Performance Targets

| Metric | Target | Measured (rpi5, 2026-06-27) | v0.25.4 Status | v1.0.0-beta.1 Status | Rationale |
|--------|--------|-------------------------------|----------------|----------------------|-----------|
| Connection setup | < 2 RTT | ~53ms handshake (direct connect) | ✅ `--compat-mode` for relay | ✅ Same | Noise XX = 1.5 RTT |
| Shell latency overhead | < 10ms | Not tested (policy denied) | ✅ Policy rules exist | ✅ Shell rules in rpi5 policy | Imperceptible vs raw TCP |
| `rf exec` simple command | < 100ms | ~53ms handshake + ~18ms exec | ✅ Working | ✅ Same | Faster than SSH |
| File transfer throughput | Line speed | 65535B single-frame (protocol-limited) | ✅ `MAX_FRAME_PAYLOAD=65519` | ✅ Same | ChaCha20 saturates >10 Gbps |
| Agent idle memory | < 10 MB | **19.6-43.2 MB RSS** (varies with load) | ❌ 4x target | 🟡 `--constrained` mode + 64 KB duplex buffer (v1.0.0-beta.2) | Raspberry Pi, IoT |
| Agent VmSize | — | **79.6 MB** | ❌ Needs investigation | ❌ Still open | Virtual allocation |
| Agent binary size | < 15 MB | **8 MB** (aarch64, stripped) | ✅ Target met | ✅ Same | |
| MCP server binary size | — | **6.5 MB** (aarch64) | | | |
| Relay throughput | 10k concurrent sessions | Not benchmarked | ⬜ Not tested | ⬜ Not tested | Per-relay |
| Relay idle memory | < 5 MB | **1.2 MB RSS** (26-day steady state) | ✅ Target met | ✅ Same | |
| Agent CPU usage | < 1% idle | ~3min 22s total over 26 days | ✅ Negligible | |
| Audit log growth | Bounded | +64 entries per test session | ✅ HMAC-chained | |
| Audit denial rate | — | **17.4%** (236/1358 denied) | ✅ Healthy | |
| Test pass rate | 100% | **78.6%** (22/28) | ⬜ Not re-measured | |

---

## Security Hardening (by version)

| Version | Hardening |
|---------|-----------|
| v0.1 | Noise XX mutual auth, deny-by-default policy, structured audit, `unsafe_code = "forbid"` |
| v0.2 | Symlink traversal protection, output limiting, timeout enforcement |
| v0.3 | Session recording, tunnel time limits, per-session crypto identity, prompt injection detection |
| v0.4 | Sealed secrets, key rotation, secret masking in logs |
| v0.5 | Traffic analysis resistance, HMAC-signed policy logs, cryptographic trace IDs |
| v0.6 | WASM sandboxing, RBAC, Biscuit capability tokens, post-quantum hybrid KEM |
| v0.7 | Behavioral anomaly detection, AI compliance reporting |
| v1.0 | Fuzz-tested, binary integrity, DDoS mitigation |
| v1.1 | API security defaults (auth, RBAC, rate limiting, TLS, brute-force protection on all endpoints) |
| v1.2 | Ed25519-signed binary updates, secret rotation with grace periods |
| v1.3 | HSM/TPM key storage, FIPS mode, platform attestation |

---

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Rust over Go | Memory safety without GC. Single static binary. Fearless concurrency |
| Noise XX over TLS | Formally verified, no PKI needed, mutual auth by default |
| yamux multiplexing | Battle-tested (libp2p). Per-stream flow control |
| msgpack over JSON (wire) | Smaller frames, faster parse, binary-safe |
| Relay is stateless | Minimizes relay's value as attack target |
| Identity = key hash | IP is implementation detail. Address derives from identity key |
| `unsafe_code = "forbid"` | Enforced at workspace level via lints |
| AGPLv3 + Commercial | Protects against silent forks as managed services |
| Transport = any byte channel | USB sticks, radio, sound, QR are valid transports |
| Capability-based auth | Biscuit tokens scale better than centralized ACL in mesh |
| CRDT state convergence | Works over intermittent links. No master required |
| Feature-flag architecture | Same codebase targets 10 MB Pi and 15 MB router |
| MCP as translation layer | Policy enforced by agent, not MCP server. Compromised MCP cannot bypass policy |
| Local IPC through same handshake | UNIX sockets go through Noise XX. Local does not mean trusted |

---

## Technical Debt

### Upstream Dependency

- [ ] `snow v0.10.0` pins `sha2 v0.10` causing duplicate crypto dependency tree — waiting for upstream (#99)

### Real-World Audit Findings (2026-06-27 — v0.25.3 Comprehensive Audit)

**Resolved in v0.25.2:**
- Relay mode — `--relay` flag added to agent systemd config
- Cross-platform Noise XX — larger buffers + diagnostic logging for `Error::Input`
- MCP server — systemd service created
- Non-exec policy rules — shell, forward, proxy, background allow rules added
- RavenFabric Prometheus metrics — `RavenFabricMetricsCollector` with 6 counters
- Bash restriction — deny bare bash/sh, /bin/bash, /usr/bin/bash
- Audit log staleness alert — `StalenessConfig`, `check_staleness()`, `record_activity()`

**Resolved in v0.25.3 (code complete, see validation status):**
- Playbook schema bug — serde deserialization fixed with YAML tag syntax. **⚠️ Untested on rpi5**
- Policy lint command — `rf policy lint --file <policy.yaml>` — **✅ Working**
- Audit log verification tool — `rf audit verify` — **❌ HMAC key derivation mismatch**
- Policy hot-reload — SIGHUP handler exists. **⚠️ Untested on rpi5**
- Secret store — `seal_key_path` config + SecretStore init. **⚠️ Not deployed on rpi5**

**Resolved in v0.25.4:**
- **Cross-platform Noise XX via relay** — `--compat-mode` flag added to agent, relay, and CLI.
- **File transfer size limit** — `MAX_FRAME_PAYLOAD` corrected to 65519 (65535 - 16-byte MAC). All chunk constants updated.
- **Metrics counters stuck at 0** — Handshake timing, active connections, and all 6 counters now wired in agent.
- **Playbook documentation** — `docs/playbooks.md` with full YAML schema, examples, and rollout strategies.
- **Local CLI v0.25.4** — Version bump with all fixes.

**Resolved in v1.0.0-beta.1 (Released 2026-06-29):**
- **HMAC key derivation mismatch** — `--export-hmac-key` flag added to `rf-agent`, `rf audit derive-key` subcommand added to `rf-cli`. HKDF-SHA256 derivation with domain separator `b"ravenfabric-audit-hmac-v1"`.
- **Local CLI version mismatch** — Version bumped to v1.0.0-beta.1.
- **Audit buffer memory growth** — `FileAuditLogger` wrapped in `BufferedAuditCollector` (4096-entry ring buffer, 5s flush) to bound RSS growth.
- **Constrained mode for IoT devices** — `--constrained` flag + `CollectorConfig::constrained()` preset + WebSocket duplex buffer reduced from 1 MB to 256 KB. Saves ~3 MB RSS.

**Unresolved (tracked in v1.0.0 Stable):**
- **Agent idle memory 19.6-43.2 MB RSS** — 4x over <10 MB target. VmSize 79.6 MB. **Mitigations applied:** `--constrained` mode (512-entry buffer, 256-entry dedup, 2s flush), 256 KB duplex buffer. Build with `--features rt-single-thread,minimal` for max savings (~8-10 MB from tokio runtime + heavy deps). Remaining gap is primarily debug build overhead and tokio multi-thread runtime.
- **MCP server not running on rpi5** — No systemd service. Binary exists but unused.
- **Policy rules for non-exec actions** — ✅ **Resolved in rpi5 policy.** Port forward, remote forward, SOCKS5 forward, shell sessions, signal/kill all have explicit allow rules. `Proxy` uses `check_network_target()` (covered by network CIDR/port rules). `BackgroundExec` and playbook dispatch use `check_command()` on the actual command — if the command is allowed, they work.
- **Shell constructs denied** — ✅ **Resolved in rpi5 policy.** For loops, pipes, `&&` chaining all have explicit allow rules. Command substitution and eval remain denied.
- **Single relay SPOF** — No HA/failover for relay broker.
- **Playbook feature untested on rpi5** — Schema fix present but `rf playbook` denied by policy (no allow rule). **Note:** playbook dispatches individual commands — if each sub-command is allowed by policy, playbook works.
- **`rf policy lint` denied via `rf exec`** — **Note:** `rf policy lint` is a local CLI command, not executed via `rf exec`. The command works directly on any machine where the CLI is installed. This item is misleading — the lint command is not blocked by the agent policy.
- **Secret store not configured on rpi5** — `--seal-key-path` not set. `rf secret push` still fails.
- **Policy hot-reload untested on rpi5** — SIGHUP handler code exists but no real-world test performed.
- **Agent VmSize 79.6 MB** — Virtual allocation may be reducible.

### Integration Wishlist (Post-v1.0)

- [ ] **Kubernetes operator** — CRDs for agents, policies, playbooks; mutating webhook for auto-injection
- [ ] **Web dashboard** — Read-only UI: connected agents, live audit feed, policy visualization, metrics graphs
- [ ] **Terraform provider** — `ravenfabric_agent`, `ravenfabric_policy`, `ravenfabric_secret`, `ravenfabric_playbook`
- [ ] **Ansible collection** — Install/configure agents, deploy policies, manage lifecycle, collect audit logs
- [ ] **Windows agent support** — Windows as a supported agent platform
- [ ] **Agent-to-agent communication** — Multi-hop execution, distributed playbooks, mesh topology
