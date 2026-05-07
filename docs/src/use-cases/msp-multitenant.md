# MSP Multi-Tenant Operations

> **Scenario:** A managed service provider (MSP) operates infrastructure for
> dozens or hundreds of client organizations — each with its own security
> policies, compliance requirements, and infrastructure. Technicians need
> secure access across all clients without compromising isolation.

---

## The Problem

MSPs face multiplied complexity as client count grows:

- **Per-client isolation** — each client expects separation from others
- **Different compliance requirements** — PCI-DSS, HIPAA, SOC 2, NIS2 per client
- **Different access controls** — per-technician permissions vary by client
- **Separate audit trails** — each client may demand their own logs
- **Constant context switching** — technicians work across many clients daily
- **Onboarding/offboarding** — must be fast and complete

### Traditional approaches

- Dozens of VPN clients, RMM agents, jump hosts per technician
- Per-client credential vaults and audit destinations
- Tool sprawl (ConnectWise, Datto, ScreenConnect, TeamViewer, custom VPNs)
- A phished credential compromises all clients simultaneously (Kaseya VSA 2021)
- Incident investigation requires correlating logs from many unrelated sources
- Offboarding is never complete — leftover access is a liability

---

## How RavenFabric Addresses This

```
MSP technician
    │  rf exec --target acme-web-01 "kubectl get pods"
    │  rf shell acme-db-primary
    │  rf playbook apply patch.yaml --selector "tenant=acme-corp"
    ▼
MSP rf-relay (tenant-aware, E2E encrypted)
    │  TenantIsolation: cross-tenant blocking enforced
    │  Capability tokens: per-technician, per-client, attenuated
    ▼
Per-client agents (in client's own infrastructure)
    ├─ acme-corp: 89 agents in their AKS cluster
    ├─ beta-industries: 12 agents on bare metal
    └─ charlie-llc: 34 agents in VMware
    Each agent enforces client's policy locally
```

| Capability | How |
|------------|-----|
| Per-client isolation | `TenantIsolation` with cross-tenant blocking |
| Role-based access | RBAC (admin, operator, viewer, auditor) per tenant |
| Capability delegation | Biscuit tokens — attenuated, offline-verifiable, expiring |
| Per-client policy | Each agent enforces the client's own `spec:` policy |
| Per-client audit | Separate audit log streams per tenant |
| Approval workflows | Human-in-loop for sensitive operations |
| Fast onboarding | Deploy agent + issue capability token |
| Clean offboarding | Revoke token = immediate access termination |

---

## Multi-Tenancy Model

Each client owns their agent identity keys. The MSP operates the relay but cannot decrypt client traffic (Noise XX end-to-end). Technician access is controlled by capability tokens that the client can revoke.

```
MSP Relay
├── Tenant: acme-corp (agents encrypted to acme's keys)
├── Tenant: beta-industries (agents encrypted to beta's keys)
└── Tenant: charlie-llc (agents encrypted to charlie's keys)

Per technician: Biscuit capability token
├── Scoped to specific tenant(s)
├── Attenuated (e.g., read-only, time-bounded)
├── Delegatable with narrowing only
└── Offline-verifiable (Ed25519)
```

---

## Policy Configuration

Each client defines their own policy enforced at the agent:

```yaml
# Client-controlled policy for MSP technician access
spec:
  commands:
    allow:
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^systemctl status .*$"
      - pattern: "^journalctl -u .* --since.*$"
      - pattern: "^df -h$"
      - pattern: "^uptime$"
      # Routine maintenance
      - pattern: "^systemctl restart [a-z-]+$"
      - pattern: "^apt-get upgrade -y --only-security$"

    deny:
      - pattern: "rm -rf"
      - pattern: "DROP DATABASE"
      - pattern: "TRUNCATE"
      - pattern: "useradd"
      - pattern: "passwd"
      - pattern: "iptables"

  filesystem:
    allow:
      - path: /var/log
      - path: /tmp
    deny:
      - path: /etc/shadow
      - path: /home

  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

The MSP cannot override client policy — it is enforced at the agent layer, which the client controls.

---

## Example Workflows

### Cross-client operations

```bash
# Execute across all agents in a specific tenant
$ rf exec --selector "tenant=acme-corp" "uptime"
[89 targets: 89 reachable]
acme-web-01: up 34 days
acme-web-02: up 34 days
acme-db-01:  up 127 days
...

# Interactive shell into a specific client's system
$ rf shell acme-db-primary

# Apply playbook with per-client approval workflow
$ rf playbook apply security-patch.yaml --selector "tenant=beta-industries"
[awaiting approval from beta-industries supervisor...]
```

### Client onboarding

1. Deploy rf-agent in client infrastructure (connects outbound to MSP relay)
2. Assign tenant ID in agent config (`raven.toml`)
3. Issue capability token to authorized technicians (scoped to tenant)
4. Client's policy file controls what technicians can do

### Client offboarding

1. Revoke capability tokens for the tenant
2. Client retains their agents, keys, audit logs (all theirs)
3. MSP retains no client-specific operational data

---

## Comparison with Alternatives

| Feature | RMM (Datto, ConnectWise) | TeamViewer | Multiple VPNs | RavenFabric |
|---------|--------------------------|------------|---------------|-------------|
| Single tool, all clients | Yes | Partial | No | Yes |
| Cryptographic per-client isolation | No | No | Partial | Yes |
| Client controls access policy | No | No | No | Yes |
| Per-client audit trail | Partial | No | No | Yes |
| Command-level policy | No | No | No | Yes |
| Self-hosted (vendor sees nothing) | No | No | N/A | Yes |
| Clean offboarding | Partial | Partial | Partial | Yes |
| Lock-in | Severe | Medium | Low | None (AGPL-3.0) |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| Cryptographic device identity | Done | Per-agent Noise XX keypair |
| Policy-validated execution | Done | Deny-by-default |
| Structured audit logging | Done | JSON-lines per agent |
| RBAC | Done | admin, operator, viewer, auditor roles |
| Tenant isolation | Done | `TenantIsolation` with cross-tenant blocking |
| Capability tokens (Biscuit) | Done | Sign/verify, delegation, attenuation |
| SecurityPolicy with immutable rules | Done | Immutable deny list, delegation depth |
| Approval workflows | Done | Human-in-loop for sensitive operations |
| Multi-agent playbooks | Done | `Orchestrator` + `rf playbook` |
| Interactive shell | Done | `rf shell` with bidirectional I/O |
| Audit report generation | Done | JSON/CSV export |
| Web dashboard | Done | Real-time agent metrics |
| Per-tenant audit segregation | Planned | Separate streams per tenant |
| Client self-service portal | Planned | Enroll, revoke, view audit |
| Compliance template library | Planned | SOC 2, HIPAA, NIS2, ISO 27001 |

---

## See Also

- [Air-Gapped ICS](airgapped-ics.md) — OT air-gap patterns
- [Edge & IoT Fleet Management](edge-iot-fleet.md) — Distributed device fleets
- [Multi-Cluster Kubernetes](multi-cluster-kubernetes.md) — Cross-cluster operations
- [CloudNativePG](cloudnativepg.md) — Database admin access
