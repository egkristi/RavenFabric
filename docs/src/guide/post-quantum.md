# Post-Quantum Keys

RavenFabric supports a hybrid key exchange that combines a classical elliptic-curve algorithm with a post-quantum key encapsulation mechanism (KEM). An attacker must break **both** the classical and the post-quantum components simultaneously to compromise the session key — "harvest now, decrypt later" attacks against recorded traffic are defeated.

## What "Hybrid" Means

A pure post-quantum handshake replaces classical crypto. A hybrid handshake runs both in parallel and derives the session key from both:

```
session_key = KDF(classical_secret || post_quantum_secret)
```

If the post-quantum algorithm is later found to have a weakness, the classical component still provides its full security margin, and vice versa. This is the conservative, standards-aligned approach (NIST SP 800-232, draft RFC 9258).

---

## Enabling Post-Quantum Keys

```toml
# raven.toml
[crypto]
post_quantum = true             # Enable PQ hybrid (default: false)
kem_algorithm = "kyber768"      # Kyber-768 (ML-KEM-768, NIST PQC round 4)
```

No other configuration or deployment changes are required. The handshake protocol negotiates PQ support automatically — agents that have `post_quantum = true` use the hybrid exchange with each other, and fall back to classical-only with older agents that do not announce PQ capability.

---

## Supported Algorithms

| Algorithm | NIST Level | Notes |
|-----------|-----------|-------|
| `kyber512` | 1 (128-bit classical equivalent) | Lowest overhead |
| `kyber768` | 3 (192-bit classical equivalent) | **Recommended** |
| `kyber1024` | 5 (256-bit classical equivalent) | Maximum security |

The classical component is always X25519 (Curve25519 Diffie-Hellman), as used in the Noise XX handshake.

---

## Handshake Flow

With `post_quantum = true`, the Noise XX handshake is extended:

1. Classical X25519 ephemeral keys are exchanged as in standard Noise XX
2. The initiator generates a Kyber ephemeral key pair and sends the public key
3. The responder encapsulates a random secret using the Kyber public key and sends the ciphertext
4. Both sides derive the session key from `KDF(x25519_shared || kyber_secret || handshake_hash)`
5. All subsequent frames are encrypted with the hybrid-derived key

The handshake adds approximately 1,200 bytes of data (Kyber-768 key sizes) and ~200 µs of CPU time on modern hardware.

---

## Performance Considerations

| Operation | Classical only | Hybrid (Kyber-768) |
|-----------|---------------|-------------------|
| Handshake bandwidth | ~200 bytes | ~1,400 bytes |
| Handshake CPU | ~50 µs | ~250 µs |
| Per-frame overhead | None | None |

Post-quantum overhead is incurred **once per session** during the handshake. Ongoing encrypted frame performance is identical — the session key is the same size regardless of how it was derived.

On constrained devices (Raspberry Pi Zero, ESP32), the handshake takes 5–30 ms, which is acceptable for most use cases. Use `kyber512` if you need lower latency on very constrained hardware.

---

## Forward Secrecy Properties

| Scenario | Classical only | Hybrid PQ |
|----------|---------------|-----------|
| Compromised long-term identity key | Session key safe (ephemeral) | Session key safe |
| Quantum computer breaks X25519 | Session key compromised if traffic was recorded | Session key safe |
| Quantum computer breaks Kyber | Session key safe (X25519 component unbroken) | Session key safe |
| Both X25519 and Kyber broken | N/A | Session key compromised |

The hybrid design provides protection today against a future quantum adversary who has recorded historical traffic.

---

## Compatibility

| Both agents PQ-enabled | Exchange used |
|------------------------|--------------|
| Yes | Hybrid X25519+Kyber |
| No (one or both classical-only) | Classical X25519 only |

Fallback to classical-only is automatic and transparent. To require PQ on a segment of your fleet, set `require_post_quantum = true` in the policy:

```yaml
# policy.yaml
spec:
  network:
    require_post_quantum: true   # Refuse sessions without PQ handshake
```

With this policy, classical-only peers are refused at the handshake stage and a `HANDSHAKE_PQ_REQUIRED` event is written to the audit log.

---

## Verifying PQ Handshake

```bash
rf status --token <TOKEN> --verbose
```

Output includes the negotiated handshake parameters:

```
Agent:       web-01
Version:     0.20.0
Uptime:      4d 12h
Transport:   quic-direct
Handshake:   Noise_XX_25519+Kyber768_ChaChaPoly_BLAKE2s
```

---

## See Also

- [Architecture: Crypto Layer](../architecture/crypto.md) — Noise XX handshake details and frame format
- [Key Management](../guide/enrollment.md) — Long-term identity key provisioning
- [Compliance: BSI TR-02102](../compliance/frameworks/nis2-directive.md) — German BSI quantum-safe requirements
