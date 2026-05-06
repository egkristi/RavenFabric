# Example Use Case 02: Edge & IoT Fleet Management with Intermittent Connectivity

> **Scenario:** An organization operates hundreds or thousands of edge devices
> across geographically dispersed locations — industrial sensors, retail
> point-of-sale systems, fleet vehicles, agricultural monitors, remote weather
> stations. Connectivity is unreliable, bandwidth is limited, and devices may
> go offline for hours or days at a time. Operations teams need to deploy
> updates, collect telemetry, run diagnostics, and respond to incidents —
> reliably, securely, and without standing up custom infrastructure per site.

---

## The Problem

Edge fleets break the assumptions that traditional management tools rely on:

- **Devices are not always reachable** — cellular dead zones, satellite
  windows, tidal connectivity patterns
- **Bandwidth is expensive** — every byte counts on metered connections
- **Devices live in hostile networks** — captive portals, transparent proxies,
  aggressive NAT, ISP-level filtering
- **Power and compute are constrained** — Raspberry Pi-class hardware, often
  battery-powered
- **Physical access is impractical** — devices are on offshore platforms,
  agricultural fields, ships at sea, mountain peaks
- **Failure modes are diverse** — frozen networks, swapped SIM cards,
  firmware corruption, environmental damage

### Traditional approaches and their problems

```
Operations team
    │
    ▼
Cloud orchestrator (proprietary IoT platform)
    │
    ▼  (MQTT broker — TLS-terminated, vendor-controlled)
    │
Always-on connection to each device
    │
    ▼
Edge device (hopeful)
```

**Issues:**

- **Vendor lock-in** — AWS IoT Core, Azure IoT Hub, Google IoT (now defunct)
  trap data and operational patterns
- **TLS-termination at broker** — vendor sees all telemetry and commands in
  plaintext
- **Always-on assumption** — if the device disconnects, queued commands
  silently expire
- **No store-and-forward** — commands sent during downtime are lost
- **Limited execution model** — usually only "send message" or "invoke
  function", not arbitrary command execution
- **Bandwidth-wasteful** — keepalives, TLS renegotiation, JSON overhead
- **Single-cloud dependency** — what happens when AWS region goes down?

---

## The RavenFabric Approach

