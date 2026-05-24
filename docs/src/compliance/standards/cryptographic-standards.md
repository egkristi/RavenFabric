# Cryptographic Standards — Implementation Reference

> This document details the cryptographic algorithms, protocols, and key management
> practices implemented in RavenFabric.

**RavenFabric version:** v0.2-dev  
**Last updated:** 2026-05-05

---

## Cryptographic Suite

### Primary Protocol: Noise XX

RavenFabric uses the Noise Protocol Framework (revision 34) for all
authenticated encrypted communication.

**Pattern:** `Noise_XX_25519_ChaChaPoly_BLAKE2s`

| Component | Algorithm | Standard | Key Size |
|-----------|-----------|----------|----------|
| Key Agreement | X25519 | RFC 7748 | 256-bit |
| Symmetric Cipher | ChaCha20-Poly1305 | RFC 8439 | 256-bit key, 96-bit nonce |
| Hash Function | BLAKE2s | RFC 7693 | 256-bit output |
| Handshake Pattern | XX (mutual authentication) | Noise Framework r34 | 3-message |

**Implementation:** `snow` crate (Rust, audited Noise implementation)

### Handshake Flow (Noise XX)

```
Initiator                                    Responder
    │                                            │
    │──── msg1: e ────────────────────────────→  │  (ephemeral key)
    │                                            │
    │←─── msg2: e, ee, s, es ────────────────── │  (ephemeral + static)
    │                                            │
    │──── msg3: s, se ────────────────────────→  │  (static key proof)
    │                                            │
    │        [Session keys established]          │
    │  [Both parties mutually authenticated]     │
```

**Properties achieved:**

- Mutual authentication (both sides prove identity)
- Forward secrecy (ephemeral keys per session)
- Identity hiding (static keys encrypted in transit)
- Resistance to key compromise impersonation (KCI)

---

## Wire Protocol

### Frame Format

```
┌──────────────┬───────────────────────────────────────┐
│ Length (4B)   │ Ciphertext + MAC (16B)                │
│ Big-endian   │ ChaCha20-Poly1305 encrypted payload   │
└──────────────┴───────────────────────────────────────┘
```

- **Maximum payload:** 65,535 bytes per frame
- **MAC:** 16-byte Poly1305 authentication tag
- **Nonce:** Counter-based, independent per direction (read/write)
- **No replay:** Monotonically increasing nonce prevents replay attacks

### Connection Handshake

```
┌──────────┬─────────┬────────────────────────────┐
│ Magic    │ Version │ Noise XX Handshake (3 msgs) │
│ "RVNF"  │ 0x01    │                              │
│ (4B)     │ (1B)    │                              │
└──────────┴─────────┴────────────────────────────┘
```

- Magic bytes `RVNF` validated before handshake (prevents protocol confusion)
- Version byte enables future protocol evolution

---

## Key Management

### Static Identity Keys

| Property | Implementation |
|----------|----------------|
| Algorithm | X25519 (Curve25519) |
| Key size | 256-bit private + 256-bit public |
| Storage format | 64-byte binary file (32B private + 32B public) |
| File permissions | Unix 0600 (owner read-write only) |
| Generation | OS CSPRNG via `rand` crate |
| Write atomicity | Temp file → set permissions → atomic rename |
| Memory protection | Private key bytes zeroed on `Drop` (byte-by-byte) |
| Derivation | None (raw key, not derived from passphrase) |

### Key Lifecycle

```
Generation ──→ Storage (0600) ──→ Load ──→ Handshake ──→ Session ──→ Drop (zeroed)
     │                                                         │
     └── Atomic write (no permission window)                   └── Memory zeroed
```

### Session Keys

| Property | Detail |
|----------|--------|
| Derived from | Noise XX handshake (X25519 ECDH) |
| Lifetime | Per-session (new keys each connection) |
| Forward secrecy | Yes — ephemeral keys used in derivation |
| Storage | In-memory only (never persisted) |
| Split | Independent send/receive keys |

