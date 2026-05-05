# RavenFabric Roadmap

> For the complete connectivity lifecycle architecture, see [CONNECTIVITY.md](CONNECTIVITY.md).

## Implementation Order (Dependency Graph)

The crates must be built bottom-up. Each layer depends only on layers below it.

```
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
    │rf-crypto│  (foundation — build FIRST)
    └────────┘
```

### Build Sequence (v0.1)

| Phase | Crate | Depends on | Deliverable |
|-------|-------|-----------|-------------|
| 1 | `rf-crypto` | nothing | Noise XX handshake, SecureChannel, key management |
| 2 | `rf-transport` | `rf-crypto` | Driver trait, WebSocket driver |
| 3 | `rf-rpc` | `rf-transport` | yamux mux, msgpack codec, Request/Response types |
| 4 | `rf-audit` | nothing | Structured JSON audit logger |
| 5 | `rf-policy` | `rf-audit` | RPCPolicy loading + enforcement |
| 6 | `rf-executor` | `rf-policy`, `rf-rpc`, `rf-audit` | Command execution under policy |
| 7 | `rf-bootstrap` | `rf-crypto` | OTP enrollment flow |
| 8 | `rf-relay` | `rf-transport` | Stateless broker binary |
| 9 | `rf-agent` | `rf-executor`, `rf-transport`, `rf-bootstrap` | Agent binary |
| 10 | `rf-cli` | `rf-rpc`, `rf-crypto`, `rf-transport` | CLI binary |

**Phase 1–3 are the critical path.** Once crypto + transport + RPC work, everything else builds on top.

### First Working Demo (target: end of Phase 10)

```bash
# Terminal 1: Start relay
rf-relay --listen 127.0.0.1:8443 --dev

# Terminal 2: Start agent
rf-agent --relay ws://127.0.0.1:8443 --id test-agent --dev

# Terminal 3: Execute command
rf exec test-agent "uname -a"
# → Linux myhost 6.8.0 #1 SMP x86_64 GNU/Linux
```

---

## v0.1 — Foundation

**Goal:** Prove the architecture. One transport. Full E2E encryption. Functional RPC. Linux.

**Success criteria:** `rf exec my-agent "uname -a"` returns output, E2E encrypted, policy-checked, audited.

### Workspace Setup
- [x] Cargo workspace (`Cargo.toml` with all 10 crate members)
- [x] CI: GitHub Actions (build, test, clippy, fmt, coverage, MSRV)
- [x] Cross-platform release workflow (Linux, macOS, ARM64)
- [x] CodeQL security scanning
- [x] Dependabot for dependency updates
- [x] Pre-commit hook (fmt + clippy + test)
- [x] In-memory transport driver for testing (no network required)
- [x] Integration test harness: spawn relay + agent + client in-process

### Crypto Layer (Phase 1) — DONE
- [x] `rf-crypto/src/noise.rs` — Noise XX handshake + wire protocol (RVNF magic + version)
- [x] `rf-crypto/src/keys.rs` — StaticKey (load/save/generate, 0600 permissions, zeroed on drop)
- [x] `rf-crypto/src/channel.rs` — SecureChannel (send/recv, 64KB frames, concurrent via split Mutex)
- [x] `rf-crypto/src/error.rs` — CryptoError enum (typed errors)
- [x] Wire protocol version byte in handshake
- [x] **Fix:** `handshake()` uses `StatelessTransportState` for split-free concurrent SecureChannel
- [x] **Fix:** Wire magic (`RVNF`) and version byte sent and validated during handshake
- [x] **Fix:** Replace `unwrap()` with `expect()` with justification in `StaticKey::generate()` and noise pattern parsing
- [x] **Fix:** `yamux` dependency removed from `rf-crypto`

### Transport Layer (Phase 2) — DONE
- [x] `rf-transport/src/driver.rs` — Driver trait + AsyncStream + Target + DriverConfig + Listener trait
- [x] `rf-transport/src/error.rs` — TransportError enum
- [x] **Fix:** `#[derive(Debug, Clone)]` added to `Target` type
- [x] `rf-transport/src/websocket.rs` — WebSocket transport driver (bridge pattern)
- [x] `rf-transport/src/memory.rs` — In-memory driver (for tests, with unit tests)

