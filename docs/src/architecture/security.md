# Security Model

RavenFabric's security is built on three pillars: cryptographic identity, deny-by-default policy, and comprehensive audit logging.

## Trust Model

```
               ┌─────────────┐
               │  Trust Root  │
               │  (key pair)  │
               └──────┬──────┘
                      │
         ┌────────────┼────────────┐
         │            │            │
    ┌────▼────┐  ┌────▼────┐  ┌───▼────┐
    │  Agent  │  │  Agent  │  │  CLI   │
    │ key pair│  │ key pair│  │key pair│
    └─────────┘  └─────────┘  └────────┘
```

Every entity has a unique Ed25519 key pair. Identity is cryptographic — there are no usernames, passwords, or certificates.

## Noise XX Handshake

All connections use the Noise XX handshake pattern:

```
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

This provides:
- **Mutual authentication** — Both sides prove their identity
- **Forward secrecy** — Ephemeral keys per session
- **Identity hiding** — Static keys encrypted during handshake
- **Relay opacity** — Relay sees only random bytes

### Handshake Flow

```
Initiator                          Responder
    │                                  │
    │── e ─────────────────────────►   │  (ephemeral key)
    │                                  │
    │   ◄──────────────── e, ee, s, es │  (ephemeral + static)
    │                                  │
    │── s, se ─────────────────────►   │  (static key, encrypted)
    │                                  │
    │         [secure channel]         │
```

## Policy Engine

The policy engine is **deny-by-default**. If a rule doesn't explicitly allow an action, it is denied.

### Two-Phase Check

1. **Controller pre-flight** — Validates the request before forwarding
2. **Agent local check** — Agent independently validates (final authority)

A compromised controller cannot override agent policy.

### Policy YAML

```yaml
spec:
  commands:
    allow:
      - pattern: "^systemctl status .*"
      - pattern: "^journalctl.*"
    deny:
      - pattern: ".*rm.*-rf.*"
  filesystem:
    allow:
      - path: /opt/app
      - path: /var/log
    deny:
      - path: /etc/shadow
  resources:
    maxOutputBytes: 10485760
    timeoutSeconds: 300
```

## Security Invariants

These invariants are enforced at all times:

1. No command executes without policy check
2. No connection accepted without completed Noise handshake
3. Audit log is append-only (no delete/truncate)
4. Private keys zeroed from memory on drop
5. OTP tokens are single-use, hash-stored, TTL-enforced
6. Symlink resolution before path policy checks
7. Output size bounded (prevent memory exhaustion)
8. Execution timeout enforced (prevent hanging)
9. No shell injection — commands policy-checked
10. Relay never decrypts payload (E2E between agent and client)

## Capability Tokens

RavenFabric supports Biscuit-inspired capability tokens:

- **Self-contained** — Carry their own signed permissions
- **Delegatable** — Agent A can grant Agent B limited capabilities
- **Attenuatable** — Capabilities can be narrowed, never widened
- **Offline-verifiable** — No central authority needed at execution time

## Post-Quantum Resistance

Hybrid key exchange (ML-KEM + X25519) protects against harvest-now-decrypt-later attacks. The post-quantum layer is additive — classical security is never weakened.
