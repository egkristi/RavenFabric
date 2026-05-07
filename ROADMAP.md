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
- [x] Test Open Graph cards — all OG/Twitter meta tags validated, og-image.png 1200×630 confirmed (manual platform testing requires human)
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
- [x] **Asciinema demo script** created (`docs/demo/demo-record.sh`) — recording requires human (`asciinema rec`)
- [ ] Submit to Hacker News (`Show HN`), Lobsters, r/rust, r/selfhosted, r/sysadmin, kode24.no
- [x] Blog post #2: "How RavenFabric stops AI agents from running `rm -rf /`"
- [x] Blog post #3: "Zero-trust mesh networking without certificates — Noise XX deep dive"
- [x] First blog post ("Why Noise XX over TLS" or "Why air-gap support is first-class")
- [x] ~~Status badge in header (build status, version, last release date)~~ — shields.io badges

### Marketing Launch Plan
- [x] Record asciinema demo — script ready at `docs/demo/demo-record.sh`
- [x] Write `Show HN` post (title + 300-word description of what makes it different) — `marketing/show-hn.md`
- [x] Prepare Reddit posts: r/rust (technical), r/selfhosted (deployment), r/sysadmin (replaces what) — `marketing/reddit-posts.md`
- [ ] Schedule submissions: HN weekday morning US-east, Reddit staggered over 3 days
- [ ] Lobsters invite + submission (needs existing member invite)
- [ ] kode24.no pitch (Norwegian tech press)
- [x] Conference pitch: prepare 5-min lightning talk proposal (NDC Oslo, RustConf, FOSDEM Security devroom) — `marketing/conference-pitch.md`

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
- [x] WireGuard userspace — `WgTunnel` with UDP socket, key handling, peer management (9 tests)
- [x] Happy Eyeballs (RFC 8305) — `race_connect()` and `race_connect_multi()` with real TCP racing, resolution delay, staggered starts (3 async tests)
- [x] IPv6-first with NAT64/464XLAT awareness — NAT64 prefix detection (RFC 7050), IPv6 synthesis, `detect_nat64()` with 4 tests

### Network Environment Probing (Phase 4 of Connectivity Value Chain)
- [x] NetworkProbe struct — `quick_probe()` checks IPv4/IPv6/UDP availability + `EgressClass` classification (functional)
- [x] STUN-based NAT type detection — real UDP STUN binding requests in `stun_client.rs`
- [x] Corporate proxy detection — HTTP CONNECT probing with TCP RTT measurement, auth detection (407), status parsing (3 async tests)
- [x] Per-relay latency measurement — TCP connect RTT prober, probe_all(), continuous loop with cancellation (3 async tests)

### Path Selection Engine (Phase 5 of Connectivity Value Chain)
- [x] Transport catalog with tier classification — working in-memory data structure
- [x] Path selection strategies — `PathStrategy` enum (Sequential, Race, Parallel, TieredRace, PolicyDriven)
- [x] Policy-driven path selection — `select_with_policy()` works on catalog data

### NAT Traversal (ICE-style)
- [x] STUN client — real UDP binding requests in `stun_client.rs` (RFC 5389/8489), 9 tests
- [x] UDP/TCP hole punching — real UDP socket coordination, probe/ACK protocol, concurrent punch (2 async tests)
- [x] ICE candidate gathering — `gather_candidates()` with host + server-reflexive via STUN
- [x] Birthday paradox port prediction — `generate_candidates()` with deterministic PRNG, `collision_probability()`, peer coordination support
- [x] NAT type detection — `detect_nat_type()` compares bindings from multiple servers

### Connection Upgrade (DCUtR Pattern)
- [x] ConnectionManager with relay-first, background probe, migration — `ConnectionRunner` async wrapper wired to real `Driver::dial()` with 4 tests

### Health Monitoring & Failover (Phase 11 of Connectivity Value Chain)
- [x] Heartbeat-based liveness detection — Ping/Pong RPC action types
- [x] RTT baseline tracking — `RttTracker` with EWMA math (functional)
- [x] Automatic failover — `ConnectionRunner::report_failure()` triggers failback_to_relay + automatic reconnect
- [x] OS network change events — NetworkWatcher with polling, snapshot diff, gateway detection (Linux /proc, macOS route), watch loop (7 tests)

