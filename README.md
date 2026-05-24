# RavenFabric

> Security-first distributed execution engine. Network-agnostic, E2E encrypted, policy-driven, ZTNA.
> From full mesh VPN, fire-and-forget commands to declarative desired state — all within an airtight policy layer.

**Status: Alpha (v0.25.1)** — Foundation complete. 14 crates, ~72,767 LOC, 1,432 tests. E2E encrypted execution, 30+ transport drivers, deny-by-default policy.

[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSES/AGPLv3.txt)
[![Version](https://img.shields.io/badge/version-0.25.1-green.svg)](https://github.com/egkristi/RavenFabric-Published/releases/latest)

**Language:** Rust | **License:** AGPL-3.0-or-later (core) + Commercial (enterprise)

---

## What is RavenFabric?

RavenFabric is a **universal agent** that provides **secure, policy-controlled access to any system** — regardless of network topology, operating system, or device class. It unifies mesh VPN, remote execution, configuration management, and zero-trust access into a single binary with no runtime dependencies.

It runs on servers, desktops, laptops, Raspberry Pis, Android phones, iPhones, routers, IoT sensors, edge appliances, satellites, and anything else that can run compiled code. If it has a CPU, it can be a RavenFabric node.

It is not a task runner with security added. It is a **security-first policy engine** that also executes tasks:

```text
❌ Other tools:  powerful execution → bolt on security controls
✅ RavenFabric:  airtight policy → execution within its bounds only
```

### The Full Spectrum

```text
NETWORK ACCESS ◄──────────────────────────────────────────────────────────────► DECLARATIVE STATE

VPN tunnel → port-forward → shell → fire-and-forget → task → playbook → desired-state
    │            │           │            │              │         │            │
  Full L3     Per-port    Interactive   No reply      Ordered   Multi-agent   Converge
  mesh        policy      session       needed        steps     w/ rollback   + drift detect

                        ┌─────────────────────────────────────────┐
                        │  DATA COLLECTION (always-on, parallel)  │
                        │  metrics · logs · traces · health       │
                        └─────────────────────────────────────────┘
```

All modes: same policy engine. Same audit log. Same E2E encryption. Same data pipeline. One agent.

---

## Replaces

| Category | Products replaced |
|----------|-----------------|
| **Mesh VPN** | Tailscale, Headscale, NetBird, ZeroTier, SoftEther |
| **ZTNA / Access Proxy** | Twingate, Pomerium, Cloudflare Access, Zscaler Private Access |
| **Remote Execution** | Ansible, Salt, Puppet Bolt |
| **Config Management** | Salt states, Puppet manifests, Chef recipes |
| **Secure Shell** | SSH + bastion hosts, Teleport, Boundary |
| **Port Forwarding** | ssh -L/-R/-D, ngrok, frp |
| **File Transfer** | scp, rsync over SSH |
| **Data Collection** | Telegraf, Metricbeat, OpenTelemetry Collector, collectd, Prometheus node_exporter, Datadog Agent, Vector, Fluent Bit |

---

## Security Architecture

```text
Orchestrator (CLI / API / Operator)
  │
  ▼
ExecutionController
  ├── Check SecurityPolicy: who can do this? blast radius?
  ├── Check RPCPolicy: are all steps allowed on all agents? (pre-flight)
  └── Reject entire execution if any check fails — no partial execution

  │ Noise XX E2E encrypted (relay sees only random bytes)
  ▼

Agent (target system — final authority)
  ├── Re-check RPCPolicy locally (agent cannot be overridden)
  ├── Execute within resource limits (CPU%, memory, output, timeout)
  ├── Write structured audit log entry
  └── Return result encrypted
```

**Two policy checks:** controller pre-flight + agent local. The agent is always the final authority. A compromised orchestrator cannot override agent policy.

### Encryption

All communication uses **Noise XX** (same cryptographic core as WireGuard):

```text
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

- Mutual authentication (both sides verify each other)
- Per-session forward secrecy (ephemeral keys)
- Relay-opaque: a relay sees only random-looking bytes — no keys, no content, no identity

---

## Execution Modes

### Single command

```bash
# Fire and forget (no output needed)
rf exec prod-server-1 "touch /workspace/.heartbeat"

# Fire and verify (wait for result)
rf exec prod-server-1 "git -C /workspace pull --rebase origin main"
```

### Playbook (multi-agent orchestration with rollback)

```yaml
# playbook.yaml — loaded by `rf playbook plan.yaml --token <token>`
command: "systemctl restart nginx"
target:
  agents: ["web-01", "web-02", "web-03"]
strategy:
  rolling:
    batch_percent: 33
on_failure:
  rollback:
    command: "systemctl restart nginx-old"
timeout_secs: 60
```

### Desired State (declarative convergence + drift detection)

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: web-server-baseline
spec:
  packages:
    - name: nginx
      state: installed
      version: ">=1.24.0"
    - name: telnet
      state: absent

  files:
    - path: /etc/nginx/nginx.conf
      content: |
        worker_processes auto;
      mode: "0644"
      owner: root

  services:
    - name: nginx
      state: running
      enabled: true
    - name: telnetd
      state: stopped
      enabled: false

  sysctl:
    - key: net.ipv4.ip_forward
      value: "0"

  convergence:
    mode: remediate       # report | remediate
    interval_seconds: 300
```

---

## Policy (Security First)

### RPCPolicy — commands, filesystem, network, HTTP, resources

```yaml
spec:
  commands:
    allow:
      - pattern: "^git -C /workspace .*"
      - pattern: "^npm (ci|test|run build|run lint)$"
      - pattern: "^systemctl (status|restart|reload) (nginx|myapp)$"
    deny:
      - pattern: ".*secret.*"
      - pattern: "^rm.*-rf"
      - pattern: "^sudo .*"
  filesystem:
    allow:
      - path: /workspace
      - path: /var/log
    deny:
      - path: /etc/shadow
      - path: /root
  http:
    allow:
      - method: "GET"
        path: "^/api/.*"
      - method: "POST"
        path: "^/api/users$"
      - path: "^/health$"
    deny:
      - method: "DELETE"
        path: "^/api/admin.*"
      - path: "^/internal/.*"
  resources:
    maxOutputBytes: 10485760  # 10MB
    timeoutSeconds: 300
    maxRequestBodyBytes: 10485760
    maxResponseBodyBytes: 10485760
```

### SecurityPolicy — immutable rules

The `SecurityPolicy` enforces rules that cannot be overridden by any RBAC role, capability token, or policy merge:

- Immutable deny patterns (rm -rf, mkfs, dd, fork bomb) — always blocked
- Maximum delegation depth for capability tokens
- Minimum roles required for policy changes
- Post-quantum crypto requirements

---

## Transport: Any Network

RavenFabric connects through any available transport. All agents initiate outbound — no inbound ports required anywhere. The Driver trait abstracts all connectivity — upper layers never know which transport is active.

### Transport Drivers

| Transport | NAT traversal | Needs internet | Use case |
|-----------|-------------|---------------|----------|
| `wireguard-direct` | Open network | Yes | Lowest latency, direct peers |
| `wireguard-stun` | Cone NAT | Yes | Home/office (STUN-assisted hole punch) |
| `wireguard-relay` | All NAT | Yes | Universal fallback (TURN-style) |
| `quic` | All NAT | Yes | Connection migration, 0-RTT, multiplexed |
| `websocket` port 443 | All NAT | Yes | Works everywhere, corporate proxies |
| `websocket-proxy` | Enterprise proxy | Yes | HTTP CONNECT through corporate proxies |
| `http3-masque` | All NAT | Yes | CONNECT-UDP/IP via HTTP/3, impossible to block |
| `reticulum` | No internet | No | Air-gap, LoRa, BLE, packet radio |
| `tor` | All | Yes | Anonymity, .onion hidden service |
| `dns-tunnel` | All | Yes | Extreme fallback through restrictive firewalls |
| `serial` | Physical | No | True air-gap (RS-232/USB) |
| `bluetooth` | Physical proximity | No | Local mesh, no infrastructure |

All transports: **Noise XX on top, always.** Relay sees only ciphertext.

### NAT Traversal Stack

RavenFabric implements a full NAT traversal stack (inspired by ICE, libp2p DCUtR, and Tailscale DERP):

```text
┌─────────────────────────────────────────────────────────────────┐
│  1. Try direct (host candidates, IPv6, LAN)                     │
│  2. STUN: discover reflexive address (server-reflexive candidates)│
│  3. UDP hole punch (simultaneous send via coordinator)          │
│  4. TCP hole punch (simultaneous open, RFC 5128)                │
│  5. Birthday attack on symmetric NAT (port prediction)          │
│  6. Relay fallback (TURN-style, always works)                   │
└─────────────────────────────────────────────────────────────────┘
```

- **STUN** — discover public IP:port mapping. Works for ~80% of NATs (full cone, restricted, port-restricted)
- **UDP hole punching** — coordinated simultaneous send. Both NATs create entries allowing bidirectional traffic
- **TCP hole punching** — simultaneous open (RFC 5128). Harder but works through some NATs that block UDP
- **Birthday paradox attack** — for symmetric NAT: predict port allocation by sending many packets. ~65% success rate
- **Relay (TURN-style)** — when all else fails: encrypted relay forwards opaque bytes. Works 100%, costs latency
- **ICE-style orchestration** — try all candidates in parallel, select fastest successful path

### Connection Lifecycle (DCUtR Pattern)

Inspired by libp2p's DCUtR (Direct Connection Upgrade through Relay):

```text
1. Agent connects to relay (immediate, always works)
2. Client connects to relay, paired with agent
3. Noise XX handshake through relay (E2E, relay sees nothing)
4. Connection is functional — commands can execute NOW
5. Background: probe for direct path (STUN, hole punch, WireGuard)
6. If direct path found → verify peer key → migrate seamlessly
7. Old relay path kept as fallback (automatic failback)
```

Key insight: **never wait for optimal path**. Use relay immediately, upgrade in background.

### Background Transport Upgrade

Unlike Tailscale (DERP→WireGuard only), RavenFabric can upgrade **across protocol families**:

1. Connect via fastest available (often relay/WebSocket)
2. Race all higher-priority transports in background
3. When faster transport succeeds → offer upgrade via channel
4. Verify peer key matches → accept, old transport kept as fallback
5. Supports multipath: use multiple transports simultaneously for redundancy

### IPv6 and Dual-Stack (Happy Eyeballs)

- **IPv6 first** — if both peers have IPv6, NAT traversal is trivial (no NAT)
- **Happy Eyeballs (RFC 8305)** — race IPv4 and IPv6 in parallel, use first responder
- **NAT64/464XLAT awareness** — detect IPv6-only networks (common on mobile), behave correctly
- **Dual-stack candidates** — generate both IPv4 and IPv6 candidates for ICE-style negotiation

### Network Environment Probing

Transport-agnostic network probing (inspired by Tailscale's netcheck but probes ALL drivers):

- NAT type detection (open, full cone, restricted, port restricted, symmetric)
- IPv4/IPv6 availability and preference
- UDP reachability (per-port)
- Per-relay latency measurement (geographic selection)
- Captive portal detection
- HTTP proxy detection (CONNECT support)
- Per-driver availability probes
- QUIC/UDP vs TCP-only detection

### Censorship Resistance (Pluggable Transports)

For hostile network environments (corporate DPI, nation-state censorship):

| Technique | How it works | Blocks it defeats |
|-----------|-------------|-------------------|
| **WebSocket on 443** | Looks like HTTPS | Basic port blocking |
| **HTTP/3 MASQUE** | CONNECT-UDP inside QUIC/HTTP/3 | DPI, protocol blocking |
| **ECH (Encrypted Client Hello)** | Hides SNI from DPI | SNI-based blocking |
| **Traffic obfuscation** | Make Noise XX look like random bytes | Protocol fingerprinting |
| **Domain fronting** | TLS SNI ≠ HTTP Host header | Domain-based blocking |
| **DNS tunneling** | Encode data in DNS queries | Everything except DNS blocking |
| **ICMP tunneling** | Data in ICMP echo payloads | Firewalls allowing ping |

Architecture: transports are pluggable. Adding a new censorship-resistant transport requires only implementing the `Driver` trait — no changes to crypto, RPC, policy, or execution layers.

### Peer Discovery

| Method | Scope | Decentralized | Use case |
|--------|-------|---------------|----------|
| **mDNS/DNS-SD** | LAN | Yes | Local network, zero-config |
| **Relay registry** | Global | No (relay-mediated) | Primary, always works |
| **DHT (Kademlia)** | Global | Yes | Censorship-resistant discovery |
| **Gossip (SWIM/HyParView)** | Mesh | Yes | Self-healing mesh topology |
| **Signed DNS records** | Global | Partial | Verifiable, cached |
| **Bluetooth/BLE beacon** | Proximity | Yes | No-infrastructure environments |

All discovery methods produce the same output: a signed peer record containing the peer's static public key and reachable addresses. The key is the identity — addresses are just hints.

### Fallback Strategies

| Strategy | Behavior | Use case |
|----------|----------|----------|
| `sequential` | Try one-by-one, stop at first success | Battery-optimized |
| `race` | Start all concurrently, use first to succeed | Latency-optimized |
| `parallel` | Establish all, use lowest-latency | Mission-critical |
| `multipath` | Keep multiple active, stripe or replicate | Ultra-reliable |

### Connectivity Value Chain

The full lifecycle — from identity genesis through session establishment to graceful teardown — is documented in [CONNECTIVITY.md](CONNECTIVITY.md). It covers 13 phases:

```text
Identity → Enrollment → Discovery → Rendezvous → NAT Assessment →
Path Selection → Tunnel Establishment → Broker Decision →
Crypto Handshake → Session → Path Upgrade → Health Monitoring → Teardown
```

### Health Monitoring & Failover

Active paths are continuously monitored:

| Indicator | Threshold |
|-----------|-----------|
| Round-trip time | > 2x baseline = degraded |
| Packet loss | > 1% sustained = degraded |
| Heartbeat miss | 3 consecutive = failed |
| Network change (OS event) | Re-probe all drivers |
| MAC verification failure | 1 = tamper alert, immediate migration |
| Unexpected frame injection | 1 = tamper alert, immediate migration |
| Protocol fingerprint anomaly | Sustained = DPI/MITM suspected |

Failover is automatic: if the active path degrades, a secondary path is promoted (or racing begins with relay as bridge). See [CONNECTIVITY.md](CONNECTIVITY.md) Phase 11.

### Tamper Detection & Adaptive Transport

If tampering or interference is detected on any connection, the agent **autonomously adapts**:

```text
Detection triggers:
  • Noise MAC verification failure (modified ciphertext)
  • Unexpected bytes outside framing (injection attempt)
  • Sudden latency spikes consistent with MITM interception
  • TLS fingerprint mismatch (protocol downgrade attack)
  • Repeated handshake failures on previously-working path

Response (automatic, no operator intervention required):
  1. Mark current path as COMPROMISED in path table
  2. Immediately migrate session to next-best available transport
  3. Emit tamper-alert audit event (signed, timestamped)
  4. Begin racing alternative transports (escalate tier if needed)
  5. If all internet paths compromised → fall back to mesh/DTN/physical
  6. Optionally escalate to censorship-resistant transport (obfs4, MASQUE)
  7. Never retry compromised path without operator acknowledgment
```

The session continues uninterrupted on the new path. The application layer never sees the migration — only the audit log records what happened.

### Connection Metrics & Monitoring (DTN-aware)

Connection health metrics are first-class data that propagates through the **same fabric** as commands — including mesh hops and DTN paths:

| Metric | Collected at | Propagation |
|--------|-------------|-------------|
| RTT per path | Each agent | Real-time (connected) or batched (DTN) |
| Packet loss rate | Each agent | Aggregated per reporting interval |
| Transport type active | Each agent | Included in heartbeat |
| Handshake success/failure count | Each agent | Audit event + metric |
| Tamper events | Each agent | Priority DTN delivery (never dropped) |
| Throughput (bytes/sec) | Each agent | Sampled, low-overhead |
| Path switch events | Each agent | Audit event + metric |
| Relay hop count | Relay | Forwarded with metadata |
| DTN custody transfers | Each hop | Propagated with bundle |
| Mesh neighbor table | Mesh nodes | Gossiped periodically |

**DTN-specific monitoring:**

- Metrics are bundled as DTN payloads with custody transfer — guaranteed delivery even over days
- Priority: tamper alerts > health metrics > throughput stats
- Offline agents accumulate metrics locally, flush on next contact window
- Mesh nodes gossip neighbor health — partial observability even without direct path to controller
- No node is ever a monitoring blind spot: if it can receive commands, it can emit telemetry

### Reconnect Strategy

| Strategy | Behavior |
|----------|----------|
| Immediate retry | Transient glitch |
| Exponential backoff + jitter | Standard (1s → 60s max) |
| Network-aware | Wait for OS network event (mobile, lid-close) |
| Scheduled | Air-gap rendezvous windows |

### Global Fleet: Region-Aware Relay Selection

RavenFabric agents automatically select the nearest relay cluster using geographic scoring, avoiding the need for manual relay URL management in multi-region deployments.

**Relay cluster configuration** in `raven.toml`:

```toml
[agent]
region = "eu-west"

[[transport.relay_clusters]]
region    = "eu-west"
continent = "EU"
latitude  = 51.5
longitude = -0.1
relays    = ["wss://eu1.relay.example.com:9090", "wss://eu2.relay.example.com:9090"]

[[transport.relay_clusters]]
region    = "us-east"
continent = "NA"
latitude  = 40.7
longitude = -74.0
relays    = ["wss://us1.relay.example.com:9090"]
```

- **Automatic selection** — the agent scores clusters by continent + Haversine distance and RTT, picks the best relay at startup.
- **Cross-region forwarding** — relay brokers can forward to peer regions using the `FORWARD:<url>|<inner_token>` protocol. The relay never decrypts the Noise payload.
- **Deny-by-default forwarding** — `ForwardConfig { allow_forwarding: false }` by default. Enable explicitly with an optional allowlist.
- **Region-aware orchestration** — set `[agent] region = "eu-west"` and the controller's `AgentRegistry::select_by_region()` filters fleet commands by region.

### Transport Philosophy

> Any channel that can move signed bytes is a valid transport.

"Transport" is not limited to TCP/IP. RavenFabric defines transport as any medium capable of carrying authenticated, encrypted frames — including physical media:

| Class | Examples | Bandwidth | Latency |
|-------|----------|-----------|---------|
| **Internet** | WireGuard, QUIC, WebSocket, HTTP/3 MASQUE | Gbps | ms |
| **Overlay mesh** | Yggdrasil, I2P, Tor, Veilid | Mbps | 100ms+ |
| **Radio** | LoRa/Meshtastic, AX.25 packet radio, HF/Winlink | bps–kbps | seconds–hours |
| **Proximity** | Bluetooth, Wi-Fi Direct, NFC | kbps–Mbps | ms |
| **Satellite** | Starlink, Iridium | Mbps/kbps | 20ms–2000ms |
| **Physical** | USB/serial, SD card, sneakernet (NNCP-style) | Variable | hours–days |
| **Extreme** | DNS tunneling, ICMP, audio modem, QR-stream | bits–kbps | seconds |

**Architectural invariant:** The policy layer, execution layer, and application code never know which transport is active. A command that executes over WireGuard has identical semantics to one that arrives via LoRa mesh or USB stick.

### Delay-Tolerant Networking (DTN)

Inspired by NASA's Bundle Protocol (RFC 9171) and NNCP, RavenFabric treats disconnection as normal:

```text
┌──────────────────────────────────────────────────────────────────────┐
│  STORE-CARRY-FORWARD                                                 │
│                                                                      │
│  Command signed → policy pre-validated → custody transferred →       │
│  stored on intermediate nodes → carried (physically if needed) →     │
│  forwarded when path opens → executed on arrival                     │
│                                                                      │
│  Every hop takes custody: responsible for delivery until next hop     │
│  accepts. No data lost due to transient disconnection.               │
└──────────────────────────────────────────────────────────────────────┘
```

- **Schedule-aware routing** — agents know contact windows ("satellite pass at 14:32, 6 minutes of connectivity")
- **Custody transfer** — each intermediate node accepts responsibility for delivery
- **Opportunistic sync** — when nodes meet (BLE, Wi-Fi, physical), they exchange queued messages
- **TTL and priority** — stale commands expire; urgent commands route preferentially
- **Idempotency** — duplicate delivery (via multiple paths) is safe

Use cases: fishing boats, oil platforms, remote cabins, military forward positions, industrial air-gaps — anywhere connectivity is intermittent or non-existent.

### Transport-Aware Policy

The policy engine considers transport characteristics when making decisions. Sensitive commands may require stronger transports, while routine telemetry can use any available channel.

### Cryptographic Identity

Identity in RavenFabric is independent of network position (inspired by Reticulum and SPIFFE):

```text
Identity = SHA-256(public_key)[0..16]    # 128-bit cryptographic address
```

- **Address = key hash** — no DHCP, no DNS required for identity. Inspired by Reticulum's destination addressing.
- **IP is a routing hint** — not identity. Agent can change IP, network, transport — identity persists.
- **TOFU + OTP enrollment** — first contact bootstrapped via one-time token, then TOFU for all future connections.
- **Post-quantum hybrid** — ML-KEM + X25519 for harvest-now-decrypt-later resistance (`HybridKemContext` + `PqxdhRatchet`).
- **Petname system** — agents are locally named (`web-01`) mapping to cryptographic identifiers. No global namespace required.

---

## Current Implementation Status

**~72,767 LOC | 1,432 tests | 0 clippy warnings**

What works today:

- Noise XX mutual authentication handshake with wire magic/version validation (full)
- Secure channel with encrypted frames using `StatelessTransportState` for concurrent send/recv (full)
- Static key management with zeroing-on-drop, cross-platform (full)
- Post-quantum hybrid KEM with HKDF-SHA256 key combination (`HybridKemContext` + `PqxdhRatchet` double ratchet)
- OTP token generation and validation with poisoning-safe locks (full)
- Policy engine with deny-by-default, regex allow/deny, path checks, symlink resolution (full)
- CRDT-based policy convergence (`GSet`, `LwwRegister`, `OrSet`, `PolicyCrdt`)
- Append-only policy log with SHA-256 hash chain + HMAC-SHA256 signature verification
- SIGHUP-triggered hot-reload of policy (atomic swap via RwLock)
- Multi-tenant isolation with cross-tenant blocking
- SecurityPolicy with immutable deny rules (rm -rf, mkfs, dd, fork bomb)
- Capability-based auth tokens with delegation and attenuation
- Policy templates library (8 templates: coding-assistant, production-read-only, security-investigator, ci-cd-agent, database-query, safe-dev-mode, production-ai-guardrails, read-only-infrastructure-ai)
- Policy template CLI (`rf policy list/show/validate/compose`) with deny-wins composition
- Prompt injection detection (base64/hex detection, homoglyphs, shell evasion, injection markers, exfiltration)
- Command execution under policy control with timeout and output limiting (full, tested)
- Structured JSON-lines audit logging (full)
- Msgpack RPC codec with length-prefixed framing and roundtrip tests (full)
- RPC session over encrypted SecureChannel (full, tested end-to-end)
- REST API dispatcher with role-based access control
- OpenTelemetry trace context (W3C traceparent) with OTLP JSON span export
- Kubernetes-style reconciler with desired/observed state diffing
- In-memory transport driver for testing (full, tested)
- WebSocket transport driver with DuplexStream bridge (full)
- QUIC transport driver (quinn, 0-RTT, connection migration, multiplexed streams)
- UNIX domain socket transport driver with peer credential verification and stale socket removal
- Stdio pipe transport driver for parent-child process communication (MCP stdio transport, embedded agents)
- WireGuard userspace tunnel (UDP socket, key handling, peer management)
- DNS tunnel codec (base32/hex encoding, query fragmentation, response decoding)
- ICMP tunnel framer (echo request framing, serialize/deserialize, session multiplexing)
- Serial port framer (sync bytes, CRC-16/CCITT, frame detection)
- Domain fronting (SNI/Host rewriting, tunnel request generation, response parsing)
- Protocol mimicry (Shadowsocks-style ChaCha20-Poly1305 AEAD framing with counter-derived nonces)
- Real STUN client (RFC 5389/8489 binding requests, NAT type detection, ICE candidate gathering)
- Relay broker with meet-token pairing over WebSocket (full)
- Per-IP rate limiting on relay (sliding window, 20 conn/min default)
- HMAC-SHA256 meet token authentication (optional, via `--secret` / `RELAY_SECRET`)
- Agent binary: connects to relay, performs handshake, runs RPC loop (full)
- Agent reconnect with exponential backoff + jitter
- Prometheus `/metrics` HTTP endpoint with `--metrics-addr` agent flag
- CLI `rf exec` command: connect, handshake, send, display result, close-notify (full)
- CLI `rf status` command: connect to agent, display version/uptime, close-notify (full)
- CLI `rf dev` mode: local relay + agent in one process (full)
- Shell completions (bash, zsh, fish)
- Linux musl static binaries (amd64 + arm64) via release workflow
- System metrics collection via sysinfo (CPU, memory, load, disk)
- Health check probes: TCP connect, HTTP GET, process alive, command exit code
- Log file tailing with rotation detection, JSON/logfmt parsing, filters
- Local TCP port forwarding (ssh -L equivalent) with bidirectional copy + RPC integration
- Remote port forwarding (ssh -R equivalent) — agent-side listener, bidirectional relay
- SOCKS5 dynamic forward proxy on agent — full protocol, policy-checked, bidirectional relay
- `rf forward -L` CLI command (connect, request forward on agent, keep alive until Ctrl+C)
- Bulk file transfer: `rf cp` — chunked upload/download with SHA-256 verification, atomic writes, resumable transfers
- TCP proxy tunneling: `rf proxy` — local listener tunnels through agent to target host:port, policy-enforced
- HTTP-aware proxy: `rf proxy --http` — per-request method+path policy enforcement, audit logging, body size limits
- PTY allocation on Unix (real openpty, shell spawn, resize, signal) + RPC Shell actions
- `rf shell` interactive terminal: raw mode, bidirectional stdin/stdout over encrypted channel
- Multi-agent orchestration via `rf playbook` (rolling, canary, parallel strategies with rollback)
- Happy Eyeballs (RFC 8305) dual-stack connection racing with staggered starts
- ConnectionManager with relay-first + background direct path upgrade (tested with 6 async tests)
- Session migration (make-before-break) with peer key verification and automatic rollback
- Sealed secret store (ChaCha20-Poly1305) with `{{ secrets.KEY }}` template resolution in commands
- Secret rotation: configurable TTL, rotation hooks, grace period, health-check before retirement
- Fleet-wide secret push (`rf secret push`): SealSecret RPC seals over Noise channel, zero-downtime rotation via grace period
- Secret enumeration (`rf secret list`): ListSecrets RPC returns names only — plaintext never returned
- External secret backends: HashiCorp Vault (AppRole/Token), AWS Secrets Manager (SigV4), Azure Key Vault (OAuth2), GCP Secret Manager, Generic HTTP — with background sync (source-of-truth pull mode)
- Policy-gated MCP tools: `allowed_tools` per caller profile restricts `tools/list` and `tools/call` to permitted subset
- SIEM integrations: Syslog RFC 5424, CEF, LEEF, OCSF, Splunk HEC, Elasticsearch/OpenSearch, Datadog log forwarding
- Buffered audit collector: in-memory ring buffer, sliding-window deduplication, age-based retention, background flush
- Real-time alert rules: pattern matching on audit events, Slack/PagerDuty/OpsGenie/webhook destinations, deduplication
- DTN offline queue with SQLite persistence (priority ordering, TTL, deduplication)
- TUN device creation: Linux (/dev/net/tun + ioctl), macOS (utun control socket), platform-agnostic API
- MagicDNS UDP server: AAAA query resolution for `*.rf.local`, authoritative responses, NXDOMAIN
- Per-relay latency probing: TCP connect RTT measurement, continuous probing loop with cancellation
- UDP hole punching: real socket coordination with probe/ACK protocol, concurrent punch
- OS network change detection: polling-based watcher, snapshot diff, platform gateway detection
- mDNS/DNS-SD LAN discovery: UDP broadcast/listen, JSON announcement protocol, self-filtering
- Gossip protocol: SWIM-style UDP gossip with transitive health propagation
- 0-RTT session resumption: ZeroRttCache with ticket store, use-count replay protection, eviction
- Corporate proxy detection: HTTP CONNECT probing, auth detection (407), TCP RTT measurement
- Collection policy: include/exclude glob patterns, label filters, sampling rate, batch limiting
- Offline telemetry buffering: MetricBuffer with overflow, batch flush, drop counter
- MCP server (`rf-mcp-server`): 10 tools (exec, query policy, file read/write, file transfer, http request, capabilities, audit query, approval request/check), API token auth, rate limiting, anomaly detection, RBAC per caller
- Named pipe transport driver for Windows IPC (`\\.\pipe\ravenfabric`)
- Vsock transport driver for VM-to-hypervisor communication (Firecracker, QEMU)
- Abstract namespace socket driver (Linux-only, no filesystem cleanup)
- Auto-select transport driver (probes available transports, selects best)
- Socket activation (systemd-style LISTEN_FDS protocol support)
- File-descriptor passing (SCM_RIGHTS) for zero-copy session handoff over UNIX sockets
- Behavioral anomaly detection: velocity, novelty, timing, escalation scoring per identity
- AI compliance reporting: EU AI Act risk classification, NIST AI RMF mapping, audit export
- Embedded Web UI dashboard: real-time agent metrics, activity feed, connected agents
- HTTP/3 MASQUE transport (RFC 9297/9298): capsule encoding, CONNECT-UDP/IP, varint framing
- Encrypted Client Hello (ECH): RFC 9460 config parsing, HPKE suites, GREASE fallback
- MCP client SDK (`rf-mcp-client`): stdio transport, typed wrappers for all RavenFabric tools
- Single-threaded async runtime (`rt-single-thread` feature) for constrained IoT devices

Working end-to-end flows:

- `rf exec --token <token> "command"` → relay → agent → execute → respond
- `rf --connect ws://host:port exec --token unused "command"` → agent (direct, no relay)
- `rf shell --token <token>` → relay → agent → PTY → interactive terminal
- `rf forward --token <token> -L 8080 -R db:5432` → relay → agent → TCP forward
- `rf playbook plan.yaml --token <token>` → multi-agent rolling deployment

See [ROADMAP.md](ROADMAP.md) for the full plan.

---

## AI Agent Integration

RavenFabric is purpose-built to be the **security layer between AI agents and production systems**. Any AI coding assistant, autonomous agent, or LLM-based tool that needs to execute commands, read files, or interact with infrastructure can do so safely through RavenFabric's policy engine.

### The Problem

AI agents (Claude Code, Cursor, Aider, Devin, custom GPT agents) need system access to be useful. But giving an AI agent unrestricted shell access is dangerous:

- No guardrails on destructive commands (`rm -rf /`, `DROP DATABASE`, credential exfiltration)
- No audit trail of what the AI did and why
- No blast radius control (one rogue loop can destroy everything)
- No human-in-the-loop for high-risk operations

### The Solution

RavenFabric sits between the AI agent and the system, enforcing policy on every action:

```text
AI Agent (Claude Code, Cursor, Aider, custom)
    │
    │ MCP / stdio / RPC
    ▼
┌─────────────────────────────────────────────┐
│  RavenFabric Policy Engine                  │
│  ┌─────────────────────────────────────┐    │
│  │ Immutable deny (rm -rf, mkfs, etc.) │◄── Cannot be overridden
│  ├─────────────────────────────────────┤    │
│  │ Template policy (safe-dev-mode, etc)│◄── Per-agent role
│  ├─────────────────────────────────────┤    │
│  │ Injection detection                 │◄── Prompt injection blocked
│  ├─────────────────────────────────────┤    │
│  │ Rate limiting per session           │◄── Runaway loop protection
│  └─────────────────────────────────────┘    │
│  Audit log ← every action + AI reasoning   │
└─────────────────────────────────────────────┘
    │
    ▼
  Target System (safe execution)
```

### What's Implemented Today

| Feature | Status | Description |
|---------|--------|-------------|
| **Stdio pipe transport** | Done | Parent-child process communication (MCP stdio protocol) |
| **MCP server binary** | Done | `rf-mcp-server` for native Claude/Cursor integration (10 tools, JSON-RPC 2.0) |
| **API token authentication** | Done | `--api-token` / `RF_API_TOKEN`, constant-time validation |
| **Per-session rate limiting** | Done | Sliding window throttle (`--rate-limit`, default 60/min) |
| **Session isolation** | Done | Unique session ID, process-level sandbox, fail-closed on policy error |
| **Anomaly detection + audit** | Done | Behavioral anomaly events enriched with baseline data, written to audit log |
| **Immutable deny rules** | Done | `rm -rf /`, `mkfs`, `dd if=/dev/zero`, fork bomb — cannot be overridden by any policy |
| **AI reasoning audit** | Done | Optional `reason` field on every request, recorded in structured audit log |
| **Injection detection** | Done | Base64/hex obfuscation, homoglyphs, shell evasion, exfiltration markers |
| **Policy templates** | Done | `safe-dev-mode`, `production-ai-guardrails`, `read-only-infrastructure-ai` |
| **Template CLI** | Done | `rf policy list/show/validate/compose` — inspect and compose policies |
| **Deny-by-default** | Done | Nothing executes without explicit allow rule |
| **Behavioral anomaly detection** | Done | Velocity, novelty, timing, and escalation anomaly scoring per identity |
| **AI compliance reporting** | Done | EU AI Act + NIST AI RMF compliance reports with human oversight tracking |
| **Embedded Web UI** | Done | Real-time agent dashboard (metrics, activity, connected agents) |
| **Integration guides** | Done | Claude Desktop, Claude Code, Cursor, Aider setup guides |
| **RBAC per caller** | Done | `--callers` TOML maps tokens to per-caller policy profiles |
| **Per-session crypto identity** | Done | Short-lived Curve25519 keypair per session for cryptographic correlation |
| **Token rotation** | Done | Comma-separated tokens for grace period, `--api-token-file` for external rotation |
| **Alert routing** | Done | `--alert-webhook` sends anomaly alerts to HTTP endpoint |
| **HTTP+SSE transport** | Done | `--http-listen` for multi-user server deployment (feature: `http-sse`) |
| **Human approval enforcement** | Done | `--approval-pattern` regex, SHA-256 command hash binding, one-time-use, 30-min TTL |

### Policy Templates for AI Agents

Three ready-to-use templates ship by default:

**Safe Dev Mode** — AI can develop freely within project boundaries:

```yaml
# AI can: read/write project files, run tests, use git, install deps
# AI cannot: touch system files, access credentials, modify network, sudo
spec:
  commands:
    allow:
      - pattern: "^(cat|head|tail|less|grep|find|ls|tree|wc|file|stat) .*"
      - pattern: "^git (status|diff|log|add|commit|push|pull|branch|checkout|stash).*"
      - pattern: "^(cargo|npm|pip|go|make) (build|test|run|install|fmt|clippy|lint).*"
    deny:
      - pattern: "^sudo .*"
      - pattern: ".*(/etc/|/root/|~/.ssh/).*"
```

**Production AI Guardrails** — read-only production access with approval for mutations:

```yaml
# AI can: query logs, metrics, status — observe everything
# AI cannot: modify anything without human approval
spec:
  commands:
    allow:
      - pattern: "^(systemctl status|journalctl|docker ps|kubectl get).*"
      - pattern: "^(cat|head|tail|grep) /var/log/.*"
    deny:
      - pattern: "^(systemctl (start|stop|restart)|docker (rm|stop)|kubectl delete).*"
```

**Read-Only Infrastructure AI** — zero mutation surface for investigation:

```yaml
# AI can: read logs, query metrics, check status
# AI cannot: write, modify, or execute anything that changes state
spec:
  commands:
    allow:
      - pattern: "^(cat|head|tail|less|grep|awk|sed|jq|yq) .*"
      - pattern: "^(ps|top|free|df|du|netstat|ss|ip|dig|nslookup|curl -s) .*"
    deny:
      - pattern: ".*"  # Deny everything else
```

### AI Reasoning in Audit Trail

Every command can include an optional `reason` field explaining why the AI performed the action. This creates a complete forensic trail:

```json
{
  "timestamp": "2026-05-06T11:00:00Z",
  "request_id": "ai-req-7f3a",
  "action": "execute",
  "command": "cargo test",
  "decision": "allowed",
  "matched_rule": "^cargo (build|test|run).*",
  "reason": "Running tests to verify the refactoring of auth module didn't break existing behavior",
  "caller_key": "a3f9b2c1..."
}
```

### Integration Paths

| AI Tool | Transport | Setup |
|---------|-----------|-------|
| **Claude Code** | MCP over stdio | `claude mcp add ravenfabric -- rf-mcp-server` |
| **Cursor** | MCP over stdio | Configure in `.cursor/mcp.json` |
| **Aider** | Stdio pipe | Configure in `.aider.conf.yml` |
| **Custom agents** | RPC over any transport | Use `rf-rpc` crate or CLI wrapper |
| **CI/CD pipelines** | CLI | `rf exec --token <token> --reason "CI deploy" "command"` |

### Security Guarantees for AI Agents

1. **Immutable deny** — catastrophic commands blocked regardless of policy configuration
2. **Injection detection** — prompt injection attempts in commands are caught and denied
3. **Rate limiting** — prevents runaway AI loops from exhausting resources
4. **Session isolation** — one AI session cannot interfere with another
5. **Audit everything** — complete record of what every AI did, when, and why
6. **Fail-closed** — if policy engine is unreachable, all actions denied
7. **No privilege escalation** — AI cannot grant itself more permissions
8. **Output bounded** — prevents memory exhaustion from unbounded output

---

---

## Grains: Agent Self-Reporting

Each agent reports facts about itself. Policy and playbooks can use them for targeting:

```yaml
# Auto-collected grains:
os: linux
distro: ubuntu
distro_version: "24.04"
cpu_count: 8
ram_gb: 16
ravenfabric_version: "0.5.0"
role: web-server              # Custom grain
environment: production       # Custom grain
```

Grains support label-matching (`matches_labels()`) for selective targeting in playbooks.

---

## Bootstrap: OTP Identity Enrollment

```text
1. Generate OTP token (programmatic or admin API)
   → Token: rf-otp-a3f9b2c1d4e5f6...

2. Token delivered out-of-band (SSH, cloud-init, etc.)

3. Agent generates Curve25519 key pair locally (private key never leaves)

4. Agent presents token to relay/bootstrap endpoint
   - Token validated: exists, not expired, not already used
   - Token marked as used (single-use, hash-stored)
   - Agent registered with public key

5. Agent stores identity + relay addresses
   All future connections use Noise XX + static key
   Bootstrap endpoint never used again
```

---

## Relay: Stateless Encrypted Broker

The relay is deliberately dumb. It only copies bytes. It sees nothing.

**What relay sees:**

```text
Frame:
┌──────────────┬──────────────────────────────────────┐
│  Length (4B) │  Noise ciphertext + 16B MAC          │
│  plaintext   │  (random-looking bytes)              │
└──────────────┴──────────────────────────────────────┘

Observable: approximate data volume, timing, IP addresses
NOT observable: command content, file content, agent identity, traffic type
```

- HMAC token auth for agent registration
- Per-IP rate limiting
- Channel-based agent/client pairing
- Meet tokens for rendezvous (one-time use, 256-bit)

---

## Why RavenFabric

| Capability gap | Existing tools need | RavenFabric |
|---|---|---|
| Execute commands on remote host | Tailscale + SSH + Ansible | Built-in |
| Encrypted mesh + config management | ZeroTier + Salt | Built-in |
| ZTNA + drift detection | Twingate + Puppet | Built-in |
| Air-gap remote execution | Manual + sneakernet | Built-in (DTN/serial) |
| Command-level policy (not just network ACL) | None available | Built-in |
| Metrics + logs from managed hosts | Telegraf + Fluent Bit + VPN | Built-in (same encrypted channel) |
| AI agent access control | None available | Built-in (MCP server + policy templates) |

**vs. Mesh VPN (Tailscale, Headscale, NetBird, ZeroTier):** These give encrypted connectivity — you still need SSH for commands, Ansible for automation, Telegraf for metrics. RavenFabric combines mesh VPN + execution + policy + data collection. Multi-transport (not locked to WireGuard), works through air-gaps.

**vs. ZTNA (Twingate, Pomerium, Cloudflare Access):** These control who can reach applications — glorified reverse proxies with identity. They can't execute commands, manage state, or work offline. RavenFabric provides application-level access plus command execution and config management.

**vs. Config Management (Ansible, Salt, Puppet):** These assume SSH/ZeroMQ connectivity exists. No answer for NAT, firewalls, or air-gaps. Security model is "whoever has SSH access can do anything." RavenFabric starts with connectivity solved, then layers policy-controlled execution.

**vs. Data Collection (Telegraf, OTEL Collector, Fluent Bit):** Each is another binary to deploy, configure, update, and secure on every host. RavenFabric's agent collects metrics, logs, and traces through the same encrypted channel, under the same policy controls.

---

## Architecture

### Layer Model

```text
┌─────────────────────────────────────────────────────┐
│  Layer 6: Interface                                 │
│  CLI · Web UI · API · Operator · SDK               │
├─────────────────────────────────────────────────────┤
│  Layer 5: Orchestration                             │
│  ExecutionController · PlaybookEngine ·             │
│  DesiredStateEngine · SessionManager                │
├─────────────────────────────────────────────────────┤
│  Layer 4: Execution (Agent-side)                    │
│  Executor · Resource plugins · MetricsCollector ·   │
│  ShellHandler · FileTransfer · TunnelManager        │
├─────────────────────────────────────────────────────┤
│  Layer 3: Policy (Agent — FINAL AUTHORITY)          │
│  SecurityPolicy · RPCPolicy · DesiredStatePolicy ·  │
│  NetworkPolicy · Grains · AuditLogger · Secrets     │
├─────────────────────────────────────────────────────┤
│  Layer 2: Crypto                                    │
│  Noise XX · Key management · SecureChannel ·        │
│  SealedSecrets · SessionKeys                        │
├─────────────────────────────────────────────────────┤
│  Layer 1: Connectivity                              │
│  Driver trait · Registry · Negotiator · Monitor ·   │
│  Upgrader · NetChecker · TUN device · DNS           │
└─────────────────────────────────────────────────────┘
```

### Binaries

| Binary | Role |
|--------|------|
| `rf` | CLI client — user interactions (`rf exec`, `rf dev`, `rf status`, `rf shell`, `rf forward`, `rf playbook`, `rf policy`, `rf cp`, `rf proxy`, `rf secret`) |
| `rf-agent` | Runs on target systems. Connects outbound, serves RPC under policy |
| `rf-relay` | Stateless encrypted broker. Pairs agents and clients. Geo-distributed |
| `rf-mcp-server` | MCP server for AI agents (Claude, Cursor, Aider). Policy-enforced tool execution |
| `rf-ingress` | HTTP ingress gateway. Routes external HTTP requests to registered agent upstreams via reverse proxy |

### Core Crates (Workspace)

| Crate | Responsibility | Status |
|-------|---------------|--------|
| `rf-crypto` | Noise XX handshake, SecureChannel, StaticKey, sealed secrets, 0-RTT resumption, post-quantum KEM, no_std frame_codec (WASM/bare-metal) | Done (~1,800 LOC, 42 tests) |
| `rf-transport` | Driver trait, WebSocket + QUIC + Memory + Named Pipe + Vsock + Abstract NS + Auto-select, ConnectionManager, proxy, latency, NAT/ICE, mesh, WireGuard, overlay networks, exotic/physical transports, LoRa, BLE, AX.25, satellite, mixnet, audio modem, QR-stream, socket activation, fd-passing, MASQUE, ECH | Done (~21,900 LOC, 542 tests) |
| `rf-mcp-client` | MCP client SDK — stdio transport, typed tool wrappers for exec/policy/files/capabilities | Done (~720 LOC, 14 tests) |
| `rf-rpc` | Request/Response types, Action enum, msgpack codec, yamux, heartbeat, DTN queue, SOCKS5, routing, controller/K8s, embedded Web UI | Done (~6,500 LOC, 118 tests) |
| `rf-audit` | Structured JSON-lines audit logging, CEF/LEEF/OCSF/Splunk HEC/Elasticsearch/Datadog audit destinations, buffered collector, AI compliance reporting (EU AI Act, NIST AI RMF), real-time alert rules with deduplication | Done (~2,415 LOC, 71 tests) |
| `rf-policy` | RPCPolicy enforcement, RBAC, collection policy, capability tokens, distributed CRDT policy, SPIFFE identity, behavioral anomaly detection, HTTP policy rules with header enforcement | Done (~5,500 LOC, 140 tests) |
| `rf-executor` | Command execution, file ops, streaming, orchestration, PTY, log tailing, metrics, WASM plugins, scraping, desired-state convergence, event triggers, result parsing, grains, secret backends (Vault/AWS/Azure/GCP) | Done (~10,700 LOC, 175 tests) |
| `rf-bootstrap` | OTP enrollment, TrustStore (single-use, hash-stored, TTL-enforced) | Done (~430 LOC, 11 tests) |
| `rf-relay` | Stateless encrypted relay broker binary | Done (~390 LOC, 7 tests) |
| `rf-agent` | Agent binary (connects outbound, serves RPC under policy) | Done (~530 LOC) |
| `rf-cli` | `rf` CLI binary (exec, status, shell, forward, playbook, policy, cp, proxy, completions) | Done (~2,000 LOC) |
| `rf-mcp-server` | MCP server binary for AI agent integration (Claude, Cursor, Aider), RBAC `allowed_tools` per caller | Done (~3,400 LOC, 61 tests) |
| `rf-ingress` | HTTP ingress gateway: axum server, routing table, API key auth, rate limiting, reverse proxy | Done (~580 LOC, 11 tests) |
| `rf-integration-tests` | End-to-end integration tests (relay pipeline + MCP server E2E) | Done (~2,050 LOC, 50 tests) |
| `sdks/python` | Python MCP client SDK — pip-installable, async + sync API, LangChain + CrewAI + OpenAI + Anthropic + AutoGen integrations | Done (41 tests) |
| `sdks/typescript` | TypeScript MCP client SDK — npm package, fully typed async API | Done (12 tests) |

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime (full features) |
| `snow` | Noise protocol framework (XX pattern) |
| `tokio-tungstenite` | WebSocket transport |
| `yamux` | Stream multiplexing over SecureChannel |
| `rmp-serde` | msgpack serialization (wire protocol) |
| `serde_yaml` | Policy/config YAML parsing |
| `serde_json` | Audit log JSON output |
| `regex` | Policy pattern matching |
| `tracing` | Structured logging |
| `sysinfo` | System metrics (CPU, memory, disk) |
| `clap` | CLI argument parsing |

---

## Design Principles

1. **Deny-by-default** — no capability without explicit policy
2. **Policy is the entry point** — security checks BEFORE execution, not after
3. **Agent is final authority** — re-checks policy locally, cannot be overridden by orchestrator
4. **Encrypted by default** — Noise XX always, regardless of transport
5. **Audit everything** — every decision logged to structured JSON
6. **Transport-agnostic** — any channel that can move signed bytes is a valid transport
7. **Graceful degradation** — transports tried in priority order, offline queue for disconnected agents
8. **No partial execution** — all pre-flight checks pass or entire execution rejected
9. **Hot-reload** — policy reloadable without reconnection
10. **Zero trust** — no implicit trust based on network position
11. **Single binary** — no runtime dependencies, no interpreters, no JVM
12. **Offline-first** — queue, retry, idempotency — disconnected agents are a normal state
13. **Identity = key, not address** — IP is a routing hint, cryptographic key is identity
14. **Delay-tolerant by design** — commands are signed orders that mature through policy and execute when path exists
15. **Content-addressed integrity** — policies and payloads identified by hash, naturally deduplicated and verifiable

---

## Technical Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Memory safety without GC. Single static binary. Fearless concurrency |
| Crypto | Noise XX (via `snow`) | Same core as WireGuard. Formally verified. Mutual auth built-in. No PKI needed |
| Multiplexing | yamux over SecureChannel | Concurrent streams over one Noise session. Battle-tested (libp2p) |
| RPC serialization | msgpack | Smaller frames, faster parse, binary-safe |
| Async runtime | tokio | Industry standard, multi-threaded, io_uring on Linux |
| Key trust model | TOFU + OTP | First enrollment via OTP, subsequent connections via cached static key |
| Identity model | Key-derived address | Address = hash(pubkey). Reticulum-inspired. No DNS/DHCP dependency |
| Authorization | Capability tokens (Biscuit) | Commands carry own permission. Scales better than centralized ACL |
| State sync | CRDT convergence | Desired-state converges without master. Works over intermittent links |
| Content integrity | Hash-addressed payloads | Natural dedup, cache, verify. Git/IPFS-inspired |

---

## Platform Support

RavenFabric is designed to run **anywhere**. The agent targets every platform that Rust can compile to:

| Tier | Platforms | Notes |
|------|-----------|-------|
| **Tier 1** (CI-tested) | Linux amd64/arm64 (musl static), macOS amd64/arm64, Windows amd64 | First-class, full feature set |
| **Tier 2** (compiles) | Linux armv7 (Raspberry Pi), Linux riscv64, FreeBSD, Android (aarch64/armv7), iOS (aarch64) | Reduced features on constrained devices |
| **Tier 3** (experimental) | WASM/WASI, OpenWrt (MIPS/ARM), ESP32, bare-metal ARM | Minimal agent profile — WASM/no_std targets compile |

**Design constraints:**

- Single static binary — no runtime dependencies, no interpreters, no JVM
- < 10 MB idle memory (runs on Raspberry Pi Zero, Android background service)
- < 15 MB binary stripped (deployable over constrained links)
- No hard libc dependency on Linux (musl static linking)
- All OS-specific code behind `#[cfg()]` — portable by default
- Feature flags: `full` (desktop/server) vs `minimal` (IoT/mobile/embedded)
- Async runtime supports single-threaded mode for constrained environments

**Mobile:**

- **Android** — agent as foreground service, NDK cross-compile, respects Doze/battery optimization
- **iOS** — agent as Network Extension, proper entitlements for background connectivity

**Embedded/IoT:**

- No filesystem required (can operate from memory-only config)
- Reticulum/LoRa/BLE transports for devices without IP networking
- Schedule-aware connectivity (wake → connect → sync → sleep)

---

## Performance Targets

| Metric | Target | Why |
|--------|--------|-----|
| Connection setup (first time) | < 2 RTT | Noise XX = 1.5 RTT. Must feel instant |
| Connection setup (resumption) | < 1 RTT | Session tickets for returning agents |
| Shell latency overhead | < 10ms | Must be imperceptible vs raw TCP |
| `rf exec` simple command | < 100ms | Faster than SSH (no TCP handshake + key exchange) |
| File transfer throughput | Line speed | ChaCha20 saturates >10 Gbps on modern CPUs |
| Agent idle memory | < 10 MB | Must run on Raspberry Pi, IoT, embedded |
| Agent binary size | < 15 MB | Static musl build, stripped |
| Relay throughput | 10k concurrent sessions | Per-relay. Horizontal scaling via geo-distributed deployment |

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the detailed roadmap with implementation checklist.

---

## Documentation

- **Docs:** [docs.ravenfabric.io](https://docs.ravenfabric.io) — installation, architecture, configuration, reference
- **Demos:** [docs.ravenfabric.io/demos/](https://docs.ravenfabric.io/demos/) — live terminal recordings with setup instructions
- **Blog:** [docs.ravenfabric.io/blog/](https://docs.ravenfabric.io/blog/) — technical deep dives
- **Website:** [ravenfabric.io](https://ravenfabric.io) — overview and architecture

---

## Getting Started

### Install

```bash
# Quick install (Linux/macOS)
curl -fsSL https://ravenfabric.io/install.sh | sh

# Homebrew (macOS/Linux) — https://github.com/egkristi/homebrew-tap
brew install egkristi/tap/ravenfabric
# Or tap first, then install by name:
brew tap egkristi/tap
brew install ravenfabric
# Upgrade:
brew update && brew upgrade ravenfabric

# Nix
nix profile install github:egkristi/RavenFabric

# Docker
docker pull ghcr.io/egkristi/ravenfabric-relay:latest
docker pull ghcr.io/egkristi/ravenfabric-agent:latest

# Debian/Ubuntu (.deb from GitHub Releases)
sudo dpkg -i ravenfabric-*.deb

# Fedora/RHEL (.rpm from GitHub Releases)
sudo rpm -i ravenfabric-*.rpm

# Windows (Scoop) — pending store submission
scoop bucket add ravenfabric https://github.com/egkristi/scoop-ravenfabric
scoop install ravenfabric

# Windows (Chocolatey) — pending store submission
choco install ravenfabric

# Snap (Linux) — pending store submission
sudo snap install ravenfabric

# Docker Compose (demo)
docker compose up -d
docker compose exec cli rf exec demo-agent "uname -a"

# From source
cargo install --git https://github.com/egkristi/RavenFabric.git rf-cli
```

Pre-built binaries for all platforms are also available at [egkristi/RavenFabric-Published](https://github.com/egkristi/RavenFabric-Published/releases).

See [ROADMAP.md — Distribution & Packaging](ROADMAP.md#distribution--packaging) for all supported platforms.

### Build from Source

### Prerequisites

- Rust 1.88+ (install via [rustup](https://rustup.rs))
- Linux, macOS, or Windows (WSL2 recommended)

### Build & Run

```bash
# Clone and build
git clone https://github.com/egkristi/RavenFabric.git
cd RavenFabric
cargo build

# Run tests
cargo test

# Install pre-commit hooks
git config core.hooksPath .githooks
```

### Try It (dev mode — no containers needed)

```bash
cargo build --release -p rf-cli

# Start relay + agent in one process
rf dev &

# Execute a command (E2E encrypted, policy-checked, audited)
rf exec --token dev "uname -a"

# Stop
kill %1
```

---

## Demos

Nine self-contained demos showcase RavenFabric on real infrastructure. Each includes a setup script, scenario scripts, and full documentation.

See the animated recordings at [ravenfabric.io/demos/](https://ravenfabric.io/demos/).

| Demo | Focus | Agents | Scenarios |
|------|-------|--------|-----------|
| [Multi-Node Ubuntu](demos/multi-node-ubuntu/) | Remote execution, policy, audit, orchestration | 2 Ubuntu agents | 17 |
| [Multi-Distro Linux](demos/multi-distro-linux/) | Static binary portability across 9 distros | 9 distro agents | 6 |
| [Kubernetes + CNPG](demos/kubernetes-cnpg/) | Encrypted database access in K8s | CNPG cluster | 6 |
| [Transport Showcase](demos/transport-showcase/) | Same protocol over 5 transport types | Per-transport | 5 |
| [Desired-State Convergence](demos/desired-state/) | Drift detection and auto-remediation | Stateful agents | 7 |
| [Data Collection](demos/data-collection/) | Fleet inventory, metrics, logs, security scan | 3 role-based agents | 8 |
| [MCP/AI Agent](demos/mcp-agent/) | Policy-bounded AI execution, human approval | 1 MCP server | 6 |
| [Resilience](demos/resilience/) | Reconnect, relay restart, network partition | 3 agents + relay | 5 |
| [Controller/Web UI](demos/controller/) | HTTP API, fleet dashboard, real-time monitoring | 2 agents + controller | 5 |
| [Direct Connection](demos/direct-connection/) | SSH-like point-to-point, no relay | 1 agent (listen mode) | 4 |

### Multi-Node Ubuntu

Two Ubuntu 24.04 agents managed through a relay — 17 scenarios covering remote execution, policy denial, audit trail, port forwarding, dev mode, fleet orchestration, and human approval for AI agents.

```bash
cd demos/multi-node-ubuntu && ./setup.sh
rf --relay ws://127.0.0.1:9091 exec --token agent1 'hostname && uname -a'
rf --relay ws://127.0.0.1:9091 exec --token agent2 'cat /etc/os-release | head -4'
./setup.sh teardown
```

### Multi-Distro Linux

One static musl binary running on 9 distributions (Ubuntu, Debian, Fedora, Rocky, Manjaro, openSUSE, Alpine, Amazon Linux, Void) — zero runtime dependencies.

```bash
cd demos/multi-distro-linux && ./setup.sh
rf --relay ws://127.0.0.1:9092 exec --token alpine 'cat /etc/os-release | head -2'
rf --relay ws://127.0.0.1:9092 exec --token fedora 'cat /etc/os-release | head -2'
./setup.sh teardown
```

### Kubernetes + CloudNativePG

A CNPG PostgreSQL cluster with a RavenFabric agent for encrypted database access in Kubernetes.

```bash
cd demos/kubernetes-cnpg && ./setup.sh
rf --relay ws://127.0.0.1:9093 exec --token cnpg 'psql -c "SELECT version();"'
rf --relay ws://127.0.0.1:9093 exec --token cnpg 'psql -c "SELECT client_addr, state FROM pg_stat_replication;"'
./setup.sh teardown
```

### Transport Showcase

Five transport types — WebSocket (TCP), QUIC (UDP), UNIX socket, stdio pipe, and in-process memory — all running identical Noise XX encrypted sessions.

```bash
cd demos/transport-showcase
./scenarios/01-websocket-tcp.sh
./scenarios/02-quic-udp.sh
./scenarios/03-unix-socket.sh
```

### Desired-State Convergence

Declarative desired-state engine with drift detection and auto-remediation — 7 scenarios covering packages, files, services, sysctl, grains targeting, and event triggers.

```bash
cd demos/desired-state
./scenarios/01-drift-detection.sh
./scenarios/02-auto-remediation.sh
```

### Data Collection

Fleet-wide inventory, resource monitoring, log collection, config audit, network topology, and security scanning — 3 role-based agents (collector, webserver, database) with strict read-only policy.

```bash
cd demos/data-collection && ./setup.sh
./scenarios/01-system-inventory.sh
./scenarios/02-resource-monitoring.sh
./setup.sh teardown
```

### MCP/AI Agent

End-to-end MCP server demo with policy-bounded AI execution, human approval workflow, and audit trail — 6 scenarios from policy discovery to file operations.

```bash
cd demos/mcp-agent && ./setup.sh
./scenarios/01-policy-discovery.sh
./scenarios/04-human-approval.sh
./setup.sh teardown
```

### Resilience

Agent reconnect after relay restart, network partition recovery, graceful degradation, and exponential backoff visualization — 4 containers (relay + 3 agents).

```bash
cd demos/resilience && ./setup.sh
./scenarios/01-agent-reconnect.sh
./scenarios/03-network-partition.sh
./setup.sh teardown
```

### Controller / Web UI

HTTP API server with fleet dashboard and real-time agent monitoring — REST endpoints for agent list, health check, remote execution, and policy inspection.

```bash
cd demos/controller && ./setup.sh
curl -s http://localhost:8080/api/agents | python3 -m json.tool
./scenarios/03-remote-execution.sh
./setup.sh teardown
```

### Direct Connection

Point-to-point connection to an agent in listen mode — no relay, no meet tokens. Like SSH but with Noise XX encryption and policy-bounded execution.

```bash
cd demos/direct-connection && ./setup.sh
rf --connect ws://127.0.0.1:9999 exec --token unused 'hostname && uname -a'
rf --connect ws://127.0.0.1:9999 status --token unused
./setup.sh teardown
```

### Project Structure

```text
RavenFabric/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── rf-crypto/          # Noise XX, keys, SecureChannel
│   ├── rf-transport/       # Driver trait, WebSocket + QUIC backends
│   ├── rf-rpc/             # Message types, msgpack codec
│   ├── rf-policy/          # Policy loading + enforcement
│   ├── rf-executor/        # Command execution under policy
│   ├── rf-audit/           # Structured JSON-lines audit logging
│   ├── rf-bootstrap/       # OTP enrollment flow
│   ├── rf-relay/           # Relay broker binary
│   ├── rf-agent/           # Agent binary
│   ├── rf-cli/             # `rf` CLI binary
│   ├── rf-mcp-server/     # MCP server for AI agents
│   └── rf-mcp-client/     # MCP client SDK (Rust library)
├── sdks/
│   ├── python/             # Python MCP client SDK (pip)
│   └── typescript/         # TypeScript MCP client SDK (npm)
├── demos/
│   ├── multi-node-ubuntu/  # 2-agent Ubuntu demo (17 scenarios)
│   ├── multi-distro-linux/ # 9-distro compatibility demo
│   ├── kubernetes-cnpg/    # CloudNativePG + K8s demo
│   ├── transport-showcase/ # 5 transport types (WS, QUIC, UNIX, stdio, memory)
│   ├── desired-state/      # Drift detection + auto-remediation
│   ├── data-collection/    # Fleet inventory, metrics, security scan
│   ├── mcp-agent/          # MCP/AI agent integration demo
│   ├── resilience/         # Reconnect, partition recovery, backoff
│   ├── controller/         # HTTP API + fleet dashboard demo
│   └── recordings/         # Asciinema recordings + SVG exports
├── docs/                   # Documentation (mdBook)
├── website/                # Landing page (ravenfabric.io)
├── .github/workflows/      # CI/CD (check, fmt, clippy, test, coverage, binary-size, release)
├── ARCHITECTURE.md         # System design + data flow
├── CONNECTIVITY.md         # Connectivity value chain (13-phase lifecycle)
├── ROADMAP.md              # Implementation plan with phases
├── SECURITY.md             # Vulnerability reporting
├── CONTRIBUTING.md         # Development workflow
└── CHANGELOG.md            # Release history
```

---

## License

RavenFabric uses a dual-license model:

- **AGPL-3.0-or-later** — open source core. Free for personal use, OSS projects, and commercial use up to 50 agents / $5M revenue.
- **Commercial** — for large commercial deployments or embedding without AGPL obligations.

See [LICENSING.md](LICENSING.md) for the full breakdown.

---

*Security first. Execute within bounds. Any network. Any system.*
