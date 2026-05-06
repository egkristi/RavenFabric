# Example Use Case 05: Managed Service Provider Multi-Tenant Operations

> **Scenario:** A managed service provider (MSP) operates infrastructure for
> dozens or hundreds of client organizations. Each client has its own network,
> security policies, compliance requirements, and infrastructure topology.
> MSP technicians need secure access to provide support, maintenance, and
> monitoring — without compromising the isolation between clients, without
> requiring expensive per-client VPN appliances, and without the operational
> burden of managing hundreds of disparate access systems.

---

## The Problem

Managed service providers face a unique multiplication of complexity:

- **Each client expects isolation** from other clients' data and systems
- **Each client has different security baselines** (PCI-DSS for retail,
  HIPAA for healthcare, SOC 2 for SaaS, NIS2 for critical infrastructure)
- **Each client may demand different access controls** for the MSP's own
  technicians
- **Each client needs different audit trails** that may need to be
  surrendered to them on request
- **Technicians work across many clients per day** — context switching is
  constant
- **New client onboarding must be fast** — traditional approaches take days
- **Client offboarding must be complete** — any leftover access is a
  liability

The traditional MSP toolkit accumulates complexity:

- Dozens of VPN clients on each technician's workstation
- Per-client RMM (remote monitoring and management) agent installations
- Per-client jump hosts or bastion servers
- Per-client credential vaults
- Per-client audit log destinations
- Per-client incident escalation paths

### Traditional approaches and their problems

```
MSP technician workstation
    │
    ├─→ ConnectWise Automate (one client)
    ├─→ Datto RMM (another client)
    ├─→ Custom VPN to Acme Corp
    ├─→ Custom VPN to Beta Industries
    ├─→ ScreenConnect to Charlie LLC
    │   ... (50+ tools and credentials)
    │
    └─→ TeamViewer for emergency
```

**Issues:**

- **Tool sprawl** — each acquisition or new client adds another tool
- **Credential management nightmare** — passwords for 50+ systems
- **Inconsistent security posture** — easy for one client's access to
  weaken
- **Audit fragmentation** — incident investigations require correlating
  logs from many sources
- **Onboarding friction** — new clients take days to integrate
- **Offboarding risk** — uninstalling tools is rarely complete
- **Single point of compromise** — a phished technician credential affects
  all clients
- **Compliance burden** — each client may demand specific evidence of
  access controls

---

## The RavenFabric Approach

```
MSP technician workstation
    │
    │  rf clients list
    │  rf exec --client acme-corp "kubectl get pods"
    │  rf shell --client beta-industries production-db-1
    │  rf tunnel --client charlie-llc -L 5432:db:5432
    ▼
MSP-operated relay (multi-tenant aware)
    │
    ▼
Per-client agents in each client's infrastructure
    │  ├─ Client A: agents in their AKS cluster
    │  ├─ Client B: agents on their VMware hosts
    │  ├─ Client C: agents on their bare metal
    │  └─ Each client's policy entirely separate
    ▼
Client systems (each client's own infrastructure)
```

### What this provides

| Capability | Description |
|------------|-------------|
| **Single access tool** | One `rf` CLI replaces dozens of remote access tools |
| **Per-client isolation** | Cryptographic separation — no client can see another |
| **Unified audit (per client)** | Each client gets their own complete audit trail |
| **Tenant-aware policy** | Per-technician permissions can vary per client |
| **Fast onboarding** | New client = deploy agents + grant access tokens |
| **Clean offboarding** | Revoke client's keys = complete access termination |
| **Same-day incident response** | Pre-approved playbooks for common emergencies |
| **Compliance evidence** | Auto-generated reports per client per period |
| **Scales to thousands of endpoints** | One technician can manage many clients |

---

## Architecture

