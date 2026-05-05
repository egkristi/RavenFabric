# Quick Start

This guide walks through a minimal setup: one relay, one agent, one CLI client.

## 1. Generate Keys

```bash
# Generate agent keypair
rf-agent --generate-key /etc/ravenfabric/agent.key

# Generate CLI keypair
rf --generate-key ~/.config/ravenfabric/cli.key
```

## 2. Start the Relay

The relay is a stateless encrypted broker. It never sees plaintext.

```bash
rf-relay --listen 0.0.0.0:9090 --secret "your-meet-secret"
```

## 3. Start the Agent

```bash
rf-agent \
  --relay wss://relay.example.com:9090/meet \
  --key /etc/ravenfabric/agent.key \
  --policy /etc/ravenfabric/policy.yaml \
  --id web-01
```

## 4. Execute a Command

```bash
# One-time enrollment token
rf exec --relay wss://relay.example.com:9090/meet \
  --token "enrollment-token" \
  "hostname"
```

Output:
```
web-01
```

## 5. Verify Audit Log

Every action is logged:

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "action": "exec",
  "command": "hostname",
  "agent_id": "web-01",
  "result": "success",
  "exit_code": 0
}
```

## Next Steps

- [Configuration](configuration.md) — Full agent configuration
- [Policy Configuration](../guide/policy-config.md) — Define what's allowed
- [Security Model](../architecture/security.md) — Understand the trust model
