# Air-Gapped Industrial Control Systems

> **Scenario:** A critical infrastructure operator runs industrial control
> systems (ICS), supervisory control and data acquisition (SCADA) systems, or
> operational technology (OT) networks that are physically isolated from the
> internet. Examples include power grid substations, water treatment plants,
> manufacturing floors, oil and gas facilities, and rail control systems.
> Engineers need to update configurations, collect operational data, and
> investigate incidents — without violating the air gap that protects
> these systems.

---

## The Problem

Air-gapped operational technology environments face a fundamental tension:

- **The air gap is the security control** — connecting these systems to
  the internet, even briefly, defeats decades of defense-in-depth
- **But systems still need maintenance** — software updates, configuration
  changes, log collection, incident response
- **Current practice is dangerous** — engineers carry USB sticks between
  IT and OT networks, sometimes via internet-connected laptops
- **Audit trails are weak** — when a USB stick crosses the air gap, what
  exactly happened? What was on it before? What did it touch?
- **Time pressure is real** — incidents in ICS environments have safety
  implications that demand fast response

The Stuxnet attack (2010) demonstrated that air gaps can be defeated by
contaminated removable media. Subsequent incidents at industrial facilities
worldwide have shown the pattern repeating: air gaps fail not because
attackers breach the network perimeter, but because operational necessity
creates uncontrolled crossing points.

### Traditional approaches and their problems

```
Engineer's IT laptop (internet-connected)
    │
    ▼ (downloads updates, scripts, tools)
    │
    ▼ (copies to USB stick)
    │
USB stick    ←─── critical trust boundary
    │
    ▼ (carried to OT network)
    │
Engineer's OT laptop (air-gapped)
    │
    ▼
ICS / SCADA / PLC
```

**Issues:**

- **No cryptographic verification** of what's on the USB stick
- **No tamper-evidence** — modifications between IT and OT laptops are invisible
- **No audit trail** — what was actually transferred is undocumented
- **No policy enforcement** — anything that fits on the USB can cross
- **Bidirectional risk** — malware can flow in, but also data can flow out
- **Operator error** — wrong file, wrong version, wrong target
- **Time-to-deploy** — physical transport is slow when systems are far apart

The alternatives are worse:

- **Connecting OT to corporate network "just for this update"** (common,
  catastrophic)
- **Maintaining duplicate systems for testing** (expensive, drift-prone)
- **Refusing to update** (security debt accumulates)

---

## The RavenFabric Approach

RavenFabric supports air-gapped environments through **multiple delay-tolerant
transport modes** that maintain end-to-end security even when no direct
network connection exists.