```
┌───────────────────────────────────────────────────────────────────┐
│  MSP Operations Center                                            │
│                                                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │ Technician 1 │  │ Technician 2 │  │ Technician N │            │
│  │              │  │              │  │              │            │
│  │ rf-cli       │  │ rf-cli       │  │ rf-cli       │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
└─────────┼─────────────────┼──────────────────┼────────────────────┘
          │                 │                  │
          └────────┬────────┴────────┬─────────┘
                   │                 │
                   ▼                 ▼
        ┌──────────────────────────────────────┐
        │  MSP rf-relay (multi-tenant)         │
        │                                      │
        │  ├─ Tenant: acme-corp                │
        │  ├─ Tenant: beta-industries          │
        │  ├─ Tenant: charlie-llc              │
        │  ├─ Tenant: delta-services           │
        │  └─ ... (hundreds of tenants)        │
        │                                      │
        │  Each tenant: separate channel       │
        │  Cryptographic isolation enforced    │
        │  No cross-tenant data flow possible  │
        └────────┬─────────────┬───────────┬───┘
                 │             │           │
        ┌────────┘    ┌────────┘    ┌──────┘
        │             │             │
        ▼             ▼             ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│ acme-corp   │ │ beta-       │ │ charlie-llc │
│             │ │ industries  │ │             │
│ ┌─────────┐ │ │             │ │ ┌─────────┐ │
│ │ Agent A1│ │ │ ┌─────────┐ │ │ │ Agent C1│ │
│ │ Agent A2│ │ │ │ Agent B1│ │ │ │ Agent C2│ │
│ │ Agent A3│ │ │ │ Agent B2│ │ │ │ Agent C3│ │
│ └─────────┘ │ │ └─────────┘ │ │ └─────────┘ │
│             │ │             │ │             │
│ Their AKS   │ │ Their bare  │ │ Their VMware│
│ cluster     │ │ metal       │ │ environment │
└─────────────┘ └─────────────┘ └─────────────┘

Cryptographic isolation:
- Each agent encrypted to client's CA + technician's key
- Client controls which technicians can access
- Per-client policy enforced at agent layer
- Per-client audit logs separate
```

---

## Multi-Tenancy Model

### Tenant identity hierarchy

```
MSP Root CA
├── MSP Operations Identity
│   └── Used for relay infrastructure only
│
├── Technician Identities
│   ├── alice@msp.example.com
│   ├── bob@msp.example.com
│   └── carol@msp.example.com
│
└── Client Tenants
    ├── acme-corp
    │   ├── Client root key (held by client)
    │   ├── Authorized technicians (subset of MSP techs)
    │   ├── Per-client policy
    │   └── Agents in client infrastructure
    │
    ├── beta-industries
    │   ├── Client root key
    │   ├── Authorized technicians
    │   ├── Per-client policy
    │   └── Agents in client infrastructure
    │
    └── ... (more clients)
```

### Per-client access control

Each client controls which MSP technicians can access their environment:

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: TenantPolicy
metadata:
  name: acme-corp-tenant
spec:
  tenant_id: acme-corp

  # Client-controlled: who from MSP can access
  authorized_technicians:
    - identity: alice@msp.example.com
      role: senior-technician
      validity:
        notBefore: "2026-01-01T00:00:00Z"
        notAfter: "2026-12-31T23:59:59Z"

    - identity: bob@msp.example.com
      role: technician
      restrictions:
        time_windows:
          - days: [Mon, Tue, Wed, Thu, Fri]
            hours: ["08:00-18:00"]
            timezone: "Europe/Oslo"

    - identity: carol@msp.example.com
      role: emergency-only
      restrictions:
        require_incident_ticket: true
        require_co_sign: true
        co_sign_from_role: senior-technician

  # Client-defined policy applied to all technicians
  policy_ref:
    name: acme-corp-msp-access-policy
```

---

## Deployment Patterns

### Pattern A: Agent in client's infrastructure (most common)

The agent is deployed inside the client's infrastructure, connecting outbound
to the MSP's relay. The client controls the agent — but their policy permits
designated MSP technicians to use it.

```yaml
# Deployed in client's infrastructure
apiVersion: ravenfabric.io/v1alpha1
kind: AgentConfig
metadata:
  name: msp-managed-agent
  namespace: ravenfabric-system
spec:
  # Connect to MSP's relay
  relay:
    url: wss://relay.msp.example.com
    expected_pubkey: "msp-relay-pubkey-fingerprint"

  # Client owns this agent's identity
  identity:
    keypair_path: /etc/ravenfabric/identity.key
    co_signed_by: client-root-ca

  # Tenant assignment (set by client)
  tenant_id: acme-corp

  # Client's policy (not MSP's)
  policy_ref:
    name: acme-corp-msp-access-policy
    immutable_baseline: client-immutable-rules
