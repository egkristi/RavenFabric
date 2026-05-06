# Policy Configuration

## Creating a Policy File

Create a YAML file defining what actions are allowed:

```yaml
# /etc/ravenfabric/policy.yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl (status|restart) nginx"
      - pattern: "^journalctl -u nginx"
      - pattern: "^cat /var/log/nginx/.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*>(>)?\\s*/dev/.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log/nginx
    deny:
      - path: /etc/shadow
      - path: /root
      - path: /proc/kcore
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Pattern Syntax

Command patterns use Rust regex syntax:

| Pattern | Matches |
|---------|---------|
| `^systemctl status .*` | `systemctl status nginx`, `systemctl status ssh` |
| `^cat /var/log/.*` | `cat /var/log/syslog`, `cat /var/log/nginx/access.log` |
| `.*rm.*-rf.*` | Any command containing `rm` and `-rf` |

## Deny Takes Precedence

If both allow and deny match a command, **deny wins**:

```yaml
commands:
  allow:
    - pattern: ".*"        # Allow everything
  deny:
    - pattern: ".*rm.*"    # Except rm (this wins)
```

## Filesystem Policies

Path-based access control with symlink resolution:

```yaml
filesystem:
  allow:
    - path: /opt/app           # Allow read/write under /opt/app
  deny:
    - path: /opt/app/secrets   # Deny the secrets subdirectory
```

Symlinks are resolved to their real path before policy checks, preventing traversal attacks.

## Resource Limits

```yaml
resources:
  maxOutputBytes: 10485760    # 10 MB max output
  timeoutSeconds: 300          # 5 minute timeout
```

These limits are enforced by the executor and cannot be overridden by the client.

## Hot Reload

Send `SIGHUP` to the agent process to reload policy without restarting:

```bash
kill -HUP $(pidof rf-agent)
```

Immutable rules persist across reloads — they can only be changed by restarting the agent.

## See Also

- [Policy YAML Reference](../reference/policy-yaml.md) — Full schema with RBAC, time windows, tenants
- [Architecture: Policy Engine](../architecture/policy.md) — How decisions are made
- [Use Cases](../use-cases/cloudnativepg.md) — Real-world policy examples
- [Audit Log Format](../reference/audit-log-format.md) — Policy decisions are logged
