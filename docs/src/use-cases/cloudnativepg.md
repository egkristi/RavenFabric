# Secure CloudNativePG Admin Access

> **Scenario:** A team operates PostgreSQL clusters using CloudNativePG (CNPG)
> on Kubernetes. Database administrators need secure `psql`, `pg_dump`, schema
> migrations, and emergency access — without VPNs, bastion hosts, or exposed
> network endpoints.

---

## The Problem

- **Application access** is solved by Kubernetes Services (no ingress needed)
- **Operator/admin access** is harder — DBAs need interactive psql, backups, performance debugging
- **VPN grants broader access than needed** — lateral movement risk
- **Bastion hosts are persistent attack targets** — credential accumulation
- **`kubectl port-forward` is not production-grade** — no audit, no command-level controls, no session recording
- **No SQL-level policy** — a DBA can run `DROP DATABASE production` with no friction

---

## The RavenFabric Approach

```text
DBA workstation (anywhere)
    │  rf exec --token <token> "psql -c 'SELECT ...'"
    │  rf shell --token <token>
    │  rf forward -L 127.0.0.1:5432 -R cnpg-rw:5432 --token <token>
    ▼
rf-relay (E2E encrypted, sees only ciphertext)
    ▼
rf-agent (sidecar or DaemonSet)
    ├─ Policy check → Audit → Session recording
    ▼
PostgreSQL (port 5432)
```

| Capability | Description |
|------------|-------------|
| No VPN required | Operators connect from any network |
| No exposed ports | Agents connect outbound only |
| Command-level policy | `DELETE FROM users` can require approval |
| Complete audit trail | Structured JSON-lines, every session recorded |
| Per-operator scoping | Read-only vs read-write via policy |
| Time-bounded access | Capability tokens with TTL |
| End-to-end encrypted | Relay sees only Noise XX ciphertext |

---

## Deployment Patterns

### Pattern A: Sidecar in each CNPG pod

Each PostgreSQL pod gets a co-located rf-agent. Communication with PostgreSQL
happens over loopback or Unix socket.

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: prod-pg-1
spec:
  instances: 3
  imageName: ghcr.io/cloudnative-pg/postgresql:17

  inheritedMetadata:
    annotations:
      ravenfabric.io/agent-enabled: "true"
      ravenfabric.io/agent-policy: "cnpg-prod-policy"
```

Trade-offs: Per-pod isolation, local-only PostgreSQL access, survives pod migrations. Higher resource overhead.

### Pattern B: DaemonSet on nodes

One rf-agent per Kubernetes node, accessing CNPG pods via shared Unix socket.

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
    spec:
      containers:
        - name: agent
          image: ghcr.io/egkristi/ravenfabric-agent:latest
          args:
            - --relay=wss://relay.example.com
            - --policy=/etc/ravenfabric/policy.yaml
            - --identity=/etc/ravenfabric/identity.key
          resources:
            requests: { cpu: 50m, memory: 32Mi }
            limits: { cpu: 200m, memory: 128Mi }
          volumeMounts:
            - name: identity
              mountPath: /etc/ravenfabric
              readOnly: true
            - name: cnpg-sockets
              mountPath: /var/run/cnpg
      volumes:
        - name: identity
          secret:
            secretName: rf-agent-identity
            defaultMode: 0400
        - name: cnpg-sockets
          hostPath:
            path: /var/run/cnpg
            type: DirectoryOrCreate
```

Trade-offs: Fewer instances, lower overhead, simpler upgrades. Larger blast radius if compromised.

---

## Policy Configuration

Granular policy is enforced **at the agent** before any command reaches PostgreSQL.

```yaml
spec:
  commands:
    allow:
      # Read-only operations
      - pattern: "^psql .* -c \"SELECT.*\"$"
      - pattern: "^psql .* -c \"EXPLAIN.*\"$"
      - pattern: "^pg_dump .* --schema-only"
      # Backup operations
      - pattern: "^pg_dump prod_db -F c -f /backup/.*"
      - pattern: "^pg_basebackup -D /backup/.*"
      # CNPG operations
      - pattern: "^kubectl cnpg status .*"
      - pattern: "^kubectl cnpg promote .*"
      - pattern: "^kubectl cnpg backup .*"

    deny:
      - pattern: "DROP DATABASE"
      - pattern: "TRUNCATE.*"
      - pattern: "ALTER USER.*SUPERUSER"
      - pattern: "COPY.*TO PROGRAM"

  filesystem:
    allow:
      - path: /backup
      - path: /var/log/postgresql
      - path: /tmp/pg_diagnostics
    deny:
      - path: /etc
      - path: /var/lib/postgresql/data

  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

---

## Operator Workflows

### Health check

```bash
$ rf exec --token <token> "kubectl cnpg status prod-pg-1"

