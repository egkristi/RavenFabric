# Maritime Vessels & Offshore Installations

Secure fleet-wide IT/OT management across commercial vessels, offshore platforms, and wind installations — operating through intermittent satellite connectivity, hostile environments, and strict regulatory requirements.

## The Problem

Maritime and offshore environments break the assumptions that conventional IT management tools rely on:

### Connectivity is fundamentally different

A typical merchant vessel transitions through five distinct connectivity regimes during a single voyage:

```
Port (high-bandwidth Wi-Fi/LTE)
   │
   ▼
Coastal waters (LTE, then degrading 3G)
   │
   ▼
Open ocean (VSAT only — Inmarsat FleetXpress, Iridium Certus, Starlink Maritime)
   │
   ▼
High-latitude or polar regions (Iridium SBD only, kilobits per second)
   │
   ▼
Total connectivity loss (equipment failure, antenna icing, jamming, deliberate)
```

Each regime has different bandwidth costs, latencies, and reliability characteristics. A fleet IT system must operate across **all** of them without failing in any.

### Bandwidth is metered and expensive

VSAT bandwidth on commercial vessels typically costs **€2-15 per megabyte**. Iridium SBD costs **€0.50-2 per kilobyte**. A 50MB software update is not a trivial operation — it can cost €100-750 per vessel per update. Multiplied across a fleet of 50 vessels, this becomes operationally significant.

### Vessels are hostile environments for hardware

- Salt corrosion attacks every connector and PCB over time
- Vibration from engines and propellers stresses solder joints
- Temperature swings of 40°C+ between cold rooms and engine rooms
- Power instability (generator transitions, shore power changes, brownouts)
- Physical access for repair may require helicopter or supply boat

Equipment must work for years without intervention, restart cleanly after power loss, and survive the occasional flooding incident.

### Regulatory compliance is non-trivial

Multiple overlapping regulatory regimes:

- **IMO MSC.428(98)** — Maritime Cyber Risk Management (mandatory since 2021)
- **NIS2** (EU) — Maritime sector explicitly covered from 2024
- **DNV/Lloyd's Register/ABS** — Class society cyber notations
- **TMSA 3** — Tanker Management and Self Assessment (oil majors)
- **CIP** for offshore platforms in some jurisdictions
- **Flag state requirements** that vary by country
- **Port state inspections** including cyber elements (increasingly common)

### Air-gap is sometimes mandatory

Naval, government research, and certain commercial vessels operate with strict air-gap policies for portions of their network. Crew Wi-Fi and operational technology (engine controls, navigation, cargo systems) must not interconnect.

### Fleets are heterogeneous

A single fleet may include:

- **Bulk carriers** with minimal IT (GPS, AIS, ECDIS, basic comms)
- **Container ships** with extensive cargo monitoring
- **Tankers** with explosion-rated equipment requirements (ATEX/IECEx)
- **Offshore supply vessels** with dynamic positioning systems
- **Drilling platforms** with industrial control systems
- **Wind installation vessels** with crane SCADA
- **Research vessels** with specialized scientific instruments
- **Fishing vessels** with catch monitoring

Each has different operational technology, different update needs, and different criticality profiles.

### Traditional approaches and their problems

```
Fleet IT operations
    │
    ├─→ VSAT vendor portal (Inmarsat / KVH / Marlink)
    │   ├─ Vessel-by-vessel manual updates
    │   ├─ Limited execution capability
    │   └─ Vendor lock-in, expensive
    │
    ├─→ TeamViewer / VNC over satellite
    │   ├─ Bandwidth disaster (full-screen video over $5/MB link)
    │   └─ Audit trail = video file someone might watch
    │
    ├─→ "Send laptop with engineer to vessel"
    │   ├─ Works, but slow and expensive
    │   ├─ Helicopter cost €5,000-15,000 per visit
    │   └─ Limited to scheduled port calls
    │
    └─→ "Hope vessel IT can fix it themselves"
        └─ Often the actual practice
```

**Issues:**

- **Vendor lock-in to maritime SaaS providers** — Inmarsat, KVH, Marlink charge significant premiums and limit what can be done
- **No unified fleet view** — each vessel is its own island, often literally
- **Bandwidth-wasteful tooling** — most enterprise IT tools assume corporate LAN bandwidth, not satellite economics
- **No air-gap support** — when satellite link fails, no fallback
- **Audit trails are weak** — incident investigation across fleet is archaeology
- **Compliance evidence is manual** — preparing for audits is a project, not an automated report
- **Update windows are operational risks** — a failed update at sea may not be recoverable until next port call

---

## How RavenFabric Solves It

