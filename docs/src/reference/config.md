# Configuration Reference

RavenFabric uses TOML configuration files. CLI flags override config file values.

## Agent Configuration (`raven.toml`)

```toml
[agent]
# Unique agent identifier (required)
id = "web-01"

# Relay WebSocket URL (required)
relay = "wss://relay.example.com/meet"

# Meet token for relay pairing (required on first connect)
token = "your-meet-token"

# Path to agent private key (default: ./agent.key)
key_path = "/etc/ravenfabric/agent.key"

# Path to policy YAML file (required)
policy_path = "/etc/ravenfabric/policy.yaml"

# Path to audit log (default: ./audit.jsonl)
audit_path = "/var/log/ravenfabric/audit.jsonl"

# Prometheus metrics endpoint (optional, omit to disable)
metrics_addr = "127.0.0.1:9100"

[transport]
# Reconnect interval in seconds (default: 5)
reconnect_interval = 5

# Maximum reconnect attempts, 0 = infinite (default: 0)
max_retries = 0
```

## Agent CLI Flags

All config values can be overridden via CLI flags:

```
rf-agent [OPTIONS]

OPTIONS:
  -c, --config <PATH>        Config file path [default: raven.toml]
  -i, --id <ID>              Agent ID (overrides config)
  -r, --relay <URL>          Relay WebSocket URL (overrides config)
  -t, --token <TOKEN>        Meet token (overrides config)
  -k, --key-path <PATH>      Key file path (overrides config)
  -p, --policy-path <PATH>   Policy file path (overrides config)
  -a, --audit-path <PATH>    Audit log path (overrides config)
      --metrics-addr <ADDR>  Prometheus metrics endpoint
```

## Relay Configuration

The relay binary accepts CLI arguments (no config file):

```
rf-relay [OPTIONS]

OPTIONS:
  -l, --listen <ADDR>    Listen address [default: 0.0.0.0:9090]
  -s, --secret <SECRET>  HMAC secret for meet token verification
                         (also via RELAY_SECRET env var)
```

## CLI Configuration (`~/.config/ravenfabric/config.toml`)

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

| Variable | Component | Description |
|----------|-----------|-------------|
| `RELAY_SECRET` | rf-relay | HMAC secret for relay meet token verification |
| `RF_RELAY` | rf (CLI) | Default relay URL |
| `RUST_LOG` | All | Logging level filter (e.g., `info`, `rf_agent=debug`) |
| `RF_POLICY_PATH` | rf-mcp-server | Path to policy YAML file |
| `RF_AUDIT_PATH` | rf-mcp-server | Path to audit log file |
| `RF_API_TOKEN` | rf-mcp-server | API authentication token |
| `RF_API_TOKEN_FILE` | rf-mcp-server | Path to token file (supports rotation) |
| `RF_ALERT_WEBHOOK` | rf-mcp-server | Webhook URL for anomaly alert notifications |
| `RF_CALLERS` | rf-mcp-server | Path to RBAC callers config (TOML) |
| `RF_RATE_LIMIT` | rf-mcp-server | Max tool calls per minute (default: 60) |
| `RF_HTTP_LISTEN` | rf-mcp-server | HTTP+SSE listen address (e.g., `0.0.0.0:8080`) |
| `RF_LOG_LEVEL` | rf-mcp-server | Log level: trace, debug, info, warn, error |

## Resolution Order

Configuration values are resolved in this order (first wins):

1. CLI flags (`--relay`, `--id`, etc.)
2. Config file values (`raven.toml`)
3. Built-in defaults

## See Also

- [Policy YAML Reference](policy-yaml.md) — Full policy file schema
- [Production Deployment](../guide/production-deployment.md) — systemd setup, TLS, hardening
