# RavenFabric Roadmap

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
- [ ] In-memory transport driver for testing (no network required)
- [ ] Integration test harness: spawn relay + agent + client in-process

### Crypto Layer (Phase 1) — DONE
- [x] `rf-crypto/src/noise.rs` — Noise XX handshake + wire protocol (RVNF magic + version)
- [x] `rf-crypto/src/keys.rs` — StaticKey (load/save/generate, 0600 permissions, zeroed on drop)
- [x] `rf-crypto/src/channel.rs` — SecureChannel (send/recv, 64KB frames, concurrent via split Mutex)
- [x] `rf-crypto/src/error.rs` — CryptoError enum (typed errors)
- [x] Wire protocol version byte in handshake

### Transport Layer (Phase 2) — Partial
- [x] `rf-transport/src/driver.rs` — Driver trait + AsyncStream + Target + DriverConfig
- [x] `rf-transport/src/error.rs` — TransportError enum
- [ ] `rf-transport/src/drivers/websocket.rs` — WebSocket transport driver
- [ ] `rf-transport/src/drivers/memory.rs` — In-memory driver (for tests)

### RPC Layer (Phase 3) — Partial
- [x] `rf-rpc/src/types.rs` — Request/Response/Action/RpcResult types
- [ ] `rf-rpc/src/mux.rs` — yamux multiplexer over SecureChannel
- [ ] `rf-rpc/src/codec.rs` — msgpack frame codec (length-delimited)

### Audit (Phase 4) — DONE
- [x] `rf-audit/src/types.rs` — AuditEntry struct (timestamp, action, decision, duration, caller)
- [x] `rf-audit/src/logger.rs` — AuditLogger trait + FileAuditLogger (JSON-lines append)

### Policy Layer (Phase 5) — DONE
- [x] `rf-policy/src/rpc_policy.rs` — RPCPolicy enforcement (allow/deny regex, path rules, symlink resolution)
- [x] `rf-policy/src/decision.rs` — Decision type (allowed/denied + reason + rule)
- [ ] Hot-reload via SIGHUP (atomic policy swap)

### Executor (Phase 6) — DONE
- [x] `rf-executor/src/command.rs` — Policy-checked execution with timeout + output limiting
- [x] Metrics action handler (sysinfo)
- [ ] Streaming stdout/stderr via mux stream

### Bootstrap (Phase 7) — DONE
- [x] `rf-bootstrap/src/otp.rs` — OTP generation, validation, single-use, TTL-enforced, hash-stored
- [ ] Agent enrollment flow (token → key exchange → registered)

### Relay (Phase 8) — Stub
- [x] `rf-relay/src/main.rs` — Binary scaffold
- [ ] WebSocket listener + HMAC token auth
- [ ] Channel-based agent/client pairing
- [ ] Per-IP rate limiting

### Agent Binary (Phase 9) — Stub
- [x] `rf-agent/src/main.rs` — Binary scaffold
- [ ] Config loading (raven.toml)
- [ ] Reconnect loop with exponential backoff + jitter
- [ ] RPC request dispatcher (Executor integration)
- [ ] Graceful shutdown (drain in-flight, flush audit)

### CLI Binary (Phase 10) — Partial
- [x] `rf-cli/src/main.rs` — clap CLI with exec/dev/status subcommands
- [ ] `rf exec` — connect, handshake, send Request, display Response
- [ ] `rf dev` — local relay + agent in one process (no auth)
- [ ] `rf status` — show connected agents
- [ ] Shell completions (bash, zsh, fish)

### Packaging
- [x] Dockerfile (multi-stage alpine build → scratch runtime)
- [x] Release workflow (5 platform targets)
- [ ] Linux amd64 + arm64 static binaries (musl)
- [ ] systemd service units (agent + relay)

---

## v0.2 — Multi-Transport + Data Collection

**Goal:** Transport diversity. Task mode. File operations. Data collection agent. Windows + macOS.

### Transport Expansion
- [ ] WebSocket driver implementation (tokio-tungstenite)
- [ ] In-memory driver for testing
- [ ] QUIC driver (quinn, 0-RTT, connection migration, multiplexed streams)
- [ ] WireGuard userspace (boringtun, direct peers on open network)
- [ ] Happy Eyeballs (RFC 8305) — race IPv4/IPv6, use first responder
- [ ] IPv6-first with NAT64/464XLAT awareness

### NAT Traversal (ICE-style)
- [ ] STUN client — discover server-reflexive candidates (public IP:port)
- [ ] UDP hole punching — coordinated simultaneous send via relay coordinator
- [ ] TCP hole punching — simultaneous open (RFC 5128)
- [ ] ICE candidate gathering — host, server-reflexive, relayed candidates
- [ ] ICE candidate selection — parallel probing, select fastest path
- [ ] Birthday paradox port prediction for symmetric NAT
- [ ] NAT type detection (full cone, restricted, port-restricted, symmetric)

