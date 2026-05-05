# Policy YAML Reference

## Full Schema

```yaml
spec:
  # Command execution policies
  commands:
    allow:
      - pattern: "<regex>"    # Rust regex syntax
    deny:
      - pattern: "<regex>"    # Deny takes precedence over allow

  # Filesystem access policies
  filesystem:
    allow:
      - path: "<absolute-path>"
    deny:
      - path: "<absolute-path>"

  # Resource limits
  resources:
    maxOutputBytes: 10485760    # Max stdout+stderr (bytes)
    timeoutSeconds: 300          # Max execution time (seconds)
```

## Rules

1. **Deny-by-default**: If no `allow` rule matches, the action is denied
2. **Deny wins**: If both `allow` and `deny` match, deny takes precedence
3. **Regex matching**: Command patterns use Rust regex syntax
4. **Path resolution**: Symlinks are resolved before policy checks
5. **Immutable denies**: Some deny rules cannot be overridden (e.g., `/etc/shadow`)

## Examples

### Web Server Administration

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl (status|restart|reload) nginx"
      - pattern: "^journalctl -u nginx.*"
      - pattern: "^cat /var/log/nginx/.*"
      - pattern: "^nginx -t"
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
      - pattern: "^systemctl status .*"
      - pattern: "^df -h"
      - pattern: "^free -m"
      - pattern: "^uptime"
      - pattern: "^cat /proc/(meminfo|cpuinfo|loadavg)"
  filesystem:
    allow:
      - path: /var/log
    deny:
      - path: /var/log/audit
  resources:
    maxOutputBytes: 1048576
    timeoutSeconds: 30
```

### Deny Everything (Lockdown)

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