### RPC Layer (Phase 3) — DONE
- [x] `rf-rpc/src/types.rs` — Request/Response/Action/RpcResult types (incl. streaming variants)
- [x] `rf-rpc/src/mux.rs` — RPC session over SecureChannel (request/response semantics)
- [x] `rf-rpc/src/yamux_mux.rs` — Yamux multiplexing for concurrent RPC (MuxClient/MuxServer)
- [x] `rf-rpc/src/codec.rs` — msgpack frame codec (length-delimited, roundtrip tested)
- [x] `rf-rpc/src/error.rs` — RpcError enum (typed errors)

### Audit (Phase 4) — DONE (with known debt)
- [x] `rf-audit/src/types.rs` — AuditEntry struct (timestamp, action, decision, duration, caller)
- [x] `rf-audit/src/logger.rs` — AuditLogger trait + FileAuditLogger (JSON-lines append)
- [x] **Fix:** Return `Result` from `log()` instead of silently swallowing write errors
- [x] **Fix:** Add `Deserialize` derive to `AuditEntry` for log reading
- [x] Add unit tests for logger (write, rotation, error handling)

### Policy Layer (Phase 5) — DONE
- [x] `rf-policy/src/rpc_policy.rs` — RPCPolicy enforcement (allow/deny regex, path rules, symlink resolution)
- [x] `rf-policy/src/decision.rs` — Decision type (allowed/denied + reason + rule)
- [x] **Fix:** Replace `Box<dyn Error>` in `load()`/`from_yaml()` with typed `PolicyError`
- [x] Hot-reload via SIGHUP (atomic policy swap)

### Executor (Phase 6) — DONE
- [x] `rf-executor/src/command.rs` — Policy-checked execution with timeout + output limiting
- [x] Metrics action handler (sysinfo)
- [x] **Done:** Unit tests (policy denial, successful exec, timeout, output limiting, env, metrics)
- [x] Streaming stdout/stderr via mux stream (`rf-executor/src/streaming.rs`)

### Bootstrap (Phase 7) — DONE
- [x] `rf-bootstrap/src/otp.rs` — OTP generation, validation, single-use, TTL-enforced, hash-stored
- [x] **Fix:** `RwLock::write()` uses `unwrap_or_else(|p| p.into_inner())` — handles poisoning gracefully
- [x] Agent enrollment flow (token → key exchange → registered) (`rf-bootstrap/src/enrollment.rs`)

### Relay (Phase 8) — DONE
- [x] `rf-relay/src/main.rs` — Full relay broker binary
- [x] WebSocket listener + meet-token pairing
- [x] Channel-based agent/client pairing (bidirectional forwarding)
- [x] Per-IP rate limiting
- [x] HMAC token auth (meet tokens)

### Agent Binary (Phase 9) — DONE
- [x] `rf-agent/src/main.rs` — Full agent binary
- [x] Connect to relay, perform Noise handshake, run RPC loop
- [x] Policy-checked executor integration
- [x] Config loading (raven.toml)
- [x] Reconnect loop with exponential backoff + jitter
- [x] Graceful shutdown (drain in-flight, flush audit)

### CLI Binary (Phase 10) — DONE
- [x] `rf-cli/src/main.rs` — clap CLI with exec/dev/status subcommands
- [x] `rf exec` — connect, handshake, send Request, display Response
- [x] `rf dev` — local relay + agent in one process (no auth)
- [x] `rf status` — show connected agents
- [x] Shell completions (bash, zsh, fish)

### Packaging
- [x] Dockerfile (multi-stage alpine build → scratch runtime)
- [x] Release workflow (5 platform targets)
- [x] Linux amd64 + arm64 static binaries (musl)
- [x] systemd service units (agent + relay)

### Workspace Cleanup (from audit)
- [x] Remove unused workspace deps (`proptest`, `base64`, `crc32fast`)
- [x] Move `yamux` dep from `rf-crypto` to `rf-rpc` (removed from crypto; yamux available for future use)
- [x] Align CI clippy settings with workspace lint config
- [x] Fix release workflow `|| true` on binary copy

---

## Website (ravenfabric.io)

