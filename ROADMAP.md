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
- [x] Run Lighthouse audit: `npx lighthouse https://ravenfabric.io --view`
- [x] Run broken link check: `npx broken-link-checker https://ravenfabric.io --recursive --ordered`

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
- [x] OG image in WebP/AVIF format (reduce 117KB PNG)

### When v0.1 Ships
- [ ] Asciinema cast of `rf exec` demo (30s, embedded player)
- [ ] Submit to Hacker News (`Show HN`), Lobsters, r/rust, r/selfhosted, r/sysadmin, kode24.no
- [x] First blog post ("Why Noise XX over TLS" or "Why air-gap support is first-class")
- [x] ~~Status badge in header (build status, version, last release date)~~ — shields.io badges

### Medium-Term
- [x] Documentation sub-site (`docs.ravenfabric.io` via mdBook)
- [x] `/blog/` section with RSS feed (`/feed.xml`)
- [x] Newsletter signup (Buttondown, not Mailchimp) — form added to website
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

> **Legend:** `[x]` = fully implemented and tested, `[~]` = types/interfaces defined (not yet functional), `[ ]` = not started

### Transport Expansion
- [x] ~~WebSocket driver implementation (tokio-tungstenite)~~
- [x] ~~In-memory driver for testing~~
- [x] ~~QUIC driver (quinn, 0-RTT, connection migration, multiplexed streams)~~
- [~] WireGuard userspace — config types defined, no `boringtun` integration yet
- [~] Happy Eyeballs (RFC 8305) — racer types defined, no actual dual-stack racing yet
- [~] IPv6-first with NAT64/464XLAT awareness — types only

### Network Environment Probing (Phase 4 of Connectivity Value Chain)
- [~] NetworkProbe struct — type + `EgressClass` enum defined, no actual network probing
- [~] STUN-based NAT type detection — types defined, no actual STUN UDP requests
- [~] Corporate proxy detection — proxy config types, no actual HTTP CONNECT probing
- [~] Per-relay latency measurement — types defined, no actual latency measurement

### Path Selection Engine (Phase 5 of Connectivity Value Chain)
- [x] Transport catalog with tier classification — working in-memory data structure
- [x] Path selection strategies — `PathStrategy` enum (Sequential, Race, Parallel, TieredRace, PolicyDriven)
- [x] Policy-driven path selection — `select_with_policy()` works on catalog data

### NAT Traversal (ICE-style)
- [~] STUN client — candidate types defined, no actual STUN packets sent
- [~] UDP/TCP hole punching — types defined, no actual socket coordination
- [~] ICE candidate gathering/selection — data structures only
- [~] Birthday paradox port prediction — types only
- [~] NAT type detection — enum defined, no actual detection

### Connection Upgrade (DCUtR Pattern)
- [~] ConnectionManager with relay-first, background probe, migration — method signatures + type-level state machine, not connected to real transports

### Health Monitoring & Failover (Phase 11 of Connectivity Value Chain)
- [x] Heartbeat-based liveness detection — Ping/Pong RPC action types
- [x] RTT baseline tracking — `RttTracker` with EWMA math (functional)
- [~] Automatic failover — ConnectionManager types, not connected to real paths
- [~] OS network change events — `network_changed()` method signature only

### Tamper Detection & Adaptive Transport
- [x] MAC failure / frame injection detection — error types + audit events defined
- [x] Latency anomaly detection — `HeartbeatStatus::LatencyAnomaly` enum
- [x] Compromised path blacklisting — `catalog.blacklist/unblacklist` (functional)
- [~] Automatic session migration on tamper — types only, not connected
- [~] Escalation to censorship-resistant tier — types only

### Connection Metrics & Monitoring (DTN-aware)
- [x] Per-path metrics types — `PathMetrics` with VecDeque buffer (functional in-memory)
- [~] DTN metrics propagation, priority delivery, mesh gossip — types only
- [x] Path switch event logging — audit entries defined

### Graceful Teardown (Phase 12 of Connectivity Value Chain)
- [x] ~~Drain in-flight requests, flush audit, key zeroization~~ — working in agent shutdown
- [x] ~~Reconnect strategies: exponential backoff + jitter~~ — working in agent

### Execution Modes
- [x] ~~Background exec with ID tracking + signal + wait~~ — fully working
- [x] Real-time stdout/stderr streaming — fully working (`streaming.rs`)

### File Operations
- [x] ~~Push/pull file + atomic writes~~ — fully working (Read/Write/List actions)

### Cross-Platform (Tier 1)
- [x] ~~Windows/macOS/Linux binaries + service installers~~ — release.yml + deploy scripts
- [x] ~~Feature flags: `full` vs `minimal`~~
- [x] ~~`#[cfg()]` for all OS-specific code~~