```
Engineer's terminal (anywhere)
    │
    │  rf bundle create --target plc-controller-east \
    │      --command "/firmware-tool update flow-meter-cfg.bin" \
    │      --output /media/usb/ot-update.rfb
    ▼
Cryptographically signed bundle on physical media
    │  ├─ Encrypted to specific target's public key only
    │  ├─ Signed by operator's identity key
    │  ├─ TTL-bounded (auto-expire)
    │  ├─ Single-use nonce (replay-protected)
    │  └─ Embedded policy validation
    ▼
Field engineer carries USB to facility
    │
    ▼
OT-side rf-agent reads bundle from USB
    │  ├─ Verifies signature
    │  ├─ Decrypts payload
    │  ├─ Re-validates policy locally
    │  ├─ Executes within policy bounds
    │  └─ Writes signed result back to USB
    ▼
USB carried back, results verified at operations center
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Air gap preserved** | No network connection ever required between IT and OT |
| **Cryptographic integrity** | Bundles signed and encrypted end-to-end |
| **Tamper-evident transport** | Modifications detectable, replay attacks prevented |
| **Policy enforced at target** | OT agent has final authority — nothing executes that policy denies |
| **Complete audit** | Every bundle, every execution, every result logged |
| **Multi-transport flexibility** | USB, optical disk, SD card, QR-code stream, radio |
| **Bidirectional control** | Bring data out as carefully as data goes in |
| **Standard interface** | Same `rf` commands as other environments — engineers don't need new training |

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  IT Network (corporate, internet-connected)                       │
│                                                                    │
│  ┌──────────────┐                                                 │
│  │ Engineer     │   $ rf bundle create --target plc-east          │
│  │ workstation  │      --command "..."                            │
│  │              │      --output /media/usb/payload.rfb            │
│  └──────────────┘                                                 │
│        │                                                          │
│        │  Bundle: encrypted to plc-east pubkey,                   │
│        │          signed by engineer's identity                   │
│        ▼                                                          │
│  ┌──────────────┐                                                 │
│  │ Removable    │                                                 │
│  │ media (USB)  │                                                 │
│  └──────┬───────┘                                                 │
└─────────┼──────────────────────────────────────────────────────────┘
          │
          │  ┌─── PHYSICAL AIR GAP ───┐
          │  │ Carried by hand        │
          │  │ Or bonded courier      │
          │  │ Or pneumatic tube      │
          │  └────────────────────────┘
          │
          ▼
┌────────────────────────────────────────────────────────────────────┐
│  OT Network (air-gapped, ICS/SCADA)                                │
│                                                                    │
│  ┌──────────────┐                                                 │
│  │ Removable    │                                                 │
│  │ media (USB)  │                                                 │
│  └──────┬───────┘                                                 │
│         │                                                         │
│         ▼ (mounted on OT-side workstation)                        │
│  ┌──────────────┐                                                 │
│  │ OT terminal  │   rf-agent watches /media/usb/                  │
│  │              │   reads payload.rfb                             │
│  │              │   verifies + decrypts + executes                │
│  └──────┬───────┘                                                 │
│         │                                                         │
│         ▼                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ PLC          │  │ HMI          │  │ Historian    │            │
│  │ (Allen-      │  │ (Wonderware) │  │ (PI System)  │            │
│  │  Bradley)    │  │              │  │              │            │
│  └──────────────┘  └──────────────┘  └──────────────┘            │
└────────────────────────────────────────────────────────────────────┘
```

---

## Transport Modes for Air Gap

### Mode A: Removable media (NNCP-style)

The most common pattern. Bundles are written to USB drives, optical media,
or SD cards.

```bash
# IT side: create bundle
$ rf bundle create \
    --target plc-controller-east \
    --command "ladder-update --file /tmp/new-logic.l5x" \
    --command "ladder-verify --file /tmp/new-logic.l5x" \
    --command "ladder-activate --slot 2" \
    --attach /tmp/new-logic.l5x \
    --signature-required ot-supervisor \
    --ttl 24h \
    --output /media/usb/ot-update-20260505.rfb

# Bundle properties:
# - Total size: 2.1 MB (ladder logic + commands + crypto envelope)
# - Encrypted to: plc-controller-east public key
# - Signed by: operations engineer (alice@corp)
# - Co-signature required: ot-supervisor (bob@ot)
# - TTL: 24 hours from creation
# - Single-use nonce embedded
```

```yaml
# OT side: agent configuration
agent:
  # Watch removable media
  watchers:
    - kind: media_directory
      path: /media/usb
      poll_interval_seconds: 5
      auto_eject_after_processing: true

  # Bundles must satisfy multiple signatures
  bundle_policy:
    require_signatures:
      - any_of: ["operations-engineers"]
      - any_of: ["ot-supervisors"]
    reject_unsigned: true
    reject_expired: true
    reject_replayed: true

  # Where to write results
  result_output:
    kind: same_media
    encrypted: true
    signed: true
```

### Mode B: Optical visual transfer (QR-stream)

For one-way data flows out of facilities where even USB media is forbidden,
RavenFabric supports QR-code streaming.

```
┌──────────────────────────────────────────────────────────────────┐
│  Sender side (OT, e.g. SCADA workstation)                        │
│                                                                  │
│  ┌──────────┐                                                    │
│  │ rf-agent │ generates rolling QR codes                         │
│  └────┬─────┘                                                    │
│       │                                                          │
│       ▼                                                          │
│  ┌──────────┐                                                    │
│  │  Screen  │  ████ ████  ████  ████                             │
│  │          │  ████ ████  ████  ████   (animated, updating)      │
│  └────┬─────┘                                                    │
│       │ photons                                                  │
└───────┼──────────────────────────────────────────────────────────┘
        │
        │  ┌─── OPTICAL AIR GAP ───┐
        │  │ Camera reads QR codes │
        │  │ from screen           │
        │  └───────────────────────┘
        │
┌───────┼──────────────────────────────────────────────────────────┐
│  Receiver side (IT, monitoring station)                          │
│       │                                                          │
│       ▼                                                          │
│  ┌──────────┐                                                    │
│  │  Camera  │  reads QR stream                                   │
│  └────┬─────┘                                                    │
│       │                                                          │
│       ▼                                                          │
│  ┌──────────┐                                                    │
│  │ rf-agent │ reassembles signed bundle                          │
│  │          │ verifies integrity                                 │
│  │          │ writes to audit log                                │
│  └──────────┘                                                    │
└──────────────────────────────────────────────────────────────────┘
```

