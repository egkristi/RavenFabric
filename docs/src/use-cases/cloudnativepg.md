# Secure CloudNativePG Admin Access

> **Scenario:** A team operates PostgreSQL clusters using CloudNativePG (CNPG)
> on Kubernetes. Database administrators, SREs, and developers need secure
> access to these clusters from anywhere — without VPNs, bastion hosts, or
> exposed network endpoints.

---

## The Problem

Modern PostgreSQL deployments on Kubernetes face a recurring access dilemma:

- **Application access** is solved by Kubernetes internal Services (no ingress
  needed)
- **Operator/admin access** is harder — DBAs need `psql`, `pg_dump`, schema
  migrations, performance debugging, and emergency intervention
- **External application access** (legacy systems, BI tools, partner
  integrations) requires careful exposure

This document focuses on **operator/admin access** — the highest-risk path,
where unrestricted command execution can damage production data.

### Traditional approaches and their problems

```
DBA workstation
    │
    ▼
Corporate VPN  ←── broad network access, lateral movement risk
    │
    ▼
Bastion host  ←── persistent attack target, audit gap
    │
    ▼  (SSH + kubectl port-forward)
    │
Kubernetes API server
    │
    ▼  (kubectl exec → pod)
    │
CNPG primary pod (port 5432)
```

**Issues:**

- **VPN grants broader access than needed** — once on the network, an attacker
  can move laterally
- **Bastion hosts are persistent attack targets** — they accumulate access
  patterns and credentials
- **`kubectl port-forward` is not a production-grade access mechanism** — no
  proper audit, no command-level controls, no session recording
- **No SQL-level policy enforcement** — a DBA can run
  `DROP DATABASE production` with no friction
- **No session recording** — no way to replay what happened during an incident
- **No tamper detection** — a compromised hop can MITM credentials
- **High latency** — every hop adds round-trip time

---

## The RavenFabric Approach

```
DBA workstation                             Anywhere on the internet
    │
    │  rf shell prod-pg-primary
    │  rf exec prod-pg-primary "psql -c 'SELECT ...'"
    │  rf tunnel -L 5432:cnpg-rw:5432
    ▼
RavenFabric Relay (E2E encrypted, sees only ciphertext)
    │
    ▼
Agent in CNPG pod (sidecar) or on Kubernetes node (DaemonSet)
    │  ├─ Policy check: is this operator allowed to run this command?
    │  ├─ Audit: log session start, command, result
    │  └─ Session recording: full PTY capture (asciinema format)
    ▼
PostgreSQL (port 5432)
```

### What this provides

| Capability | Description |
|------------|-------------|
| **No VPN required** | Operators connect from any network — home office, hotel, coffee shop |
| **No exposed ports on cluster** | Agents connect outbound only; relay accepts inbound |
| **Command-level policy** | `DELETE FROM users` can require approval workflow |
| **Complete audit trail** | Every session recorded with session ID, structured JSON-lines |
| **Per-operator scoping** | Operator A gets read+write to prod, Operator B gets read-only |
| **Time-bounded access** | Capability tokens expire after configurable TTL |
| **MFA on operations** | Require WebAuthn confirmation for DDL operations |
| **End-to-end encrypted** | Relay sees only Noise XX ciphertext — no PostgreSQL traffic visible |
| **Air-gap fallback** | Same fabric works over Reticulum, serial, or sneakernet for emergency access |

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  Internet / Corporate Network / Home Office                        │
│                                                                    │
│  ┌──────────┐                                                     │
│  │ Operator │  $ rf exec prod-pg-1 "psql -c 'SELECT ...'"         │
│  │ laptop   │  $ rf shell prod-pg-1                               │
│  │          │  $ rf tunnel -L 5432:cnpg-rw:5432                   │
│  └─────┬────┘                                                     │
└────────┼───────────────────────────────────────────────────────────┘
         │ Noise XX (E2E encrypted, mutual auth)
         │ Transport: WireGuard direct → QUIC → WebSocket fallback
         ▼