---

## Standards Conformance

### Implemented (verified in code)

| Standard | Description | Verification |
|----------|-------------|--------------|
| **RFC 7748** | X25519 key agreement | Via `snow` crate (Noise XX) |
| **RFC 8439** | ChaCha20-Poly1305 AEAD | Via `snow` crate (Noise XX) |
| **RFC 7693** | BLAKE2s hash | Via `snow` crate (Noise XX) |
| **Noise Framework r34** | Handshake patterns | Via `snow` crate, XX pattern |
| **RFC 8489** | STUN binding requests | `stun_client.rs` — real UDP STUN |
| **RFC 8445** | ICE candidate gathering | `stun_client.rs` — priority computation per RFC |
| **RFC 9116** | security.txt | `website/.well-known/security.txt` |

### Planned

| Standard | Description | Timeline |
|----------|-------------|----------|
| **FIPS 203 (ML-KEM)** | Post-quantum key encapsulation | v0.6 |
| **FIPS 204 (ML-DSA)** | Post-quantum digital signatures | v0.6 |
| **FIPS 140-3** | Validated cryptographic modules (opt-in mode) | v0.6 |
| **RFC 9180 (HPKE)** | Hybrid Public Key Encryption for DTN | v0.5 |

---

## Cryptographic Decisions and Rationale

### Why Noise XX over TLS?

| Property | Noise XX | TLS 1.3 |
|----------|----------|---------|
| Mutual authentication | Built-in (3-message handshake) | Optional (client certs rarely used) |
| Certificate authority | None required | Requires PKI infrastructure |
| Identity hiding | Static keys encrypted in transit | Server identity exposed in ServerHello |
| Implementation complexity | ~500 LOC (via snow) | ~50,000 LOC (typical TLS stack) |
| Protocol confusion attacks | Magic byte validation | ALPN (but complex) |
| Relay opacity | Relay sees only random bytes | Relay sees TLS ClientHello metadata |

### Why ChaCha20-Poly1305 over AES-GCM?

- Constant-time without hardware support (no timing side-channels on ARM/IoT)
- Better performance on devices without AES-NI (Raspberry Pi, Android, embedded)
- Same security level (256-bit key, 128-bit authentication)
- No nonce-misuse catastrophic failure mode (vs AES-GCM nonce reuse)

### Why BLAKE2s over SHA-256?

- Faster in software (no hardware acceleration needed)
- Designed for 32-bit platforms (s = small, optimized for embedded)
- Same security margin as SHA-3
- Native to Noise Protocol Framework

---

## Threat Model (Cryptographic)

| Threat | Mitigation |
|--------|------------|
| Passive eavesdropping | ChaCha20-Poly1305 encryption on all data |
| Active man-in-the-middle | Noise XX mutual authentication (static key verification) |
| Replay attacks | Monotonic nonce counter per direction |
| Key compromise (past sessions) | Forward secrecy via ephemeral keys |
| Quantum computer (future) | ML-KEM hybrid planned for v0.6 |
| Side-channel (timing) | ChaCha20 is constant-time without hardware support |
| Key extraction from memory | Keys zeroed on drop, file permissions 0600 |
| Protocol downgrade | Single protocol version, no negotiation |
| Relay compromise | End-to-end encryption — relay never holds keys |

---

## Compliance Relevance

| Framework | Relevant Controls |
|-----------|-------------------|
| **NIST SP 800-53** | SC-8 (Transmission Confidentiality), SC-12 (Key Establishment), SC-13 (Cryptographic Protection) |
| **NIS2 Art. 21(2)(h)** | Policies on cryptography and encryption |
| **NSM 2.3** | Krypter kommunikasjon |
| **ISO 27001 A.10** | Cryptographic controls |
| **PCI-DSS 4.0** | Requirement 4 (Protect cardholder data with strong cryptography) |