```

### Pattern B: Centralized agent in MSP infrastructure

For clients with simpler infrastructure (e.g., a few servers), the agent
runs in the MSP's infrastructure and connects to client systems via tunnels.
This is simpler operationally but provides less isolation.

```yaml
# Deployed in MSP infrastructure
apiVersion: ravenfabric.io/v1alpha1
kind: ProxyAgent
metadata:
  name: client-acme-corp-proxy
spec:
  tenant_id: acme-corp

  # MSP technicians connect through this proxy
  upstream_targets:
    - name: acme-mail-server
      address: 10.20.30.40:22
      protocol: ssh

    - name: acme-database
      address: 10.20.30.41:5432
      protocol: postgres

  # Per-target ACLs
  access_rules:
    - target: acme-mail-server
      allowed_technicians:
        - alice@msp.example.com
        - bob@msp.example.com

    - target: acme-database
      allowed_technicians:
        - alice@msp.example.com
      additional_requirements:
        - require_approval_from: customer-acme-supervisor
```

---

## Policy Configuration

### Per-client policy template

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: acme-corp-msp-access-policy
spec:
  # Client-side enforcement of MSP technician actions
  commands:
    allow:
      # Standard operational commands
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^systemctl status .*$"
      - pattern: "^journalctl -u .* --since.*$"
      - pattern: "^df -h$"
      - pattern: "^free -h$"
      - pattern: "^uptime$"

      # Routine maintenance
      - pattern: "^systemctl restart [a-z-]+$"
      - pattern: "^kubectl rollout restart deployment/.*$"

      # Patches and updates
      - pattern: "^apt-get update$"
      - pattern: "^apt-get upgrade -y --only-security$"

    deny:
      # Client data must never be touched
      - pattern: ".*--rm.*"
      - pattern: "rm -rf"
      - pattern: "DROP DATABASE"
      - pattern: "TRUNCATE"

      # No system modifications
      - pattern: "useradd"
      - pattern: "userdel"
      - pattern: "passwd"
      - pattern: "iptables"

  # Sensitive operations require client approval
  approval:
    required:
      - pattern: "^apt-get upgrade$"  # full upgrade, not just security
        approvers: ["acme-it-supervisor"]
        timeoutSeconds: 86400  # 24-hour window

      - pattern: "^kubectl scale deployment.*--replicas=0"
        approvers: ["acme-it-supervisor"]

      - pattern: "^kubectl delete pod.*--force"
        approvers: ["acme-on-call"]

  # Audit forwarded to client's SIEM
  audit:
    destinations:
      - kind: file
        path: /var/log/ravenfabric/msp-access.jsonl
      - kind: syslog
        endpoint: "syslog://siem.acme.local:514"
        format: rfc5424
      - kind: webhook
        url: "https://acme.com/api/audit/msp-access"
        sign_payloads: true
```

### Per-technician role policy

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: TechnicianPolicy
metadata:
  name: msp-senior-technician-base
spec:
  # MSP-internal policy applied across all clients

  # Default permissions across all clients
  default_permission: read_only

  # Client-specific overrides (subject to client's TenantPolicy)
  client_permissions:
    - tenant_id: acme-corp
      permission: full_admin
    - tenant_id: beta-industries
      permission: read_write
    - tenant_id: charlie-llc
      permission: read_only

  # Technician must always meet these
  workstation_requirements:
    require_mfa: true
    require_recent_authentication_minutes: 480  # 8 hours
    require_managed_device: true
    require_yubikey: true

  # Time-of-day restrictions
  default_hours:
    days: [Mon, Tue, Wed, Thu, Fri]
    hours: ["07:00-19:00"]
    timezone: "Europe/Oslo"

  # On-call exception
  on_call_override:
    when_pager_duty: true
    requires_co_sign: false  # urgency
```

---

## Operator Workflows

### Workflow 1: Daily multi-client check-in

```bash
$ rf clients list

CLIENT              STATUS          LAST CONTACT  PENDING ALERTS
acme-corp           healthy         2 min ago     0
beta-industries     healthy         5 min ago     1 (low priority)
charlie-llc         degraded        12 min ago    3 (1 high)
delta-services      healthy         1 min ago     0
echo-financial      healthy         3 min ago     0
foxtrot-retail      healthy         8 min ago     0
golf-mfg            offline         2 hours ago   2 (1 critical)
... (43 more clients)

