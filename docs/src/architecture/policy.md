# Policy Engine

The policy engine is the security core of RavenFabric. It enforces deny-by-default access control at every level.

## Deny-by-Default

If a policy does not explicitly allow an action, it is denied. There is no implicit "allow all" — every capability must be granted.

## Policy YAML Format

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
      - pattern: "^cat /var/log/.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*shutdown.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
    deny:
      - path: /etc/shadow
      - path: /root
  resources:
    maxOutputBytes: 10485760    # 10 MB
    timeoutSeconds: 300          # 5 minutes
```

## RBAC

Role-Based Access Control with five built-in roles:

| Role | Permissions |
|------|-------------|
| Admin | Full access (execute, read, write, view status, manage policy) |
| Operator | Execute commands, read/write files, view status |
| Viewer | View status only |
| Auditor | View status, read audit logs |
| Custom | User-defined permission set |

## Capability Tokens

Biscuit-inspired capability tokens for fine-grained, delegatable authorization:

- Authority block defines maximum scope
- Attenuation blocks narrow (never widen) permissions
- Caveats enforce constraints (time windows, source IPs)
- Offline verification — no central authority needed

## Distributed Policy

For disconnected or mesh deployments:

- **CRDT convergence** — Policies converge without a master
- **Append-only logs** — Signed hash chain (Scuttlebutt-inspired)
- **Content-addressed** — Request policy by hash, any node can serve
- **Quorum verification** — Multi-signer approval
- **Conflict resolution** — Most-restrictive policy wins by default

## Telemetry Governance

Policy controls what telemetry data is collected and exported:

```yaml
telemetry:
  allow:
    - type: system_metrics
    - type: app_metrics
  deny:
    - type: process_metrics
  exporter:
    type: otlp
    endpoint: "https://otel.example.com:4317"
```
