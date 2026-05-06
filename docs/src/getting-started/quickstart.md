# Quick Start

Get a working RavenFabric setup in under 2 minutes.

## Option A: Dev Mode (Fastest)

Dev mode starts a relay + agent in a single process with a permissive policy — ideal for trying out `rf exec`:

```bash
# Terminal 1: Start dev mode (relay + agent, no auth)
rf dev --port 9090
```

```bash
# Terminal 2: Execute a command
rf exec --relay ws://127.0.0.1:9090 --token dev "hostname"
```

> Dev mode is not for production. It uses a permissive policy and no authentication.

## Option B: Full Setup (Production-like)

### 1. Start the Relay

The relay is a stateless encrypted broker. It never sees plaintext.

```bash
rf-relay --listen 0.0.0.0:9090 --secret "your-meet-secret"
```

### 2. Start the Agent

```bash
rf-agent \
  --relay ws://127.0.0.1:9090/meet \
  --id web-01 \
  --policy /etc/ravenfabric/policy.yaml
```

### 3. Execute a Command

```bash
rf exec --relay ws://127.0.0.1:9090 --token "meet-token" "hostname"
```

Output:
```
web-01
```

### 4. Verify Audit Log

Every action is logged as structured JSON:

```json
{
  "timestamp": "2026-05-06T10:30:00Z",
  "action": "exec",
  "command": "hostname",
  "agent_id": "web-01",
  "decision": "allowed",
  "exit_code": 0,
  "duration_ms": 12
}
```

## What Just Happened?

1. CLI connected to relay via WebSocket
2. Relay paired CLI and agent using the meet token
3. Noise XX handshake established E2E encrypted channel (relay sees only ciphertext)
4. CLI sent `Execute` RPC request over yamux-multiplexed session
5. Agent checked policy → allowed → ran command → returned output
6. Audit entry written to structured log

## Next Steps

- [Configuration](configuration.md) — Full raven.toml reference
- [Policy Configuration](../guide/policy-config.md) — Define what's allowed
- [CLI Reference](../reference/cli.md) — All commands and options
- [Architecture Overview](../architecture/overview.md) — How the system fits together
- [Security Model](../architecture/security.md) — Understand the trust model