The site is live at [ravenfabric.io](https://ravenfabric.io). Below are prioritized improvements.

### Validation & SEO Setup
- [ ] Test Open Graph cards (LinkedIn Post Inspector, X Card Validator, Facebook Debugger, opengraph.xyz)
- [ ] Set up Google Search Console (DNS TXT verification via Namecheap)
- [ ] Submit sitemap: `https://ravenfabric.io/sitemap.xml`
- [ ] Run Lighthouse audit: `npx lighthouse https://ravenfabric.io --view`
- [ ] Run broken link check: `npx broken-link-checker https://ravenfabric.io --recursive --ordered`

### Critical Quick-Wins (this week)
- [x] Fix broken links — create stubs for `SECURITY.md`, `LICENSING.md`, `CONTRIBUTING.md`, `CHANGELOG.md`
- [x] Threat model section (what relay cannot see, agent compromise scope, controller compromise scope, immutable rules)
- [x] About / "Built by" section (name + LinkedIn, gives credibility)
- [x] JSON-LD structured data (`SoftwareApplication` schema)
- [x] Add `og:image:alt`, `og:image:width`, `og:image:height` meta tags

### Content Improvements (high value, low effort)
- [x] ~~"Why" section between hero and "What it is" (one binary, one policy, one trust root)~~
- [x] ~~Live GitHub stats (shields.io badges for stars, last commit)~~
- [x] ~~Concrete comparison table (RavenFabric vs Tailscale vs Ansible — E2E encryption, policy, air-gap)~~
- [x] ~~Architecture diagram as SVG (from README's 6-layer ASCII diagram)~~
- [x] ~~"Why now" section (2026 context: cable sabotage, NIS2, ZTNA mandates, monoculture risk)~~
- [x] ~~Use-case personas (public sector architects, MSPs, remote-first, edge/IoT)~~
- [x] ~~FAQ section (vs Tailscale+Ansible, why Rust, why AGPLv3, production-ready?, who)~~
- [x] ~~Terminal example tab-style rotation (reduce visual noise on desktop)~~ — CSS-only tabs at 1100px+

### Technical Improvements
- [x] Skip-link for accessibility (`<a href="#main" class="skip-link">Skip to content</a>`)
- [x] Declare `color-scheme: dark` (prevent flash-of-white)
- [x] Preload critical fonts (self-hosted in `website/assets/fonts/`, no Google Fonts dependency)
- [x] Content-Security-Policy meta tag (strict, no external font CDN)
- [x] Twitter card meta tags (`twitter:creator`, `twitter:site`)
- [x] ~~Mobile-responsive tables (stack layout on `<600px`)~~
- [ ] OG image in WebP/AVIF format (reduce 117KB PNG)

### When v0.1 Ships
- [ ] Asciinema cast of `rf exec` demo (30s, embedded player)
- [ ] Submit to Hacker News (`Show HN`), Lobsters, r/rust, r/selfhosted, r/sysadmin, kode24.no
- [ ] First blog post ("Why Noise XX over TLS" or "Why air-gap support is first-class")
- [x] ~~Status badge in header (build status, version, last release date)~~ — shields.io badges

### Medium-Term
- [ ] Documentation sub-site (`docs.ravenfabric.io` via mdBook)
- [ ] `/blog/` section with RSS feed (`/feed.xml`)
- [ ] Newsletter signup (Buttondown, not Mailchimp)
- [ ] Live demo sandbox (`rf-demo.ravenfabric.io`)

### Explicitly Not Planned
- No cookie banner (no cookies, no analytics)
- No animated hero backgrounds (CPU waste, AI-slop aesthetic)
- No "Get Started" CTA before product works (use "View on GitHub")
- No live chat widget (signals sales, not engineering)
- No pricing page before commercial features exist

---

## v0.2 — Multi-Transport + Data Collection

**Goal:** Transport diversity. Task mode. File operations. Data collection agent. Windows + macOS.

### Transport Expansion
- [x] ~~WebSocket driver implementation (tokio-tungstenite)~~
- [x] ~~In-memory driver for testing~~
- [x] ~~QUIC driver (quinn, 0-RTT, connection migration, multiplexed streams)~~
- [ ] WireGuard userspace (boringtun, direct peers on open network)
- [x] Happy Eyeballs (RFC 8305) — race IPv4/IPv6, use first responder
- [x] IPv6-first with NAT64/464XLAT awareness

### Network Environment Probing (Phase 4 of Connectivity Value Chain)
- [x] ~~NetworkProbe struct — unified assessment of network environment~~ — rf-transport::probe
- [x] ~~EgressClass classification (Open, HomeRouter, EnterpriseProxy, RestrictiveDPI, Hostile, AirGap)~~
- [x] STUN-based NAT type detection (full cone, restricted, port-restricted, symmetric)
- [x] ~~IPv4/IPv6 availability and preference detection~~ — NetworkProbe checks both
- [x] ~~UDP reachability (per-port), captive portal detection~~ — UDP check in probe
- [ ] Corporate proxy detection (HTTP CONNECT support)
- [ ] Per-relay latency measurement (geographic selection)

### Path Selection Engine (Phase 5 of Connectivity Value Chain)
- [x] ~~Transport catalog with tier classification (direct, NAT-traversal, relay, overlay, hostile, out-of-band)~~
- [x] ~~Path selection strategies: sequential, race, parallel, tiered-race, policy-driven~~
- [x] ~~Driver probing (which drivers can work in current network environment)~~ — catalog.record_probe + mark_unavailable
- [x] ~~Policy-driven path selection (sensitive commands require specific transports)~~ — select_with_policy()

### NAT Traversal (ICE-style)
- [x] STUN client — discover server-reflexive candidates (public IP:port)
- [x] UDP hole punching — coordinated simultaneous send via relay coordinator
- [x] TCP hole punching — simultaneous open (RFC 5128)
- [x] ICE candidate gathering — host, server-reflexive, relayed candidates
- [x] ICE candidate selection — parallel probing, select fastest path
- [ ] Birthday paradox port prediction for symmetric NAT
- [x] NAT type detection (full cone, restricted, port-restricted, symmetric)

### Connection Upgrade (DCUtR Pattern)
- [x] ~~Relay-first connection (immediate, always works)~~ — ConnectionManager::connect_relay_first()
- [x] ~~Background direct-path probing while relay is active~~ — background_probe_targets()
- [x] ~~Seamless migration to direct path when found (verify peer key)~~ — migrate_to_direct()
- [x] ~~Automatic failback to relay if direct path fails~~ — failback_to_relay()
- [x] ~~Connection migration on network change (WiFi ↔ cellular)~~ — network_changed() + reprobing

### Health Monitoring & Failover (Phase 11 of Connectivity Value Chain)
- [x] ~~Heartbeat-based liveness detection (miss 3 = failed)~~ — Ping/Pong RPC action
- [x] ~~RTT baseline tracking (> 2x baseline = degraded)~~ — RttTracker with EWMA + anomaly detection
- [x] ~~Automatic failover (promote secondary path or start race + relay bridge)~~ — ConnectionManager fallover logic
- [x] ~~OS network change events (route table, default gateway) → re-probe all drivers~~ — network_changed()
- [x] ~~Sticky/adaptive/hybrid path selection modes~~ — PathStrategy enum (Sequential, Race, Parallel, TieredRace, PolicyDriven)

### Tamper Detection & Adaptive Transport
- [x] ~~MAC failure detection (Noise ciphertext tampered) → immediate path abandon~~ — TamperDetected error + audit event
- [x] ~~Frame injection detection (unexpected bytes outside protocol framing)~~ — FrameInjection error for <16 byte frames
- [x] ~~Latency anomaly detection (sudden spikes consistent with MITM)~~ — HeartbeatStatus::LatencyAnomaly
- [x] Protocol fingerprint verification (detect DPI/downgrade)
- [x] Automatic session migration to alternative transport on tamper detection
- [x] ~~Compromised path blacklisting (no retry without operator acknowledgment)~~ — catalog.blacklist/unblacklist
- [x] Escalation to censorship-resistant transport tier when all standard paths fail
- [x] ~~Tamper-alert audit events (signed, timestamped, priority-delivered)~~ — security audit entries on tamper/injection

### Connection Metrics & Monitoring (DTN-aware)
- [x] ~~Per-path metrics collection (RTT, loss, throughput, transport type, hop count)~~ — rf-rpc::metrics module
- [x] Metrics propagation through DTN store-carry-forward (bundled with custody transfer)
- [x] Priority delivery for security events (tamper alerts never dropped by TTL)
- [x] ~~Offline metric accumulation (local buffer, flush on next contact window)~~ — PathMetrics VecDeque buffer
- [x] Mesh neighbor health gossip (partial observability without direct controller path)
- [x] ~~Path switch event logging (transport changes recorded as audit + metric)~~ — audit entries for tamper-driven path abandonment
- [x] Relay-reported metrics (hop count, forwarding latency, queue depth)

### Graceful Teardown (Phase 12 of Connectivity Value Chain)
- [x] ~~Drain in-flight requests before disconnect (with timeout)~~ — agent graceful shutdown
- [x] ~~Flush audit log to durable storage before close~~ — agent graceful shutdown
- [x] ~~Noise close-notify + yamux stream close~~ — close-notify via empty encrypted frame
- [x] ~~Session key zeroization on disconnect~~ — SecureChannel drops keys via zeroize
- [x] ~~Cache last-known-good endpoint for fast reconnect~~
- [x] ~~Reconnect strategies: exponential backoff + jitter, network-aware, scheduled~~ — agent reconnect loop

### Execution Modes
- [x] ~~Task mode (ordered steps, conditions, onFailure, workdir)~~ — partial: background exec with job ID, query, wait
- [x] ~~Background exec with ID tracking + signal + wait~~
- [x] Real-time stdout/stderr streaming via mux stream (StreamExecute action + StreamChunk/StreamEnd)

### File Operations
- [x] ~~Push file (orchestrator → agent)~~ — Write action with base64 data + atomic temp+rename
- [x] ~~Pull file (agent → orchestrator)~~ — Read action returns base64-encoded content
- [x] ~~Atomic writes (temp + rename)~~ — implemented in Write handler

### Cross-Platform (Tier 1)
- [x] ~~Windows binary + Windows Service installer~~ — release.yml + deploy/install-windows-service.ps1
- [x] ~~macOS binary + launchd plist~~ — release.yml + deploy/io.ravenfabric.agent.plist
- [x] ~~Linux static musl binaries (amd64 + arm64) with systemd units~~ — release.yml + deploy/*.service
- [x] ~~Feature flags: `full` (default, all transports) vs `minimal` (no TUN, no sysinfo, no QUIC)~~
- [x] ~~`#[cfg()]` for all OS-specific code (no Unix-only paths without alternatives)~~ — all cfg(unix) has cfg(not(unix)) counterpart

### Data Collection Agent
- [x] Metrics collector framework (plugin trait, scrape loop, push/pull modes)
- [x] Built-in system metrics (CPU, memory, disk, network, load, filesystems, processes)
- [x] Prometheus-compatible `/metrics` endpoint (pull mode)
- [ ] Application metrics scraping (scrape localhost Prometheus endpoints)
- [x] Log tailing (glob patterns, journald, structured parsing: JSON/logfmt/regex/grok)
- [x] OTLP exporter (metrics + logs + traces to any OTLP-compatible backend)
- [x] Prometheus remote-write exporter
- [x] InfluxDB line protocol exporter
- [x] Health check probes (HTTP/TCP/UDP endpoints, process alive, cert expiry)
- [x] Collection policy (what to collect governed by same deny-by-default policy)
- [x] Offline telemetry buffering (queue metrics/logs while disconnected, flush on reconnect)

---

## v0.3 — Shell + Tunnels + Playbooks

**Goal:** Interactive shell. Port forwarding. Multi-agent orchestration. Cross-protocol path upgrade.

### Interactive Shell
- [x] PTY allocation + terminal session handler
- [x] Session recording (asciinema v2 format)
- [x] `rf shell <agent>` — interactive terminal through fabric

### Port Forwarding
- [x] Local port forward (ssh -L equivalent)
- [x] Remote port forward (ssh -R equivalent)
- [x] SOCKS5 dynamic forward (ssh -D equivalent)

### Cross-Protocol Path Upgrade (Phase 10 of Connectivity Value Chain)
- [x] Background transport upgrade (relay → direct, any driver → any driver)
- [x] Session ticket resumption (re-handshake on new transport, same session ID)
- [x] Atomic swap (make-before-break, overlap window, then close old path)
- [x] 0-RTT resumption for known peers

### Playbook Engine
- [x] Multi-agent orchestration (rolling/canary/parallel strategies)
- [x] Rollback on failure
- [x] Grain-based targeting

---

## v0.4 — VPN + DNS + Secrets + Delay-Tolerant Delivery

**Goal:** Full mesh VPN. MagicDNS. Secrets injection. DTN store-carry-forward.

### Mesh VPN
- [ ] TUN device creation (cross-platform)
- [x] Mesh IP allocation (key-derived addresses)
- [x] MagicDNS (agent-name.rf.local)
- [x] Petname system (local names → cryptographic identifiers)

### Secrets
- [x] Sealed secret store (encrypted at rest)
- [x] `{{ secrets.KEY }}` resolution at execution time

### Delay-Tolerant Networking
- [x] SQLite-backed persistent offline queue (survives restart)
- [x] Custody transfer protocol (each hop acknowledges responsibility)
- [x] Schedule-aware routing (contact windows, satellite passes)
- [x] Opportunistic sync (exchange queued messages when agents meet)
- [ ] NNCP-style physical media transport (USB, SD card, file-based delivery)
- [x] TTL, priority, and idempotency for queued commands
- [x] Multi-hop store-carry-forward (intermediate nodes relay when path opens)
- [x] Content-addressed command payloads (deduplication across paths)

---

## v0.5 — Alternative Transports + Censorship Resistance

**Goal:** Air-gap support. Anonymity. Hostile network traversal. Peer discovery. Radio mesh.

### Censorship-Resistant Transports
- [ ] HTTP/3 MASQUE driver (CONNECT-UDP/CONNECT-IP via HTTP/3, impossible to distinguish from browsing)
- [x] Traffic obfuscation layer (make Noise XX indistinguishable from random bytes, obfs4-inspired)
- [ ] Encrypted Client Hello (ECH) support for WebSocket TLS connections
- [ ] Domain fronting transport (TLS SNI ≠ HTTP Host, CDN-routed)
- [ ] DNS tunneling driver (encode frames in DNS queries, iodine/dnscat2-style)
- [ ] ICMP tunneling driver (data in echo payloads, restricted environments)
- [ ] Shadowsocks/Trojan-style protocol mimicry (look like standard HTTPS)

### Air-Gap and Proximity Transports
- [ ] Reticulum Network Stack driver (multi-hop mesh, announce-based discovery, FEC)
- [ ] Tor hidden service driver (.onion endpoints, garlic routing via I2P optional)
- [ ] Serial port driver (RS-232/USB, true physical air-gap)
- [ ] Bluetooth/BLE driver (proximity mesh, no infrastructure, Briar-inspired)
- [ ] Wi-Fi Direct driver (ad-hoc local connections)
- [ ] Audio modem driver (data over sound, extreme air-gap, chirp/quietnet-style)
- [ ] QR-stream visual channel (animated QR codes for air-gap transfer)

### Radio Transports
- [ ] LoRa/Meshtastic driver (sub-GHz, 250bps–11kbps, 10+ km range, mesh routing)
- [ ] AX.25 packet radio driver (amateur radio, global coverage, no commercial infra)
- [ ] HF radio / Winlink bridge (global reach via amateur radio e-mail gateways)
- [ ] Satellite link driver (Iridium/Starlink with DTN buffering for high-latency)

### Overlay Networks
- [ ] Yggdrasil driver (self-configuring IPv6 mesh, key-derived addresses, spanning tree)
- [ ] I2P driver (garlic routing, anonymous internal services)
- [ ] Veilid driver (DHT-based, onion-routed by default)
- [ ] Mixnet integration (Nym/Loopix — for high-paranoia mode, traffic analysis resistant)

### Peer Discovery
- [x] mDNS/DNS-SD — zero-config LAN discovery (first attempt before external)
- [ ] DHT (Kademlia-style) — decentralized global discovery, censorship-resistant
- [ ] Gossip protocol (SWIM/HyParView) — self-healing mesh topology
- [ ] Signed DNS records (DNSSEC SRV) — verifiable rendezvous
- [ ] BLE beacon discovery — proximity without infrastructure
- [ ] Announce-flood (Reticulum-style) — path discovery without central coordination

### Advanced NAT Traversal
- [ ] STUN server (self-hosted, for deployments without public STUN)
- [ ] TURN relay mode on rf-relay (full TURN compliance)
- [ ] Multipath TCP/QUIC — single logical connection over multiple physical paths
- [ ] Traffic analysis resistance (noise floor, packet normalization, timing obfuscation)
- [ ] Connection migration across interfaces (WiFi ↔ cellular ↔ Ethernet seamless)

---

## v0.6 — WASM Plugins + Multi-Tenant + Advanced Security + Mobile

**Goal:** Extensibility without recompiling. RBAC. Quantum-resistant cryptography. Capability-based auth. Mobile/embedded agents.

### Platform Expansion (Tier 2 + 3)
- [ ] Android agent (NDK cross-compile, foreground service, Doze-aware reconnect)
- [ ] iOS agent (Network Extension, background entitlements)
- [ ] Linux armv7 (Raspberry Pi 3/4/Zero 2W) — verified CI target
- [ ] Linux riscv64 — cross-compile verification
- [ ] FreeBSD agent
- [ ] OpenWrt package (MIPS/ARM, minimal feature set)
- [ ] WASM/WASI compilation target (browser-side client, edge workers)
- [ ] `no_std` subset evaluation for bare-metal ARM (ESP32, nRF52)
- [ ] Single-threaded async runtime mode (for constrained devices < 256KB RAM)

### Plugin System
- [ ] Wasmtime-based plugin runtime
- [ ] Custom resource types via WASM
- [ ] Custom transport drivers via WASM (extend without recompiling)

### Multi-Tenant & RBAC
- [ ] Tenant isolation
- [ ] RBAC (admin, operator, viewer, auditor)
- [ ] SecurityPolicy with immutable rules

### Capability-Based Authorization
- [ ] Biscuit token integration (commands carry their own signed permission)
- [ ] Capability delegation (agent A grants agent B limited capabilities)
- [ ] Attenuation (capabilities can be narrowed, never widened)
- [ ] Offline-verifiable (no central authority needed at execution time)

### Post-Quantum Cryptography
- [ ] Post-quantum hybrid handshake (ML-KEM + X25519, Noise XX with hybrid KEM)
- [ ] Signal PQXDH-inspired key exchange for long-lived sessions
- [ ] Harvest-now-decrypt-later resistance for all stored data

### CRDT State Propagation
- [ ] CRDT-based desired-state convergence (no master required)
- [ ] Append-only signed policy logs (Scuttlebutt-inspired)
- [ ] Opportunistic policy sync between neighboring agents
- [ ] Conflict-free policy merging across disconnected clusters
- [ ] Content-addressed policy distribution (request by hash, any node can serve)
- [ ] SPIFFE-style workload identity (identity independent of network position)

---

## v0.7 — Web UI + API

**Goal:** Web dashboard. REST/gRPC API. Observability.

- [ ] Controller binary with web UI
- [ ] REST + gRPC API
- [ ] OpenTelemetry traces
- [ ] Prometheus metrics endpoint

---

## v1.0 — Production Ready

**Goal:** Battle-tested. Fully documented. Packaged. The first system where "network" is fully abstracted from application and policy layers.

- [x] ~~Fuzz testing (transport, policy, codec)~~ — libfuzzer targets in fuzz/
- [x] ~~Performance benchmarks~~ — criterion benches for crypto + codec
- [ ] Kubernetes CRDs + operator
- [x] ~~Homebrew formula, apt/rpm repos, AUR, Nix flake~~ — Homebrew formula in deploy/ravenfabric.rb
- [ ] Documentation site
- [ ] Named Data Networking concepts for policy distribution (interest/data pattern)
- [ ] Subsea-cable resilience (mesh fallback when physical links fail)
- [ ] Full SPIFFE workload identity compliance

---

## Testing Strategy

### Unit Tests
- Every crate has isolated unit tests (no network, no filesystem)
- In-memory transport driver for all RPC tests
- Property-based testing for codec/parser edge cases (via `proptest`)

### Integration Tests
- Full pipeline: client → relay → agent → policy → execute → response (in-process)
- Policy denial verification (E2E denied flows)
- Reconnect after relay restart
- Hot-reload policy during active session

### Fuzz Testing
- Transport frame parsing (malformed frames must not panic)
- Policy YAML parsing (malformed input must not crash)
- RPC codec (malformed msgpack must not crash)
- Noise handshake (malformed messages must fail cleanly)

### CI Pipeline
- `cargo fmt --check` — formatting
- `cargo clippy --all-targets -- -D warnings` — lints
- `cargo test --all` — all unit + integration tests
- Coverage threshold: 60%
- Cross-compile: Linux (amd64, arm64, musl), macOS (amd64, arm64)
- MSRV check: Rust 1.85

---

## Performance Targets

| Metric | Target | Why |
|--------|--------|-----|
| Connection setup (first time) | < 2 RTT | Noise XX = 1.5 RTT |
| Shell latency overhead | < 10ms | Must be imperceptible vs raw TCP |
| `rf exec` simple command | < 100ms | Faster than SSH |
| File transfer throughput | Line speed | ChaCha20 saturates >10 Gbps on modern CPUs |
| Agent idle memory | < 10 MB | Must run on Raspberry Pi, IoT |
| Agent binary size | < 15 MB | Static musl build, stripped |
| Relay throughput | 10k concurrent sessions | Per-relay |

---

## Security Hardening Milestones

| Version | Hardening |
|---------|-----------|
| v0.1 | Noise XX mutual auth, deny-by-default policy, structured audit, `unsafe_code = "forbid"` |
| v0.2 | Symlink traversal protection, output limiting, timeout enforcement |
| v0.3 | Session recording, forced command mode, tunnel time limits |
| v0.4 | Sealed secrets, key rotation, secret masking in logs |
| v0.5 | Traffic analysis resistance (noise floor, packet normalization) |
| v0.6 | WASM sandboxing, RBAC, approval workflows |
| v1.0 | Fuzz-tested, binary integrity, DDoS mitigation |

---

## Technical Debt (Audit Findings — 5 May 2026)

All critical and important issues resolved. Minor items tracked below.

### Critical
- [x] ~~**Executor has zero tests**~~ — resolved: 12 tests covering policy denial, exec, timeout, output limiting, streaming

### Important (code correctness)
- [x] ~~`unwrap()` in library code~~ — resolved: all production code uses `?` or `expect()` with justification
- [x] ~~Wire magic/version constants defined but never exchanged during handshake~~ — resolved: RVNF + version byte sent and validated
- [x] ~~`TransportState` API gap~~ — resolved: `handshake()` returns `StatelessTransportState` for split-free SecureChannel
- [x] ~~`RwLock` poisoning not handled in `rf-bootstrap`~~ — resolved: `unwrap_or_else(|p| p.into_inner())`
- [x] ~~Audit log write errors silently swallowed~~ — resolved: `log()` returns `Result<(), AuditError>`

### Minor (code hygiene)
- [x] ~~Unused workspace deps: `proptest`, `base64`, `crc32fast`~~ — removed
- [x] ~~`yamux` declared in `rf-crypto` Cargo.toml (should be in `rf-rpc`)~~ — fixed, yamux now in rf-rpc
- [x] ~~`Target` type in `rf-transport` missing `Debug`/`Clone` derives~~ — fixed
- [x] ~~`rf-rpc` types have no serialization roundtrip tests~~ — added 8 roundtrip tests
- [x] ~~`RpcPolicy` error types use `Box<dyn Error>` instead of typed error~~ — resolved: uses `PolicyError` enum
- [x] ~~`sysinfo::System::new_all()` called per-request in executor metrics (should cache)~~ — uses `Arc<Mutex<System>>` now
- [x] ~~`#[allow(dead_code)]` on `agent_id` field in `rf-bootstrap`~~ — field now used
- [x] ~~CI clippy allows `unwrap_used` but workspace has it as `warn`~~ — aligned
- [x] ~~Release workflow uses `|| true` on binary copy~~ — fixed

---

## Design Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-05-05 | Rust over Go | Memory safety without GC. Single static binary. Fearless concurrency |
| 2026-05-05 | yamux for multiplexing | Battle-tested (libp2p). Per-stream flow control |
| 2026-05-05 | msgpack over JSON for RPC | Smaller frames, faster parse, binary-safe |
| 2026-05-05 | Wire protocol version in handshake | Enables rolling upgrades |
| 2026-05-05 | `rf` as CLI name | Short, memorable, fast to type |
| 2026-05-05 | Cargo workspace with 10 crates | Compile-time isolation, parallel compilation |
| 2026-05-05 | Noise XX over all transports | Formally verified, no PKI needed |
| 2026-05-05 | Relay is stateless and dumb | Minimizes relay's value as attack target |
| 2026-05-05 | `unsafe_code = "forbid"` | Enforced at workspace level via lints |
| 2026-05-05 | AGPLv3 + Commercial dual-license | Protects against silent forks as managed services |
| 2026-05-05 | Identity = key hash (Reticulum-inspired) | IP is implementation detail. Address derives from identity key |
| 2026-05-05 | DTN store-carry-forward | Disconnection is normal state. NASA Bundle Protocol concepts |
| 2026-05-05 | Transport = any byte-moving channel | USB sticks, radio, sound, QR are valid transports |
| 2026-05-05 | Capability-based auth (future) | Biscuit tokens scale better than centralized ACL in distributed mesh |
| 2026-05-05 | CRDT state convergence (future) | Desired-state reconciliation without master. Works over intermittent links |
| 2026-05-05 | Content-addressed payloads | Hash-identified commands/policies. Dedup, verify, cache naturally |
| 2026-05-05 | Transport-aware policy | Sensitivity level determines acceptable transport channels |
| 2026-05-05 | 13-phase connectivity value chain | Connection lifecycle is a formal pipeline (CONNECTIVITY.md). Each phase is independent and composable |
| 2026-05-05 | Universal platform target | Agent runs anywhere: server, desktop, mobile, IoT, embedded. No platform excluded by design |
| 2026-05-05 | Feature-flag architecture | `full` vs `minimal` feature sets allow same codebase to target 10 MB Raspberry Pi and 15 MB router |
