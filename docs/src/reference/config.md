# Configuration File Reference

RavenFabric uses TOML configuration files.

## Agent (`/etc/ravenfabric/raven.toml`)

```toml
[agent]
# Unique agent identifier
id = "web-01"

# Relay WebSocket URL
relay = "wss://relay.example.com/meet"

# Path to agent private key
key_path = "/etc/ravenfabric/agent.key"

# Path to policy file
policy_path = "/etc/ravenfabric/policy.yaml"

# Path to audit log (JSON lines)
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
# Transport driver: "websocket", "quic", "memory"
driver = "websocket"

# Reconnect interval (seconds)
reconnect_interval = 5

# Maximum reconnect attempts (0 = infinite)
max_retries = 0

[resources]
# Maximum output size per command (bytes)
max_output_bytes = 10485760  # 10 MB

# Maximum execution time per command (seconds)
timeout_seconds = 300

# Maximum concurrent executions
max_concurrent = 10
```

## Relay

```toml
[relay]
# Listen address
listen = "0.0.0.0:9090"

# Meet secret (use env: prefix for environment variables)
meet_secret = "env:RELAY_SECRET"

[rate_limit]
# Requests per second per source IP
requests_per_second = 100

# Burst allowance
burst = 200
```

## CLI (`~/.config/ravenfabric/config.toml`)

```toml
[cli]
# Default relay URL
default_relay = "wss://relay.example.com/meet"

# CLI key path
key_path = "~/.config/ravenfabric/cli.key"

# Default timeout (seconds)
timeout = 30
```

## Environment Variables

All configuration values can be overridden via environment variables:

| Variable | Overrides |
|----------|-----------|
| `RF_AGENT_ID` | `agent.id` |
| `RF_RELAY` | `agent.relay` |
| `RF_KEY_PATH` | `agent.key_path` |
| `RF_POLICY` | `agent.policy_path` |
| `RELAY_SECRET` | `relay.meet_secret` (with `env:` prefix) |
| `RUST_LOG` | Logging level |