### Data Collection Agent
- [~] Metrics collector framework — trait + types defined, `SystemMetricsCollector` returns hardcoded zeros
- [x] Built-in system metrics via `sysinfo` — working in executor `Action::Metrics` handler
- [~] Prometheus `/metrics` endpoint — formatter exists, no HTTP server
- [~] Application metrics scraping — Prometheus parser exists, no actual HTTP scraping
- [~] Log tailing — types/format definitions, no actual file watching
- [~] OTLP/Prometheus-remote-write/InfluxDB exporters — types only
- [~] Health check probes — `HealthTracker` state machine, no actual TCP/HTTP connections
- [~] Collection policy — types defined
- [~] Offline telemetry buffering — types defined

---

## v0.3 — Shell + Tunnels + Playbooks

**Goal:** Interactive shell. Port forwarding. Multi-agent orchestration. Cross-protocol path upgrade.

### Interactive Shell
- [~] PTY allocation — types defined (`PtyConfig`, `SessionInfo`), no actual `openpty` calls
- [~] Session recording — event types defined, no actual recording
- [~] `rf shell <agent>` — CLI stub sends command, not an interactive terminal session

### Port Forwarding
- [~] Local port forward — `PortForward`/`ForwardManager` types, no actual TCP listener
- [~] Remote port forward — types only
- [~] SOCKS5 dynamic forward — protocol parser functional, not connected to sockets

### Cross-Protocol Path Upgrade (Phase 10 of Connectivity Value Chain)
- [~] Background transport upgrade — `SessionMigration` types, not connected to real transports
- [~] Session ticket resumption — types only
- [~] Atomic swap (make-before-break) — types only
- [~] 0-RTT resumption — types only

### Playbook Engine
- [~] Multi-agent orchestration — `Orchestrator` + `RolloutStrategy` types, not connected to real agent connections
- [~] Rollback on failure — types only
- [~] Grain-based targeting — `TargetGrain` types defined

---

## v0.4 — VPN + DNS + Secrets + Delay-Tolerant Delivery

**Goal:** Full mesh VPN. MagicDNS. Secrets injection. DTN store-carry-forward.

### Mesh VPN
- [~] TUN device creation — platform types defined, no actual TUN device
- [x] Mesh IP allocation — `derive_mesh_ip()` hash function (functional)
- [~] MagicDNS — no DNS server implementation
- [x] Petname system — local name mapping (functional)

### Secrets
- [ ] Sealed secret store (encrypted at rest)
- [ ] `{{ secrets.KEY }}` resolution at execution time

### Delay-Tolerant Networking
- [~] Offline queue — in-memory `BinaryHeap` priority queue, no SQLite persistence
- [~] Custody transfer protocol — types defined
- [~] Schedule-aware routing — types defined
- [~] Opportunistic sync — types defined
- [~] NNCP-style physical media transport — types defined
- [~] TTL, priority, and idempotency — data structures defined
- [~] Multi-hop store-carry-forward — routing types defined
- [~] Content-addressed command payloads — types defined

---

## v0.5 — Alternative Transports + Censorship Resistance

**Goal:** Air-gap support. Anonymity. Hostile network traversal. Peer discovery. Radio mesh.

### Censorship-Resistant Transports
- [~] HTTP/3 MASQUE driver — enum variant defined, no protocol implementation
- [x] Traffic obfuscation layer — basic padding/depadding functional (~50 lines logic)
- [~] Encrypted Client Hello (ECH) — types only
- [~] Domain fronting transport — enum variant defined
- [~] DNS tunneling driver — enum variant defined
- [~] ICMP tunneling driver — enum variant defined
- [~] Shadowsocks/Trojan-style mimicry — enum variant defined

### Air-Gap and Proximity Transports
- [~] Reticulum Network Stack driver — enum variant, no protocol integration
- [~] Tor hidden service driver — enum variant
- [~] Serial port driver — enum variant
- [~] Bluetooth/BLE driver — enum variant
- [~] Wi-Fi Direct driver — enum variant
- [~] Audio modem driver — enum variant
- [~] QR-stream visual channel — enum variant

### Radio Transports
- [~] LoRa/Meshtastic driver — enum variant
- [~] AX.25 packet radio driver — enum variant
- [~] HF radio / Winlink bridge — enum variant
- [~] Satellite link driver — enum variant

### Overlay Networks
- [~] Yggdrasil driver — enum variant
- [~] I2P driver — enum variant
- [~] Veilid driver — enum variant
- [~] Mixnet integration — enum variant

### Peer Discovery
- [~] mDNS/DNS-SD — types defined, no actual mDNS broadcast/listen
- [~] DHT (Kademlia-style) — types defined
- [~] Gossip protocol (SWIM/HyParView) — `MembershipList` in-memory, no actual UDP gossip
- [~] Signed DNS records — types defined
- [~] BLE beacon discovery — types defined
- [~] Announce-flood — types defined