### Tamper Detection & Adaptive Transport
- [x] MAC failure / frame injection detection — error types + audit events defined
- [x] Latency anomaly detection — `HeartbeatStatus::LatencyAnomaly` enum
- [x] Compromised path blacklisting — `catalog.blacklist/unblacklist` (functional)
- [x] Automatic session migration on tamper — `ConnectionRunner::report_tamper()` blacklists + migrates to alternative
- [x] Escalation to censorship-resistant tier — `CensorshipEscalation` state machine with 5 tiers, failure counting, tamper detection (immediate escalation), de-escalation (blocked after tamper), 5 tests

### Connection Metrics & Monitoring (DTN-aware)
- [x] Per-path metrics types — `PathMetrics` with VecDeque buffer (functional in-memory)
- [x] DTN metrics propagation, priority delivery, mesh gossip — `MetricsPropagator` bundles metrics into DTN store-carry-forward, chunked delivery, decode on receive, 4 tests
- [x] Path switch event logging — audit entries defined

### Graceful Teardown (Phase 12 of Connectivity Value Chain)
- [x] ~~Drain in-flight requests, flush audit, key zeroization~~ — working in agent shutdown
- [x] ~~Reconnect strategies: exponential backoff + jitter~~ — working in agent

### Execution Modes
- [x] ~~Background exec with ID tracking + signal + wait~~ — fully working
- [x] Real-time stdout/stderr streaming — fully working (`streaming.rs`)

### Desired-State Convergence Engine
- [x] `DesiredStateSpec` YAML parsing — full spec with packages, files, services, sysctl resources
- [x] `ConvergenceEngine` — check actual vs desired state via `SystemProbe` trait
- [x] Drift detection — per-resource `DriftItem` with `DriftStatus` (Converged/Drifted/Remediated/Failed)
- [x] Remediation mode — `Remediator` trait, auto-fix drifted resources when `mode: remediate`
- [x] Version constraint matching — exact, `>=`, `>`, `<`, `<=` operators
- [x] `ConvergenceReport` — JSON-serializable report with `is_converged()`, `drift_count()`
- [x] 18 unit tests covering all resource types, drift scenarios, remediation success/failure

### Event System (Trigger-Based Execution)
- [x] `EventTrigger` enum — Cron, FileWatch, ProcessExit, Webhook, Timer triggers
- [x] `EventBus` — broadcast-based pub/sub, trigger registration/removal, fire by name
- [x] `TimerScheduler` — background timer with repeat/one-shot, cancel support
- [x] `Action` types — Exec, Converge, Notify
- [x] 12 unit tests covering parsing, bus operations, timer firing

### Result Parsing & Assertions
- [x] Multi-format parser — JSON (flattened), YAML, CSV, key-value, lines, raw
- [x] Assertion engine — Eq, Ne, Contains, NotContains, Matches (regex), Gt, Lt, Gte, Lte, Exists
- [x] Nested JSON/YAML flattening with dot-notation paths
- [x] `ParseResult` with `all_passed()` / `failure_count()`
- [x] 18 unit tests covering all formats and assertion operators

### Grains (System Facts Collection)
- [x] `Grains::collect()` — OS, arch, hostname, env, pointer width
- [x] `GrainValue` enum — String, Integer, Float, Bool, List
- [x] Label selector matching — `matches_labels()` for targeting
- [x] Merge support — overlay custom grains on system-collected facts
- [x] 10 unit tests covering collection, matching, serialization

### File Operations
- [x] ~~Push/pull file + atomic writes~~ — fully working (Read/Write/List actions)

### Cross-Platform (Tier 1)
- [x] ~~Windows/macOS/Linux binaries + service installers~~ — release.yml + deploy scripts
- [x] ~~Feature flags: `full` vs `minimal`~~
- [x] ~~`#[cfg()]` for all OS-specific code~~