┌────────────────────────────────────────────────────────────────────┐
│  rf-relay (geo-distributed, sees only ciphertext)                 │
│  Deployable as Container App, Kubernetes Service, or VM           │
└────────┼───────────────────────────────────────────────────────────┘
         │ Noise XX (same E2E session, relay is just a bytes-pump)
         │ Outbound only (agent initiates, relay accepts)
         ▼
┌────────────────────────────────────────────────────────────────────┐
│  Kubernetes Cluster                                                │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Namespace: cnpg-system                                   │    │
│  │                                                           │    │
│  │  ┌─────────────────────────────────────────────────┐     │    │
│  │  │  CNPG Cluster: prod-pg-1                        │     │    │
│  │  │                                                 │     │    │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐      │     │    │
│  │  │  │ Primary  │  │ Replica  │  │ Replica  │      │     │    │
│  │  │  │  Pod     │  │  Pod     │  │  Pod     │      │     │    │
│  │  │  │          │  │          │  │          │      │     │    │
│  │  │  │ pg 18    │  │ pg 18    │  │ pg 18    │      │     │    │
│  │  │  │ + rf-    │  │ + rf-    │  │ + rf-    │      │     │    │
│  │  │  │  agent   │  │  agent   │  │  agent   │      │     │    │
│  │  │  │  side-   │  │  side-   │  │  side-   │      │     │    │
│  │  │  │  car     │  │  car     │  │  car     │      │     │    │
│  │  │  └──────────┘  └──────────┘  └──────────┘      │     │    │
│  │  └─────────────────────────────────────────────────┘     │    │
│  │                                                           │    │
│  │  Each agent:                                              │    │
│  │   - Connects outbound to relay                            │    │
│  │   - Re-validates policy locally (final authority)         │    │
│  │   - Logs every decision to structured audit               │    │
│  │   - Executes only within policy bounds                    │    │
│  └──────────────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────────────┘
```

---

## Deployment Patterns

There are two reasonable deployment patterns for RavenFabric agents alongside
CNPG. Choose based on operational preferences.

### Pattern A: Sidecar in each PostgreSQL pod

Each CNPG instance pod gets a co-located RavenFabric agent. Communication
between agent and PostgreSQL happens over the pod's local loopback or Unix
socket — never over the network.

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: prod-pg-1
  namespace: cnpg-system
spec:
  instances: 3

  imageName: ghcr.io/cloudnative-pg/postgresql:18

  postgresql:
    parameters:
      shared_preload_libraries: "pg_oidc_validator,pg_cron,pg_partman"

  # CNPG sidecar injection (CNPG 1.21+)
  managed:
    services:
      additional:
        - selectorType: rw
          serviceTemplate:
            metadata:
              name: prod-pg-1-rf

  inheritedMetadata:
    annotations:
      ravenfabric.io/agent-enabled: "true"
      ravenfabric.io/agent-policy: "cnpg-prod-policy"
      ravenfabric.io/agent-image: "ghcr.io/ravenfabric/rf-agent:0.1"
```

**Trade-offs:**

- Per-pod isolation — compromise of one agent doesn't affect others
- Local-only PostgreSQL access (no network hop)
- Survives pod migrations naturally
- More agent instances to manage
- Higher resource overhead (agent per pod)

### Pattern B: DaemonSet on Kubernetes nodes

A single RavenFabric agent runs on each Kubernetes node, accessing CNPG pods
on the same node via Unix socket or shared volume.

```yaml
apiVersion: apps/v1
kind: DaemonSet
metadata:
  name: rf-agent
  namespace: ravenfabric-system
spec:
  selector:
    matchLabels:
      app: rf-agent
  template:
    metadata:
      labels:
        app: rf-agent
    spec:
      hostNetwork: false
      serviceAccountName: rf-agent
      containers:
        - name: agent
          image: ghcr.io/ravenfabric/rf-agent:0.1
          args:
            - --relay=wss://relay.ravenfabric.example.com
            - --policy=/etc/ravenfabric/policy.yaml
            - --identity=/etc/ravenfabric/identity.key
            - --audit-log=/var/log/ravenfabric/audit.jsonl
          resources:
            requests:
              cpu: 50m
              memory: 32Mi
            limits:
              cpu: 200m
              memory: 128Mi
          volumeMounts:
            - name: identity
              mountPath: /etc/ravenfabric
              readOnly: true
            - name: audit-log
              mountPath: /var/log/ravenfabric
            - name: cnpg-sockets
              mountPath: /var/run/cnpg
              readOnly: false
      volumes:
        - name: identity
          secret:
            secretName: rf-agent-identity
            defaultMode: 0400
        - name: audit-log
          hostPath:
            path: /var/log/ravenfabric
            type: DirectoryOrCreate
        - name: cnpg-sockets
          hostPath:
            path: /var/run/cnpg
            type: DirectoryOrCreate
```