```
Fleet operations center (shore)
    │
    │  rf exec --selector "fleet=tankers" "system_health_check"
    │  rf state apply security-baseline-2026q2.yaml
    │  rf telemetry sync --all-vessels --priority cost-optimized
    ▼
RavenFabric Relay (geo-distributed shore-side)
    │
    ▼  Multi-transport, cost-aware:
    ▼  - Wi-Fi/LTE in port (cheap, high-bandwidth)
    ▼  - VSAT at sea (expensive, metered)
    ▼  - Iridium SBD as fallback (very expensive, very low bandwidth)
    ▼  - LoRa/Wi-Fi mesh between vessels in convoy
    ▼  - Sneakernet via crew laptop in port (free)
    ▼
Each vessel: rf-agent on shipboard servers
    │  ├─ Bandwidth-aware: defers low-priority traffic until cheaper link
    │  ├─ Air-gap-aware: separate policy zones for OT vs IT networks
    │  ├─ Resilient: queues commands during connectivity loss
    │  ├─ Bidirectional: telemetry flows out as carefully as commands flow in
    │  └─ Compliance-grade audit: every action recorded, signed, time-stamped
    ▼
Vessel systems (IT and selectively OT)
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Bandwidth-cost-aware operation** | Policy distinguishes cheap (port Wi-Fi) from expensive (VSAT) from extreme (Iridium) transports |
| **Connectivity-tolerant by design** | Vessels remain manageable when satellite link fails |
| **Multi-transport per vessel** | Same fabric uses Wi-Fi, VSAT, Iridium, LoRa, sneakernet automatically |
| **Compliance-grade audit** | Cryptographically signed audit trail per vessel for IMO/NIS2 evidence |
| **Air-gap support for OT** | Operational technology networks can be reached only via deliberate physical media transfer |
| **Fleet-wide consistency** | Same configuration baseline enforced across heterogeneous fleet |
| **Vendor-neutral** | No lock-in to single satellite provider or maritime IT vendor |
| **Self-hosted control plane** | Data residency under fleet operator's jurisdiction |
| **Tamper detection** | Detects MITM attempts including state-actor interception of satellite traffic |
| **Continuous compliance** | Automated evidence generation for class society inspections |

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  Fleet Operations Center (shore-based, multiple offices)           │
│                                                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐             │
│  │ Fleet IT     │  │ Engineering  │  │ Compliance   │             │
│  │ operators    │  │ supervisors  │  │ officers     │             │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘             │
└─────────┼─────────────────┼──────────────────┼────────────────────┘
          │                 │                  │
          ▼                 ▼                  ▼
┌────────────────────────────────────────────────────────────────────┐
│  RavenFabric Relay Mesh (geo-distributed)                          │
│                                                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐             │
│  │ relay-eu     │  │ relay-asia   │  │ relay-am     │             │
│  │ (Oslo/AMS)   │  │ (SG/HK)     │  │ (NY/SF)      │             │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘             │
└─────────┼─────────────────┼──────────────────┼────────────────────┘
          │                 │                  │
          │  Multiple paths per vessel:        │
          │  - VSAT (Inmarsat/KVH/Iridium/Starlink)
          │  - LTE in coastal waters
          │  - Wi-Fi in port
          │  - Iridium SBD as last resort
          │
    ┌─────┴─────────────┬────────────────┬──────────────┐
    │                   │                │              │
    ▼                   ▼                ▼              ▼
┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
│ M/V Atlantic│  │ M/V Pacific │  │ Drilling    │  │ Wind farm   │
│ Trader      │  │ Hauler      │  │ platform    │  │ install     │
│             │  │             │  │ "Sea Crown" │  │ vessel      │
│ ┌─────────┐ │  │ ┌─────────┐ │  │             │  │             │
│ │Bridge   │ │  │ │Bridge   │ │  │ ┌─────────┐ │  │ ┌─────────┐ │
│ │ network │ │  │ │ network │ │  │ │Driller's│ │  │ │Crane    │ │
│ │         │ │  │ │         │ │  │ │ cabin   │ │  │ │ control │ │
│ │rf-agent │ │  │ │rf-agent │ │  │ │         │ │  │ │         │ │
│ │ (IT)    │ │  │ │ (IT)    │ │  │ │rf-agent │ │  │ │rf-agent │ │
│ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │
│             │  │             │  │             │  │             │
│ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │
│ │Engine   │ │  │ │Cargo    │ │  │ │Drill    │ │  │ │Marine   │ │
│ │ control │ │  │ │ monitor │ │  │ │ control │ │  │ │ ops     │ │
│ │         │ │  │ │         │ │  │ │         │ │  │ │         │ │
│ │rf-agent │ │  │ │rf-agent │ │  │ │rf-agent │ │  │ │rf-agent │ │
│ │ (OT,    │ │  │ │ (OT)   │ │  │ │ (OT,    │ │  │ │ (IT/OT) │ │
│ │  air-   │ │  │ │         │ │  │ │  air-   │ │  │ │         │ │
│ │  gap)   │ │  │ │         │ │  │ │  gap)   │ │  │ │         │ │
│ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │
│             │  │             │  │             │  │             │
│ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │
│ │Crew     │ │  │ │Crew     │ │  │ │Crew     │ │  │ │Crew     │ │
│ │ network │ │  │ │ network │ │  │ │ network │ │  │ │ network │ │
│ │         │ │  │ │         │ │  │ │         │ │  │ │         │ │
│ │(no agent│ │  │ │(no agent│ │  │ │(no agent│ │  │ │(no agent│ │
│ │ — sep.) │ │  │ │ — sep.) │ │  │ │ — sep.) │ │  │ │ — sep.) │ │
│ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │
└─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘
```