```
Operations team                          Anywhere
    │
    │  rf exec edge-fleet --selector "site=offshore-1" \
    │      "tail -100 /var/log/sensor.log"
    │  rf state apply firmware-v2.3.yaml
    ▼
RavenFabric Relay (E2E encrypted)
    │
    ▼  Multiple transport tiers:
    ▼  - Direct WireGuard (when connected)
    ▼  - QUIC with connection migration (mobile)
    ▼  - Reticulum mesh (LoRa fallback)
    ▼  - DTN store-and-forward (offline tolerance)
    ▼
Edge device with rf-agent
    │  ├─ Queues commands when offline
    │  ├─ Validates policy locally
    │  ├─ Buffers telemetry until reachable
    │  └─ Resyncs on reconnect
    ▼
Local sensors / actuators / applications
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Delay-tolerant operation** | Commands queue and execute when device reconnects |
| **Multi-transport resilience** | Falls back from cellular to LoRa to satellite to packet radio |
| **Bandwidth-efficient** | msgpack codec, deduplicated commands, opportunistic batching |
| **Self-hosted control plane** | No vendor lock-in, no cross-border data flow |
| **Same fabric as data center** | Edge devices use the same protocol, identity, and policy as servers |
| **Air-gap recovery** | Sneakernet via USB/SD card when all else fails |
| **Cryptographic device identity** | Each device has a unique keypair — clones cannot impersonate |
| **Tamper-evident audit** | Every command, every result, signed and recorded |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Operations team — anywhere                                         │
│                                                                     │
│  ┌──────────┐                                                      │
│  │ Operator │  $ rf exec edge-fleet --selector "fleet=trucks"      │
│  │          │      "uptime"                                        │
│  │          │  $ rf state apply --target site=arctic-1 \           │
│  │          │      sensor-config.yaml                              │
│  └─────┬────┘                                                      │
└────────┼────────────────────────────────────────────────────────────┘
         │
         │  Noise XX (E2E encrypted)
         │  Transport: WebSocket via relay (operator side)
         ▼
┌─────────────────────────────────────────────────────────────────────┐
│  rf-relay (geo-distributed, sees only ciphertext)                  │
│  Multiple regional relays for low-latency edge connection          │
└────────┼────────────────────────────────────────────────────────────┘
         │
         │  Commands are routed to target devices
         │  Queued at relay if device offline
         │  Multiple paths attempted in parallel
         │
    ┌────┴────────────────────────┬─────────────────┬───────────────┐
    │                             │                 │               │
    ▼                             ▼                 ▼               ▼
┌────────────┐            ┌────────────┐    ┌────────────┐  ┌────────────┐
│ Cellular   │            │ Satellite  │    │ LoRa mesh  │  │ Packet     │
│ (4G/5G)    │            │ (Starlink, │    │ via gateway│  │ radio      │
│            │            │  Iridium)  │    │            │  │ (HF/VHF)   │
└─────┬──────┘            └─────┬──────┘    └─────┬──────┘  └─────┬──────┘
      │                         │                 │                │
      ▼                         ▼                 ▼                ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Edge devices (hundreds to thousands)                               │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │
│  │ Sensor node  │  │ Vehicle ECU  │  │ Retail POS   │              │
│  │  (RPi 4)     │  │  (i.MX8)     │  │  (NUC)       │              │
│  │              │  │              │  │              │              │
│  │ rf-agent     │  │ rf-agent     │  │ rf-agent     │              │
│  │  ├─ <10MB    │  │  ├─ <10MB    │  │  ├─ <10MB    │              │
│  │  ├─ static   │  │  ├─ static   │  │  ├─ static   │              │
│  │  └─ musl     │  │  └─ musl     │  │  └─ musl     │              │
│  │              │  │              │  │              │              │
│  │ Local app    │  │ Local CAN    │  │ Local POS    │              │
│  │ (sensor      │  │ bus reader   │  │ application  │              │
│  │  reader)     │  │              │  │              │              │
│  └──────────────┘  └──────────────┘  └──────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Deployment Patterns

### Pattern A: Always-on cellular/wired devices

For devices with stable connectivity (retail POS, fixed sensors with reliable
power and network), the agent maintains a persistent connection to the relay.

```yaml
# /etc/ravenfabric/config.yaml on the device
agent:
  identity_path: /etc/ravenfabric/identity.key
  policy_path: /etc/ravenfabric/policy.yaml
  audit_path: /var/log/ravenfabric/audit.jsonl

  # Primary connection — always-on cellular
  relay:
    primary: wss://relay-eu.ravenfabric.example.com
    fallbacks:
      - wss://relay-us.ravenfabric.example.com
      - quic://relay-eu.ravenfabric.example.com:443

  # Reconnect strategy
  reconnect:
    initial_delay_ms: 1000
    max_delay_ms: 60000
    multiplier: 2.0
    jitter: 0.3

  # Bandwidth conservation
  transport:
    keepalive_interval_seconds: 300  # 5 min — not 30 sec
    compress_payloads: true
    batch_telemetry: true
    batch_window_ms: 5000

  # Telemetry collection
  metrics:
    interval_seconds: 60
    buffer_size: 10000  # buffer 10k samples if disconnected
    drop_policy: oldest_first
```

### Pattern B: Intermittent connectivity (fleet vehicles, mobile devices)

For devices that connect periodically (vehicles parking near WiFi, ships
docking, agricultural drones returning to base), the agent operates in
**delay-tolerant mode**.

```yaml
agent:
  mode: delay_tolerant

  # Connection strategy — opportunistic
  connection:
    attempt_interval_seconds: 300   # try every 5 min when offline
    success_min_duration_seconds: 30 # require stable connection
    transports:
      - wireguard       # direct if peer reachable
      - quic            # connection migration handles roaming
      - websocket       # universal fallback
      - reticulum       # mesh fallback

  # Local command queue — survives reboots
  queue:
    persistence_path: /var/lib/ravenfabric/queue.db
    max_size_mb: 100
    ttl_hours: 168     # commands expire after 1 week

  # Telemetry buffering — also survives reboots
  telemetry:
    persistence_path: /var/lib/ravenfabric/telemetry.db
    max_size_mb: 500
    flush_on_connect: true
    compression: zstd
