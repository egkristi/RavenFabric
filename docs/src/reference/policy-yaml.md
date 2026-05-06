# Policy YAML Reference

## Full Schema

```yaml
spec:
  # Command execution policies
  commands:
    allow:
      - pattern: "<regex>"          # Rust regex syntax
        requires_role: "<role>"     # Optional: restrict to specific RBAC role
    deny:
      - pattern: "<regex>"          # Deny takes precedence over allow

  # Filesystem access policies
  filesystem:
    allow:
      - path: "<absolute-path>"
    deny:
      - path: "<absolute-path>"

  # Resource limits
  resources:
    maxOutputBytes: 10485760        # Max stdout+stderr (bytes)
    timeoutSeconds: 300             # Max execution time (seconds)
    maxConcurrent: 10               # Max parallel executions

  # Immutable rules (cannot be overridden by policy reload)
  immutable:
    deny:
      - pattern: "<regex>"          # Permanent deny — survives policy changes
```

## Rules

1. **Deny-by-default**: If no `allow` rule matches, the action is denied
2. **Deny wins**: If both `allow` and `deny` match, deny takes precedence
3. **Immutable rules**: Cannot be removed or overridden by policy reload (SIGHUP)
4. **Regex matching**: Command patterns use [Rust regex syntax](https://docs.rs/regex/latest/regex/#syntax)
5. **Path resolution**: Symlinks are resolved before policy checks (prevents traversal)
6. **Role gating**: Commands with `requires_role` are only allowed for callers with that role

## RBAC Roles

| Role | Description |
|------|-------------|
| `admin` | Full access (can modify policy, manage agents) |
| `operator` | Execute commands, view status |
| `senior-operator` | Execute + dangerous operations (restart, scale) |
| `viewer` | Read-only (status, logs) |
| `auditor` | Read audit logs only |

Role is determined by the caller's identity and the tenant policy.

## Multi-Tenant Extensions

For MSP and multi-tenant deployments:

```yaml
spec:
  tenant_id: "acme-corp"

  authorized_identities:
    - identity: "alice@msp.example.com"
      role: senior-operator
      validity:
        notBefore: "2026-01-01T00:00:00Z"
        notAfter: "2026-12-31T23:59:59Z"

    - identity: "bob@msp.example.com"
      role: operator
      restrictions:
        time_windows:
          - days: [Mon, Tue, Wed, Thu, Fri]
            hours: ["08:00-18:00"]
            timezone: "Europe/Oslo"
```

### Time Window Restrictions

Restrict access to specific days and hours:

```yaml
restrictions:
  time_windows:
    - days: [Mon, Tue, Wed, Thu, Fri]
      hours: ["08:00-18:00"]
      timezone: "Europe/Oslo"
    - days: [Sat, Sun]
      hours: ["10:00-14:00"]
      timezone: "Europe/Oslo"
```

Outside defined windows, all commands are denied regardless of allow rules.

## Examples

### Web Server Administration

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl (status|restart|reload) nginx$"
      - pattern: "^journalctl -u nginx.*"
      - pattern: "^cat /var/log/nginx/.*"
      - pattern: "^nginx -t$"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /etc/nginx
      - path: /var/log/nginx
    deny:
      - path: /etc/nginx/ssl
  resources:
    maxOutputBytes: 5242880
    timeoutSeconds: 60
```

### Read-Only Monitoring

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*$"
      - pattern: "^df -h$"
      - pattern: "^free -m$"
      - pattern: "^uptime$"
      - pattern: "^cat /proc/(meminfo|cpuinfo|loadavg)$"
  filesystem:
    allow:
      - path: /var/log
    deny:
      - path: /var/log/audit
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
```

### Kubernetes Operations (Role-Gated)

```yaml
spec:
  commands:
    allow:
      # Read-only for all operators
      - pattern: "^kubectl get .*$"
      - pattern: "^kubectl describe .*$"
      - pattern: "^kubectl logs .*$"

      # Mutations require senior-operator role
      - pattern: "^kubectl rollout restart .*$"
        requires_role: senior-operator
      - pattern: "^kubectl scale .*$"
        requires_role: senior-operator

    deny:
      - pattern: ".*kubectl delete namespace.*"
      - pattern: ".*--force --grace-period=0.*"
  resources:
    timeoutSeconds: 120
    maxOutputBytes: 10485760
```

### Air-Gapped Lockdown (Immutable)

```yaml
spec:
  commands:
    allow:
      - pattern: "^cat /var/log/.*\\.log$"
      - pattern: "^systemctl status .*$"
      - pattern: "^df -h$"
      - pattern: "^uptime$"
    deny:
      - pattern: ".*"
  immutable:
    deny:
      - pattern: ".*systemctl (start|stop|restart|enable|disable).*"
      - pattern: ".*reboot.*"
      - pattern: ".*shutdown.*"
      - pattern: ".*rm .*"
      - pattern: ".*dd .*"
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
```

### Deny Everything (Emergency Lockdown)

```yaml
spec:
  commands:
    allow: []
    deny:
      - pattern: ".*"
  filesystem:
    allow: []
    deny:
      - path: /
  resources:
    maxOutputBytes: 0
    timeoutSeconds: 0
```

## Hot Reload

Policy can be reloaded without restarting the agent:

```bash
# Reload policy via SIGHUP
kill -HUP $(pidof rf-agent)

# Or via systemd
systemctl reload rf-agent
```

Immutable rules persist across reloads — they can only be changed by restarting the agent with a new policy file.