### Per-vessel network segmentation

A typical merchant vessel has three or four distinct network segments, each with different management requirements:

```
┌─────────────────────────────────────────────────────────────────┐
│  Bridge / Navigation Network                                    │
│  - ECDIS, ARPA, AIS, GNSS, anemometer                          │
│  - Relatively standard IT, manageable from shore                │
│  - rf-agent: full management capability                         │
├─────────────────────────────────────────────────────────────────┤
│  Operational Technology (OT) Network                            │
│  - Engine controls, ballast, cargo handling                     │
│  - Air-gapped from internet by design                           │
│  - Updates only via authorized physical media                   │
│  - rf-agent: NNCP/sneakernet mode only                          │
├─────────────────────────────────────────────────────────────────┤
│  Crew Welfare Network                                           │
│  - Personal devices, internet access, entertainment             │
│  - Logically separate from operational systems                  │
│  - rf-agent: NOT deployed (out of scope)                        │
├─────────────────────────────────────────────────────────────────┤
│  Cargo Monitoring (where applicable)                            │
│  - Reefer containers, tank levels, cargo tracking               │
│  - Requires real-time-ish telemetry                             │
│  - rf-agent: telemetry collection + occasional commands         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Deployment Patterns

### Pattern A: Single shipboard server with multiple agents

Most modern vessels have an "IT room" with one or two ruggedized servers. Each network segment hosts a RavenFabric agent that communicates with the shore-side relay.

```yaml
# /etc/ravenfabric/agent-bridge.yaml — Bridge network agent
agent:
  identity_path: /etc/ravenfabric/identity-bridge.key
  policy_path: /etc/ravenfabric/policy-bridge.yaml
  audit_path: /var/log/ravenfabric/audit-bridge.jsonl

  vessel_metadata:
    imo: "9123456"
    name: "M/V Atlantic Trader"
    flag: "NO"
    vessel_type: "container"
    classification: "DNV"
    fleet: "northern-shipping"

  # Multi-transport with cost awareness
  transports:
    - kind: opportunistic_wifi
      ssid_patterns: ["port-*", "marina-*"]
      cost_class: free
      priority: 1

    - kind: lte
      cost_class: cheap
      bandwidth_estimated_kbps: 5000
      priority: 2

    - kind: vsat_inmarsat
      cost_per_mb_eur: 8.50
      cost_class: expensive
      bandwidth_estimated_kbps: 384
      priority: 3

    - kind: iridium_sbd
      cost_per_kb_eur: 0.85
      cost_class: extreme
      bandwidth_estimated_bps: 2400
      priority: 4
      use_only_when:
        - vsat_unavailable_for_minutes: 30
        - command_priority: critical

  # Bandwidth-aware queue management
  queue:
    persistence_path: /var/lib/ravenfabric/queue.db
    max_size_mb: 500

    # Defer low-priority traffic for cheaper links
    deferral_rules:
      - priority: low
        defer_unless_transport_class: free
      - priority: normal
        defer_unless_transport_class: ["free", "cheap"]
      - priority: high
        allow_transport_class: ["free", "cheap", "expensive"]
      - priority: critical
        allow_transport_class: any
```

### Pattern B: OT-side agent in air-gap mode

For operational technology networks (engine controls, drilling, dynamic positioning), the agent operates in strict air-gap mode — never connecting to any network, only processing bundles from removable media.

```yaml
# /etc/ravenfabric/agent-ot.yaml — OT network agent
agent:
  identity_path: /etc/ravenfabric/identity-ot.key
  policy_path: /etc/ravenfabric/policy-ot.yaml

  mode: air_gap_strict

  # No network transports configured
  transports:
    # Only physical media accepted
    - kind: nncp_removable_media
      watch_paths:
        - /media/usb
        - /media/sd
      auto_eject: true
      verify_signatures_required: 2

  # Air-gap-specific policy
  air_gap:
    require_co_signature: true
    minimum_signers: 2
    require_recent_timestamp_hours: 24
    reject_replayed: true

    # Outbound (telemetry export to IT side via media)
    outbound_export:
      enabled: true
      schedule: daily_at_port_call
      destination: media_directory
      sign_with_ot_identity: true
      encrypt_to: shore_compliance_team
