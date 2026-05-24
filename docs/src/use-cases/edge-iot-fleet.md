# Edge & IoT Fleet Management

> **Scenario:** An organization operates hundreds or thousands of edge devices
> across dispersed locations — industrial sensors, retail POS, fleet vehicles,
> agricultural monitors, remote weather stations. Connectivity is unreliable,
> bandwidth is limited, and devices go offline for hours or days. Operations
> teams need to deploy updates, collect telemetry, run diagnostics, and
> respond to incidents — without vendor lock-in or always-on assumptions.

---

## The Problem

Edge fleets break the assumptions that traditional management tools rely on:

- **Devices are not always reachable** — cellular dead zones, satellite windows, tidal connectivity
- **Bandwidth is expensive** — every byte counts on metered connections
- **Devices live in hostile networks** — captive portals, transparent proxies, aggressive NAT
- **Compute and power are constrained** — Raspberry Pi-class hardware, often battery-powered
- **Physical access is impractical** — offshore platforms, ships, mountain peaks
- **Cloud IoT platforms lock you in** — AWS IoT Core, Azure IoT Hub terminate TLS (see your data in plaintext), assume always-on connectivity, and silently drop commands when devices are offline

---

## The RavenFabric Approach

```text
Operations team (anywhere)
    │  rf exec --selector "fleet=sensors" "uptime"
    │  rf playbook apply firmware-rollout.yaml
    ▼
rf-relay (E2E encrypted, sees only ciphertext)
    ▼  Multiple transports attempted in parallel:
    ▼  WireGuard → QUIC → WebSocket → DTN → NNCP (sneakernet)
    ▼
Edge device (rf-agent, <10 MB static binary)
    ├─ Queues commands when offline, executes on reconnect
    ├─ Validates policy locally (final authority)
    ├─ Buffers telemetry until reachable
    └─ Cryptographic device identity (unique keypair)
```

| Capability | How |
|------------|-----|
| Delay-tolerant | Commands queue at relay/device, execute on reconnect |
| Multi-transport | Cellular → satellite → LoRa → serial → USB fallback |
| Bandwidth-efficient | msgpack codec, deduplicated commands, batched telemetry |
| Self-hosted | No vendor lock-in, no cross-border data flow |
| Same fabric everywhere | Edge devices use identical protocol, identity, and policy as servers |
| Air-gap recovery | USB/SD card via NNCP when all else fails |
| Tamper-evident audit | Every command and result signed and recorded |

---

## Deployment Patterns

### Always-on devices (cellular/wired)

```toml
# /etc/ravenfabric/raven.toml
[agent]
id = "sensor-042"
relay = "wss://relay-eu.example.com"
key_path = "/etc/ravenfabric/identity.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0
```

### Intermittent connectivity (fleet vehicles, mobile)

Agent operates in delay-tolerant mode — queues commands locally, syncs on reconnect.
Multiple transports attempted in priority order: WireGuard → QUIC (connection migration
handles roaming) → WebSocket → NNCP.

### Severe constraints (offshore, arctic, satellite-only)

DTN store-carry-forward with multi-day TTL. Connectivity via Iridium SBD windows,
LoRa gateways, or NNCP sneakernet for last-resort recovery.

---

## Policy Configuration

Edge devices need strict resource limits to avoid competing with primary workloads.

```yaml
spec:
  commands:
    allow:
      # Diagnostics
      - pattern: "^uptime$"
      - pattern: "^free -h$"
      - pattern: "^df -h$"
      - pattern: "^journalctl --since.*$"
      # Sensor management
      - pattern: "^/usr/local/bin/sensor-cli status$"
      - pattern: "^/usr/local/bin/sensor-cli calibrate.*$"
      - pattern: "^/usr/local/bin/sensor-cli read --since.*$"
      # Firmware (read-only verify)
      - pattern: "^/usr/local/bin/firmware-update verify .*$"

    deny:
      - pattern: "/bin/sh"
      - pattern: "/bin/bash"
      - pattern: "sudo "
      - pattern: "ip route"
      - pattern: "iptables"
      - pattern: "systemctl.*network"

  filesystem:
    allow:
      - path: /var/log
      - path: /etc/sensor
      - path: /tmp
    deny:
      - path: /etc
      - path: /usr
      - path: /var/lib

  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 60
```