Cluster Summary
Name:       prod-pg-1
Status:     Cluster in healthy state
Instances:  3/3 ready
Primary:    prod-pg-1-1

  audited · 87ms · noise-xx
```

### Interactive psql

```bash
$ rf shell --token <token>
[noise-xx handshake complete]
[policy: cnpg-prod-dba-policy]
[session recording: sess-abc123]

postgres=# SELECT count(*) FROM accounts WHERE created_at > now() - interval '1 day';
 count
-------
   847

postgres=# DROP TABLE old_logs;
ERROR: command requires approval
  approvers: dba-lead
  request ID: appr-xyz789
```

### Port-forward for local tooling

```bash
$ rf forward -L 127.0.0.1:5432 -R prod-pg-1-rw:5432 --token <token>
[forward: localhost:5432 → prod-pg-1-rw:5432]

# Use any PostgreSQL tool locally
$ pgcli -h localhost -p 5432 -U dba_readonly prod_db
```

---

## Comparison with Alternatives

| Feature | Bastion + kubectl | Teleport | Tailscale | RavenFabric |
|---------|-------------------|----------|-----------|-------------|
| E2E encrypted (proxy blind) | No | TLS-terminated | WireGuard | Noise XX |
| No exposed ports | No | HTTP/HTTPS | Yes | Yes |
| Command-level policy | No | Limited | No | Regex allow/deny |
| Session recording | External | Yes | No | Built-in (asciicast) |
| SQL-level audit | No | No | No | Via command policy |
| Approval workflow | No | Teleport | No | Yes |
| Self-hosted | Yes | Paid | SaaS | Yes (AGPL-3.0) |

---

## Implementation Status

| Feature | Status | Notes |
|---------|--------|-------|
| `rf exec` — policy-validated execution | Done | v0.1 |
| Noise XX end-to-end encryption | Done | All transports |
| Structured audit logging | Done | JSON-lines |
| Deny-by-default command policy | Done | Regex allow/deny |
| WebSocket relay transport | Done | Stateless |
| Interactive shell (`rf shell`) | Done | PTY + session recording |
| Port forwarding (`rf forward -L/-R`) | Done | TCP bidirectional |
| Streaming stdout/stderr | Done | Real-time |
| WireGuard direct path | Done | Userspace |
| QUIC transport | Done | 0-RTT, mux |
| STUN/ICE hole-punching | Done | NAT traversal |
| File read/write | Done | Via MCP server |
| Approval workflow | Done | MCP `rf_request_approval` |
| Metrics collection | Done | sysinfo + Prometheus |
| Session recording (asciicast) | Done | `SessionRecorder` |
| Desired-state convergence | Done | `ConvergenceEngine` |
| MagicDNS | Done | UDP DNS server |
| WebAuthn MFA per command | Planned | |
| Sealed secrets injection | Done | ChaCha20-Poly1305 `SecretStore` |

---

## Adoption Path

| Phase | Scope | Risk |
|-------|-------|------|
| 1 | Read-only automation: health checks, schema inspection, backup verification | Low |
| 2 | Approved writes: VACUUM, REINDEX, scheduled maintenance via desired-state | Medium |
| 3 | Full DBA workflow: retire bastion, replace `kubectl exec` with `rf shell` | Medium |
| 4 | Enterprise: IdP integration, WebAuthn, ITSM-linked approval workflows | Low |

---

## See Also

- [Air-Gapped ICS](airgapped-ics.md)
- [Edge & IoT Fleet Management](edge-iot-fleet.md)
- [Multi-cluster Kubernetes](multi-cluster-kubernetes.md)
- [MSP Multi-tenant Operations](msp-multitenant.md)
- [CloudNativePG documentation](https://cloudnative-pg.io/documentation/)