**Trade-offs:**

- Fewer agent instances (one per node)
- Lower total resource overhead
- Simpler upgrade story
- Larger blast radius if node-level agent compromised
- Must handle pod-to-node routing

---

## Policy Configuration

This is where RavenFabric's value is most apparent. Granular policy is enforced
**before** any command reaches PostgreSQL.

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: RPCPolicy
metadata:
  name: cnpg-prod-dba-policy
  namespace: cnpg-system
spec:
  # Which commands are allowed
  commands:
    allow:
      # Read-only operations
      - pattern: "^psql .* -c \"SELECT.*\"$"
      - pattern: "^psql .* -c \"EXPLAIN.*\"$"
      - pattern: "^pg_dump .* --schema-only"

      # Backup operations
      - pattern: "^pg_dump prod_db -F c -f /backup/.*"
      - pattern: "^pg_basebackup -D /backup/.*"

      # CNPG-specific operations
      - pattern: "^kubectl cnpg status .*"
      - pattern: "^kubectl cnpg promote .*"
      - pattern: "^kubectl cnpg backup .*"

    deny:
      # Always deny dangerous operations
      - pattern: "DROP DATABASE"
      - pattern: "TRUNCATE.*"
      - pattern: "DELETE FROM users"
      - pattern: "ALTER USER.*SUPERUSER"
      - pattern: "COPY.*TO PROGRAM"      # Prevent COPY-based RCE
      - pattern: "pg_read_server_files"  # Prevent file read via SQL

  # Filesystem access
  filesystem:
    allow:
      - path: /backup
        operations: [read, write]
      - path: /var/log/postgresql
        operations: [read]
      - path: /tmp/pg_diagnostics
        operations: [read, write]
    deny:
      - path: /etc
      - path: /var/lib/postgresql/data
        operations: [write]
        # Read OK for diagnostics, write never
      - path: /etc/ravenfabric
        # Agent's own config is sacrosanct

  # Resource limits
  resources:
    maxCPUPercent: 30
    maxMemoryMB: 1024
    taskTimeoutSeconds: 300
    maxOutputBytes: 10485760

  # Time-bounded access
  validity:
    notBefore: "2026-05-05T08:00:00Z"
    notAfter: "2026-05-05T17:00:00Z"

  # Approval workflow for sensitive operations
  approval:
    required:
      - pattern: "^psql .*-c \"INSERT.*\"$"
        approvers: ["security-team", "dba-lead"]
        minApprovers: 1
        timeoutSeconds: 1800
      - pattern: "DROP TABLE"
        approvers: ["security-team", "dba-lead", "engineering-lead"]
        minApprovers: 2
        timeoutSeconds: 3600
      - pattern: "ALTER TABLE.*DROP COLUMN"
        approvers: ["dba-lead"]
        minApprovers: 1
        timeoutSeconds: 1800
```

### Immutable rules (cannot be overridden)

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: SecurityPolicy
metadata:
  name: cnpg-immutable-baseline
spec:
  immutable:
    neverAllowAsRoot: true
    neverAllowFileModify:
      - /etc/ravenfabric/*
      - /var/lib/postgresql/data/*
      - /etc/ssh/sshd_config
    neverAllowPackageRemove:
      - postgresql-18
      - cloudnative-pg
      - ravenfabric-agent
    neverAllowSqlPattern:
      - "DROP DATABASE postgres"
      - "DROP ROLE postgres"
    alwaysAudit: true
    alwaysRecordSession: true
```

