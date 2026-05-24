# Maritime Vessels & Offshore Installations

> **Scenario:** A fleet operator manages IT/OT systems across commercial
> vessels, offshore platforms, and wind installations — operating through
> intermittent satellite connectivity, hostile physical environments, and
> maritime regulatory requirements (IMO, NIS2, class societies).

---

## The Problem

Maritime environments break conventional IT management assumptions:

- **Connectivity varies per voyage** — port Wi-Fi → coastal LTE → VSAT at sea → Iridium SBD in polar regions → total loss
- **Bandwidth is metered** — VSAT costs ~€5-15/MB, Iridium SBD ~€0.50-2/KB. A 50MB update costs €100-750 per vessel
- **Hardware lives in hostile conditions** — salt corrosion, vibration, 40°C temperature swings, power instability
- **Physical access is impractical** — helicopter visits cost €5,000-15,000
- **Air-gap is mandatory for OT** — engine controls, navigation, and cargo systems must not touch the internet
- **Regulatory compliance** — IMO MSC.428(98), NIS2, class society notations (DNV, Lloyd's Register, ABS), TMSA 3
- **Fleets are heterogeneous** — bulk carriers, tankers, platforms, wind vessels each have different OT and criticality

### Current approaches

- **Maritime SaaS (Inmarsat/KVH/Marlink)** — vendor lock-in, per-vessel manual updates, expensive
- **TeamViewer/VNC over satellite** — bandwidth disaster ($5/MB for screen video)
- **Send engineer to vessel** — slow, helicopter costs €5,000-15,000
- **Hope the vessel crew fixes it** — often the actual practice

---

## How RavenFabric Addresses This

```text
Fleet operations (shore)
    │  rf exec --selector "fleet=tankers" "system_health_check"
    │  rf playbook apply security-patch-q2.yaml
    ▼
rf-relay (geo-distributed, E2E encrypted)
    ▼  Agent selects cheapest available transport:
    ▼  Wi-Fi in port → LTE coastal → VSAT at sea → DTN queue if offline
    ▼
Each vessel: rf-agent
    ├─ Queues commands during connectivity loss
    ├─ Separate agents for IT (networked) and OT (air-gapped, NNCP only)
    ├─ Validates policy locally (final authority)
    └─ Signed audit trail per vessel for compliance evidence
```

| Capability | How |
|------------|-----|
| Connectivity-tolerant | DTN store-carry-forward, commands survive days offline |
| Multi-transport | Wi-Fi, LTE, VSAT, Iridium SBD, LoRa mesh, USB sneakernet |
| Air-gap support for OT | NNCP bundles via USB with multi-signature requirement |
| Compliance-grade audit | Cryptographically signed per-vessel audit logs |
| Fleet-wide operations | Grain-based targeting across heterogeneous vessels |
| Vendor-neutral | No satellite provider lock-in |
| Tamper detection | Detects MITM on satellite links |

---

## Vessel Network Segmentation

A typical vessel has distinct network zones, each with different management:

| Network | Systems | rf-agent mode |
|---------|---------|---------------|
| Bridge/Navigation | ECDIS, AIS, GNSS, anemometer | Full management (networked) |
| OT (engine/cargo) | Engine controls, ballast, cargo handling | Air-gapped (NNCP/USB only) |
| Crew welfare | Personal devices, internet | Not managed (out of scope) |
| Cargo monitoring | Reefer, tank levels, tracking | Telemetry collection |

---

## Policy Configuration

```yaml
spec:
  commands:
    allow:
      # Standard IT operations
      - pattern: "^uptime$"
      - pattern: "^df -h$"
      - pattern: "^systemctl status .*$"
      - pattern: "^journalctl --since.*$"
      # Vessel-specific
      - pattern: "^/usr/local/bin/ecdis-status$"
      - pattern: "^/usr/local/bin/ais-receiver-stats$"
      - pattern: "^/usr/local/bin/voyage-data-recorder-export$"

    deny:
      # Never modify navigation systems remotely
      - pattern: ".*ecdis-config.*write"
      - pattern: ".*safety-system.*"

  filesystem:
    allow:
      - path: /var/log
      - path: /tmp
    deny:
      - path: /etc
      - path: /opt/safety-system
      - path: /var/lib/voyage-data-recorder

  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 60
```

For OT networks, bundles require multi-signature (fleet IT supervisor + OT engineering supervisor) and are policy-validated both ashore and at the vessel.

---

## Example Workflows

### Fleet-wide health check

```bash
$ rf exec --selector "fleet=northern-shipping" "uptime"

[47 targets: 39 reachable (VSAT/LTE/WiFi), 8 offline (commands queued)]
agent-atlantic-trader: up 23 days, load: 0.08
agent-pacific-hauler:  up 14 days, load: 0.12
...
  39 results · 8 queued · 3.1s
```

### OT update via sneakernet at port call

```bash
# Shore: create signed bundle for vessel OT systems
$ rf bundle create \
    --target "atlantic-trader-ot" \
    --command "/usr/local/bin/ballast-firmware-update apply v3.2" \
    --require-cosign fleet-it-supervisor \
    --require-cosign ot-engineering-supervisor \
    --ttl 72h \
    --output /media/usb/ot-update.rfb

# Engineer carries USB on board during port visit
# OT agent: verify signatures → decrypt → policy check → execute → result to USB
# USB carried back, results decrypted ashore
```

### Emergency response over Iridium SBD

```bash
# Vessel on Iridium fallback — optimize for minimal bandwidth
$ rf exec --target "nordic-star" \
    --priority critical \
    "/usr/local/bin/engine-diagnostics --critical-only"

# 187 bytes out, 942 bytes back — total cost ~€1 via Iridium
# Result: cylinder 4 overtemp, cooling pump 2 degraded
```

---

## Regulatory Alignment

RavenFabric's audit trail maps to maritime cyber compliance:

| Framework | What RavenFabric provides |
|-----------|--------------------------|
| IMO MSC.428(98) | Cryptographic audit trail, access control evidence, incident response logs |
| NIS2 (EU maritime) | Continuous monitoring, tamper detection, structured incident records |
| DNV Cyber Secure notation | Per-vessel asset inventory (grains), authentication events, anomaly detection |
| TMSA 3 | Evidence of system maintenance, configuration management, access control |

Audit logs are retained per vessel, signed, and exportable for class society inspections.

---

## Comparison with Alternatives

| Feature | Maritime SaaS | TeamViewer/VNC | Engineer visit | RavenFabric |
|---------|--------------|----------------|----------------|-------------|
| Self-hosted | No | No | N/A | Yes |
| Vendor-neutral satellite | Locked | Any (wasteful) | N/A | Yes |
| Air-gap support (OT) | No | No | Yes (manual) | Yes (NNCP) |
| Multi-transport | Vendor only | No | N/A | Yes |
| Compliance-grade audit | Partial | No | No | Yes |
| Tamper detection | No | No | N/A | Yes |
| Lock-in | Severe | Medium | Low | None (AGPL-3.0) |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Cryptographic device identity | Done | Per-vessel agent keypair |
| Policy-validated execution | Done | Deny-by-default |
| Structured audit logging | Done | JSON-lines, signed |
| End-to-end Noise XX encryption | Done | Transport-agnostic |
| Outbound-only agent connection | Done | Works through satellite NAT |
| QUIC with connection migration | Done | Roaming support |
| WireGuard direct path | Done | Userspace |
| DTN store-carry-forward | Done | Priority queue + persistence |
| NNCP sneakernet (USB bundles) | Done | Write/read/dedup |
| Serial port driver | Done | RS-232, CRC-16 |
| Multi-agent playbooks | Done | `Orchestrator` + rollback |
| Grain-based fleet targeting | Done | Label selector matching |
| Offline telemetry buffering | Done | MetricBuffer with overflow |
| Audit report generation | Done | JSON/CSV export |
| Path selection (policy-driven) | Done | `select_with_policy()` |
| Reticulum/LoRa mesh | Stub | Enum variant only |
| Bandwidth-cost-aware routing | Planned | Transport cost classes |
| Iridium SBD transport driver | Planned | Low-bandwidth optimization |
| Compliance reporting frameworks | Planned | IMO/DNV mapping |
| Fleet status dashboard | Planned | |

---

## Adoption Path

| Phase | Scope | Duration |
|-------|-------|----------|
| 1 — Pilot | Single vessel, read-only ops, validate VSAT connectivity | 1-3 months |
| 2 — Fleet subset | 5-10 vessels, write ops with approval, compliance evidence | 4-9 months |
| 3 — OT integration | Air-gapped agents, USB bundle workflow, class society validation | 10-15 months |
| 4 — Full fleet | All vessels, decommission legacy SaaS | 16-24 months |

---

## See Also

- [Air-Gapped ICS](airgapped-ics.md) — OT air-gap patterns in detail
- [Edge & IoT Fleet Management](edge-iot-fleet.md) — Distributed device fleet patterns
- [MSP Multi-Tenant Operations](msp-multitenant.md) — Multi-client isolation
