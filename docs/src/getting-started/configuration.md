# Configuration

RavenFabric uses TOML configuration files. CLI flags override config values.

## Agent Configuration (`raven.toml`)

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
token = "your-meet-token"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"
metrics_addr = "127.0.0.1:9100"  # Optional, omit to disable

[transport]
reconnect_interval = 5   # Seconds between reconnect attempts
max_retries = 0          # 0 = infinite retries
```

## Relay Configuration

The relay is configured via CLI flags and environment variables (no config file):

```bash
rf-relay --listen 0.0.0.0:9090
# RELAY_SECRET env var enables HMAC token verification
```

## CLI Configuration

The CLI reads from `~/.config/ravenfabric/config.toml`:

```toml
[cli]
default_relay = "wss://relay.example.com/meet"
key_path = "~/.config/ravenfabric/cli.key"
timeout = 30
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RELAY_SECRET` | HMAC secret for relay meet token verification |
| `RUST_LOG` | Logging level (`info`, `debug`, `rf_agent=trace`) |

## Override Priority

1. CLI flags (highest priority)
2. Config file values
3. Built-in defaults

## Next Steps

- [Full Configuration Reference](../reference/config.md) — complete schema and all options
- [Policy Configuration](../guide/policy-config.md) — define allowed commands
- [Quick Start](quickstart.md) — get running immediately
