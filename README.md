# RavenFabric

> Security-first distributed execution engine. Network-agnostic, E2E encrypted, policy-driven, ZTNA.
> From full mesh VPN, fire-and-forget commands to declarative desired state — all within an airtight policy layer.

**Status: Pre-alpha** — Foundation layers implemented. Not yet functional end-to-end.

[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-AGPLv3-blue.svg)](LICENSES/AGPLv3.txt)
[![CI](https://github.com/egkristi/RavenFabric/actions/workflows/ci.yml/badge.svg)](https://github.com/egkristi/RavenFabric/actions/workflows/ci.yml)

**Language:** Rust | **License:** AGPLv3 (core) + Commercial (enterprise)

---

## What is RavenFabric?

RavenFabric is a **universal agent** that provides **secure, policy-controlled access to any system** — regardless of network topology, operating system, or device class. It unifies mesh VPN, remote execution, configuration management, and zero-trust access into a single binary with no runtime dependencies.

It runs on servers, desktops, laptops, Raspberry Pis, Android phones, iPhones, routers, IoT sensors, edge appliances, satellites, and anything else that can run compiled code. If it has a CPU, it can be a RavenFabric node.

It is not a task runner with security added. It is a **security-first policy engine** that also executes tasks:

```
❌ Other tools:  powerful execution → bolt on security controls
✅ RavenFabric:  airtight policy → execution within its bounds only
```

### The Full Spectrum

```
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

```
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

```
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

- Mutual authentication (both sides verify each other)
- Per-session forward secrecy (ephemeral keys)
- Relay-opaque: a relay sees only random-looking bytes — no keys, no content, no identity

---

## Execution Modes

### Fire and Forget
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Execution
spec:
  mode: fire-and-forget
  target:
    agent: prod-server-1
  task:
    command: "touch /workspace/.heartbeat"
```

### Fire and Verify
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Execution
spec:
  mode: fire-and-verify
  target:
    agent: prod-server-1
  task:
    command: "git -C /workspace pull --rebase origin main"
    timeoutSeconds: 60
```

### Task (ordered steps with conditions)
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Execution
spec:
  mode: task
  target:
    agent: prod-server-1
  task:
    name: deploy
    workdir: /workspace/app
    steps:
      - name: pull latest
        command: "git pull --rebase origin main"
      - name: install deps
        command: "npm ci --production"
      - name: test
        command: "npm test"
        onFailure: abort
      - name: restart
        command: "systemctl restart myapp"
        condition: "{{ steps.test.exitCode == 0 }}"
```

### Playbook (multi-agent, rolling, rollback)
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Playbook
metadata:
  name: rolling-deploy
spec:
  strategy:
    kind: rolling
    batchSize: 1
    pauseOnFailure: true
  plays:
    - name: update web servers
      targets:
        selector:
          labels:
            role: web-server
      tasks:
        - command: "git -C /app pull --rebase origin main"
        - command: "systemctl restart nginx"
      rollback:
        - command: "git -C /app reset --hard HEAD~1"
        - command: "systemctl restart nginx"
```

### Desired State (declarative convergence + drift detection)
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: web-server-baseline
spec:
  targets:
    selector:
      labels:
        role: web-server

  state:
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
          ...
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
    intervalSeconds: 300  # Continuous drift detection
```

---

## Policy (Security First)

### SecurityPolicy — top level, some rules immutable
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: SecurityPolicy
metadata:
  name: cluster-default
spec:
  allowedModes: [fire-and-forget, fire-and-verify, task, playbook, desired-state]

  authorization:
    allowedCallers:
      - kind: Operator
        tenant: "*"

  blastRadius:
    maxConcurrentAgents: 10
    maxAffectedAgents: 50

  # These cannot be overridden by any tenant or agent policy
  immutable:
    neverAllowAsRoot: true
    neverAllowNetworkReconfigure: true
    neverAllowPackageRemove: [ssh, ravenfabric]
    neverAllowFileModify:
      - /etc/ravenfabric/*
      - /etc/ssh/sshd_config
    alwaysAudit: true
```

### RPCPolicy — commands, filesystem, services
```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: default
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
        operations: [read, write, list]
    deny:
      - path: /etc/ravenfabric
      - path: /etc/ssh
      - path: /root
  resources:
    maxCPUPercent: 50
    maxMemoryMB: 512
    maxOutputBytes: 10485760  # 10MB
    taskTimeoutSeconds: 300
```

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

```
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

```
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

```
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

Failover is automatic: if the active path degrades, a secondary path is promoted (or racing begins with relay as bridge). See [CONNECTIVITY.md](CONNECTIVITY.md) Phase 11.

### Reconnect Strategy

| Strategy | Behavior |
|----------|----------|
| Immediate retry | Transient glitch |
| Exponential backoff + jitter | Standard (1s → 60s max) |
| Network-aware | Wait for OS network event (mobile, lid-close) |
| Scheduled | Air-gap rendezvous windows |

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

```
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

The policy engine doesn't just decide *what* can execute — it decides *how* it may travel:

```yaml
spec:
  transport_policy:
    # High-sensitivity commands require strong transport
    high_sensitivity:
      require_transport: [wireguard, quic]
      require_post_quantum: true
      deny_transport: [dns-tunnel, lora, relay]

    # Status reporting can use any channel
    low_sensitivity:
      allow_transport: [any]
      allow_delay_tolerant: true

    # Air-gapped zones accept physical media
    air_gapped:
      allow_transport: [serial, bluetooth, usb, nncp]
      require_dual_signature: true
```

### Cryptographic Identity

Identity in RavenFabric is independent of network position (inspired by Reticulum and SPIFFE):

```
Identity = SHA-256(public_key)[0..16]    # 128-bit cryptographic address
```

- **Address = key hash** — no DHCP, no DNS required for identity. Inspired by Reticulum's destination addressing.
- **IP is a routing hint** — not identity. Agent can change IP, network, transport — identity persists.
- **TOFU + OTP enrollment** — first contact bootstrapped via one-time token, then TOFU for all future connections.
- **Post-quantum hybrid** — ML-KEM + X25519 for harvest-now-decrypt-later resistance (planned).
- **Petname system** — agents are locally named (`web-01`) mapping to cryptographic identifiers. No global namespace required.

---

## Current Implementation Status

**~1,260 LOC | 13 tests | 0 clippy warnings | All CI green**

What works today:
- Noise XX mutual authentication handshake (full)
- Secure channel with encrypted frames (full)
- Static key management with zeroing-on-drop (full)
- OTP token generation and validation (full)
- Policy engine with deny-by-default, regex allow/deny, path checks, symlink resolution (full)
- Command execution under policy control with timeout and output limiting (full)
- Structured JSON-lines audit logging (full)
- CLI argument parsing with clap (skeleton)

What's next (critical path to working demo):
1. Fix `TransportState` split for `SecureChannel` to accept connections
2. Wire protocol magic/version exchange
3. In-memory transport driver (enables testing without network)
4. yamux multiplexer + msgpack codec
5. WebSocket transport driver
6. Relay, agent, and CLI implementation

See [ROADMAP.md](ROADMAP.md) for the full plan.

---

## Capabilities (Planned)

> The following capabilities describe the target architecture. See [Current Implementation Status](#current-implementation-status) above for what exists today.

### Full Mesh VPN (Layer 3)
What Tailscale/ZeroTier/NetBird do — but over any transport, not just WireGuard:
- **TUN device** — full L3 network access between agents
- **Subnet routing** — expose LAN segments through agents
- **Exit nodes** — route internet traffic through a specific agent
- **Split tunneling** — policy controls which traffic enters the mesh
- **MagicDNS** — automatic DNS for all agents (`agent-name.rf.local`)
- **Multicast/mDNS relay** — service discovery across the mesh
- All VPN traffic: policy-checked, auditable, revocable per-agent

### Interactive Shell
What SSH/Teleport/Boundary do — but without SSH, without ports, through any transport:
- **`rf shell <agent>`** — interactive terminal session through the fabric
- **Session recording** — full terminal capture (asciinema format), policy-controlled
- **Session replay** — auditors can replay any session
- **Live session monitoring** — attach read-only to active sessions
- **Session sharing** — multiple users in one session (pair programming)
- **Forced commands** — policy can restrict shell to specific commands only
- **No SSH daemon required** — agent handles PTY allocation natively

### Remote Execution (RPC)
What Ansible/Salt do — but E2E encrypted, policy-first:
- **fire-and-forget** — send, no reply needed
- **fire-and-verify** — send, wait for exit code
- **task** — ordered steps with conditions, retry, abort
- **playbook** — multi-agent orchestration with rolling/canary/parallel strategies, rollback
- **desired-state** — declarative convergence + continuous drift detection + remediation

### File Operations
What scp/rsync do — but policy-checked, encrypted, audited:
- **push** — orchestrator → agent (single file or directory)
- **pull** — agent → orchestrator
- **agent-to-agent transfer** — via encrypted relay
- **sync** — rsync-style incremental with checksums, delta transfer
- **file.watch** — streaming tail with regex filters
- **atomic writes** — write to temp + rename (no partial files)

### Port Forwarding / Tunneling
What `ssh -L/-R/-D` and ngrok do — but policy-controlled:
- **local-forward** (`ssh -L` equivalent) — expose remote port locally
- **remote-forward** (`ssh -R` equivalent) — expose local port on remote (disabled by default)
- **dynamic-forward / SOCKS5** (`ssh -D` equivalent) — full proxy
- **agent-to-agent tunnel** — orchestrator as encrypted relay
- **HTTP proxy** through agent
- **TCP/UDP forwarding** — any protocol, not just TCP
- All tunnels: policy-checked per port/host/protocol, time-limited, audited

### Process Management
- **background exec** with ID-based tracking
- **streaming exec** (real-time stdout/stderr via multiplexed channel)
- **signal** (SIGTERM/SIGHUP/etc — SIGKILL may be denied by policy)
- **pid-wait** — wait for process exit
- **process inventory** — list running processes (policy-filtered)

### Secrets Injection
What Vault/SOPS do at execution time — built into the fabric:
- **`{{ secrets.KEY }}`** — inject secrets into commands/files at execution time
- **Agent-side resolution** — secrets never transit the network in plaintext
- **Sealed secrets** — encrypted at rest, decrypted only on target agent
- **Rotation support** — secrets can be rotated without re-deploying tasks
- **No secret in audit log** — masked automatically

### Result Parsing
- Structured parsers: `raw`, `trim`, `trim-int`, `json`, `yaml`, `csv`, `regex`, `lines`, `table`
- Assertions on parsed output (`gt`, `eq`, `lt`, `semver`, `regex`, `contains`)
- Expose parsed values to subsequent steps via `{{ steps.NAME.parsed.FIELD }}`

### Event System
- **file-created**, **file-modified**, **file-deleted** — inotify/kqueue
- **process-exit**, **process-started**
- **service-state-changed**
- **cron-like schedules** — agent-local, no external cron needed
- **webhook triggers** — agent can emit events to external systems

### Data Collection Agent
What Telegraf, Metricbeat, collectd, node_exporter, and OpenTelemetry Collector do — built into every agent:

**Metrics:**
- Built-in system metrics (CPU, memory, disk, network, load, processes, filesystems)
- Prometheus-compatible `/metrics` endpoint (pull mode) or push to remote
- Custom metric plugins via policy-defined commands (scrape output of any command)
- Application metrics scraping (Prometheus endpoints on localhost)
- Continuous streaming to OTLP, Prometheus remote-write, InfluxDB, StatsD, Datadog
- Per-metric labels (agent ID, grains, custom tags)

**Logs:**
- File tailing with glob patterns (`/var/log/**/*.log`)
- Journald integration (systemd journal forwarding)
- Structured log parsing (JSON, logfmt, regex, grok patterns)
- Forward to OTLP, Loki, Elasticsearch, S3, local file rotation
- Policy-controlled: which logs can be collected, redaction rules

**Traces:**
- OTLP receiver (accept traces from local applications)
- Forward to Jaeger, Tempo, OTLP endpoint
- Trace context injection for RavenFabric RPC calls (built-in)

**Health checks:**
- HTTP/TCP/UDP endpoint probes (up/down, latency, cert expiry)
- Process alive checks (by name, PID file, systemd unit)
- Custom health commands with exit-code semantics
- Anomaly detection (threshold alerts on any metric)

**Key differentiator:** No separate collection agent needed. The same binary that executes commands and enforces policy also ships telemetry — through the same encrypted channel, under the same policy controls, with the same audit trail. Zero additional attack surface.

### Offline Queue & Delay-Tolerant Delivery
What no other tool does — handle disconnected agents as a first-class concern:
- **Store-carry-forward** — commands traverse intermediate nodes, each taking custody
- **Queue commands** while agent is offline (SQLite-backed, persistent across restart)
- **Deliver on reconnect** with policy-controlled TTL
- **Opportunistic sync** — agents exchange queued messages when they meet (BLE, Wi-Fi, physical)
- **Schedule-aware routing** — route via known contact windows (satellite passes, shift changes)
- **Idempotency tokens** — duplicate delivery (via multiple paths) is safe
- **Priority queue** — urgent commands route preferentially
- **Expiry** — stale commands auto-discarded after TTL
- **Physical media transport** — commands can travel via USB, SD card, or any file-moving mechanism (NNCP-style)

All capabilities: policy-checked, audited, E2E encrypted — regardless of how many hops or how long the journey.

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
ravenfabric_version: "0.1.0"
role: web-server              # Custom grain
environment: production       # Custom grain
packages:
  nginx: "1.24.0"
services:
  nginx: running
```

```yaml
# Use in playbook:
targets:
  where:
    grains.os_family: debian
    grains.distro_version: ">= 22.04"
```

---

## Bootstrap: OTP Identity Enrollment

```
1. Admin generates token:
   $ ravenfabric token generate --agent=prod-server-1 --ttl=1h
   → Token: rf-otp-a3f9b2c1d4e5f6...

2. Token delivered out-of-band (SSH, cloud-init, etc.)

3. Agent generates Curve25519 key pair locally (private key never leaves)

4. Agent sends to bootstrap endpoint:
   POST https://relay.example.com/agent/bootstrap
   {
     "token": "rf-otp-a3f9b2c1d4e5f6...",
     "agentId": "prod-server-1",
     "publicKey": "hex-encoded-32-bytes"
   }

5. Server validates:
   - Token exists + not expired + not already used
   - Mark as used (single-use)
   - Register agent → tenant mapping

6. Agent stores identity + relay addresses
   All future connections use Noise XX + static key
   Bootstrap endpoint never used again
```

---

## Relay: Stateless Encrypted Broker

The relay is deliberately dumb. It only copies bytes. It sees nothing.

**What relay sees:**
```
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

## Why RavenFabric Wins

### vs. Tailscale / Headscale / NetBird / ZeroTier (Mesh VPN)
These tools give you encrypted connectivity. Period. You still need SSH for commands, Ansible for automation, Telegraf/Prometheus for metrics, and a separate ZTNA product for application access. RavenFabric gives you the mesh **plus** everything that runs on top of it — in one agent, one policy layer, one audit trail.

**Specific advantages:**
- Multi-transport (not locked to WireGuard) — works through corporate proxies, air-gaps, Tor
- Cross-protocol upgrade (not just DERP→WG — any driver→any driver)
- Command-level policy (not just IP:port ACLs)
- Built-in execution — no SSH, no Ansible, no separate tools
- Built-in data collection — no Telegraf, no node_exporter, no Fluent Bit
- Session recording — auditors can replay everything
- Offline queue — disconnected agents catch up automatically

### vs. Twingate / Pomerium / Cloudflare Access (ZTNA)
These tools control who can reach which application. They're glorified reverse proxies with identity. They can't execute commands, manage state, or work without internet. RavenFabric provides the same application-level access **plus** command execution, config management, and air-gap support.

**Specific advantages:**
- Command-level granularity (not just "can access web app")
- No TLS termination — E2E encrypted even through the access layer
- Desired state + drift detection — active management, not just access gating
- Works without internet (Reticulum, serial)
- Self-hosted only — no vendor cloud dependency

### vs. Ansible / Salt / Puppet (Config Management)
These tools assume SSH/ZeroMQ connectivity already exists. They have no answer for NAT, firewalls, or air-gaps. Their security model is "whoever has SSH access can do anything." RavenFabric starts with the connectivity problem solved, then layers policy-controlled execution on top.

**Specific advantages:**
- No SSH required — agent connects outbound through any transport
- Command-level deny rules — not just "can run playbook" but "cannot run rm -rf"
- Immutable rules — even admins cannot override certain protections
- Blast radius control — max concurrent/affected agents enforced
- Double policy check — controller + agent, neither trusts the other
- Streaming execution — real-time stdout/stderr (Ansible shows nothing until complete)
- Air-gap execution — over Reticulum mesh, LoRa, serial

### vs. SoftEther (Multi-protocol VPN)
SoftEther is a VPN server — client/server topology, no mesh, server terminates encryption, written in C (CVE-prone). RavenFabric is a mesh with E2E encryption where the relay sees nothing, written in memory-safe Rust.

### vs. Telegraf / Metricbeat / OpenTelemetry Collector / collectd (Data Collection)
These tools are dedicated collection agents. Each one is **another binary to deploy, configure, update, and secure** on every host. They have no policy engine, no E2E encryption to the backend, no execution capability, and no awareness of each other.

**Specific advantages:**
- One agent instead of three (execution + collection + VPN in one binary)
- Same E2E encrypted channel for telemetry — no separate TLS config, no cert management
- Policy-controlled collection — what gets collected is governed by the same deny-by-default policy
- Audited collection — every scrape/forward is logged
- Air-gap telemetry — metrics/logs ship over Reticulum, serial, or any transport
- No additional attack surface — no extra listening ports, no separate auth systems
- Offline buffering — telemetry queued when disconnected, delivered on reconnect

### vs. SSH + Bastion / Teleport / Boundary (Secure Access)
These tools solve "how do I SSH securely." RavenFabric solves "how do I securely do anything on a remote system." Shell sessions are one capability among many, not the entire product. Plus: no SSH daemon needed, no port 22, no bastion hosts.

---

## Comparison

### vs. Config Management / Execution

| | Ansible | Salt | Puppet | RavenFabric |
|---|---|---|---|---|
| **Primary purpose** | Config mgmt | Config mgmt | Config mgmt | Secure execution engine |
| Language | Python | Python | Ruby | **Rust** |
| Transport security | SSH | AES+HMAC | SSL/TLS | **Noise XX (E2E)** |
| NAT traversal | ❌ | ❌ | ❌ | **✅ Multi-transport** |
| Air-gap support | ❌ | ❌ | ❌ | **✅ Reticulum/serial** |
| Transport diversity | SSH only | ZeroMQ only | HTTPS only | **Any (WG/QUIC/WS/Tor/mesh)** |
| Desired state | ✅ | ✅ | ✅ | **✅** |
| Drift detection | ⚠️ | ✅ | ✅ | **✅ continuous** |
| Command execution | ✅ | ✅ | ✅ | **✅ Built-in + policy** |
| Policy engine | Minimal | Reactor | Catalog | **Security-first (command-level)** |
| Immutable rules | ❌ | ❌ | ❌ | **✅ neverAllow*** |
| Blast radius control | ⚠️ | ⚠️ | ⚠️ | **✅ Policy-enforced** |
| Double policy check | ❌ | ❌ | ❌ | **✅ Controller + Agent** |
| Memory safety | ❌ | ❌ | ❌ | **✅ (Rust)** |

### vs. Mesh VPN / ZTNA

| | Tailscale | Headscale | NetBird | ZeroTier | SoftEther | Twingate | Pomerium | RavenFabric |
|---|---|---|---|---|---|---|---|---|
| **Primary purpose** | Mesh VPN | Mesh VPN (self-hosted) | Mesh VPN + ACL | Mesh VPN | VPN server | ZTNA proxy | ZTNA proxy | **Secure execution + VPN** |
| Open source | Partial (client) | ✅ | ✅ | Partial | ✅ (GPLv2) | ❌ | ✅ (core) | **✅ (AGPLv3)** |
| Self-hosted control plane | ❌ (SaaS) | ✅ | ✅ | ❌ (SaaS) | ✅ | ❌ (SaaS) | ✅ | **✅** |
| Transport protocol | WireGuard | WireGuard | WireGuard | Custom (ChaCha) | OpenVPN/SSL/L2TP | WireGuard | HTTPS | **Noise XX over any** |
| E2E encryption | ✅ (WG) | ✅ (WG) | ✅ (WG) | ✅ | ⚠️ (server terminates) | ✅ (WG) | ❌ (TLS termination) | **✅ (Noise XX)** |
| NAT traversal | ✅ STUN/DERP | ✅ STUN/DERP | ✅ STUN/TURN | ✅ (root servers) | ⚠️ (manual) | ✅ | ✅ | **✅ Multi-method** |
| Relay fallback | DERP only | DERP only | TURN only | Root servers | ❌ | Connector | ❌ | **Any (WS/QUIC/mesh)** |
| Transport diversity | WG only | WG only | WG only | Custom only | Multi-protocol | WG only | HTTPS only | **WG/QUIC/WS/Tor/Reticulum/serial** |
| Air-gap support | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ Reticulum/serial** |
| Cross-protocol upgrade | ❌ (DERP→WG) | ❌ (DERP→WG) | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ Any→Any** |
| Command execution | ❌ (SSH needed) | ❌ (SSH needed) | ❌ (SSH needed) | ❌ (SSH needed) | ❌ | ❌ | ❌ | **✅ Built-in + policy** |
| Desired state | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅** |
| Drift detection | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ continuous** |
| Command-level policy | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ regex allow/deny** |
| Network-level ACL | ✅ | ✅ | ✅ | ✅ (rules) | ⚠️ (routing) | ✅ (resource) | ✅ (route) | **✅ + command-level** |
| Immutable rules | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ neverAllow*** |
| Identity provider | OIDC | OIDC | OIDC/SAML | Custom | RADIUS/LDAP | IdP (SCIM) | IdP (OIDC) | **OTP bootstrap + Noise keys** |
| Per-app access | ❌ (network-level) | ❌ (network-level) | ⚠️ (network ACL) | ❌ (network-level) | ❌ | ✅ (resource-level) | ✅ (route-level) | **✅ (command-level)** |
| Audit trail | ⚠️ (network) | ⚠️ (network) | ⚠️ (network) | ⚠️ (network) | ⚠️ (conn log) | ✅ (access log) | ✅ (access log) | **✅ (every command)** |
| Data collection agent | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **✅ Built-in (metrics/logs/traces)** |
| K8s native | ⚠️ Operator | ❌ | ❌ | ❌ | ❌ | ⚠️ Connector | ⚠️ Ingress | **✅ CRD** |
| Memory safety | ✅ (Go) | ✅ (Go) | ✅ (Go) | ⚠️ (C++) | ❌ (C) | N/A (SaaS) | ✅ (Go) | **✅ (Rust)** |
| No GC pauses | ❌ | ❌ | ❌ | ❌ | ✅ | N/A | ❌ | **✅** |

### Key Differentiators

> **Tailscale/Headscale/NetBird/ZeroTier** solve connectivity (mesh VPN). **Twingate/Pomerium** solve application-level zero-trust access. **Ansible/Salt** solve config management. **Telegraf/Metricbeat/OTEL** solve data collection. **RavenFabric** combines all four: **secure connectivity + command execution + policy enforcement + data collection** in a single agent with transport-agnostic E2E encryption and no GC pauses.

| Capability gap | Existing tools need | RavenFabric |
|---|---|---|
| Execute commands on remote host | Tailscale + SSH + Ansible | Built-in |
| Encrypted mesh + config management | ZeroTier + Salt | Built-in |
| ZTNA + drift detection | Twingate + Puppet | Built-in |
| Air-gap remote execution | SoftEther + manual | Built-in (Reticulum/serial) |
| Command-level policy (not just network ACL) | None available | Built-in |
| Metrics + logs from managed hosts | Tailscale + Telegraf + Fluent Bit | Built-in |
| Secure telemetry from air-gapped hosts | VPN + Prometheus + manual | Built-in (same encrypted channel) |

---

## Architecture

### Layer Model

```
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
| `rf` | CLI client — user interactions (`rf exec`, `rf dev`, `rf status`) |
| `rf-agent` | Runs on target systems. Connects outbound, serves RPC under policy |
| `rf-relay` | Stateless encrypted broker. Pairs agents and clients. Geo-distributed |

### Core Crates (Workspace)

| Crate | Responsibility | Status |
|-------|---------------|--------|
| `rf-crypto` | Noise XX handshake, SecureChannel, StaticKey management | Done (470 LOC, 5 tests) |
| `rf-transport` | Driver trait, AsyncStream abstraction, WebSocket backend | Trait only (53 LOC) |
| `rf-rpc` | Request/Response types, Action enum, msgpack codec | Types only (63 LOC) |
| `rf-audit` | Structured JSON-lines audit logging | Done (53 LOC) |
| `rf-policy` | RPCPolicy enforcement (allow/deny regex, path rules, deny-by-default) | Done (281 LOC, 4 tests) |
| `rf-executor` | Command execution under policy control with timeout + output limiting | Done (170 LOC) |
| `rf-bootstrap` | OTP enrollment (single-use, hash-stored, TTL-enforced) | Done (122 LOC, 4 tests) |
| `rf-relay` | Stateless encrypted relay broker binary | Stub |
| `rf-agent` | Agent binary (connects outbound, serves RPC under policy) | Stub |
| `rf-cli` | `rf` CLI binary (exec, dev, status) | Skeleton |

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
| Language | Rust | Memory safety without GC. No pause-the-world. Single static binary. Fearless concurrency |
| Crypto | Noise XX (via `snow`) | Same core as WireGuard. Formally verified. Mutual auth built-in. No PKI needed |
| Multiplexing | yamux over SecureChannel | Concurrent streams (shell + exec + tunnel) over one Noise session. Battle-tested (libp2p) |
| RPC serialization | msgpack | Smaller frames, faster parse, binary-safe. JSON fallback via `--format json` CLI flag |
| Async runtime | tokio | Industry standard, multi-threaded, io_uring support on Linux |
| Wire protocol versioning | Version byte in handshake | Enables rolling upgrades without breaking deployed agents |
| Key trust model | Trust-on-first-use (TOFU) + OTP | First enrollment via OTP, subsequent connections via cached static key |
| Policy transitions | Atomic swap + grace period | In-flight executions complete under old policy. New connections get new policy |
| Cargo workspace | 10 small crates | Compile-time isolation, clear boundaries, parallel compilation |
| CLI name | `rf` (not `ravenfabric`) | Short, memorable, fast to type. `rf exec`, `rf dev`, `rf status` |
| Identity model | Key-derived address | Address = hash(pubkey). No DNS/DHCP dependency. Reticulum-inspired |
| Disconnection model | DTN store-carry-forward | Offline is normal. Commands queue, deliver when path exists. NASA Bundle Protocol inspired |
| Authorization (future) | Capability tokens (biscuit) | Commands carry own permission. Scales better than centralized ACL in distributed systems |
| State sync (future) | CRDT convergence | Desired-state converges without master. Automerge-inspired. Works over intermittent links |
| Content integrity | Hash-addressed payloads | Policies/payloads identified by content hash. Natural dedup, cache, verify. Git/IPFS-inspired |

---

## Platform Support

RavenFabric is designed to run **anywhere**. The agent targets every platform that Rust can compile to:

| Tier | Platforms | Notes |
|------|-----------|-------|
| **Tier 1** (CI-tested) | Linux amd64/arm64 (musl static), macOS amd64/arm64, Windows amd64 | First-class, full feature set |
| **Tier 2** (compiles) | Linux armv7 (Raspberry Pi), Linux riscv64, FreeBSD, Android (aarch64/armv7), iOS (aarch64) | Reduced features on constrained devices |
| **Tier 3** (planned) | WASM/WASI, OpenWrt (MIPS/ARM), ESP32, bare-metal ARM | Minimal agent profile |

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

## Getting Started

### Prerequisites

- Rust 1.85+ (install via [rustup](https://rustup.rs))
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

### Project Structure

```
RavenFabric/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── rf-crypto/          # Noise XX, keys, SecureChannel
│   ├── rf-transport/       # Driver trait, WebSocket backend
│   ├── rf-rpc/             # Message types, msgpack codec
│   ├── rf-policy/          # Policy loading + enforcement
│   ├── rf-executor/        # Command execution under policy
│   ├── rf-audit/           # Structured JSON-lines audit logging
│   ├── rf-bootstrap/       # OTP enrollment flow
│   ├── rf-relay/           # Relay broker binary
│   ├── rf-agent/           # Agent binary
│   └── rf-cli/             # `rf` CLI binary
├── docs/                   # Documentation
├── .github/workflows/      # CI/CD (check, fmt, clippy, test, coverage, release)
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

- **AGPLv3** — open source core (transport, crypto, RPC, policy). Free for personal use, OSS projects, and commercial use up to 50 agents / $5M revenue.
- **Commercial** — enterprise features (playbooks, desired state, RBAC, SSO, compliance). Required for large commercial deployments or embedding without AGPLv3 obligations.

See [LICENSING.md](LICENSING.md) for the full breakdown.

---

*Security first. Execute within bounds. Any network. Any system.*
