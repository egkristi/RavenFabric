# Configuration

RavenFabric uses a TOML configuration file (`raven.toml`).

## Agent Configuration

```toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"

[transport]
driver = "websocket"
reconnect_interval = 5
max_retries = 0  # infinite

[resources]
max_output_bytes = 10485760  # 10 MB
timeout_seconds = 300        # 5 minutes
max_concurrent = 10
```

## Relay Configuration

```toml
[relay]
listen = "0.0.0.0:9090"
meet_secret = "env:RELAY_SECRET"  # Load from environment

[rate_limit]
requests_per_second = 100
burst = 200
per_ip = true
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
| `RELAY_SECRET` | Relay meet secret (when using `env:` prefix) |
| `RF_KEY_PATH` | Override key file path |
| `RF_RELAY` | Override relay URL |
| `RF_POLICY` | Override policy file path |
| `RUST_LOG` | Logging level (`info`, `debug`, `trace`) |