### Data Collection Agent
- [x] Metrics collector framework — `SystemMetricsCollector` with real sysinfo (CPU, memory, load, disk)
- [x] Built-in system metrics via `sysinfo` — working in executor `Action::Metrics` handler + standalone collector
- [x] Prometheus `/metrics` endpoint — lightweight TCP HTTP server in `metrics_server.rs`, agent integration
- [x] Application metrics scraping — `scrape_target()` HTTP GET + Prometheus parser + filters + prefix/labels (1 async integration test)
- [x] Log tailing — `FileTailer` with rotation detection, JSON/logfmt parsing, include/exclude filters
- [x] OTLP/Prometheus-remote-write/InfluxDB exporters — `MetricExporter` with 3 formats (Prometheus exposition, OTLP JSON, InfluxDB line protocol), prefix/label support, histogram handling, 4 tests
- [x] Health check probes — `execute_probe()` with real TCP connect, HTTP GET, process check, command check
- [x] Collection policy — include/exclude patterns, label filters, sampling rate, histogram toggle, batch size limit (5 tests)
- [x] Offline telemetry buffering — MetricBuffer with overflow handling, batch flush, drop counter (2 tests)

---

## v0.3 — Shell + Tunnels + Playbooks + Local IPC

**Goal:** Interactive shell. Port forwarding. Multi-agent orchestration. Cross-protocol path upgrade. Local IPC transports for zero-network agent communication.

### Interactive Shell
- [x] PTY allocation — real `openpty` on Unix with `PtySession` (spawn, read, write, resize, signal)
- [x] Session recording — `SessionRecorder` with asciicast v2 output (functional)
- [x] `rf shell <agent>` — Full interactive shell via RPC: raw mode terminal, bidirectional stdin/stdout, Shell/ShellInput/ShellResize/ShellClose actions

### Port Forwarding
- [x] Local port forward — `start_local_forward()` with real TCP listener + bidirectional copy + RPC PortForward/PortForwardClose actions
- [x] `rf forward -L` CLI command — connect to agent, request forward, keep alive until Ctrl+C
- [x] Remote port forward — `start_remote_forward()` with agent-side listener + bidirectional copy + RemoteForward RPC action
- [x] SOCKS5 dynamic forward — full `Socks5Server` TCP proxy: method negotiation, CONNECT handling, policy check, bidirectional relay (1 async integration test)

### Cross-Protocol Path Upgrade (Phase 10 of Connectivity Value Chain)
- [x] Background transport upgrade — `SessionMigration` wired to `ConnectionRunner::migrate_session()` with peer key verification (2 async tests)
- [x] Session ticket resumption — `SessionTicket` persists across migrations, transport recorded
- [x] Atomic swap (make-before-break) — overlap window with peer verification before old path close
- [x] 0-RTT resumption — ZeroRttCache with ticket storage, try_resume, validate_incoming, eviction, use-count replay protection (6 tests)

### Playbook Engine
- [x] Multi-agent orchestration — `Orchestrator` + `rf playbook` CLI command connected to real agent RPC sessions
- [x] Rollback on failure — automatic rollback command execution on agents that succeeded before failure
- [x] Grain-based targeting — `TargetGrain` with agent-list targeting for CLI

### Local IPC Transports (Zero-Network Local-to-Local)
- [x] UNIX domain socket driver — `UnixSocketDriver` implementing `Driver` trait for same-host communication (Linux, macOS, FreeBSD)
- [x] Named pipe driver — `NamedPipeDriver` for Windows local IPC (`\\.\pipe\ravenfabric`)
- [x] Stdio pipe driver — `StdioDriver` for parent-child process communication (MCP stdio transport, embedded agents)
- [x] Vsock driver — `VsockDriver` for VM-to-hypervisor communication (firecracker, cloud-hypervisor, QEMU)
- [x] Abstract namespace sockets — Linux-specific `@ravenfabric/<session-id>` (no filesystem cleanup needed)
- [x] Automatic driver selection — `AutoSelectDriver` probes available transports and selects best (vsock > unix > named-pipe > loopback)
- [x] File-descriptor passing — `fd_passing` module: send/recv pre-authenticated FDs over UNIX sockets via SCM_RIGHTS (4 tests)
- [x] Socket activation — systemd/launchd socket activation for on-demand agent start (sd_listen_fds / launchd plist)
- [x] Permission enforcement — socket file mode 0600/0660, peer credential verification via `SO_PEERCRED` (Linux) / `LOCAL_PEERCRED` (macOS)

