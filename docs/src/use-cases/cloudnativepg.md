# CloudNativePG Database Access

Secure, policy-controlled access to PostgreSQL clusters managed by CloudNativePG in Kubernetes — without exposing database ports or creating VPN tunnels.

## The Problem

CloudNativePG runs PostgreSQL inside Kubernetes with no external listeners. DBAs need access for maintenance, but traditional approaches (port-forward, jump boxes, VPN) either leak credentials, bypass audit, or create persistent network paths into production.

## How RavenFabric Solves It

```
DBA (rf-cli) ──Noise XX──▶ Relay ──▶ Agent (in-cluster)
                                         │
                                         ├─ Policy check
                                         ├─ Audit log
                                         └─ Execute: psql / pg_dump / etc.
```

1. **Agent runs as a sidecar or DaemonSet** inside the cluster
2. **No inbound ports** — agent connects outbound to relay
3. **Every command policy-checked** — only approved SQL and maintenance commands
4. **Full audit trail** — who ran what, when, result, duration

## Example Policy

```yaml
spec:
  commands:
    allow:
      - pattern: "^psql -h .* -U readonly -c 'SELECT.*'$"
      - pattern: "^pg_dump --schema-only.*"
      - pattern: "^kubectl cnpg status.*"
    deny:
      - pattern: ".*DROP.*"
      - pattern: ".*DELETE FROM.*"
      - pattern: ".*TRUNCATE.*"
  resources:
    timeoutSeconds: 60
    maxOutputBytes: 5242880
```

## Example Session

```bash
# Check cluster status
rf exec cnpg-agent "kubectl cnpg status my-cluster"

# Run a read-only query
rf exec cnpg-agent "psql -h my-cluster-rw -U readonly -c 'SELECT count(*) FROM orders'"

# Schema dump for review
rf exec cnpg-agent "pg_dump --schema-only -h my-cluster-rw mydb"
```

## Why Not Alternatives?

| Approach | Problem |
|----------|---------|
| `kubectl port-forward` | No audit, no policy, credential on DBA laptop |
| Jump box | Persistent attack surface, shared credentials |
| VPN into cluster | Over-provisioned network access |
| Cloud IAM proxy | Vendor lock-in, no air-gap support |
| **RavenFabric** | E2E encrypted, policy-checked, audited, no inbound ports |

## Deployment

Deploy the agent as a Kubernetes Deployment with access to the CNPG service:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ravenfabric-agent
  namespace: cnpg-system
spec:
  replicas: 1
  template:
    spec:
      containers:
        - name: rf-agent
          image: ghcr.io/egkristi/ravenfabric-agent:latest
          args:
            - --relay=wss://relay.company.com/meet
            - --id=cnpg-agent
            - --policy=/etc/ravenfabric/policy.yaml
          volumeMounts:
            - name: config
              mountPath: /etc/ravenfabric
      volumes:
        - name: config
          configMap:
            name: ravenfabric-policy
```
