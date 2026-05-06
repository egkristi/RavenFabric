# Agent Enrollment

RavenFabric uses a one-time password (OTP) enrollment flow. No certificate authority. No centralized key server.

## Enrollment Flow

```
Admin                      Agent                    TrustStore
  │                          │                          │
  │─── generate OTP ─────────┼──────────────────────►   │
  │    (returns token)       │                          │
  │                          │                          │
  │─── give token to agent ─►│                          │
  │                          │                          │
  │                          │── enroll(token) ────────►│
  │                          │   (generates keypair)    │
  │                          │                          │
  │                          │◄── enrolled(pubkey) ─────│
  │                          │                          │
```

## Generate an Enrollment Token

```bash
# Generate a one-time token (valid for 5 minutes)
rf admin enroll --ttl 300 --agent-id web-01
# Output: otp_abc123def456
```

## Enroll an Agent

```bash
rf-agent --enroll \
  --token otp_abc123def456 \
  --relay wss://relay.example.com/meet \
  --key-path /etc/ravenfabric/agent.key
```

The agent:
1. Generates a new Ed25519 key pair locally
2. Sends the public key to the relay with the OTP token
3. TrustStore validates the OTP (single-use, hash-stored, TTL-enforced)
4. On success, the agent is registered and can receive RPC calls

## Security Properties

- **OTP tokens are single-use** — Once consumed, the token hash is marked used
- **Hash-stored** — Only the hash of the OTP is stored, not the plaintext
- **TTL-enforced** — Tokens expire after a configurable time window
- **No secrets in transit** — The agent generates its key pair locally
- **No certificate authority** — Identity is the key pair itself

## See Also

- [Security Model](../architecture/security.md) — Trust model and key management
- [Production Deployment](production-deployment.md) — Key rotation and hardening
- [Quick Start](../getting-started/quickstart.md) — First enrollment walkthrough