This is the pattern used in some intelligence and defense environments where
even physical media transfer is restricted. Bandwidth is low (typically
1-10 KB/sec) but sufficient for daily operational telemetry.

### Mode C: Diode-protected unidirectional network

Hardware data diodes physically permit data flow in only one direction. For
environments with diodes already installed, RavenFabric can use them as a
transport.

```yaml
agent:
  transports:
    - kind: data_diode
      direction: outbound_only        # OT → IT
      device: /dev/diode0
      protocol: udp_with_fec          # Forward Error Correction
      bandwidth_mbps: 100

    # Inbound returns require physical media
    - kind: removable_media
      direction: inbound_only         # IT → OT
      watch_path: /media/usb
```

### Mode D: HF radio / packet radio

For installations where even physical media transfer is logistically
difficult (offshore platforms, remote substations), licensed radio frequencies
provide an air-gap-respecting transport.

```yaml
agent:
  transports:
    - kind: ax25_packet_radio
      device: /dev/ttyUSB0
      frequency_mhz: 144.39
      callsign: VK3ABC
      bandwidth_kbps: 1.2
      mode: opportunistic           # use when window opens
```

This is the same protocol amateur radio operators have used for decades —
proven, well-understood, and operates entirely outside the internet.

---

## Policy Configuration

ICS environments have unique policy requirements: certain operations must
*never* be permitted, and even authorized operations may need multi-party
approval.

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: SecurityPolicy
metadata:
  name: ics-immutable-baseline