These rules cannot be relaxed by any tenant policy — they apply globally.

---

## Operator Workflows

### Workflow 1: Quick health check

```bash
$ rf exec prod-pg-1 "kubectl cnpg status prod-pg-1"

Cluster Summary
Name:                prod-pg-1
Namespace:           cnpg-system
PostgreSQL Image:    ghcr.io/cloudnative-pg/postgresql:18
Primary instance:    prod-pg-1-1
Status:              Cluster in healthy state
Instances:           3
Ready instances:     3
Current Write LSN:   0/3000060

  audited · 87ms · noise-xx · wireguard-direct
```

### Workflow 2: Interactive psql session (v0.3+)

```bash
$ rf shell prod-pg-1 --container postgres
[connecting via wireguard-direct...]
[noise-xx handshake complete]
[policy: cnpg-prod-dba-policy loaded]
[session recording: enabled, ID=sess-abc123]

postgres@prod-pg-1-1:~$ psql
psql (18.0)
Type "help" for help.

postgres=# SELECT count(*) FROM accounts WHERE created_at > now() - interval '1 day';
 count
-------
   847
(1 row)

postgres=# DROP TABLE old_logs;
ERROR: command requires approval
  rule: commands.approval.required[1]
  approvers: dba-lead
  request ID: appr-xyz789
  status: pending (timeout in 1800s)
  approve at: https://approvals.example.com/appr-xyz789

postgres=# \q
$ exit

[session ended, recording saved: /audit/sess-abc123.cast]
```

### Workflow 3: Port-forward for local tooling (v0.3+)

```bash
$ rf tunnel -L 5432:prod-pg-1-rw:5432
[tunnel established: localhost:5432 → prod-pg-1-rw:5432]
[policy: read-only access enforced]
[time limit: 4 hours]

# In another terminal — use any PostgreSQL tool locally
$ pgcli -h localhost -p 5432 -U dba_readonly prod_db
```

### Workflow 4: Scheduled maintenance via desired-state

```yaml
apiVersion: ravenfabric.io/v1alpha1
kind: DesiredState
metadata:
  name: cnpg-weekly-maintenance
spec:
  targets:
    selector:
      labels:
        cnpg.io/cluster: prod-pg-1

  state:
    schedule: "0 3 * * 0"  # Sundays at 03:00

    steps:
      - name: vacuum-analyze
        command: "psql -c 'VACUUM ANALYZE;'"
        timeout: 1800

      - name: reindex-concurrent
        command: "psql -c 'REINDEX DATABASE CONCURRENTLY prod_db;'"
        timeout: 7200

      - name: backup-to-s3
        command: "pg_dump prod_db -F c | aws s3 cp - s3://backups/prod-pg-1/$(date +%Y%m%d).dump"
        timeout: 3600

  convergence:
    mode: remediate
    onFailure: alert
```

---

## Comparison with Alternatives

| Feature | Bastion + kubectl | Pomerium / Teleport | Tailscale + kubectl | RavenFabric |
|---------|-------------------|---------------------|---------------------|-------------|
| **E2E encrypted (relay/proxy sees nothing)** | No | TLS-termination | WireGuard | Noise XX |
| **No exposed ports on cluster** | No | HTTP/HTTPS | Yes | Yes |
| **Command-level policy** | No | Limited | No | Regex |
| **Session recording** | External tool | Yes | No | asciinema |
| **SQL-level audit** | No | No | No | Via policy |
| **Time-bounded access** | Manual | Yes | ACL | Capability tokens |
| **Approval workflow** | No | Teleport | No | Planned |
| **Air-gap fallback** | No | No | No | Reticulum |
| **MFA per command** | Login only | Yes | No | Yes |
| **Multi-cluster federation** | Complex | Yes | Yes | Yes |
| **Self-hosted (data residency)** | Yes | Pomerium yes / Teleport SaaS | Tailscale SaaS | Yes |
| **No commercial license required** | Yes | Limited | Free tier limited | AGPLv3 |

---

## Implementation Status

This use case relies on RavenFabric capabilities at varying maturity levels.

### Available today (v0.1)