### Connection Upgrade (DCUtR Pattern)
- [ ] Relay-first connection (immediate, always works)
- [ ] Background direct-path probing while relay is active
- [ ] Seamless migration to direct path when found (verify peer key)
- [ ] Automatic failback to relay if direct path fails
- [ ] Connection migration on network change (WiFi ↔ cellular)

### Execution Modes
- [ ] Task mode (ordered steps, conditions, onFailure, workdir)
- [ ] Background exec with ID tracking + signal + wait
- [ ] Real-time stdout/stderr streaming via mux stream

### File Operations
- [ ] Push file (orchestrator → agent)
- [ ] Pull file (agent → orchestrator)
- [ ] Atomic writes (temp + rename)

### Cross-Platform
- [ ] Windows binary + Windows Service installer
- [ ] macOS binary + launchd plist

### Data Collection Agent
- [ ] Metrics collector framework (plugin trait, scrape loop, push/pull modes)
- [ ] Built-in system metrics (CPU, memory, disk, network, load, filesystems, processes)
- [ ] Prometheus-compatible `/metrics` endpoint (pull mode)
- [ ] Application metrics scraping (scrape localhost Prometheus endpoints)
- [ ] Log tailing (glob patterns, journald, structured parsing: JSON/logfmt/regex/grok)
- [ ] OTLP exporter (metrics + logs + traces to any OTLP-compatible backend)
- [ ] Prometheus remote-write exporter
- [ ] InfluxDB line protocol exporter
- [ ] Health check probes (HTTP/TCP/UDP endpoints, process alive, cert expiry)
- [ ] Collection policy (what to collect governed by same deny-by-default policy)
- [ ] Offline telemetry buffering (queue metrics/logs while disconnected, flush on reconnect)

---

## v0.3 — Shell + Tunnels + Playbooks

**Goal:** Interactive shell. Port forwarding. Multi-agent orchestration.

### Interactive Shell
- [ ] PTY allocation + terminal session handler
- [ ] Session recording (asciinema v2 format)
- [ ] `rf shell <agent>` — interactive terminal through fabric

### Port Forwarding
- [ ] Local port forward (ssh -L equivalent)
- [ ] Remote port forward (ssh -R equivalent)
- [ ] SOCKS5 dynamic forward (ssh -D equivalent)

### Playbook Engine
- [ ] Multi-agent orchestration (rolling/canary/parallel strategies)
- [ ] Rollback on failure
- [ ] Grain-based targeting

---

## v0.4 — VPN + DNS + Secrets + Delay-Tolerant Delivery

**Goal:** Full mesh VPN. MagicDNS. Secrets injection. DTN store-carry-forward.

### Mesh VPN
- [ ] TUN device creation (cross-platform)
- [ ] Mesh IP allocation (key-derived addresses)
- [ ] MagicDNS (agent-name.rf.local)
- [ ] Petname system (local names → cryptographic identifiers)

### Secrets
- [ ] Sealed secret store (encrypted at rest)
- [ ] `{{ secrets.KEY }}` resolution at execution time

### Delay-Tolerant Networking
- [ ] SQLite-backed persistent offline queue (survives restart)
- [ ] Custody transfer protocol (each hop acknowledges responsibility)
- [ ] Schedule-aware routing (contact windows, satellite passes)
- [ ] Opportunistic sync (exchange queued messages when agents meet)
- [ ] NNCP-style physical media transport (USB, SD card, file-based delivery)
- [ ] TTL, priority, and idempotency for queued commands
- [ ] Multi-hop store-carry-forward (intermediate nodes relay when path opens)
- [ ] Content-addressed command payloads (deduplication across paths)

---

## v0.5 — Alternative Transports + Censorship Resistance

**Goal:** Air-gap support. Anonymity. Hostile network traversal. Peer discovery. Radio mesh.

### Censorship-Resistant Transports
- [ ] HTTP/3 MASQUE driver (CONNECT-UDP/CONNECT-IP via HTTP/3, impossible to distinguish from browsing)
- [ ] Traffic obfuscation layer (make Noise XX indistinguishable from random bytes, obfs4-inspired)
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
- [ ] mDNS/DNS-SD — zero-config LAN discovery (first attempt before external)
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

## v0.6 — WASM Plugins + Multi-Tenant + Advanced Security

**Goal:** Extensibility without recompiling. RBAC. Quantum-resistant cryptography. Capability-based auth.

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

- [ ] Fuzz testing (transport, policy, codec)
- [ ] Performance benchmarks
- [ ] Kubernetes CRDs + operator
- [ ] Homebrew formula, apt/rpm repos, AUR, Nix flake
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
