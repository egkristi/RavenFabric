# Wire Protocol Stability

This document defines the stability guarantees of the RavenFabric wire protocol.

## Stable (will not break without major version bump)

The following are considered stable and will not change without a major version increment (breaking change):

| Element | Value | Guaranteed since |
|---------|-------|------------------|
| Wire magic | `RVNF` (4 bytes: `0x52 0x56 0x4E 0x46`) | v0.1.0 |
| Version byte | `0x01` | v0.1.0 |
| Noise pattern | `Noise_XX_25519_ChaChaPoly_BLAKE2s` | v0.1.0 |
| Frame format | `[length: 4 bytes BE][ciphertext + 16-byte MAC]` | v0.1.0 |
| Handshake message framing | `[length: 2 bytes BE][noise message]` | v0.1.0 |
| RPC encoding | MessagePack (via `rmp-serde`) | v0.1.0 |
| Multiplexing | yamux over SecureChannel | v0.1.0 |
| Max frame payload | 65,535 bytes | v0.1.0 |
| Key file format | 64 bytes (32 private + 32 public) | v0.1.0 |

## Connection Sequence (Stable)

```text
Client                          Server
  │                               │
  ├──── "RVNF" + 0x01 ──────────►│  Wire magic + version
  │◄──── "RVNF" + 0x01 ──────────┤  Wire magic + version
  │                               │
  ├──── [len:2][msg1: e] ────────►│  Noise XX message 1
  │◄──── [len:2][msg2: e,ee,s,es]─┤  Noise XX message 2
  ├──── [len:2][msg3: s,se] ─────►│  Noise XX message 3
  │                               │
  │  ═══ SecureChannel active ═══  │
  │                               │
  ├──── [len:4][encrypted frame]──►│  Application data
  │◄──── [len:4][encrypted frame]─┤  Application data
  │                               │
```

## RPC Types (Stable Serialization)

The following RPC types have stable msgpack serialization. Fields may be added (with `#[serde(default)]`) but existing fields will not be renamed or removed:

- `Request` — `{ id, action, timeout_ms, reason? }`
- `Response` — `{ id, result }`
- `Action` — enum with stable variants (new variants may be added)
- `RpcResult` — enum with stable variants (new variants may be added)

## Versioning Policy

- **Version 1** is the current and only supported wire version
- When version 2 is introduced, agents will attempt v2 first and fall back to v1
- The server will reject unknown versions with a clear error
- Version negotiation happens before the Noise handshake (in plaintext)

## Backward Compatibility Commitment

Starting with v1.0.0-rc.1:

- Wire protocol format will not change without a version byte increment
- New RPC `Action` variants may be added (enums are `#[non_exhaustive]`)
- New fields with `#[serde(default)]` may be added to Request/Response
- Existing fields will not be removed or renamed
- Old agents will gracefully reject unknown actions with `RpcResult::Error`

## Breaking Change Policy

A wire protocol breaking change requires:

1. Major version bump (e.g., 1.0 → 2.0) OR version byte increment
2. Documented migration path
3. At least one release with dual-version support (speak both old and new)
4. Minimum 3-month deprecation notice for the old version