```

### Pattern C: Severe connectivity constraints (offshore, arctic, satellite-only)

For devices in extreme environments, the agent uses **store-carry-forward**
DTN-style routing with multiple alternative transports.

```yaml
agent:
  mode: dtn_bundle

  # Bundle Protocol v7 (RFC 9171) — NASA-style DTN
  bundles:
    custody_transfer: true
    encryption: required
    ttl_hours: 720    # 30 days

  # Multi-modal connectivity
  transports:
    # When satellite window opens
    - kind: iridium_sbd
      schedule: "*/15 * * * *"
      priority_only: true

    # When LoRa gateway in range
    - kind: reticulum_lora
      always_on: true
      bandwidth_kbps: 5

    # When ship/vehicle docks at WiFi
    - kind: opportunistic_wifi
      ssid_whitelist: ["fleet-base", "depot-*"]

    # When physical media is exchanged
    - kind: nncp_sneakernet
      watch_path: /media/usb
```

---

## Policy Configuration

Edge devices need policies that account for resource constraints and the
diversity of device roles.

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: edge-sensor-fleet-policy
spec:
  # Targeting — applies to devices with these labels
  selector:
    matchLabels:
      fleet: sensors
      role: monitoring

  # Allowed commands
  commands:
    allow:
      # Diagnostics
      - pattern: "^uptime$"
      - pattern: "^free -h$"
      - pattern: "^df -h$"
      - pattern: "^uname -a$"
      - pattern: "^journalctl --since.*$"

      # Sensor management
      - pattern: "^/usr/local/bin/sensor-cli status$"
      - pattern: "^/usr/local/bin/sensor-cli calibrate.*$"
      - pattern: "^/usr/local/bin/sensor-cli read --since.*$"

      # Firmware operations (with approval)
      - pattern: "^/usr/local/bin/firmware-update verify .*$"

    deny:
      # Never allow shell escape
      - pattern: "/bin/sh"
      - pattern: "/bin/bash"
      - pattern: "su -"
      - pattern: "sudo "

      # Never allow network reconfiguration
      - pattern: "ip route"
      - pattern: "iptables"
      - pattern: "systemctl.*network"

  # Resource limits — strict on constrained hardware
  resources:
    maxCPUPercent: 20         # don't compete with sensor reading
    maxMemoryMB: 64
    taskTimeoutSeconds: 60
    maxOutputBytes: 1048576   # 1MB max output (bandwidth)

  # Filesystem — read-mostly
  filesystem:
    allow:
      - path: /var/log
        operations: [read]
      - path: /etc/sensor
        operations: [read]
      - path: /tmp
        operations: [read, write]
        max_size_mb: 10
    deny:
      - path: /etc
      - path: /usr
      - path: /var/lib

  # Sensitive operations require approval
  approval:
    required:
      - pattern: "firmware-update apply .*"
        approvers: ["fleet-ops", "security-team"]
        minApprovers: 2
        timeoutSeconds: 86400  # 24 hour window for fleet rollout
```

---

## Operator Workflows

### Workflow 1: Fleet-wide health check

```bash
$ rf exec --selector "fleet=sensors" "uptime"

[connecting to 247 agents...]
[214 reachable, 33 offline]

agent-sensor-001: 14:22:01 up 14 days, 7:14, load: 0.12, 0.08, 0.05
agent-sensor-002: 14:22:01 up 14 days, 7:14, load: 0.15, 0.11, 0.07
...
agent-sensor-214: 14:22:03 up 23 days, 11:42, load: 0.22, 0.18, 0.14

[33 agents offline — commands queued]
agent-sensor-215: queued (last seen 4h ago)
agent-sensor-216: queued (last seen 12h ago)
...
agent-sensor-247: queued (last seen 6d 14h ago)

  audited · 2.4s · 214 results received · 33 queued
```