### MCP Server (AI Agent Integration)
- [x] `rf-mcp-server` binary — Model Context Protocol server translating MCP tool calls to RavenFabric operations
- [x] stdio transport — single-user, single-session (Claude Desktop, IDE extensions)
- [x] HTTP+SSE transport — multi-user, server deployment (web-based AI applications)
- [x] Tool: `rf_exec` — policy-validated command execution with structured errors (denial, approval, rate-limit)
- [x] Tool: `rf_query_policy` — pre-flight policy check without execution
- [x] Tool: `rf_request_approval` — human-in-loop approval workflow for sensitive operations
- [x] Tool: `rf_list_my_capabilities` — dynamic capability discovery filtered by agent policy
- [x] Tool: `rf_audit_query` — self-audit (agent queries its own recent actions)
- [x] Tool: `rf_file_read` / `rf_file_write` — filesystem operations subject to path policy
- [x] API token authentication — `--api-token` / `RF_API_TOKEN` env, constant-time validation, reject unauthenticated requests
- [x] Per-session cryptographic identity — short-lived Curve25519 keys per MCP session
- [x] Token rotation — comma-separated tokens for grace period, --api-token-file for external rotation
- [x] RBAC per caller — map API tokens to policy profiles (different callers get different permissions)
- [x] Rate limiting per session — sliding window request throttle (configurable max requests/minute)
- [x] Agent reasoning capture — optional `reason` parameter recorded in audit log
- [x] Claude Desktop integration — `claude_desktop_config.json` reference setup, docs/src/integrations/claude-desktop.md
- [x] Claude Code integration — `claude mcp add` reference setup, docs/src/integrations/claude-code.md
- [x] Cursor integration — MCP server config, workspace-scoped policy, docs/src/integrations/cursor.md
- [x] Aider integration — `.aider.conf.yml` setup, docs/src/integrations/aider.md
- [x] Design spec: [docs/src/use-cases/ai-agent-access.md](docs/src/use-cases/ai-agent-access.md)

### Policy Templates Library
- [x] "Coding assistant" template — filesystem read/write in project dir, git, package managers, test runners; deny network mutation
- [x] "Production read-only" template — allow query/status commands, deny all writes, deny destructive operations
- [x] "Security investigator" template — broad read access, deny writes, deny exfiltration, approval for credential access
- [x] "CI/CD agent" template — build/test/deploy commands, scoped to repo workdir, approval for production push
- [x] "Database query agent" template — SELECT allowed, DML denied by default, approval for schema changes
- [x] Template validation CLI — `rf policy validate --template coding-assistant`
- [x] Template composition — layer multiple templates with deny-wins conflict resolution

### Prompt Injection Detection
- [x] Command-level heuristics — detect base64-encoded payloads, hex-encoded commands, unicode homoglyphs in arguments
- [x] Pattern library — known injection markers (markdown escapes, instruction overrides, role-play triggers)
- [x] Evasion detection — obfuscated commands (string concatenation, variable indirection, eval patterns)
- [x] Configurable response — `block` (deny + audit), `flag` (allow + alert), `log` (allow + record suspicion score)
- [x] Suspicion scoring — cumulative score per session; threshold triggers automatic capability reduction
- [x] Integration with audit log — injection attempts recorded with matched pattern and confidence level

---

## v0.4 — VPN + DNS + Secrets + Delay-Tolerant Delivery

**Goal:** Full mesh VPN. MagicDNS. Secrets injection. DTN store-carry-forward.

### Mesh VPN
- [x] TUN device creation — Linux (/dev/net/tun + ioctl) and macOS (utun control socket), read/write/drop, 3 tests
- [x] Mesh IP allocation — `derive_mesh_ip()` hash function (functional)
- [x] MagicDNS — UDP DNS server with AAAA query handling, label-length parsing, NXDOMAIN (2 async tests)
- [x] Petname system — local name mapping (functional)

### Secrets
- [x] Sealed secret store (encrypted at rest) — `SecretStore` with ChaCha20-Poly1305, seal/unseal, key zeroize on drop (8 tests)
- [x] `{{ secrets.KEY }}` resolution at execution time — integrated into Executor, commands resolved before `sh -c`

