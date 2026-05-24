# Security Model

RavenFabric's security is built on three pillars: cryptographic identity, deny-by-default policy, and comprehensive audit logging.

## Trust Model

```text
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

```text
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

This provides:

- **Mutual authentication** — both sides prove their identity
- **Forward secrecy** — ephemeral keys per session
- **Identity hiding** — static keys encrypted during handshake
- **Relay opacity** — relay sees only random bytes

### Handshake Flow

```text
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

After handshake, all frames are encrypted with ChaCha20-Poly1305 (16-byte MAC per frame).

## Policy Engine

The policy engine is **deny-by-default**. If a rule doesn't explicitly allow an action, it is denied.

### Two-Phase Check

1. **Controller pre-flight** — validates the request before forwarding
2. **Agent local check** — agent independently validates (final authority)

A compromised controller cannot override agent policy. The agent always has the last word.

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

Deny rules always win. If both allow and deny match, the action is denied.

## Security Invariants

These invariants are enforced at all times and verified by tests:

1. No command executes without policy check
2. No connection accepted without completed Noise handshake
3. Audit log is append-only (no delete/truncate operations)
4. Private keys zeroed from memory on drop
5. OTP tokens are single-use, hash-stored, TTL-enforced
6. Symlink resolution before path policy checks (prevent traversal)
7. Output size bounded (prevent memory exhaustion)
8. Execution timeout enforced (prevent hanging)
9. No shell injection — commands run via `sh -c` with policy-checked string
10. Relay never decrypts payload (E2E between agent and client)
11. Wire protocol magic (`RVNF`) and version byte validated on every connection
12. `RwLock`/`Mutex` poisoning handled gracefully (no panics on poisoned locks)
13. Tamper detection triggers automatic transport migration — compromised paths abandoned immediately
14. Connection metrics propagate even over DTN/mesh — no blind spots regardless of topology

## Capability Tokens

RavenFabric supports Biscuit-inspired capability tokens for fine-grained authorization:

- **Self-contained** — carry their own signed permissions (Ed25519)
- **Delegatable** — Agent A can grant Agent B limited capabilities with depth limits
- **Attenuatable** — capabilities narrowed via subset restriction, never widened
- **Offline-verifiable** — no central authority needed at execution time
- **Expiring** — TTL-enforced, tokens have limited lifetime

## Post-Quantum Resistance

RavenFabric implements hybrid key exchange combining classical and post-quantum algorithms:

- **Hybrid KEM** — classical + PQ secrets combined via HKDF-SHA256
- **PQXDH-inspired ratchet** — double ratchet with skipped key tracking
- **Harvest-now-decrypt-later protection** — data encrypted today remains safe against future quantum computers

## CRDT Policy Propagation

Policies are distributed across agents using CRDTs (Conflict-free Replicated Data Types):

- **GSet, LwwRegister, OrSet** — different convergence strategies per data type
- **Deny-wins semantics** — conflicts always resolve toward restriction
- **Append-only signed logs** — SHA-256 hash chain with HMAC signatures
- **Content-addressed distribution** — policies identified by content hash

## Threat Model

| Threat | Mitigation |
|--------|-----------|
| Compromised relay | E2E encryption — relay sees only ciphertext |
| Compromised controller | Agent-side policy re-check is final authority |
| Network MITM | Noise XX mutual authentication + forward secrecy |
| Replay attacks | Per-session ephemeral keys, nonce-based encryption |
| Key theft | Keys zeroed on drop, file permissions enforced |
| Policy bypass | Deny-by-default, symlink resolution, regex validation |
| Resource exhaustion | Output limits, timeout enforcement, rate limiting |
| Transport tampering | MAC verification, automatic path migration |
| Quantum adversary | Hybrid PQ KEM, harvest-resistant key exchange |
