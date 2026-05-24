# Air-Gapped Industrial Control Systems

> **Scenario:** A critical infrastructure operator runs ICS/SCADA/OT networks
> that are physically isolated from the internet — power substations, water
> treatment, manufacturing, oil and gas, rail. Engineers need to update
> configurations, collect data, and respond to incidents without violating
> the air gap.

---

## The Problem

Air-gapped OT environments face a fundamental tension:

- **The air gap is the security control** — any internet connection defeats defense-in-depth
- **Systems still need maintenance** — updates, configuration, log collection, incident response
- **Current practice is dangerous** — USB sticks carried between IT and OT networks with no verification
- **Audit trails are weak** — what crossed the air gap, when, and what did it touch?
- **Time pressure is real** — ICS incidents have safety implications demanding fast response

Stuxnet (2010) proved that air gaps fail not through network perimeter breaches, but through
uncontrolled crossing points created by operational necessity.

### Current approach

```text
IT laptop → downloads update → copies to USB → carried to OT → plugged into OT workstation → ICS
```

**Problems:** No cryptographic verification, no tamper-evidence, no audit trail, no policy
enforcement, bidirectional malware risk, operator error, slow physical transport.

The alternatives — connecting OT to corporate network "just briefly", maintaining duplicate
systems, or refusing to update — are all worse.

---

## The RavenFabric Approach

RavenFabric maintains end-to-end security across air gaps through **delay-tolerant
cryptographic bundles** on physical media.

```text
IT side:                                    OT side:
┌────────────┐                              ┌────────────┐
│ Engineer   │  rf bundle create            │ rf-agent   │  watches /media/usb
│ workstation│  → encrypted .rfb file       │            │  verifies + decrypts
└─────┬──────┘                              └─────┬──────┘
      │                                           │
      ▼                                           ▼
┌─────────────┐    PHYSICAL AIR GAP    ┌─────────────────┐
│  USB drive  │ ──────────────────────→│ Policy check    │
│  (.rfb)     │                        │ Execute         │
│             │ ←──────────────────────│ Result → USB    │
└─────────────┘                        └─────────────────┘
```

Each bundle is:

- **Encrypted** to the target agent's public key (only that device can decrypt)
- **Signed** by the operator's identity key (+ optional co-signatures)
- **TTL-bounded** with automatic expiry
- **Replay-protected** via single-use nonce
- **Policy-validated** both at creation and at the target before execution

| Capability | How |
|------------|-----|
| Air gap preserved | No network connection ever required |
| Tamper-evident | Cryptographic signature over entire bundle |
| Policy enforced at target | OT agent has final authority — denies anything outside policy |
| Complete audit | Every bundle, execution, and result logged with structured entries |
| Bidirectional | Results written back to media, encrypted to operations team |

---

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│  IT Network                                             │
│  ┌────────────┐    rf bundle create                     │
│  │ Engineer   │ → /media/usb/payload.rfb               │
│  └────────────┘   (encrypted + signed + TTL + nonce)   │
└──────────┬──────────────────────────────────────────────┘
           │  PHYSICAL AIR GAP (USB, courier, pneumatic tube)
┌──────────▼──────────────────────────────────────────────┐
│  OT Network (ICS/SCADA)                                 │
│  ┌────────────┐                                         │
│  │ OT terminal│  rf-agent reads bundle from /media/usb  │
│  │            │  → verify sigs → decrypt → policy check │
│  └─────┬──────┘    → execute → write result to USB     │
│        │                                                │
│  ┌─────┴────┐  ┌─────────┐  ┌────────────┐            │
│  │ PLC      │  │ HMI     │  │ Historian  │            │
│  └──────────┘  └─────────┘  └────────────┘            │
└─────────────────────────────────────────────────────────┘
```

---

## Transport Modes

| Mode | Mechanism | Direction | Bandwidth | Use case |
|------|-----------|-----------|-----------|----------|
| **Removable media** | USB, optical, SD card | Bidirectional | GB/s | Standard updates, config changes |
| **Data diode** | Hardware unidirectional link | OT→IT only | 100 Mbps | Continuous telemetry export |
| **QR-stream** | Screen → camera | OT→IT only | 1-10 KB/s | Facilities where USB is forbidden |
| **Serial** | RS-232, USB serial | Bidirectional | 1-115 kbps | Legacy systems, diode connectivity |
| **Packet radio** | AX.25, HF/VHF | Bidirectional | 1.2 kbps | Offshore, remote substations |

### Removable media (primary mode)

```bash
# IT side: create bundle
$ rf bundle create \
    --target plc-controller-east \
    --command "ladder-update --file /tmp/new-logic.l5x" \
    --command "ladder-verify --file /tmp/new-logic.l5x" \
    --attach /tmp/new-logic.l5x \
    --signature-required ot-supervisor \
    --ttl 24h \
    --output /media/usb/ot-update.rfb
```

```yaml
# OT side: agent watches for bundles
agent:
  watchers:
    - kind: media_directory
      path: /media/usb
      poll_interval_seconds: 5
      auto_eject_after_processing: true

  bundle_policy:
    require_signatures:
      - any_of: ["operations-engineers"]
      - any_of: ["ot-supervisors"]
    reject_unsigned: true
    reject_expired: true
    reject_replayed: true

  result_output:
    kind: same_media
    encrypted: true
    signed: true
```

### Data diode (continuous export)

```yaml
agent:
  transports:
    - kind: data_diode
      direction: outbound_only
      device: /dev/diode0
      protocol: udp_with_fec
      bandwidth_mbps: 100
    - kind: removable_media
      direction: inbound_only
      watch_path: /media/usb