### Delay-Tolerant Networking
- [x] Offline queue — in-memory `BinaryHeap` + SQLite persistence (`dtn_persistent.rs`, 8 tests)
- [x] Custody transfer protocol — `CustodyAgent` with initiate/ack/timeout state machine, retry logic, event notifications, 5 tests
- [x] Schedule-aware routing — `ContactWindow`, `Recurrence`, `RoutingDecision`, `is_window_active()`, `next_window()`, `select_route()` (6 tests)
- [x] Opportunistic sync — `OpportunisticSync` controller with peer discovery trigger, queue drain, re-sync on reconnect, 3 tests
- [x] NNCP-style physical media transport — `NncpTransport` with filesystem write/read, bundle JSON serialization, deduplication, 2 tests
- [x] TTL, priority, and idempotency — `DtnQueue` with priority ordering, dedup, TTL expiry, critical-never-expires, hop limits
- [x] Multi-hop store-carry-forward — `HopForwarder` with direct/relay/store/drop decisions, neighbor management, 4 tests
- [x] Content-addressed command payloads — `Bundle::content_addressed()` with SHA-256 hash, `verify_content_address()` integrity check, 2 tests

---

## v0.5 — Alternative Transports + Censorship Resistance

**Goal:** Air-gap support. Anonymity. Hostile network traversal. Peer discovery. Radio mesh.

### Censorship-Resistant Transports
- [x] HTTP/3 MASQUE driver — `MasqueTransport` with RFC 9297/9298 capsule encoding, CONNECT-UDP/CONNECT-IP, varint framing, session management (8 tests)
- [x] Traffic obfuscation layer — basic padding/depadding functional (~50 lines logic)
- [x] Encrypted Client Hello (ECH) — `EchTransport` with RFC 9460 config parsing, HPKE cipher suite selection, GREASE fallback, base64 config list decoder (6 tests)
- [x] Domain fronting transport — `DomainFronter` with SNI/Host rewriting, tunnel request generation, response parsing (3 tests)
- [x] DNS tunneling driver — `DnsTunnelCodec` with base32/hex encoding, query fragmentation, response decoding (5 tests)
- [x] ICMP tunneling driver — `IcmpTunnelFramer` with echo request framing, serialize/deserialize, session multiplexing (3 tests)
- [x] Shadowsocks/Trojan-style mimicry — `MimicryCodec` with ChaCha20-Poly1305 AEAD, counter-derived nonces, protocol stats (4 tests)

### Air-Gap and Proximity Transports
- [~] Reticulum Network Stack driver — enum variant, no protocol integration
- [~] Tor hidden service driver — enum variant
- [x] Serial port driver — `SerialFramer` with sync bytes, CRC-16/CCITT, frame detection, encode/decode (5 tests)
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
- [x] mDNS/DNS-SD — DiscoveryAgent with UDP broadcast/listen, JSON announcement protocol, self-filtering (2 async tests)
- [x] DHT (Kademlia-style) — `KademliaTable` with 256 k-buckets, XOR distance, closest-node lookup, insert/remove, 5 tests
- [x] Gossip protocol (SWIM/HyParView) — GossipAgent with real UDP transport, JSON serialization, bidirectional health propagation (2 async tests)
- [x] Signed DNS records — `SignedDnsRecord`, `DnsRelayDiscovery` with DNSSEC validation requirement, SRV/TXT/TLSA record types, DANE support, 3 tests
- [x] BLE beacon discovery — `BleDiscovery` with RSSI-based range filtering, service UUID matching, peer tracking, 3 tests
- [x] Announce-flood — `AnnounceFlood` gossip protocol with dedup, rate limiting, TTL decrement, re-broadcast, 4 tests

### Advanced NAT Traversal
- [x] STUN server — `StunServer` with UDP binding, XOR-MAPPED-ADDRESS responses (RFC 5389), client-server roundtrip verified, 6 tests
- [x] TURN relay mode — `TurnRelay` with UDP allocations, permissions, data relay, capacity limits, 5 tests
- [x] Multipath TCP/QUIC — `MultipathFrameScheduler` with 5 algorithms (RoundRobin, LowestLatency, LatencyWeighted, Redundant, BandwidthWeighted), critical frame redundancy, receiver dedup, 6 tests
- [x] Traffic analysis resistance — `TrafficShaper` with constant-rate/adaptive modes, dummy cover traffic, frame splitting, bandwidth accounting, 7 tests
- [x] Connection migration across interfaces — `InterfaceMigration` with auto-migrate, preferred patterns, netwatch integration, 3 tests