```

### Pattern C: Convoy mesh for vessels in formation

For naval auxiliaries, fishing fleets, or cruise ships traveling together, LoRa-mesh provides intra-fleet communication that doesn't depend on satellite.

```yaml
agent:
  transports:
    - kind: reticulum_lora
      device: /dev/ttyUSB-lora0
      frequency_band: 868mhz_eu
      tx_power_dbm: 20
      bandwidth_kbps: 11
      mesh_role: relay

      # Other vessels in convoy
      neighbors_dynamic: true
      announce_interval_seconds: 300

      # LoRa-mesh for fleet coordination, not high-bandwidth ops
      use_for:
        - fleet_status_messages
        - coordinated_navigation_alerts
        - low_priority_telemetry_aggregation
```

---

## Policy Configuration

### Cost-aware command policy

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: vessel-bridge-policy
  vessel_class: merchant
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
      - pattern: ".*chart-overlay-modify"
      - pattern: ".*safety-system.*"

  # Bandwidth-cost awareness
  transport_policy:
    # Routine maintenance — only on cheap links
    routine:
      command_patterns:
        - "^apt-get update$"
        - "^apt-get upgrade.*$"
      allowed_transport_classes: ["free", "cheap"]
      deny_classes: ["expensive", "extreme"]

    # Health checks — any link
    monitoring:
      command_patterns:
        - "^uptime$"
        - "^df -h$"
        - "^/usr/local/bin/.*-status$"
      allowed_transport_classes: any
      max_response_size_kb: 4

    # Emergency operations — even Iridium acceptable
    emergency:
      command_patterns:
        - "^/usr/local/bin/emergency-.*"
      allowed_transport_classes: any
      requires_approval: false
      max_response_size_kb: 16

    # Large data transfer — only port Wi-Fi
    bulk:
      command_patterns:
        - "^.*(backup|export|sync).*"
      allowed_transport_classes: ["free"]
      defer_until_cheap_link: true

  # Resource limits — modest, vessel hardware is constrained
  resources:
    maxCPUPercent: 25
    maxMemoryMB: 256
    taskTimeoutSeconds: 60
    maxOutputBytes: 1048576

  # Vessel-specific approval requirements
  approval:
    required:
      # Software updates need fleet ops + on-board officer
      - pattern: "apt-get upgrade"
        approvers: ["fleet-it-supervisor", "ship-master"]
        minApprovers: 2
        timeoutSeconds: 86400

      # Anything touching VDR (Voyage Data Recorder) is sensitive
      - pattern: ".*voyage-data-recorder.*"
        approvers: ["fleet-it-supervisor", "compliance-officer"]
        minApprovers: 2
```

### Air-gap (OT) policy with multi-signature

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: SecurityPolicy
metadata:
  name: vessel-ot-immutable
