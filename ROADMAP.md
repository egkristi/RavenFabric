# RavenFabric Roadmap

> For the complete connectivity lifecycle architecture, see [CONNECTIVITY.md](CONNECTIVITY.md).

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

## Current Status

**Version:** 0.25.0 (Alpha) — Released 2026-05-24

**Stats:** 14 crates, ~72,767 LOC, 1,432 tests, 0 clippy warnings, 0 known vulnerabilities.

**What works today:**

- Full E2E encrypted remote execution (`rf exec agent "cmd"`)
- Interactive shell with session recording (`rf shell agent`)
- Port forwarding: local (-L), remote (-R), SOCKS5 (-D)
- Multi-agent orchestration with playbooks and rollback
- Mesh VPN with MagicDNS and sealed secrets
- Fleet-wide secret push (`rf secret push`) with zero-downtime rotation via grace period
- Secret enumeration (`rf secret list`) — names only, plaintext never returned
- External secret backends: HashiCorp Vault, AWS Secrets Manager, Azure Key Vault, GCP Secret Manager, Generic HTTP
- 46+ transport drivers (WebSocket, QUIC, WireGuard, LoRa, BLE, Tor, satellite, etc.)
- Delay-tolerant networking (store-carry-forward, custody transfer)
- MCP server for AI agent integration (Claude, Cursor, Aider)
- Behavioral anomaly detection and AI compliance reporting
- Desired-state convergence with drift detection
- Web UI dashboard with REST API
- WASM plugin system, RBAC, multi-tenancy
- Post-quantum hybrid KEM (ML-KEM + X25519)

---

## Completed Milestones

<details>
<summary><strong>v0.1 — Foundation</strong></summary>

Proved the architecture. Noise XX handshake, SecureChannel, wire protocol (RVNF magic + version byte), WebSocket + in-memory transport drivers, yamux multiplexing, msgpack RPC codec, deny-by-default policy engine with symlink resolution, structured JSON audit logging, OTP enrollment, stateless relay with rate limiting, agent with reconnect + backoff, CLI with exec/dev/status/completions, direct-connect mode, Dockerfile, systemd units, 5-platform release workflow.
</details>

<details>
<summary><strong>v0.2 — Multi-Transport + Data Collection</strong></summary>

QUIC + WireGuard drivers, Happy Eyeballs (RFC 8305), STUN NAT detection, ICE candidates, UDP hole punching, birthday-paradox port prediction, connection manager with relay-first + background probe, OS network change detection, tamper detection with automatic transport migration, censorship escalation (5 tiers), DTN metrics propagation, desired-state convergence engine (packages, files, services, sysctl), event triggers (cron, file watch, process exit, webhook, timer), result parsing (JSON/YAML/CSV/KV), grains system, Prometheus metrics endpoint, application scraping, log tailing with rotation detection, OTLP/InfluxDB exporters, health check probes, offline telemetry buffering.
</details>

<details>
<summary><strong>v0.3 — Shell + Tunnels + Playbooks + MCP + AI (Released 2026-05-14)</strong></summary>

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

---

## Path to Beta

**Current:** Alpha v0.25.0 — all planned features implemented and tested.

**Beta means:** "Ready for external testers with stable APIs and wire protocol." It is a stability promise, not a feature milestone.

