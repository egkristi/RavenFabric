# Configuration

RavenFabric uses a TOML configuration file (`raven.toml`) and YAML policy files.

## Agent Configuration (raven.toml)

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"           # websocket | quic | wireguard
reconnect_interval = 5         # seconds between reconnect attempts
max_retries = 0                # 0 = infinite

[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"  # "env:VAR" reads from environment
```

## Policy (policy.yaml)

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl (status|restart) .*"
      - pattern: "^journalctl.*"
      - pattern: "^docker ps.*"
    deny:
      - pattern: ".*rm.*-rf.*"
      - pattern: ".*dd.*if=.*of=.*/dev/.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
      - path: /tmp/ravenfabric
    deny:
      - path: /etc/shadow
      - path: /etc/passwd
      - path: /root
  resources:
    maxOutputBytes: 10485760   # 10 MB
    timeoutSeconds: 300        # 5 minutes
```

## Relay Configuration

```toml
[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"
max_connections = 1000
idle_timeout = 300
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `RELAY_SECRET` | Shared secret for relay meet protocol | (required) |
| `RF_LOG` | Log level filter (`trace`, `debug`, `info`, `warn`, `error`) | `info` |
| `RF_LOG_FORMAT` | Log format (`text`, `json`) | `text` |

## Key Management

Keys are generated automatically on first run:

```bash
# Generate a new key pair
rf-agent --generate-key /etc/ravenfabric/agent.key

# The public key is printed to stdout for registration
```

Key files are:

- 64 bytes (32-byte private + 32-byte public, hex-encoded)
- Permission-protected (0600)
- Private key zeroed from memory on process exit