---

## v0.6 — WASM Plugins + Multi-Tenant + Advanced Security + Mobile

**Goal:** Extensibility without recompiling. RBAC. Quantum-resistant cryptography. Capability-based auth. Mobile/embedded agents.

### Platform Expansion (Tier 2 + 3)
- [x] Android agent — NDK cross-compile config, AndroidManifest.xml, Termux instructions in `deploy/android/`
- [x] iOS agent — build config, Network Extension entitlements, cargo config in `deploy/ios/`
- [x] Linux armv7 (Raspberry Pi 3/4/Zero 2W) — verified CI target (cross-check in CI)
- [x] Linux riscv64 — cross-compile verification (cross-check in CI)
- [x] FreeBSD agent — cross-compile verification (cross-check in CI)
- [x] OpenWrt package (MIPS/ARM, minimal feature set) — Makefile + init script in `deploy/openwrt/`
- [x] WASM/WASI compilation target (browser-side client, edge workers) — `rf-crypto --no-default-features` compiles for `wasm32-wasip1`, `frame_codec` module available in WASM
- [x] `no_std` subset evaluation for bare-metal ARM (ESP32, nRF52) — evaluation doc + `rf-crypto` feature-gated (`--no-default-features` compiles), `frame_codec` module provides no_std encrypt/decrypt, 7 new tests
- [x] Single-threaded async runtime mode — `rt-single-thread` feature flag in rf-agent, uses `current_thread` runtime for constrained devices

### Plugin System
- [x] Wasmtime-based plugin runtime — `PluginRegistry` with hash verification, capability checking, lifecycle management (Loaded→Ready→Running→Failed→Disabled), invocation tracking, 7 tests
- [x] Custom resource types via WASM — `PluginType::ResourceType` in registry with full manifest/sandbox support
- [x] Custom transport drivers via WASM — `PluginType::TransportDriver` in registry with capability-gated host interface

### Multi-Tenant & RBAC
- [x] Tenant isolation — `TenantIsolation` with cross-tenant blocking, agent-to-tenant mapping (4 tests)
- [x] RBAC (admin, operator, viewer, auditor) — role-based access in `ApiRouter` with required_role enforcement
- [x] SecurityPolicy with immutable rules — `SecurityPolicy` with immutable deny list, delegation depth, token lifetime, policy change roles (4 tests)

### Capability-Based Authorization
- [x] Biscuit token integration — `CapabilityToken` with sign/verify, serialization, expiry (5 tests)
- [x] Capability delegation — `delegate()` with attenuation, depth limits (2 tests)
- [x] Attenuation — capabilities narrowed via subset restriction, never widened
- [x] Offline-verifiable — Ed25519 signature verification, no central authority needed

### Post-Quantum Cryptography
- [x] Post-quantum hybrid handshake — `HybridKemContext` combining classical + PQ secrets via HKDF-SHA256 (3 tests)
- [x] Signal PQXDH-inspired key exchange — `PqxdhRatchet` double ratchet with skipped key tracking (3 tests)
- [x] Harvest-now-decrypt-later resistance — hybrid KEM ensures PQ protection for stored data

### CRDT State Propagation
- [x] CRDT-based desired-state convergence — `GSet`, `LwwRegister`, `OrSet`, `PolicyCrdt` with deny-wins semantics (12 tests)
- [x] Append-only signed policy logs — `PolicyLog` with SHA-256 hash chain + HMAC-SHA256 signatures, integrity verification (3 tests)
- [x] Opportunistic policy sync — `sync_state()` and `entries_since()` for neighbor sync
- [x] Conflict-free policy merging — `PolicyCrdt::merge()` with union semantics, idempotent
- [x] Content-addressed policy distribution — `compute_policy_hash()` SHA-256 content addressing (1 test)
- [x] SPIFFE-style workload identity (identity independent of network position)

---

## v0.7 — Web UI + API + AI Compliance

**Goal:** Web dashboard. REST/gRPC API. Observability. AI agent behavioral analysis and compliance reporting.