### Workflow 2: Targeted firmware upgrade with canary

```yaml
# firmware-rollout-v2.3.yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Playbook
metadata:
  name: sensor-firmware-v2.3
spec:
  targets:
    selector:
      labels:
        fleet: sensors

  strategy:
    type: canary
    canary_percent: 5
    canary_duration_minutes: 60
    failure_threshold_percent: 2
    rollback_on_failure: true

  steps:
    - name: download-firmware
      command: |
        /usr/local/bin/firmware-update download \
          --url https://artifacts.example.com/sensor-fw-v2.3.bin \
          --signature https://artifacts.example.com/sensor-fw-v2.3.sig
      timeout_seconds: 300

    - name: verify-signature
      command: /usr/local/bin/firmware-update verify /tmp/sensor-fw-v2.3.bin
      onFailure: abort

    - name: apply-firmware
      command: /usr/local/bin/firmware-update apply /tmp/sensor-fw-v2.3.bin
      timeout_seconds: 600
      approval_required: true

    - name: verify-running
      command: /usr/local/bin/sensor-cli version
      expect_output: "v2.3"

    - name: report-success
      command: /usr/local/bin/firmware-update report-success
```

```bash
$ rf playbook apply firmware-rollout-v2.3.yaml

[playbook: sensor-firmware-v2.3]
[targets: 247 sensors matching fleet=sensors]
[strategy: canary 5% (12 devices) for 60 minutes]
[approval pending: 2/2 approvals needed]

✓ Approval received from fleet-ops (alice)
✓ Approval received from security-team (bob)

[canary phase started: 12 devices]
[12/12 download-firmware: ok]
[12/12 verify-signature: ok]
[12/12 apply-firmware: ok]
[11/12 verify-running: ok, 1 failed]
[failure rate: 8.3% (threshold 2%)]

✗ Canary failure threshold exceeded
[rolling back canary devices...]
[canary devices rolled back: 12/12]
[playbook aborted]

  See logs: rf logs playbook sensor-firmware-v2.3
```

### Workflow 3: Telemetry collection from disconnected devices

```bash
# Devices have been offline — collect their buffered telemetry on reconnect
$ rf telemetry sync --selector "site=arctic-research-station"

[3 devices in selector]
[arctic-1: connecting via iridium-sbd...]
[arctic-1: connected, syncing 14,238 buffered samples...]
[arctic-1: sync complete, 4.2 MB transferred over 12 minutes]

[arctic-2: connecting via reticulum-lora...]
[arctic-2: connected, syncing 8,491 buffered samples...]
[arctic-2: sync complete, 2.1 MB transferred over 28 minutes]

[arctic-3: still offline — last seen 14 days ago]
[arctic-3: telemetry sync deferred until next contact window]

  3 devices · 22,729 samples synced · 6.3 MB · est. cost $0.42 (Iridium)
```

### Workflow 4: Air-gap rescue via sneakernet

When a device cannot be reached by any electronic means, RavenFabric supports
USB-based command and configuration delivery.

```bash
# At operations center
$ rf bundle create \
    --target arctic-research-3 \
    --command "systemctl restart sensor-daemon" \
    --command "ip link set wlan0 up" \
    --command "/usr/local/bin/firmware-update apply /tmp/firmware-v2.3.bin" \
    --output /media/usb/recovery.rfb

[bundle: 3 commands signed and encrypted]
[ttl: 30 days]
[recipient: arctic-research-3 (pubkey: abc123...)]
[bundle saved: /media/usb/recovery.rfb]

# Field engineer carries USB to device, plugs it in
# Agent's NNCP driver picks up the bundle and executes locally
# Results queued in /media/usb/recovery-result.rfb
# USB carried back, results synced to operations center
```