```

---

## Policy for ICS Environments

ICS environments require that certain operations are **never** permitted, and safety-impacting
changes require multi-party approval.

```yaml
spec:
  # Immutable safety rules
  commands:
    deny:
      - pattern: ".*safety-interlock.*disable"
      - pattern: ".*emergency-shutdown.*override"
      - pattern: "^iptables"
      - pattern: "^route add"
      - pattern: "^ip link set.*up"

    allow:
      # Read-only diagnostics
      - pattern: "^/opt/scada/bin/scada-status"
      - pattern: "^/opt/scada/bin/scada-tag-read .*"
      - pattern: "^/opt/scada/bin/historian-query .*"
      # Backup operations
      - pattern: "^/opt/scada/bin/scada-backup --config-only --output /backup/.*"

  filesystem:
    deny:
      - path: /etc/safety-config
      - path: /etc/interlocks
      - path: /opt/scada/safety
    allow:
      - path: /backup
      - path: /tmp

  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 30

  # Multi-party approval for safety-related commands
  # (bundle must carry required co-signatures)
  bundles:
    require_co_signature: true
    max_age_hours: 24
    reject_replayed: true
```

---

## Example Workflows

### Scheduled update with multi-party signoff

```bash
# Engineer creates bundle, requires two co-signatures
$ rf bundle create \
    --target substation-controller-12 \
    --playbook quarterly-config-update.yaml \
    --require-cosign safety-officer \
    --require-cosign operations-supervisor \
    --output /tmp/q2-update.rfb

# Each reviewer signs independently
$ rf bundle review /tmp/q2-update.rfb
$ rf bundle sign /tmp/q2-update.rfb --as safety-officer
$ rf bundle sign /tmp/q2-update.rfb --as operations-supervisor

# Export to USB, carry to facility
$ rf bundle export /tmp/q2-update.rfb /media/usb/q2-update.rfb
```

OT agent log:

```text
[14:32:01] Bundle detected: /media/usb/q2-update.rfb
[14:32:01] Signatures: 3/3 valid | TTL: ok (4h old) | Nonce: ok
[14:32:02] Executing 1/3: scada-backup → success (12s)
[14:32:14] Executing 2/3: scada-config-import → success (19s)
[14:32:33] Executing 3/3: scada-restart-soft → success (69s)
[14:33:42] Result written: /media/usb/q2-update-result.rfb
```

### Emergency incident response

```bash
# Read-only diagnostics — single signature, short TTL
$ rf bundle create-emergency \
    --target sensor-cluster-7 \
    --command "/opt/scada/bin/diag-capture --full" \
    --command "/opt/scada/bin/scada-tag-read SENSOR_* --since 1h" \
    --priority critical \
    --ttl 5m \
    --output /media/usb/emergency-diag.rfb

# Agent processes in seconds, writes results back to USB
```

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Cryptographic device identity (Curve25519) | Done | `rf-crypto` |
| Policy-validated execution | Done | `rf-policy` + `rf-executor` |
| Structured audit logging | Done | `rf-audit` |
| End-to-end Noise XX encryption | Done | Online sessions |
| DTN bundle protocol | Done | `rf-rpc` — store-carry-forward |
| NNCP transport (filesystem bundles) | Done | `rf-rpc` — write/read/dedup |
| Serial framing (RS-232) | Done | `rf-transport` — CRC-16, sync bytes |
| Content-addressed payloads | Done | SHA-256 integrity verification |
| Sealed bundles (signed + encrypted offline packages) | Planned | v0.4 |
| Multi-signature bundle requirements | Planned | v0.4 |
| TTL + replay protection for bundles | Planned | v0.4 |
| QR-stream visual channel | Stub | Enum variant exists |
| AX.25 packet radio | Stub | Enum variant exists |
| Hardware diode integration | Planned | v0.6 |

---

## Regulatory Alignment

For operators under NIS2 (EU), NERC CIP (North America), or IEC 62443:

- **Cryptographic integrity** of every operation crossing the air gap
- **Multi-party authorization** for safety-impacting changes
- **Complete audit trail** — command, environment, result, timestamps
- **Replay protection** against compromised or reused media
- **Time-bounded authority** that auto-expires
- **Vendor neutrality** — no single point of vendor compromise

---

## Comparison with Alternatives

| Feature | Manual USB | Hardware data diode | OPSWAT MetaDefender | RavenFabric |
|---------|-----------|--------------------|--------------------|-------------|
| End-to-end encryption | No | Yes | Partial | Yes |
| Cryptographic signature | No | Vendor-specific | Partial | Yes (operator keys) |
| Policy enforcement at target | No | No | Partial | Yes |
| Multi-party approval | No | No | No | Yes |
| Bidirectional with audit | Uncontrolled | One-way only | Partial | Yes |
| Open source | N/A | No | No | Yes (AGPL-3.0) |
| Dedicated hardware required | No | Yes | No | No |

RavenFabric does not replace hardware data diodes where they are deployed. It provides a
software-layer complement that extends to environments where dedicated hardware is impractical.

---

## See Also

- [Edge & IoT Fleet Management](edge-iot-fleet.md)
- [Multi-cluster Kubernetes](multi-cluster-kubernetes.md)
- [MSP Multi-tenant Operations](msp-multitenant.md)
- [Maritime & Offshore](maritime-offshore.md)
- [IEC 62443](https://www.iec.ch/cyber-security) — Industrial cybersecurity standards
- [NIST SP 800-82 Rev.3](https://csrc.nist.gov/publications/detail/sp/800-82/rev-3/final) — Guide to ICS Security
- [NNCP Project](http://www.nncpgo.org/) — Sneakernet-friendly file transfer