| # | Requirement | Status |
|---|-------------|--------|
| 1 | **Soak test** — continuous deployment 2-4 weeks | Not started |
| 2 | **Wire protocol stability guarantee** | Done |
| 3 | **Code coverage metrics** (60% threshold) | Done |
| 4 | **Security self-audit** (17 tests) | Done |
| 5 | **API stability markers** (`#[non_exhaustive]`) | Done |
| 6 | **External testers** (2-3 people) | Not started |
| 7 | **SECURITY.md updated** | Done |
| 8 | **Publish to crates.io** | Not started (#44) |

**Why not beta yet:** No production deployment. No external users. Single developer (no peer review on security-critical code). Exotic transports untested on real hardware. No external security audit.

**Target:** v1.0.0-beta.1 — after 4-6 weeks of real-world bake time with prerequisites complete.

---

## ✓ Implemented: v1.1 — Secure Access Layer

### Secure API Proxy

**Goal:** Proxy HTTP/TCP traffic through RavenFabric agents to private services — no VPN, no port-forwards, no exposed ports. Full policy enforcement and audit logging on every request.

Replaces Tailscale Funnel, Cloudflare Tunnel, and SSH port-forwarding with a single, policy-controlled, audited mechanism.

**Security-first defaults (non-negotiable):**

- All API access requires authentication by default — no anonymous access, no opt-in auth
- API tokens mandatory — cryptographically random, minimum 256-bit entropy, constant-time comparison
- RBAC enforced on every request — caller identity mapped to role, role mapped to permitted operations
- Deny-by-default — if no explicit allow rule matches, the request is rejected
- Rate limiting enabled by default — per-caller sliding window, configurable but never disabled
- TLS required — plaintext HTTP rejected; minimum TLS 1.2, prefer 1.3
- Request validation — reject malformed requests, validate Content-Type, enforce size limits
- No sensitive data in URLs — tokens and secrets must be in headers, never query parameters
- Response sanitization — strip internal headers, stack traces, and debug info in production mode
- CORS restricted — no wildcard origins; explicit allowlist only
- Security headers on every response — `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Cache-Control: no-store` for authenticated responses
- Token expiration — all tokens have configurable TTL, no permanent tokens without explicit policy override
- Token rotation — grace period support for zero-downtime rotation
- Audit every authentication event — success, failure, token used, source IP, timestamp
- Brute-force protection — exponential backoff after repeated auth failures per source IP

#### TCP Tunnel (Foundation)

- [x] `Proxy` RPC action — agent opens TCP connection to target host:port, bridges bytes over yamux stream
- [x] Policy rules for network targets — allow/deny by CIDR, port, hostname (mirrors filesystem policy structure)
- [x] `rf proxy <agent> --target <host:port> --listen <local:port>` — CLI command opens local listener, tunnels to agent
- [x] Connection audit logging — every tunnel open/close recorded with caller identity, target, bytes transferred, duration
- [x] Concurrent tunnels — multiple proxy sessions multiplexed over single agent connection via yamux
- [x] Idle timeout + max duration — configurable limits prevent resource exhaustion from abandoned tunnels

#### HTTP-Aware Proxy (Policy-Rich)

- [x] HTTP request inspection — agent parses method, path, headers before forwarding to upstream
- [x] HTTP policy rules — allow/deny by method + path pattern (e.g., allow `GET /api/**`, deny `DELETE /**`)
- [x] Header injection/stripping — policy can require/forbid specific headers (auth tokens, X-Forwarded-For)
- [x] Per-request audit logging — method, path, status code, latency, response size logged with caller identity
- [x] Request body size limits — configurable max request/response body (prevent exfiltration of large datasets)
- [x] `rf proxy <agent> --target http://localhost:8080 --http --listen :3000` — HTTP-aware mode

#### MCP + AI Agent Integration

- [x] MCP tool: `rf_http_request` — AI agents call private APIs through RavenFabric with full policy enforcement
- [x] Structured responses — JSON body returned to AI agent with status code, headers, parsed body
- [x] Policy-gated endpoints — different AI agents get different API access based on their RBAC profile
- [x] Rate limiting per destination — prevent AI agent loops from overwhelming upstream services

---

### Secure Reverse Proxy (Ingress)

**Goal:** Expose private services through RavenFabric without opening ports, VPNs, or third-party tunnels. External callers hit a public ingress endpoint; requests route over existing agent connections to upstream services.

Replaces: ngrok, Cloudflare Tunnel, Tailscale Funnel — with deny-by-default policy, per-request audit, and zero third-party dependency.

**Security-first defaults (non-negotiable):**

- Authentication required on every request — no unauthenticated access to any proxied service
- Multiple auth methods supported — API tokens (default), mTLS client certificates, OAuth2/OIDC bearer tokens
- API tokens: cryptographically random (256-bit), constant-time validation, automatic expiration
- RBAC per caller — each authenticated identity maps to a role with explicit allowed endpoints/methods
- Deny-by-default — ingress rejects any request that doesn't match an explicit allow rule
- Double authentication — ingress authenticates the external caller AND the agent re-validates against its own policy
- Rate limiting per caller and per endpoint — prevents abuse, DDoS, and credential stuffing
- TLS 1.3 required on public ingress — no TLS 1.0/1.1, no weak cipher suites
- Input validation — reject oversized headers, malformed requests, invalid Content-Type before routing
- Request body limits — configurable max size (default 10MB), enforced before forwarding to agent
- No sensitive data in URLs or logs — tokens redacted in audit entries, no query-string auth
- IP allowlisting — optional but available; restrict access to known CIDR ranges
- Automatic token revocation — compromised tokens can be revoked instantly across all ingress instances
- Security headers enforced — HSTS, X-Content-Type-Options, X-Frame-Options, CSP on all responses
- Brute-force protection — lockout after N failed auth attempts per source IP (configurable, default 5/minute)
- Audit every request — caller identity, source IP, target agent, method, path, response status, latency
- No CORS wildcards — explicit origin allowlist per endpoint, credentials never exposed cross-origin
- Token scoping — tokens can be scoped to specific agents, endpoints, or methods (principle of least privilege)
- Short-lived tokens preferred — default TTL 24h for API tokens; long-lived tokens require explicit policy approval

#### Ingress Component (`rf-ingress`)

- [x] HTTP ingress server — TLS-terminating public endpoint (axum/hyper), accepts inbound HTTPS requests
- [x] Agent routing table — map incoming requests to connected agents by subdomain, path prefix, or header (`X-RF-Agent`)
- [x] Caller authentication — API key validation at the edge before routing
- [x] Rate limiting per caller — sliding window throttle at ingress to prevent abuse
- [x] Ingress audit logging — external caller identity, source IP, target agent, timing, response status (structured)
- [x] Health check passthrough — `/health` endpoint bypasses auth (for load balancers)

#### Agent-Side Reverse Proxy Handler

- [x] `ReverseProxy` RPC action — agent receives HTTP request metadata + body over RPC channel
- [x] Policy enforcement — HTTP-aware rules (method + path pattern allow/deny via `check_http_request`)
- [x] Agent-level audit logging — full request details (method, path, caller, policy decision, latency)
- [x] Upstream connection — agent connects to local service, forwards request, returns response
- [x] Response size limits — configurable max response body to prevent data exfiltration
- [x] Timeout enforcement — per-request timeout kills slow upstream connections

#### Routing & Registration

- [x] Agent self-registration — `IngressRegister` RPC action registers agent with upstream URL, subdomain, path prefix
- [x] Dynamic routing updates — agents can register/deregister without ingress restart
- [x] Multi-agent load balancing — multiple agents serving same endpoint, round-robin or least-connections
- [x] Sticky sessions — optional session affinity by caller identity or cookie

---

### Bulk File Transfer

**Goal:** Native streaming file transfer replacing scp, rsync, and Ansible's `copy` module. Efficient over the existing encrypted channel with full policy enforcement, progress reporting, and audit logging.

#### Core Transfer Engine

- [x] `FilePush` RPC action — chunked upload from client to agent (configurable chunk size, default 256KB)
- [x] `FilePull` RPC action — chunked download from agent to client
- [x] Streaming over yamux — chunks flow over a dedicated mux stream, no base64 encoding overhead
- [x] Progress reporting — byte count, percentage, transfer rate reported back to caller
- [x] Integrity verification — SHA-256 checksum of entire file verified after transfer completes
- [x] Atomic write — transfer to temp file, rename on completion (no partial files on failure)
- [x] Resumable transfers — track byte offset, resume interrupted transfers without restarting

#### Policy & Security

- [x] Path policy enforcement — same allow/deny rules as existing `Read`/`Write` actions
- [x] Size limits — per-transfer max file size configurable in policy (prevent disk exhaustion)
- [x] Audit logging — source path, destination path, file size, checksum, duration, caller identity
- [x] Bandwidth throttling — optional rate limit per transfer (prevent saturating network)

#### Advanced Features

- [x] Recursive directory transfer — `rf cp -r` with directory tree traversal
- [x] Delta/incremental sync — rsync-like rolling checksum for efficient updates of large files
- [x] Compression — optional zstd compression for transfer (transparent, negotiated)
- [x] Glob patterns — `rf cp agent:/var/log/*.gz ./logs/` wildcard expansion
- [x] `rf cp` CLI command — familiar syntax: `rf cp <agent>:<path> <local>` and `rf cp <local> <agent>:<path>`
- [x] MCP tool: `rf_file_transfer` — AI agents can move files with policy enforcement

---

## ✓ Implemented: v1.2 — Fleet Operations

### Agent Auto-Update

**Goal:** Agents update themselves without manual intervention. Staged rollout with health-check gates, automatic rollback on failure, and full audit trail.

#### Update Mechanism

- [x] Version announcement — controller/relay broadcasts available version to connected agents
- [x] Update policy — agents check local policy before accepting update (allow/deny version ranges)
- [x] Binary download — agent pulls new binary from configured artifact source (HTTPS + checksum)
- [x] Integrity verification — SHA-256 + Ed25519 signature validation before applying
- [x] Atomic binary swap — download to temp, verify, rename over running binary
- [x] Graceful restart — drain active RPC sessions, then exec() new binary (zero-downtime on Linux)
- [x] Rollback on failure — if new binary fails health-check within 60s, revert to previous version

#### Fleet Coordination

- [x] Staged rollout — canary (1 agent) → percentage (10%) → fleet (100%)
- [x] Health-check gates — proceed to next stage only if all updated agents pass health check
- [x] Rollout pause/abort — controller can halt rollout mid-flight
- [x] Version pinning — specific agents can be pinned to a version (skip auto-update)
- [x] Update windows — only apply updates during configured maintenance windows

#### Audit & Observability

- [x] Update audit log — version transitions recorded with timestamp, source, verification status
- [x] Fleet version dashboard — `GetVersionInfo` RPC returns current version, pinned version, and update window per agent
- [x] Update failure alerts — webhook notification on rollback events

---

### Secrets Lifecycle Management

**Goal:** Automated secret rotation with grace periods, external secret manager integration, and full audit trail. Zero-downtime rotation.

#### Automated Rotation

- [x] Time-based rotation triggers — configurable TTL per secret (e.g., rotate every 30 days)
- [x] Rotation hooks — execute custom command/script to generate new secret value
- [x] Grace period — old and new secret both valid during configurable overlap window
- [x] Rotation audit trail — every rotation event logged (who triggered, old hash, new hash, TTL)
- [x] Health-check after rotation — verify new secret works before retiring old

#### External Secret Manager Integration

- [x] HashiCorp Vault — read/write via Vault HTTP API (AppRole or Token auth)
- [x] AWS Secrets Manager — fetch/rotate via AWS SDK (IAM role-based auth)
- [x] Azure Key Vault — managed identity or service principal authentication
- [x] GCP Secret Manager — workload identity federation
- [x] Generic HTTP backend — configurable URL + auth headers for custom backends
- [x] Sync mode — external manager is source of truth; agent pulls on schedule

#### Secret Distribution

- [x] Fleet-wide secret push — update across all agents with grace period
- [x] Per-agent secrets — different values for same secret name per agent
- [x] Secret versioning — track version history, audit access patterns
- [x] Emergency revocation — immediately invalidate across all agents

---

### Log Forwarding & SIEM Export

**Goal:** Push audit logs and telemetry to external systems in real-time. Enterprise SOC integration.

#### Remote Log Sinks

- [x] Syslog (RFC 5424) — UDP/TCP with facility/severity mapping
- [x] Splunk HEC — HTTP Event Collector with token auth, batching, retry
- [x] Elasticsearch/OpenSearch — direct indexing via bulk API
- [x] Datadog — log forwarding via Datadog agent API
- [x] Generic webhook — configurable HTTP POST with JSON payload

#### Audit Log Formats

- [x] CEF (Common Event Format) — standard SIEM format
- [x] LEEF (Log Event Extended Format) — IBM QRadar compatible
- [x] OCSF (Open Cybersecurity Schema Framework) — modern security event schema
- [x] Native JSON-lines — existing format, now with remote push

#### Fleet Aggregation

- [x] Centralized audit collector — agents push events to controller in real-time
- [x] Buffered delivery — local queue for network interruptions, guaranteed delivery
- [x] Deduplication — handle replay during reconnect without duplicate events
- [x] Retention policies — configurable per-agent log retention before forwarding

#### Alerting

- [x] Real-time alert rules — pattern matching on audit events (policy denial → alert)
- [x] Alert destinations — Slack, PagerDuty, OpsGenie, generic webhook
- [x] Alert deduplication — suppress repeated alerts within configurable window

---

## ✓ Implemented: v1.3 — Enterprise & Compliance

### Regulatory Compliance Coverage

The table below maps known regulatory requirements to existing RavenFabric capabilities.

| Regulation | Requirements | Covered by | Status |
|---|---|---|---|
| GDPR / personopplysningsloven | Data minimization, tilgangskontroll | Path policies, output limiting | Delvis dekket |

---

### Hardware Security Module Support

**Goal:** Hardware-backed key storage for FIPS 140-2, PCI-DSS, and government environments.

#### PKCS#11 Integration

- [x] PKCS#11 provider trait — `HsmKeyProvider` implementing `StaticKey` interface
- [x] Key generation in HSM — generate Curve25519 keys inside hardware module
- [x] Sign/verify operations — Noise XX handshake uses HSM for private key operations
- [x] Token/PIN management — configurable slot, PIN from env or sealed secret
- [x] YubiHSM2 support — tested with YubiHSM2 via yubihsm-connector

#### TPM Integration

- [x] TPM 2.0 key storage — seal keys to PCR state (Linux tpm2-tss, Windows TBS)
- [x] Platform attestation — prove agent identity via TPM quote
- [x] Measured boot — verify agent binary integrity via PCR extension

#### Feature Gating

- [x] Behind `hsm` feature flag — no compile-time or runtime cost when unused
- [x] Graceful fallback — if HSM unavailable, log warning and use file-based keys
- [x] FIPS mode — when HSM is configured, enforce FIPS-approved algorithms only

---

### Geolocation-Aware Routing

**Goal:** Lowest-latency relay selection based on geographic proximity for global deployments.

#### Geo-Routing

- [x] GeoIP database integration — MaxMind GeoLite2 or ip2location for relay location mapping
- [x] Relay region tags — relays self-report region (us-east, eu-west, ap-south, etc.)
- [x] Nearest-relay selection — agents connect to geographically closest relay on startup
- [x] Multi-relay affinity — prefer regional relay but failover to global
- [x] Latency-weighted selection — combine geo proximity with measured RTT for optimal path

#### Global Fleet

- [x] Region-aware orchestration — target agents by region (e.g., "all eu-west agents")
- [x] Regional relay clusters — multiple relays per region with load balancing
- [x] Cross-region routing — requests to agents in other regions route via optimal relay chain

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

**Completed:** Landing page, blog (3 posts + RSS), demos page (13 scenarios with animated SVGs), newsletter signup, security headers, JSON-LD, OG cards, self-hosted fonts, accessibility skip-link.

**Pending (requires human):**

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

| Metric | Target | Rationale |
|--------|--------|-----------|
| Connection setup | < 2 RTT | Noise XX = 1.5 RTT |
| Shell latency overhead | < 10ms | Imperceptible vs raw TCP |
| `rf exec` simple command | < 100ms | Faster than SSH |
| File transfer throughput | Line speed | ChaCha20 saturates >10 Gbps |
| Agent idle memory | < 10 MB | Raspberry Pi, IoT |
| Agent binary size | < 15 MB | Static musl, stripped |
| Relay throughput | 10k concurrent sessions | Per-relay |

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

All critical and important issues resolved. One upstream dependency issue remains:

- [ ] `snow v0.10.0` pins `sha2 v0.10` causing duplicate crypto dependency tree — waiting for upstream (#99)