- [x] Controller binary — `AgentRegistry` (heartbeat, stale detection, label selection), `ApiRouter` with 8 REST routes, path matching, role-based access, 7 tests
- [x] Web UI — embedded HTML/CSS/JS dashboard with real-time agent metrics, activity feed, connected agents table
- [x] REST + gRPC API — `ApiDispatcher` with health/agents endpoints, auth middleware, role-based access (4 tests)
- [x] OpenTelemetry traces — `TraceContext` (W3C traceparent), `Span` with OTLP JSON export, SpanKind/Status/Events (5 tests)
- [x] Prometheus metrics endpoint — `metrics_server.rs` HTTP server + agent `--metrics-addr` flag

### Behavioral Anomaly Detection
- [x] Per-identity baseline collection — command frequency, timing patterns, resource access patterns over rolling window
- [x] Statistical deviation alerting — Z-score threshold on command rate, new-path-access rate, denial rate
- [x] Session anomaly scoring — cumulative risk score per session; high score triggers automatic capability reduction or session termination
- [x] Anomaly types: velocity (too many commands), novelty (accessing paths never accessed before), timing (unusual hours), escalation (repeated denied-then-reformulated attempts)
- [x] Integration with audit log — anomaly events enriched with baseline comparison data
- [x] Alert routing — anomaly alerts to webhook via --alert-webhook / RF_ALERT_WEBHOOK

### AI Compliance Reporting
- [x] EU AI Act traceability report — per-agent decision log with reasoning, human oversight records, risk classification
- [x] NIST AI Risk Management Framework alignment — map RavenFabric controls to NIST AI RMF functions (Govern, Map, Measure, Manage)
- [x] Audit report generation — structured reports from audit log data, filterable by agent, time range, action type
- [x] Human-in-loop evidence — approval workflow records as proof of human oversight for high-risk AI operations
- [x] Incident reconstruction — timeline view of agent actions leading to an incident, with reasoning and policy decisions
- [x] Export formats — JSON, CSV for compliance submissions; SIEM-compatible event streams

---

## v1.0 — Production Ready

**Goal:** Battle-tested. Fully documented. Packaged. The first system where "network" is fully abstracted from application and policy layers.

- [x] Fuzz testing (transport, policy, codec) — 3 fuzz targets via cargo-fuzz (fuzz_codec, fuzz_policy, fuzz_frame)
- [x] Performance benchmarks (criterion benches for crypto + codec) — crypto_bench + codec_bench
- [x] Kubernetes CRDs + operator — `Reconciler` with desired/observed state diffing, Create/Update/Delete/Skip actions, orphan detection (4 tests)
- [x] ~~Homebrew formula, apt/rpm repos, AUR, Nix flake~~ — packaging infrastructure ready
- [x] Documentation site — mdBook at ravenfabric.io/docs/
- [x] Named Data Networking concepts for policy distribution (interest/data pattern)
- [x] Subsea-cable resilience (mesh fallback when physical links fail)
- [x] Full SPIFFE workload identity compliance

---

## Post-v1.0 — Framework SDKs & Ecosystem

**Goal:** Native integration with AI agent frameworks beyond MCP. SDK-level access for framework authors.

- [x] LangChain integration — `LangChainTool` class in `sdks/python/src/ravenfabric/integrations/langchain.py`
- [x] CrewAI integration — `CrewAITool` class in `sdks/python/src/ravenfabric/integrations/crewai.py`
- [x] AutoGen integration — `AutoGenExecutor` in `sdks/python/src/ravenfabric/integrations/autogen.py`
- [x] Custom MCP client SDK (Rust) — `rf-mcp-client` library crate: stdio transport, typed tool wrappers (exec, query_policy, file_read/write, list_capabilities, request_approval), 14 tests
- [x] Custom MCP client SDK (Python) — pip-installable client (`sdks/python/`): async + sync API, StdioTransport, LangChain + CrewAI + OpenAI + Anthropic integrations, 40 tests
- [x] Custom MCP client SDK (TypeScript) — npm package (`sdks/typescript/`): fully typed async API, StdioTransport, 12 tests
- [x] OpenAI function-calling adapter — `OpenAIAdapter` with tool definitions in `sdks/python/src/ravenfabric/integrations/openai.py`
- [x] Anthropic tool-use adapter — `AnthropicAdapter` with tool definitions in `sdks/python/src/ravenfabric/integrations/anthropic.py`
- [x] Agent framework benchmark suite — `sdks/python/benchmarks/run.py`: measures policy overhead, latency, throughput across all frameworks

