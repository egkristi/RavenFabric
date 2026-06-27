# RavenFabric Roadmap

> **Version:** 0.25.2 (Alpha) — Released 2026-06-27
> **Stats:** 14 crates, ~73,128 LOC, 1,423 tests, 0 clippy warnings, 0 known vulnerabilities
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

## Release Checklist: v0.25.2 — Audit Fixes

**Target:** Fix critical and medium issues found in the 2026-06-27 rpi5 audit.

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

- [ ] **Add `rf policy lint` command** — Warn about dangerous patterns, overly broad regex, missing deny rules
- [ ] **Add `rf audit verify` command** — Check HMAC chain continuity
- [ ] **Add policy hot-reload** — SIGHUP handler or inotify file watcher
- [ ] **Configure secret store** — Enable secret management on rpi5 deployment

---

## Release Checklist: v1.0.0-beta.1 — Beta Readiness

**Target:** "Ready for external testers with stable APIs and wire protocol."

### Beta Requirements

- [ ] **Soak test** — 2-4 weeks continuous deployment (26 days on rpi5 as of 2026-06-27 — **in progress**)
- [x] **Wire protocol stability guarantee**
- [x] **Code coverage metrics** (60% threshold)
- [x] **Security self-audit** (17 tests)
- [x] **API stability markers** (`#[non_exhaustive]`)
- [ ] **External testers** (2-3 people)
- [x] **SECURITY.md updated**
- [ ] **Publish to crates.io** (#44)

### Beta Blockers

- [x] All v0.25.2 critical items resolved
- [x] Relay mode verified working from macOS and Linux controllers
- [x] MCP server running and tested
- [x] Policy rules cover all CLI actions (exec, shell, forward, proxy, background, cp, secret)

### Why Not Beta Yet

- No production deployment
- No external users
- Single developer (no peer review on security-critical code)
- Exotic transports untested on real hardware
- No external security audit

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

**Issues found:** See [v0.25.2 release checklist](#release-checklist-v0252--audit-fixes).
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

| Metric | Target | Measured (rpi5, 2026-06-27) | Rationale |
|--------|--------|-------------------------------|-----------|
| Connection setup | < 2 RTT | ~50ms handshake (direct connect) | Noise XX = 1.5 RTT |
| Shell latency overhead | < 10ms | Not tested (policy denied) | Imperceptible vs raw TCP |
| `rf exec` simple command | < 100ms | ~50ms handshake + ~18ms exec | Faster than SSH |
| File transfer throughput | Line speed | Not benchmarked | ChaCha20 saturates >10 Gbps |
| Agent idle memory | < 10 MB | **33 MB RSS** (26-day steady state) | Raspberry Pi, IoT — target not met |
| Agent binary size | < 15 MB | Not measured | Static musl, stripped |
| Relay throughput | 10k concurrent sessions | Not benchmarked | Per-relay |
| Relay idle memory | < 5 MB | **1.8 MB RSS** (26-day steady state) | Minimal footprint |

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

### Real-World Audit Findings (2026-06-27)

See [v0.25.2 release checklist](#release-checklist-v0252--audit-fixes) for all items.
