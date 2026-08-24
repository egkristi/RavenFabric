# Controller (Management Plane)

The controller is the management plane binary. It maintains an agent registry,
serves the REST API, and hosts the embedded Web UI dashboard. It is a read-only
observability and routing surface — the agent remains the final policy
authority, so a compromised controller cannot override agent-side enforcement.

## Quick Start

```bash
rf-controller --listen 0.0.0.0:9091
```

Open <http://localhost:9091> for the dashboard.

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--listen <ADDR>` | `0.0.0.0:9091` | HTTP bind address (dashboard + REST API) |
| `--token <TOKEN>` | unset | Bearer token required for authenticated endpoints (`RF_CONTROLLER_TOKEN`) |
| `--max-agents <N>` | `10000` | Maximum agents tracked in the registry |
| `--heartbeat-timeout-ms <N>` | `30000` | Mark agents stale after N ms without a heartbeat |

## REST API

### Agent Registry

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/api/v1/agents/heartbeat` | none | Register an agent or refresh its heartbeat |
| `GET` | `/api/v1/agents` | viewer | List all agents |
| `GET` | `/api/v1/agents/{id}` | viewer | Get a single agent |

### Health

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/healthz` | none | Liveness/readiness — `{"status":"healthy","agents_online":N}` |
| `GET` | `/api/health` | none | Alias for `/healthz` |

### Agent Heartbeat Payload

```json
{
  "id": "web-01",
  "key_hash": "hex-encoded-agent-key-hash",
  "version": "1.0.0-rc.16",
  "region": "eu-west",
  "relay_url": "wss://relay.example.com/meet",
  "labels": { "role": "web", "env": "prod" }
}
```

`id` is required. All other fields are optional and default to empty/`None`.

## Example

```bash
# Register an agent
curl -X POST http://localhost:9091/api/v1/agents/heartbeat \
  -H 'Content-Type: application/json' \
  -d '{"id":"web-01","version":"1.0.0-rc.16","region":"eu-west"}'

# List agents (authenticated)
curl http://localhost:9091/api/v1/agents -H 'Authorization: Bearer <token>'
```

## Systemd Service

```ini
# /etc/systemd/system/rf-controller.service
[Service]
ExecStart=/usr/local/bin/rf-controller --listen 0.0.0.0:9091
Restart=on-failure
```

See `deploy/rf-controller.service` for the hardened unit with security and
resource limits.

## See Also

- [Relay Setup](relay.md) — the relay pairs agents and clients; the controller observes them
- [Fleet Orchestration](fleet-orchestration.md) — multi-agent playbooks
- [Production Deployment](production-deployment.md) — TLS termination and systemd