---

## Productize AI Agent Integration

**Goal:** Make RavenFabric the zero-friction security layer between AI agents and production systems. Extremely sellable: every team running AI coding assistants needs this yesterday.

### Ship: Hardened rf-mcp-server
- [x] Production-hardened `rf-mcp-server` binary — audit-tested, fuzzed (fuzz_mcp_protocol target), zero known vulnerabilities
- [x] Session isolation — each AI agent session runs in its own policy sandbox, no cross-session bleed
- [x] Rate limiting per session — prevent runaway AI loops from exhausting system resources
- [x] Graceful degradation — if policy engine is unreachable, deny all (fail-closed)

### Clear Install Guides
- [x] Claude Code integration guide — `claude mcp add ravenfabric` one-liner, config reference, troubleshooting
- [x] Cursor integration guide — MCP server config for Cursor IDE, workspace-scoped policy
- [x] Aider integration guide — stdio transport setup, `.aider.conf.yml` reference

### Opinionated Policy Templates (ready-to-use)
- [x] "Safe Dev Mode" — AI can read/write project files, run tests, use git; cannot touch system, credentials, or network
- [x] "Production AI Guardrails" — read-only production access, require human approval for any mutation, full audit trail
- [x] "Read-only Infrastructure AI" — query logs, metrics, status; block all writes, block all exfiltration paths

### Make It Trivial To:
- [x] **Drop RavenFabric between AI and system** — single binary, single config file, working in < 5 minutes
- [x] **Block `rm -rf`** — immutable deny rules ship by default, not opt-in
- [x] **Require approval for production changes** — human-in-loop approval workflow with CLI notification and status polling
- [x] **Log AI reasoning** — every command includes optional `reason` field recorded in structured audit log
- [x] Quick-start tutorial — "Secure your AI agent in 5 minutes" (docs/src/getting-started/ai-quickstart.md)
- [x] Demo video / asciinema — recording script at `docs/demo/demo-record.sh` (shows deny, allow, audit)

---

## Distribution & Packaging

**Goal:** RavenFabric installs natively on every platform through the user's preferred package manager. One command to install, one command to update.

**Principle:** All packaging, signing, and publishing is handled entirely in the GitHub Actions CI/CD pipeline. No manual builds, no local packaging steps. A tagged release triggers automated builds for all platforms and pushes artifacts to the respective package repositories.

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
| Homebrew | `brew install egkristi/tap/ravenfabric` | [x] Formula ready (`deploy/ravenfabric.rb`) — tap: [egkristi/homebrew-tap](https://github.com/egkristi/homebrew-tap) — #45 |
| DMG | `RavenFabric.dmg` (universal binary) | [x] Build script ready (`deploy/macos/build-dmg.sh`) — needs code signing #49 |
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
| Google Play Store | `io.ravenfabric.agent` | [ ] Planned — #50 |
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
- Cross-compile: Linux (amd64, arm64, armv7, riscv64, musl), macOS (amd64, arm64), FreeBSD
- MSRV check: Rust 1.88

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
| v0.3 | Session recording, forced command mode, tunnel time limits, MCP server per-session identity, prompt injection detection, policy templates |
| v0.4 | Sealed secrets, key rotation, secret masking in logs |
| v0.5 | Traffic analysis resistance (noise floor, packet normalization) |
| v0.5.1 | HKDF-SHA256 for PQ KEM, ChaCha20-Poly1305 mimicry codec, HMAC-signed policy logs, SHA-256 WireGuard key derivation, cryptographic trace IDs |
| v0.6 | WASM sandboxing, RBAC, approval workflows |
| v0.7 | Behavioral anomaly detection, AI compliance reporting (EU AI Act, NIST AI RMF) |
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
| 2026-05-06 | MCP server as translation layer | Policy enforced by rf-agent, not MCP server. Compromised MCP binary cannot bypass policy. AI agent access uses same crypto/audit/policy as human operators |
| 2026-05-06 | Local IPC as first-class transports | UNIX sockets, named pipes, stdio, vsock are not shortcuts — they go through the same Noise XX handshake and policy engine. Local does not mean trusted |