spec:
  # Rules that cannot be overridden by any tenant policy
  immutable:
    # Block specific dangerous operations entirely
    neverAllowCommands:
      - pattern: "^/.*\\.(?:cmd|bat|exe|sh|ps1)\\s+--reset-safety"
      - pattern: ".*safety-interlock.*disable"
      - pattern: ".*emergency-shutdown.*override"

    # Protect critical files
    neverAllowFileWrite:
      - /etc/safety-config/*
      - /etc/interlocks/*
      - /opt/scada/safety/*

    # Block network reconfiguration
    neverAllowCommands:
      - pattern: "^iptables"
      - pattern: "^route add"
      - pattern: "^ip link set.*up"
      - pattern: "^modprobe"

    # Require multi-party authorization for any change to safety systems
    requireApproval:
      - command_pattern: ".*safety.*"
        approvers: ["safety-officer", "operations-supervisor"]
        minApprovers: 2
        cooldown_minutes: 60      # 1-hour reflection period

    # Audit everything, no exceptions
    alwaysAudit: true
    alwaysRecordSession: true
    auditRetentionYears: 7        # regulatory requirement
---
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: ot-engineer-policy
spec:
  commands:
    allow:
      # Read-only diagnostics
      - pattern: "^/opt/scada/bin/scada-status"
      - pattern: "^/opt/scada/bin/scada-tag-read .*"
      - pattern: "^/opt/scada/bin/historian-query .*"

      # Process value tagging (operator-level)
      - pattern: "^/opt/scada/bin/scada-tag-write FT_[0-9]+ value=[0-9.]+ reason=\".*\"$"

      # Backup operations
      - pattern: "^/opt/scada/bin/scada-backup --config-only --output /backup/.*"

    deny:
      # Even with bundle, never allow these in this policy
      - pattern: "scada-tag-write.*safety"
      - pattern: "scada-config-import"
      - pattern: "ladder-update.*production"

  # All bundle executions require live signature verification
  bundles:
    require_co_signature: true
    require_recent_timestamp: true
    max_age_hours: 24
    reject_replayed: true

  # Resource limits — don't disrupt control system
  resources:
    maxCPUPercent: 10           # extremely conservative
    maxMemoryMB: 256
    taskTimeoutSeconds: 30
    nice_level: 19              # lowest priority
```

---

## Operator Workflows

### Workflow 1: Scheduled configuration update with multi-party signoff

```bash
# Engineer creates bundle on IT side
$ rf bundle create \
    --target substation-controller-12 \
    --playbook quarterly-config-update.yaml \
    --require-cosign safety-officer \
    --require-cosign operations-supervisor \
    --output /tmp/q2-2026-update.rfb

[bundle created: 3 commands, 2.4 MB]
[awaiting co-signatures...]

# Safety officer reviews and signs
$ rf bundle review /tmp/q2-2026-update.rfb
[bundle contents: ...detailed display...]
$ rf bundle sign /tmp/q2-2026-update.rfb --as safety-officer

# Operations supervisor reviews and signs
$ rf bundle review /tmp/q2-2026-update.rfb
$ rf bundle sign /tmp/q2-2026-update.rfb --as operations-supervisor

# Bundle is now ready for OT delivery
$ rf bundle export /tmp/q2-2026-update.rfb /media/usb/q2-update.rfb

# Field engineer carries USB to substation
# (Hours or days may pass)

# At substation: OT-side agent reads, verifies, executes
[OT agent log:]
[14:32:01] Bundle detected: /media/usb/q2-update.rfb
[14:32:01] Verifying signatures... 3/3 valid
[14:32:01] Verifying TTL... ok (created 4h ago, expires in 20h)
[14:32:01] Verifying nonce... ok (not previously seen)
[14:32:01] Re-validating policy... ok
[14:32:02] Executing command 1/3: scada-backup --config-only --output /backup/pre-q2.tgz
[14:32:14] Command 1 result: success (12 sec)
[14:32:14] Executing command 2/3: scada-config-import /payload/new-config.cfg
[14:32:33] Command 2 result: success (19 sec)
[14:32:33] Executing command 3/3: scada-restart-soft
[14:33:42] Command 3 result: success (69 sec)
[14:33:42] All commands complete, writing result bundle...
[14:33:43] Result written to: /media/usb/q2-update-result.rfb
[14:33:43] Result encrypted to: operations-team
[14:33:43] Bundle marked as consumed, awaiting media removal
```

### Workflow 2: Emergency incident response

```bash
# Incident detected: anomalous readings from sensor cluster
# Engineer needs to capture diagnostic data quickly

$ rf bundle create-emergency \
    --target sensor-cluster-7 \
    --command "/opt/scada/bin/diag-capture --full --output /tmp/diag.tar.gz" \
    --command "/opt/scada/bin/scada-tag-read SENSOR_* --since 1h" \
    --command "/opt/scada/bin/historian-query --range 24h --tags SENSOR_*" \
    --priority critical \
    --no-cosign-required-for-readonly \
    --output /media/usb/emergency-diag.rfb

[emergency bundle: read-only operations, single-signature, 5min TTL]
[bundle: 3 commands, awaiting media transfer]

# Field engineer rushes to facility, plugs in USB
# Agent processes bundle in seconds

[OT agent: emergency bundle detected, fast-track mode]
[verified, executing all 3 commands in parallel...]
[all commands complete in 47 seconds]
[result bundle written to USB: 14.2 MB of diagnostic data]

# Engineer carries USB back, decrypts and analyzes
$ rf bundle decrypt /media/usb/emergency-diag-result.rfb \
    --output ~/incident-data/

[result: 3 command outputs, signed by sensor-cluster-7]
[incident data extracted to ~/incident-data/]
```

### Workflow 3: Continuous data exfiltration via diode

For environments with hardware data diodes, RavenFabric provides continuous
one-way telemetry export.

```yaml
# OT-side configuration
apiVersion: ravenfabric.io/v1alpha1
kind: ContinuousExport
metadata:
  name: hourly-historian-export
spec:
  source:
    kind: scada_historian
    tags: ["FLOW_*", "PRESSURE_*", "TEMP_*"]
    interval_minutes: 60

  destination:
    kind: data_diode
    device: /dev/diode0
    encrypted_to: monitoring-team

  policy:
    max_records_per_hour: 86400
    rate_limit_kbps: 100
    retention_local_hours: 168
```

---

## Comparison with Alternatives

| Feature | USB stick (manual) | Waterfall data diodes | Owl ReCon | OPSWAT MetaDefender | RavenFabric |
|---------|-------------------|----------------------|-----------|---------------------|-------------|
| **End-to-end encryption** | No | Yes | Yes | Partial | Yes (Noise XX) |
| **Cryptographic signature** | No | Partial (vendor) | Partial (vendor) | Partial | Yes (operator + co-sign) |
| **Tamper-evident** | No | Yes | Yes | Partial | Yes |
| **Policy enforcement at target** | No | No | No | Partial | Yes |
| **Bidirectional with audit** | Partial | One-way only | One-way only | Partial | Yes |
| **Multi-party approval** | No | No | No | No | Yes |
| **No vendor hardware required** | Yes | No | No | No | Yes |
| **Open source** | N/A | No | No | No | Yes (AGPLv3) |
| **Cost (small site)** | $ | $$$$ | $$$$$ | $$$ | Hardware only |
| **Cost (1000 sites)** | $ (operational risk) | $$$$$ | $$$$$$ | $$$$$ | Hardware only |

---

## Implementation Status

### Available today (v0.1)

- Cryptographic device identity (Curve25519)
- Policy-validated execution
- Structured audit logging
- End-to-end Noise XX encryption (online use)

### Coming in v0.4

- Sealed bundles (signed + encrypted command packages)
- Multi-signature requirements
- TTL-bounded bundles with replay protection

### Coming in v0.5

- NNCP transport (sneakernet via removable media)
- Serial driver (RS-232, USB) for diode connectivity
- DTN bundle protocol (RFC 9171)

### Coming in v0.6+

- QR-code stream transport (optical air gap)
- HF/VHF packet radio integration
- Hardware diode integration patterns

---

## Why This Matters

Critical infrastructure cybersecurity is not a theoretical concern. Public
incidents in recent years have included:

- **Power grid attacks** in multiple countries
- **Water treatment facility intrusions** with attempts to alter chemical
  dosing
- **Manufacturing ransomware** halting production for weeks
- **Pipeline shutdowns** with cascading economic impact
- **Hospital systems** losing operational technology to ransomware

The standard response — disconnect critical systems from networks — is
correct in principle but creates the operational dilemma described in this
document. Engineers find ways to update air-gapped systems regardless of
official policy, and these workarounds tend to be the actual failure points.

RavenFabric proposes that the air gap need not be a binary choice between
"connected and at risk" or "disconnected and unmaintainable":

> **The air gap is preserved physically, but operations teams retain
> cryptographically secure, policy-enforced, fully audited access through
> physical media or alternative transports. The trust boundary moves from
> the network to the cryptographic envelope.**

For operators of regulated critical infrastructure — under frameworks like
NIS2 (EU), NERC CIP (North America), IEC 62443 (industrial), or
sector-specific regulations — this approach addresses requirements that
existing tools cannot:

- **Demonstrable cryptographic integrity** of every operation
- **Multi-party authorization** for safety-impacting changes
- **Complete audit trail** including the full command, environment, and
  result
- **Replay protection** against compromised media
- **Time-bounded authority** that auto-expires
- **Vendor neutrality** — no single point of vendor compromise

The traditional answer to ICS security has been hardware data diodes from
specialist vendors at significant cost. RavenFabric does not replace these
where they are deployed, but provides a software-layer complement that
extends to environments where dedicated hardware is impractical or
economically infeasible.

---

## See Also

- [README.md](../README.md) — RavenFabric overview
- [CONNECTIVITY.md](../CONNECTIVITY.md) — Multi-transport architecture
- [usecase-cloudnativepg.md](usecase-cloudnativepg.md) — CloudNativePG admin access
- [usecase-edge-iot-fleet.md](usecase-edge-iot-fleet.md) — Edge & IoT fleet management
- [usecase-multi-cluster-kubernetes.md](usecase-multi-cluster-kubernetes.md) — Multi-cluster Kubernetes
- [usecase-msp-multitenant.md](usecase-msp-multitenant.md) — MSP multi-tenant operations
- [IEC 62443](https://www.iec.ch/cyber-security) — Industrial cybersecurity
  standards
- [NIST SP 800-82](https://csrc.nist.gov/publications/detail/sp/800-82/rev-3/final) —
  Guide to ICS Security
- [NNCP Project](http://www.nncpgo.org/) — Sneakernet-friendly file transfer