---

## Comparison with Alternatives

| Feature | AWS IoT Core | Azure IoT Hub | Balena | RavenFabric |
|---------|--------------|---------------|--------|-------------|
| **End-to-end encrypted** | No (TLS-termination) | No (TLS-termination) | No | Yes (Noise XX) |
| **Vendor lock-in** | High | High | Medium | None (AGPLv3) |
| **Self-hosted control plane** | No | No | Partial (openBalena) | Yes |
| **Multi-transport (LoRa, satellite)** | Limited | Limited | No | Yes |
| **Delay-tolerant operation** | Limited (device shadow) | Limited (device twin) | No | Yes (DTN bundles) |
| **Air-gap (USB) recovery** | No | No | No | Yes (NNCP) |
| **Arbitrary command execution** | Lambda only | Direct methods | Yes | Yes (policy-controlled) |
| **Bandwidth efficiency** | Medium | Medium | Low | High (msgpack) |
| **Memory footprint** | 50-100 MB | 50-100 MB | 200+ MB | <10 MB |
| **Static binary (no runtime)** | No | No | No | Yes |
| **Cross-cloud agnostic** | No | No | Partial | Yes |
| **Cost at scale (10k devices)** | $$$$$ | $$$$$ | $$$ | Compute only |

---

## Implementation Status

### Available today (v0.1)

- Policy-validated remote command execution
- Cryptographic device identity (Curve25519 keypair per device)
- End-to-end Noise XX encryption
- Structured audit logging
- Static musl binary, <10 MB, runs on Raspberry Pi

### Coming in v0.2

- QUIC transport with connection migration (mobile fleet)
- Built-in telemetry collection with offline buffering
- Metrics push/pull modes
- Health check probes

### Coming in v0.3

- Multi-agent playbooks with canary/rolling strategies
- Grain-based fleet targeting

### Coming in v0.5

- Reticulum mesh transport (LoRa, BLE, packet radio)
- Serial driver (RS-232, USB)
- NNCP sneakernet support
- DTN bundle protocol (RFC 9171)

---

## Why This Matters

The IoT and edge computing space has become balkanized:

- **Cloud-native IoT platforms** (AWS IoT, Azure IoT Hub) lock organizations
  into single vendors and require always-on connectivity
- **Linux-based edge platforms** (Balena, KubeEdge) are heavyweight and
  assume container-friendly hardware
- **Embedded RTOS solutions** (Zephyr, FreeRTOS) lack management tooling
- **Custom solutions** are common but consume engineering budget that should
  go elsewhere

The result: most organizations end up with **multiple incompatible management
planes** — one for cloud, one for edge, one for devices, one for vehicles.
Each has its own identity model, audit trail, and operational quirks.

RavenFabric proposes a different approach:

> **One fabric, one identity, one policy plane — from data center to edge to
> end device. The transport is pluggable; the security model is uniform.**

For organizations that operate at scale across heterogeneous environments,
this consolidation has compelling economics: fewer tools to license, fewer
runbooks to maintain, fewer security audits to pass, fewer credentials to
rotate.

For organizations operating in regulated industries, the data-residency and
air-gap properties become not just operational conveniences but regulatory
necessities.

---

## See Also

- [README.md](../README.md) — RavenFabric overview
- [CONNECTIVITY.md](../CONNECTIVITY.md) — Multi-transport architecture
- [usecase-cloudnativepg.md](usecase-cloudnativepg.md) — CloudNativePG admin access
- [usecase-multi-cluster-kubernetes.md](usecase-multi-cluster-kubernetes.md) — Multi-cluster Kubernetes
- [usecase-airgapped-ics.md](usecase-airgapped-ics.md) — Air-gapped industrial systems
- [usecase-msp-multitenant.md](usecase-msp-multitenant.md) — MSP multi-tenant operations
- [Reticulum Network Stack](https://reticulum.network/) — Mesh networking
  reference
- [DTN Research Group](https://www.dtnrg.org/) — Delay-tolerant networking