---

## Operator Workflows

### Fleet-wide health check

```bash
$ rf exec --selector "fleet=sensors" "uptime"

[247 targets: 214 reachable, 33 offline (commands queued)]
agent-sensor-001: up 14 days, load: 0.12
agent-sensor-002: up 14 days, load: 0.15
...
agent-sensor-247: queued (last seen 6d ago)

  214 results · 33 queued · 2.4s
```

### Firmware rollout with canary

```bash
$ rf playbook apply firmware-rollout-v2.3.yaml

[targets: 247 sensors | strategy: canary 5% (12 devices) for 60 min]
[canary: 12/12 download ok → 12/12 verify ok → 12/12 apply ok]
[canary: 11/12 verify-running ok, 1 failed (8.3% > 2% threshold)]
[canary FAILED — rolling back 12 devices...]
[rollback complete]
```

### Air-gap rescue via sneakernet

```bash
# At operations center — create signed bundle for specific device
$ rf bundle create \
    --target arctic-research-3 \
    --command "systemctl restart sensor-daemon" \
    --command "/usr/local/bin/firmware-update apply /tmp/fw-v2.3.bin" \
    --ttl 30d \
    --output /media/usb/recovery.rfb

# Field engineer carries USB to device
# Agent's NNCP driver picks up bundle, verifies, executes
# Results written to USB, carried back
```

---

## Comparison with Alternatives

| Feature | AWS IoT Core | Azure IoT Hub | Balena | RavenFabric |
|---------|-------------|---------------|--------|-------------|
| End-to-end encrypted | No (TLS-terminated) | No (TLS-terminated) | No | Yes (Noise XX) |
| Vendor lock-in | High | High | Medium | None (AGPL-3.0) |
| Self-hosted | No | No | Partial | Yes |
| Multi-transport (LoRa, satellite, USB) | Limited | Limited | No | Yes |
| Delay-tolerant (DTN) | Device shadow only | Device twin only | No | Yes |
| Air-gap recovery | No | No | No | Yes (NNCP) |
| Command execution | Lambda only | Direct methods | Yes | Yes (policy-controlled) |
| Memory footprint | 50-100 MB | 50-100 MB | 200+ MB | <10 MB |
| Static binary | No | No | No | Yes (musl) |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Policy-validated command execution | Done | `rf-executor` |
| Cryptographic device identity (Curve25519) | Done | `rf-crypto` |
| End-to-end Noise XX encryption | Done | All transports |
| Structured audit logging | Done | `rf-audit` |
| Static musl binary <10 MB | Done | Linux arm64/amd64/armv7 |
| QUIC with connection migration | Done | `rf-transport` |
| WireGuard direct path | Done | Userspace |
| STUN/ICE NAT traversal | Done | UDP hole-punching |
| Offline telemetry buffering | Done | `MetricBuffer` with overflow handling |
| Built-in system metrics (sysinfo) | Done | Push/pull modes |
| DTN store-carry-forward | Done | Priority queue + SQLite persistence |
| NNCP sneakernet transport | Done | Filesystem write/read/dedup |
| Serial port driver (RS-232) | Done | CRC-16, sync bytes |
| Grains (system facts for targeting) | Done | OS, arch, hostname, env |
| Multi-agent playbooks | Done | `Orchestrator` + `rf playbook` |
| Rollback on failure | Done | Automatic |
| Grain-based fleet targeting | Done | Label selector matching |
| BLE beacon discovery | Done | RSSI filtering, peer tracking |
| Reticulum mesh | Stub | Enum variant only |
| LoRa/Meshtastic | Stub | Enum variant only |
| Canary/rolling deployment strategy | Planned | |

---

## See Also

- [Air-Gapped ICS](airgapped-ics.md)
- [CloudNativePG Admin Access](cloudnativepg.md)
- [Multi-cluster Kubernetes](multi-cluster-kubernetes.md)
- [Maritime & Offshore](maritime-offshore.md)
- [Reticulum Network Stack](https://reticulum.network/)
- [DTN Research Group](https://www.dtnrg.org/)