# Investigate the offline one first
$ rf client status golf-mfg
[client: golf-mfg]
[contact: 2 hours, 14 minutes ago]
[last known state: 47 agents healthy]
[recent agent disconnections: 47 (within 30 sec window)]
[likely cause: ISP outage at primary site]
[next scheduled contact: when connection restored]

# Check the high-priority charlie-llc alert
$ rf client alerts charlie-llc --priority high
[1 high-priority alert:]
[Storage 92% full on db-primary-1]
[detected: 4 hours ago]
[playbook available: rf playbook apply storage-cleanup --client charlie-llc]
```

### Workflow 2: Cross-client patch deployment

```bash
# Same security patch needed across many clients
$ rf playbook apply security-patch-2026-05.yaml \
    --selector "client.tier=managed-plus"

[playbook: apt security upgrade for all managed-plus clients]
[selector: 23 clients]
[total agents in scope: 1,847]

[per-client approval workflow:]
[acme-corp: auto-approved per contract]
[beta-industries: pending acme-it-supervisor approval]
[charlie-llc: pending charlie-cto approval]
... (20 more pending)

# Approvals come in over the next hours
$ rf playbook status security-patch-2026-05.yaml
[playbook progress:]
[acme-corp: 89/89 agents patched, 0 errors]
[beta-industries: 142/142 agents patched, 2 reboots required]
[charlie-llc: pending approval (24h timeout)]
[delta-services: 67/67 agents patched, 0 errors]
... (continuing across clients)

[overall: 18/23 clients complete, 4 in progress, 1 awaiting approval]
[estimated completion: 6 hours]
```

### Workflow 3: New client onboarding

```bash
# Onboard new client "hotel-tech" (a hospitality SaaS company)
$ rf client create hotel-tech \
    --tier managed-plus \
    --compliance "soc-2,hipaa" \
    --primary-contact alice@hotel-tech.com \
    --policy-template hospitality-saas-base

[creating tenant: hotel-tech]
[generating client root key (held by client)]
[generating MSP-side relay channel]
[applying policy template: hospitality-saas-base]
[customizing for compliance: SOC 2 Type II, HIPAA Technical Safeguards]

[tenant created. Next steps for client:]
[1. Client downloads enrollment package from secure link]
[2. Client deploys rf-agent in their infrastructure]
[3. Client confirms enrollment]
[4. MSP technicians can begin access]

[estimated time to first access: 30 minutes]

# After client confirms enrollment
$ rf client grant hotel-tech \
    --technicians "alice@msp.example.com,bob@msp.example.com" \
    --role senior-technician \
    --validity-days 365

[granted access:]
[alice@msp.example.com: senior-technician until 2027-05-05]
[bob@msp.example.com: senior-technician until 2027-05-05]
[hotel-tech can revoke at any time via their console]
```

### Workflow 4: Compliance reporting

```bash
# Generate quarterly access report for client
$ rf client report acme-corp \
    --period 2026-Q1 \
    --format pdf \
    --evidence-grade

[generating report for acme-corp Q1 2026]
[gathering audit data from 89 agents]
[total events: 14,238]
[summary by category:]
  read-only operations: 12,847
  routine maintenance: 1,124
  approved changes: 247
  emergency interventions: 18
  denied actions: 2

[compliance evidence:]
  SOC 2 Type II:
    CC6.1 (logical access): 100% covered
    CC7.2 (system monitoring): 100% covered
    CC7.3 (incident response): 18 incidents documented
  HIPAA Technical Safeguards:
    164.312(a)(1) access control: 100% covered
    164.312(b) audit controls: 100% covered

[report generated: acme-corp-Q1-2026-access-report.pdf]
[cryptographically signed by MSP and counter-signable by client]
[chain of custody preserved for legal hold if needed]
```

### Workflow 5: Client offboarding

```bash
# Client charlie-llc has terminated MSP contract
$ rf client offboard charlie-llc \
    --termination-date 2026-05-15 \
    --data-handover-required

[offboarding charlie-llc]
[scheduled effective: 2026-05-15 at 23:59 UTC]

[automatic actions on effective date:]
[1. Revoke all MSP technician access tokens]
[2. Generate final audit log archive (signed)]
[3. Generate operational handover document]
[4. Disable relay channel for tenant]
[5. Notify all listed contacts]

[client retains:]
[- All audit logs (encrypted to their key)]
[- Their installed agents (which they control)]
[- Their policies and configurations]
[- Right to fully verify access termination]