- `rf exec prod-pg-1 "..."` — policy-validated command execution
- Noise XX end-to-end encryption
- Structured JSON-lines audit logging
- Deny-by-default command policy with regex allow/deny
- Hot-reload of policy via SIGHUP
- WebSocket transport via stateless relay
- OTP-based agent enrollment

### Coming in v0.2

- WireGuard direct, QUIC, STUN hole-punching
- File push/pull operations (backup retrieval)
- Real-time stdout/stderr streaming
- Built-in metrics collection

### Coming in v0.3

- Interactive shell (`rf shell`)
- Port forwarding (`rf tunnel -L/-R/-D`)
- Session recording (asciinema format)
- Multi-agent playbooks

### Coming in v0.4+

- Approval workflows
- Sealed secrets injection
- Full mesh VPN with MagicDNS
- WebAuthn MFA per command

---

## Adoption Path

For teams wanting to evaluate RavenFabric for CNPG admin access, a phased
approach reduces risk:

### Phase 1 — Read-only automation (today)

Use RavenFabric for low-risk, automated CNPG operations:

- Health checks and status reporting
- Schema inspection
- Backup verification
- Drift detection against expected configuration
- Metrics collection from PostgreSQL

This validates the core architecture and builds operational familiarity
without putting interactive sessions at risk.

### Phase 2 — Approved write operations (v0.3)

Once interactive shell and tunneling are available, expand to:

- Routine maintenance (VACUUM, REINDEX) under desired-state
- Scheduled backups with verification
- Approved INSERT/UPDATE operations through approval workflow
- Local tool access via `rf tunnel` (pgcli, DBeaver, DataGrip)

### Phase 3 — Full DBA workflow (v0.4+)

Replace existing access patterns:

- Retire bastion hosts for CNPG access
- Reduce VPN scope (CNPG no longer requires VPN)
- Migrate from `kubectl exec` to `rf shell` for all DBA work
- Federate across multiple Kubernetes clusters

### Phase 4 — Enterprise integration (v0.6+)

- Integrate with corporate IdP for operator authentication
- Enable WebAuthn MFA for sensitive operations
- Configure approval workflows tied to ITSM (ServiceNow, Jira)
- Generate compliance reports (NIS2, SOC 2, ISO 27001)

---

## Why This Matters

PostgreSQL is the most widely deployed open-source database. CloudNativePG is
its emerging operator standard for Kubernetes. Yet the security story for
**operator access** to these clusters has not kept pace with the rest of cloud
native security:

- Network-level policies (NetworkPolicy, Cilium) protect application traffic
  but say nothing about who runs `psql`
- Service meshes (Istio, Linkerd) protect service-to-service traffic but
  bypass operator workflows
- Cloud IAM (Azure AD, AWS IAM) controls cluster access but not
  in-database operations

RavenFabric fills the gap between cluster-level access control and
SQL-level operations. It is purpose-built for the case where:

> **A compromised operator session must not be able to do more than what
> policy explicitly permits — and every action must be recorded with
> replay-grade fidelity.**

For regulated industries — healthcare, finance, public sector, critical
infrastructure — this gap is increasingly recognized as a compliance
liability. NIS2 in the EU, FedRAMP in the US, and similar frameworks
elsewhere are pushing organizations toward command-level audit, immutable
policy, and zero-trust operator access.

CNPG admin access is a concrete, high-value use case that demonstrates
RavenFabric's design principles in production-relevant detail.

---

## See Also

- [README.md](../README.md) — RavenFabric overview
- [CONNECTIVITY.md](../CONNECTIVITY.md) — Transport and connectivity model
- [usecase-edge-iot-fleet.md](usecase-edge-iot-fleet.md) — Edge & IoT fleet management
- [usecase-multi-cluster-kubernetes.md](usecase-multi-cluster-kubernetes.md) — Multi-cluster Kubernetes
- [usecase-airgapped-ics.md](usecase-airgapped-ics.md) — Air-gapped industrial systems
- [usecase-msp-multitenant.md](usecase-msp-multitenant.md) — MSP multi-tenant operations
- [CloudNativePG documentation](https://cloudnative-pg.io/documentation/) — CNPG project
