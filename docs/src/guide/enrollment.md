# Agent Enrollment

RavenFabric uses a one-time password (OTP) enrollment flow. No certificate authority. No centralized key server.

## Enrollment Flow

```text
Admin                      Agent                    TrustStore
  │                          │                          │
  │─── generate OTP ─────────┼──────────────────────►   │
  │    (returns token)       │                          │
  │                          │                          │
  │─── provide token ───────►│                          │
  │                          │                          │
  │                          │── enroll(token) ────────►│
  │                          │   (generates keypair)    │
  │                          │                          │
  │                          │◄── enrolled(pubkey) ─────│
  │                          │                          │
```

## How It Works

1. **Admin generates an OTP** — a short-lived token that authorizes one agent enrollment
2. **Token is delivered to the agent** — via config file, environment variable, or CLI flag
3. **Agent connects to the relay** — presenting the meet token alongside its freshly generated public key
4. **Mutual authentication completes** — the relay pairs the agent with the controller, the Noise XX handshake verifies both sides

## Agent Enrollment via CLI

```bash
# Start an agent with a one-time meet token
rf-agent \
  --id web-01 \
  --relay wss://relay.example.com/meet \
  --token <meet-token> \
  --key-path /etc/ravenfabric/agent.key \
  --policy-path /etc/ravenfabric/policy.yaml \
  --audit-path /var/log/ravenfabric/audit.jsonl
```

Or via configuration file:

```toml
# /etc/ravenfabric/raven.toml
[agent]
id = "web-01"
relay = "wss://relay.example.com/meet"
token = "your-meet-token-here"
key_path = "/etc/ravenfabric/agent.key"
policy_path = "/etc/ravenfabric/policy.yaml"
audit_path = "/var/log/ravenfabric/audit.jsonl"
```

Then simply:

```bash
rf-agent --config /etc/ravenfabric/raven.toml
```

## Key Generation

On first run, the agent generates an Ed25519 key pair at the configured `key_path`. The private key is stored with restricted permissions (0600) and zeroed from memory on drop.

```bash
# Keys are generated automatically on first run
# To pre-generate a key:
rf-agent --config /etc/ravenfabric/raven.toml
# The agent creates agent.key if it doesn't exist
```

## Security Properties

- **Meet tokens are shared secrets** — the relay uses HMAC verification if a `RELAY_SECRET` is configured
- **No secrets traverse the network in plaintext** — the Noise XX handshake encrypts everything after message 1
- **No certificate authority** — identity is the key pair itself
- **Deny-by-default** — even after enrollment, the agent only executes commands allowed by its policy file

## See Also

- [Security Model](../architecture/security.md) — Trust model and key management
- [Production Deployment](production-deployment.md) — systemd setup and hardening
- [Configuration Reference](../reference/config.md) — Full `raven.toml` schema