[MSP retains:]
[- Aggregated billing/usage data only]
[- No client-specific operational data]
[- Cryptographic proof of access termination]

[offboarding scheduled. Confirm with: rf client offboard-confirm charlie-llc]
```

---

## Comparison with Alternatives

| Feature | RMM Tools (Datto, ConnectWise) | TeamViewer / AnyDesk | Multiple VPNs | Bastion Hosts | RavenFabric |
|---------|-------------------------------|----------------------|---------------|---------------|-------------|
| **Single tool, all clients** | Yes | Partial | No | No | Yes |
| **Cryptographic per-client isolation** | Partial | No | Partial | Partial | Yes |
| **Client controls technician access** | Partial | No | No | Partial | Yes |
| **Per-client audit (client-readable)** | Partial | No | No | Partial | Yes |
| **Command-level policy** | No | No | No | No | Yes |
| **Approval workflows per client** | Partial | No | No | No | Yes |
| **Vendor-neutral (no lock-in)** | No | No | Partial | Yes | Yes |
| **Compliance evidence generation** | Partial | No | No | No | Yes |
| **Fast onboarding (hours not days)** | Partial | Yes | No | No | Yes |
| **Clean offboarding** | Partial | Partial | Partial | Partial | Yes |
| **Self-hosted relay option** | No | No | N/A | N/A | Yes |
| **End-to-end encrypted (vendor sees nothing)** | No | Partial | Yes | Partial | Yes |
| **Cost per technician per month** | $$$ | $$ | $$ (per VPN) | $$$ | Compute only |

---

## Implementation Status

### Available today (v0.1)

- Cryptographic identity per agent and per operator
- Policy-validated command execution
- Structured audit logging
- Single-tenant operation

### Coming in v0.4

- Multi-tenant relay (logical isolation)
- Per-tenant policy enforcement
- Per-tenant audit segregation

### Coming in v0.6

- Tenant federation (cross-tenant policies)
- Approval workflow engine
- Compliance reporting framework
- RBAC with role hierarchies

### Coming in v0.7+

- Client self-service portal (enroll, revoke, audit)
- MSP operations dashboard
- Compliance template library (SOC 2, HIPAA, NIS2, ISO 27001)

---

## Why This Matters

The MSP industry has consolidated around a small number of large vendors —
ConnectWise, Kaseya, N-able, Datto. These platforms work, but they create
strategic dependencies:

- **Pricing power** rests with vendors as switching costs grow
- **Vulnerability** to vendor security incidents affects all customers
  simultaneously (the 2021 Kaseya VSA attack affected over 1,500 downstream
  organizations)
- **Vendor lock-in** makes alternative tooling impractical
- **Roadmap control** belongs to vendors, not MSPs

Smaller MSPs in particular face a difficult choice: invest in expensive
commercial platforms that may not serve their specific needs, or assemble
collections of point tools that fragment their operations.

RavenFabric proposes a different model:

> **The MSP operates the control plane (relay infrastructure). Each client
> controls their own access policy. Technicians use one tool across all
> clients. Audit trails are cryptographically separated and client-readable.
> No vendor sees client data.**

For MSPs serving regulated industries (healthcare, finance, public sector,
critical infrastructure), this model addresses a growing requirement:
clients increasingly demand that their MSPs prove they cannot exfiltrate
client data, and that technician access is independently auditable.

For clients of MSPs, this model addresses a growing concern: every
additional remote access tool deployed in their environment is an
additional attack surface, and consolidation under a cryptographically
sound architecture is materially better than tool sprawl.

The economic value proposition for MSPs is compelling: replacing five or
ten point tools with one fabric reduces operational complexity, improves
security posture, and creates a defensible position relative to competitors
still relying on traditional approaches.

---

## See Also

- [README.md](../README.md) — RavenFabric overview
- [CONNECTIVITY.md](../CONNECTIVITY.md) — Multi-transport architecture
- [usecase-cloudnativepg.md](usecase-cloudnativepg.md) — CloudNativePG admin access
- [usecase-edge-iot-fleet.md](usecase-edge-iot-fleet.md) — Edge & IoT fleet management
- [usecase-multi-cluster-kubernetes.md](usecase-multi-cluster-kubernetes.md) — Multi-cluster Kubernetes
- [usecase-airgapped-ics.md](usecase-airgapped-ics.md) — Air-gapped industrial systems