spec:
  immutable:
    # OT systems are never directly accessible
    networkAccess: forbidden

    # Bundles only — and require multiple signatures
    bundleRequirements:
      minSigners: 2
      requireRoles:
        - fleet-it-supervisor
        - ot-engineering-supervisor
      maxAgeHours: 48
      rejectReplayed: true

    # Never execute these even with valid bundles
    neverAllowCommands:
      - pattern: ".*safety-interlock.*disable"
      - pattern: ".*emergency-shutdown.*override"
      - pattern: ".*classification-society.*"
      - pattern: ".*flag-state-reporting.*delete"

    # Critical files protected absolutely
    neverAllowFileWrite:
      - /etc/ravenfabric/*
      - /opt/safety-system/*
      - /var/lib/voyage-data-recorder/*
      - /opt/dynamic-positioning/configs/*

    # Audit retention for class society inspections
    auditRetentionYears: 7
```

---

## Operator Workflows

### Workflow 1: Daily fleet status check

```bash
$ rf fleet status

FLEET: northern-shipping (47 vessels, 3 platforms)

REGION: At sea
─────────────────────────────────────────────────────────────────
M/V Atlantic Trader   IMO 9123456   VSAT good    last contact 8m
M/V Pacific Hauler    IMO 9234567   VSAT good    last contact 4m
M/V Nordic Star       IMO 9345678   Iridium      last contact 18m  ⚠
M/V Baltic Pioneer    IMO 9456789   VSAT good    last contact 2m
... (33 more vessels at sea)

REGION: In port
─────────────────────────────────────────────────────────────────
M/V Hamburg Express   IMO 9567890   Wi-Fi        in port: Hamburg
M/V Rotterdam Star    IMO 9678901   Wi-Fi        in port: Rotterdam
... (8 more in port)

OFFSHORE:
─────────────────────────────────────────────────────────────────
Platform Sea Crown    Field: North Sea    VSAT good    last contact 1m
Platform Northern     Field: Barents      VSAT degr.   last contact 47m  ⚠
Wind installer Atlas  Field: Doggerbank   Wi-Fi (port) in port

Issues requiring attention:
─────────────────────────────────────────────────────────────────
⚠ M/V Nordic Star: VSAT down, on Iridium fallback
  └─ scheduled satellite swap at next port call (Tromsø, 14 days)

⚠ Platform Northern: high latency on VSAT (>2000ms)
  └─ likely weather-related antenna issue
  └─ remote diagnostic queued, will run when latency improves
```

### Workflow 2: Bandwidth-aware fleet patch deployment

```yaml
# fleet-security-patch-2026-q2.yaml
apiVersion: ravenfabric.io/v1alpha1
kind: Playbook
metadata:
  name: q2-2026-security-baseline
spec:
  targets:
    selector:
      matchLabels:
        fleet: northern-shipping

  strategy:
    type: bandwidth_aware_canary

    # Phase 1: vessels currently in port (free Wi-Fi)
    phase_1:
      selector_addition:
        connectivity: "wifi-port"
      execution: parallel
      timeout: 4_hours

    # Phase 2: vessels with cheap LTE coastal connection
    phase_2:
      selector_addition:
        connectivity: ["lte-coastal", "wifi-port"]
      execution: rolling
      parallel: 5
      pause_between_minutes: 30

    # Phase 3: VSAT vessels (expensive, careful)
    phase_3:
      selector_addition:
        connectivity: ["vsat", "wifi-port"]
      execution: rolling
      parallel: 2
      pause_between_minutes: 60
      bandwidth_budget_per_vessel_mb: 25

    # Phase 4: defer until next port call
    phase_4:
      selector_addition:
        connectivity: "iridium-only"
      execution: deferred
      defer_until_transport_class: ["free", "cheap"]

  steps:
    - name: download-patches
      command: "/usr/local/bin/security-update download --priority high"
      timeout_seconds: 1800

    - name: verify-signatures
      command: "/usr/local/bin/security-update verify"
      onFailure: abort

    - name: pre-update-backup
      command: "/usr/local/bin/system-backup --essential-only"
      timeout_seconds: 600

    - name: apply-patches
      command: "/usr/local/bin/security-update apply"
      timeout_seconds: 1800
      approval_required: true
      approvers: ["fleet-it-supervisor", "ship-master"]

    - name: verify-systems
      command: "/usr/local/bin/post-update-verification"
      timeout_seconds: 300
      onFailure: rollback
```

```bash
$ rf playbook apply fleet-security-patch-2026-q2.yaml

[playbook: q2-2026-security-baseline]
[targets: 47 vessels, 3 platforms]

[phase 1: 8 vessels in port — parallel execution]
[phase 1: all 8 patched successfully in 2h 14m]
[bandwidth used: 0 MB billable (free Wi-Fi)]

[phase 2: 12 vessels in coastal LTE — rolling]
[phase 2: 12/12 patched successfully in 6h 47m]
[bandwidth used: ~480 MB LTE (≈€48 across vessels)]

[phase 3: 23 vessels on VSAT — careful rolling, 2 at a time]
[phase 3: 18/23 patched, 5 in progress]
[bandwidth used so far: 412 MB VSAT (≈€3,500 across vessels)]
[estimated remaining: 110 MB / €935]

[phase 4: 4 vessels on Iridium-only — deferred to next port]
[phase 4: queued for next port call (estimated 3-9 days)]

OVERALL:
  total vessels: 47
  patched: 38 (81%)
  in progress: 5
  deferred: 4 (waiting for cheaper link)
  failed: 0
  total bandwidth cost: ~€3,580
  alternative cost (full VSAT): ~€18,400
  saved: ~€14,820 through bandwidth-aware deployment
```

### Workflow 3: Compliance evidence generation

```bash
# Generate IMO MSC.428(98) compliance report for fleet
$ rf compliance report --framework imo-msc-428-98 \
    --fleet northern-shipping \
    --period 2026-Q1 \
    --output q1-2026-imo-compliance.pdf

[generating IMO MSC.428(98) report for Q1 2026]
[gathering audit data from 47 vessels...]
[audit events processed: 487,234]

REPORT SECTIONS:
  ✓ Section 1: Risk Management Framework
    - Policy enforcement: 100% (every action audited)
    - Identification of vulnerabilities: documented
    - Continuous monitoring: 47/47 vessels reporting
    - Audit findings: 0 critical, 3 medium, 12 low

  ✓ Section 2: Cyber Security Risk Assessment
    - Asset inventory: complete (468 IT assets, 89 OT assets)
    - Vulnerability scans: monthly, all vessels
    - Patch compliance: 96.3% (vessels in deferred queue: 4)
    - Threat intelligence integration: documented

  ✓ Section 3: Detection
    - Tamper detection events: 2 (resolved, no compromise)
    - Anomalous traffic: 17 events flagged, all benign
    - Failed authentication attempts: 234 (all blocked)

  ✓ Section 4: Response and Recovery
    - Incidents: 0 critical, 1 medium, 8 informational
    - Mean time to detect: 4.2 minutes
    - Mean time to respond: 47 minutes
    - Recovery procedures tested: monthly

  ✓ Section 5: Continual Improvement
    - Lessons learned documented: 12 items
    - Policy revisions: 3 implemented
    - Training completed: 89% crew, 100% IT staff

[report generated: q1-2026-imo-compliance.pdf, 47 pages]
[cryptographically signed by fleet IT supervisor]
[ready for class society inspection]
```

### Workflow 4: Emergency response when satellite link fails

```bash
# Vessel reports critical issue via Iridium SBD (low-bandwidth fallback)
[INCIDENT]
M/V Nordic Star (IMO 9345678) reports:
  Engine room temperature anomaly
  Main engine performance degraded
  VSAT inoperative — Iridium SBD only
  Vessel position: 71°N, 23°E (Barents Sea)
  Crew safety: not threatened
  Estimated nearest port: 4 days

# Operator queues diagnostic commands optimized for low-bandwidth
$ rf exec --vessel "nordic-star" \
    --priority critical \
    --transport iridium-sbd-acceptable \
    --max-response-bytes 1024 \
    "/usr/local/bin/engine-diagnostics --critical-only"

[transmission: 187 bytes outbound (command)]
[transmission: 942 bytes inbound (response)]
[total cost: €0.96 over Iridium SBD]

DIAGNOSTIC RESULT:
  Engine 1 (main): cylinder 4 head temperature 487°C (normal: 400-450°C)
  Engine 1 oil pressure: nominal
  Engine 1 turbocharger: nominal
  Cooling system: pump 2 flow rate 15% below normal
  Recommendation: reduce power to 75%, inspect pump 2

# Fleet operations coordinates with ship master
$ rf exec --vessel "nordic-star" \
    --priority critical \
    --transport iridium-sbd-acceptable \
    "/usr/local/bin/operator-message broadcast \
     'OPS: Reduce engine power to 75%. \
     Inspect cooling pump 2 (location: ER-port-3). \
     Acknowledge.'"

[message delivered to vessel]
[acknowledgment received: 4 minutes]
```

### Workflow 5: Sneakernet update for OT systems at port call

```bash
# Vessel arrives at scheduled port call. Engineer prepares OT update bundle.
$ rf bundle create \
    --target "vessel-atlantic-trader-ot-network" \
    --playbook "ballast-system-firmware-v3.2.yaml" \
    --require-cosign fleet-it-supervisor \
    --require-cosign ot-engineering-supervisor \
    --output /media/usb/atlantic-trader-ot-update.rfb

[bundle: 47 commands across 3 OT systems]
[ballast control: firmware v3.2 update]
[engine monitoring: configuration update]
[cargo handling: certificate rotation]

[awaiting cosignatures...]
[fleet-it-supervisor signed]
[ot-engineering-supervisor signed]

[bundle ready: 14.2 MB on USB]
[encrypted to vessel's OT identity key]
[TTL: 72 hours]

# Engineer carries USB on board during port visit
# Plugs into ship's OT-network workstation
# OT agent processes bundle:

[OT agent on ATLANTIC-TRADER-OT-1]
[14:32:01] USB media detected: /media/usb
[14:32:01] Bundle found: atlantic-trader-ot-update.rfb
[14:32:01] Verifying signatures...
[14:32:01] - fleet-it-supervisor: valid
[14:32:01] - ot-engineering-supervisor: valid
[14:32:01] Both required co-signatures present: ok
[14:32:02] Bundle TTL: 47 hours remaining: ok
[14:32:02] Bundle nonce: not previously seen: ok
[14:32:02] Re-validating policy locally...
[14:32:02] Policy approved: 47 commands within bounds

[14:32:03] Executing playbook: ballast-system-firmware-v3.2
[14:32:03] - backup current config
[14:32:18] - verify backup integrity
[14:32:33] - apply ballast firmware update
[14:35:47] - verify ballast system online
[14:36:12] - run integration tests
[14:38:33] - all tests passed

[14:38:33] All 47 commands complete
[14:38:34] Writing result bundle...
[14:38:34] Result encrypted to: shore-fleet-operations
[14:38:34] Result written to: /media/usb/result-atlantic-trader-ot.rfb
[14:38:34] Bundle marked consumed
[14:38:34] Audit log entry: 47 actions, all successful

# Engineer takes USB back to shore office
# Result bundle decrypted and verified
$ rf bundle decrypt /media/usb/result-atlantic-trader-ot.rfb \
    --output ~/audit/atlantic-trader-ot-2026-05-15.json

[result extracted]
[all 47 actions: success]
[total OT update time: 6 minutes 31 seconds]
[bandwidth used: 0 (sneakernet)]
[shore-side audit log updated: 47 entries added]
[compliance evidence: archived for class society review]
```

---

## Specialized Maritime Considerations

### Class society integration

Major class societies (DNV, Lloyd's Register, ABS, ClassNK) increasingly require cyber-readiness evidence. RavenFabric's audit logs map directly to common requirements:

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: ComplianceMapping
metadata:
  name: dnv-cyber-secure-class-notation
spec:
  framework: "DNV-RP-0496 Cyber security resilience management"

  evidence_requirements:
    - requirement: "REQ-IDENT-1 Asset inventory"
      ravenfabric_evidence:
        - source: agent_grains
        - aggregation: per_vessel_asset_list

    - requirement: "REQ-PROT-3 Access control"
      ravenfabric_evidence:
        - source: audit_log
        - filter: "auth_events"
        - aggregation: per_vessel_per_operator

    - requirement: "REQ-DETECT-2 Anomaly detection"
      ravenfabric_evidence:
        - source: tamper_detection_events
        - source: failed_handshakes

    - requirement: "REQ-RESP-1 Incident response capability"
      ravenfabric_evidence:
        - source: playbook_execution_logs
        - filter: "tag=incident-response"
```

### Flag state and port state requirements

Different flag states have different cybersecurity requirements. RavenFabric's policy structure supports per-vessel customization:

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: VesselPolicy
metadata:
  name: norwegian-flag-vessel
spec:
  vessel_metadata:
    flag: "NO"

  # Flag state: Norway requires specific evidence
  flag_state_compliance:
    framework: "Sjøfartsdirektoratet maritime cyber guidelines"
    evidence_export_format: "norwegian-maritime-authority-format"
    retention_years: 7

  # Port state: vessel may visit ports with different requirements
  port_state_preparedness:
    expected_ports:
      - country: "US"
        framework: "USCG MERS"
      - country: "EU"
        framework: "EMSA cyber risk management"
      - country: "SG"
        framework: "MPA cyber risk guidelines"

    # Pre-arrival cyber attestation
    pre_arrival_attestation:
      generate_hours_before_arrival: 48
      include_audit_summary: true
      include_compliance_certificates: true
```

### Iridium SBD optimization

Iridium Short Burst Data is the universal fallback for global maritime connectivity. It is **extremely** bandwidth-constrained (340 bytes per message inbound, 270 bytes outbound) and expensive (per-byte pricing). RavenFabric optimizes for this constraint:

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: TransportPolicy
metadata:
  name: iridium-sbd-optimization
spec:
  transport: iridium_sbd

  # Compression and encoding
  encoding:
    compress: zstd_max
    binary_format: msgpack_minimal
    omit_optional_fields: true

  # Message size discipline
  size_discipline:
    inbound_max_bytes: 340
    outbound_max_bytes: 270
    fragmentation: enabled
    reassembly_timeout_seconds: 1800

  # Cost-aware throttling
  rate_limits:
    max_messages_per_hour: 24
    max_total_bytes_per_day: 5120

  # Priority queue (Iridium expensive — only critical traffic)
  allowed_priorities:
    - critical
    - high

  reject_priorities:
    - normal
    - low
    - bulk
```

---

## Comparison with Alternatives

| Feature | Maritime SaaS (Inmarsat/KVH/Marlink) | TeamViewer/VNC | Manual (engineer flies out) | RavenFabric |
|---------|---------------------------------------|----------------|------------------------------|-------------|
| **Self-hosted** | No | No | N/A | Yes |
| **Vendor-neutral satellite** | Locked to provider | Any link, bandwidth-blind | N/A | Yes |
| **Bandwidth-cost-aware** | Some metering | Disastrous over satellite | N/A | Native |
| **Air-gap support (OT)** | No | No | Yes (manual) | Yes (NNCP) |
| **Multi-transport** | Vendor's stack only | No | N/A | Yes |
| **Compliance-grade audit** | Partial | No | No | Yes |
| **Class society reporting** | Manual | No | No | Automatable |
| **Tamper detection** | No | No | N/A | Yes |
| **Cost (50-vessel fleet, annual)** | $500k-2M | $50k-200k + bandwidth | $100k-500k (helicopter) | Hardware + relay hosting |
| **Time to onboard new vessel** | 2-4 weeks | 1 week | N/A | Hours |
| **Lock-in** | Severe | Medium | Low | None (AGPLv3) |

---

## Implementation Status

This use case relies on RavenFabric capabilities at varying maturity levels.

### Available today (v0.1)

- Cryptographic identity per agent (vessel/platform)
- Policy-validated command execution
- Structured audit logging
- Outbound-only agent connection (works through any satellite NAT)
- End-to-end encryption uniform across transports

### Coming in v0.2-v0.3

- Multiple transport drivers (QUIC, WireGuard direct)
- Cost-aware transport selection
- Multi-vessel fleet playbooks
- Bandwidth-aware rolling deployment
- Interactive shell (for in-port engineering)

### Coming in v0.4-v0.5

- Approval workflows (fleet-IT + ship-master multi-sig)
- Sealed bundles for OT systems (sneakernet/NNCP)
- Reticulum/LoRa for inter-vessel mesh
- Iridium SBD transport driver
- Starlink Maritime optimization
- Compliance reporting framework
- Class society evidence export

---

## Adoption Path

### Phase 1: Pilot vessel (months 1-3)

Deploy on a single non-critical vessel as a proof of concept:

- One agent on vessel's IT network
- Connection to test relay infrastructure ashore
- Read-only operations only (status checks, log collection)
- Validate connectivity over actual VSAT in actual operating conditions
- Measure actual bandwidth costs vs estimates

### Phase 2: Expand to fleet subset (months 4-9)

- Roll out to 5-10 vessels of similar profile
- Add write operations under approval workflow
- Begin compliance evidence generation
- Establish operational runbooks
- Train fleet IT operators

### Phase 3: OT integration (months 10-15)

- Add OT-side agents with strict air-gap policy
- Establish bundle creation/signing workflow ashore
- Train engineers on USB-based update procedures
- Validate with class society on audit evidence quality

### Phase 4: Full fleet rollout (months 16-24)

- Expand to entire fleet
- Decommission legacy vendor SaaS where possible
- Establish ongoing compliance reporting cadence
- Develop in-house playbook library

### Phase 5: Advanced operations (months 24+)

- Fleet-wide optimization based on collected telemetry
- Predictive maintenance from collected metrics
- Inter-vessel mesh for convoy operations
- Customer-specific compliance evidence packages

---

## Why This Matters

The maritime industry is navigating an accelerating cybersecurity transition:

**Regulatory:** IMO MSC.428(98) made cyber risk management mandatory in 2021. NIS2 added EU-specific requirements in 2024. Class societies (DNV, Lloyd's Register, ABS, ClassNK) are introducing cyber notations that will increasingly affect insurance and charter rates. Port states are adding cyber elements to inspections.

**Operational:** Maritime cyber incidents are no longer hypothetical. Major attacks on shipping companies (Maersk 2017 NotPetya, COSCO 2018, MSC 2020, DNV's own ShipManager 2023) have caused operational disruption costing hundreds of millions of dollars. Smaller incidents at the vessel level happen continuously and are largely unreported.

**Technological:** The shift from analog navigation to electronic chart displays and integrated bridge systems means more attack surface. Increasing automation and remote monitoring connect previously isolated systems. Crew Wi-Fi and operational technology must be separated, but in practice often aren't.

**Commercial:** Major charterers (oil majors, container line customers, governments) are starting to demand evidence of cyber readiness as part of charter party and contract negotiations. Vessels without demonstrable cyber posture may find themselves with reduced commercial opportunities.

**For RavenFabric specifically**, the maritime use case is exceptionally well-aligned because:

1. **Multi-transport is a maritime requirement, not a feature** — vessels genuinely need VSAT + LTE + Wi-Fi + Iridium + sneakernet, not as options but as operational realities.

2. **DTN and bandwidth-awareness solve real problems** — every other tool either fails or burns money in maritime conditions.

3. **Air-gap support matches OT realities** — bridge and engine room systems should not be on the internet, ever, but still need maintenance.

4. **Compliance-grade audit is a regulatory requirement** — class societies and port state inspectors increasingly demand exactly the kind of audit trail RavenFabric produces by default.

5. **Self-hosted and vendor-neutral aligns with fleet operator interests** — no fleet wants to be locked into a single satellite vendor or single maritime SaaS provider.

---

## See Also

- [Edge & IoT Fleet Management](edge-iot-fleet.md) — Related patterns for distributed device fleets
- [Air-Gapped Industrial Systems](airgapped-ics.md) — OT air-gap patterns in detail
- [MSP Multi-Tenant Operations](msp-multitenant.md) — Multi-client isolation patterns