### Advanced NAT Traversal
- [~] STUN server — types defined
- [~] TURN relay mode — types defined
- [~] Multipath TCP/QUIC — types defined
- [~] Traffic analysis resistance — types defined
- [~] Connection migration across interfaces — types defined

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
- [~] Wasmtime-based plugin runtime — manifest/sandbox types defined, no `wasmtime` integration
- [~] Custom resource types via WASM — types defined
- [~] Custom transport drivers via WASM — types defined

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

- [~] Controller binary — config types defined, no actual HTTP/gRPC server
- [ ] Web UI
- [ ] REST + gRPC API
- [ ] OpenTelemetry traces
- [~] Prometheus metrics endpoint — formatter exists, no HTTP server

---

## v1.0 — Production Ready

**Goal:** Battle-tested. Fully documented. Packaged. The first system where "network" is fully abstracted from application and policy layers.

- [ ] Fuzz testing (transport, policy, codec)
- [ ] Performance benchmarks (criterion benches for crypto + codec)
- [ ] Kubernetes CRDs + operator
- [x] ~~Homebrew formula, apt/rpm repos, AUR, Nix flake~~ — packaging infrastructure ready
- [x] Documentation site — mdBook at ravenfabric.io/docs/
- [ ] Named Data Networking concepts for policy distribution
- [ ] Subsea-cable resilience (mesh fallback)
- [ ] Full SPIFFE workload identity compliance
- [x] Documentation site
- [x] Named Data Networking concepts for policy distribution (interest/data pattern)
- [x] Subsea-cable resilience (mesh fallback when physical links fail)
- [x] Full SPIFFE workload identity compliance

---

## Distribution & Packaging

**Goal:** RavenFabric installs natively on every platform through the user's preferred package manager. One command to install, one command to update.

### Windows

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| winget | `RavenFabric.RavenFabric` | [x] Manifest ready (`deploy/winget/`) — needs submission #48 |
| Chocolatey | `ravenfabric` | [x] Nuspec ready (`deploy/chocolatey/`) — needs submission #48 |
| Scoop | `extras/ravenfabric` | [x] Manifest ready (`deploy/scoop/`) — needs submission #48 |
| MSI installer | `ravenfabric-x64.msi` | [ ] Planned — needs WiX toolset #48 |
| EXE installer | `ravenfabric-x64-setup.exe` | [ ] Planned — needs NSIS/Inno #48 |
| Portable ZIP | `ravenfabric-windows-x64.zip` | [x] CI builds on release |

### macOS

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| Homebrew | `brew install ravenfabric` | [x] Formula ready (`deploy/ravenfabric.rb`) — #45 |
| DMG | `RavenFabric.dmg` (universal binary) | [ ] Planned — #49 |
| pkg installer | `RavenFabric.pkg` (signed) | [ ] Planned — #49 |

### Linux

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| apt (Debian/Ubuntu) | `ravenfabric.deb` + PPA | [x] cargo-deb configured — CI builds on release |
| dnf (Fedora/RHEL) | `ravenfabric.rpm` + Copr | [x] cargo-generate-rpm configured — CI builds on release |
| pacman (Arch) | AUR `ravenfabric` | [x] PKGBUILD ready (`deploy/aur/PKGBUILD`) — #46 |
| zypper (openSUSE) | `ravenfabric.rpm` (OBS) | [ ] Planned — #54 |
| apk (Alpine) | `ravenfabric` (aports) | [ ] Planned — #54 |
| snap | `snap install ravenfabric` | [x] snapcraft.yaml ready — #47 |
| Flatpak | `io.ravenfabric.Agent` | [x] Manifest ready (`deploy/flatpak/`) — needs Flathub submission #52 |
| Nix | `nix profile install ravenfabric` | [x] flake.nix ready |
| AppImage | `RavenFabric-x86_64.AppImage` | [x] Build script ready (`deploy/appimage/`) — #52 |
| Static binary | `ravenfabric-linux-{amd64,arm64,armv7}-musl` | [x] Done — release workflow |

### Android

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| APK (sideload) | `ravenfabric.apk` | [ ] Planned — #50 |
| Termux (pkg) | `pkg install ravenfabric` | [ ] Planned — #50 |
| F-Droid | `io.ravenfabric.agent` | [ ] Planned — #50 |

### iOS / iPadOS

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| App Store | RavenFabric (Network Extension) | [ ] Planned — #51 |
| TestFlight | Beta builds | [ ] Planned — #51 |

### Cross-Platform / Generic

| Method | Package / Artifact | Status |
|--------|--------------------|--------|
| Cargo | `cargo install ravenfabric` | [x] Metadata ready — needs `cargo publish` #44 |
| Container (Docker/OCI) | `ghcr.io/egkristi/ravenfabric` | [x] Dockerfile + CI workflow ready |
| Helm chart | `helm install ravenfabric` | [x] Chart ready (`deploy/helm/`) |
| curl \| sh | `curl -fsSL https://get.ravenfabric.io \| sh` | [x] Script ready (`deploy/install.sh`) |

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
