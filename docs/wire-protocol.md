# Wire Protocol

## Overview

RavenFabric uses a custom binary protocol built on the Noise framework.
All communication is encrypted and mutually authenticated.

## Connection Establishment

```text
1. TCP / WebSocket connection established
2. Magic bytes: "RVNF" (4 bytes) + version (1 byte)
3. Noise XX handshake (3 round-trips):
   - → e                    (initiator ephemeral)
   - ← e, ee, s, es        (responder ephemeral + static)
   - → s, se               (initiator static)
4. Handshake complete — both parties verified
5. Switch to transport mode (encrypted frames)
```

## Frame Format

After handshake, all data is sent as encrypted frames:

```text
┌──────────────────┬────────────────────────────────┐
│  Length (4 bytes) │  Ciphertext + MAC (16 bytes)   │
│  big-endian u32   │  ChaCha20-Poly1305 encrypted   │
└──────────────────┴────────────────────────────────┘
```

- Maximum plaintext per frame: 65,535 bytes
- MAC overhead: 16 bytes per frame
- Length field covers ciphertext + MAC

## Multiplexing

Over the encrypted channel, yamux provides stream multiplexing:

- Multiple concurrent RPC requests over a single connection
- Flow control per stream
- Keep-alive pings
- Graceful stream close

## RPC Encoding

Within each yamux stream, messages are msgpack-encoded:

```text
Request:
  id: String (UUID)
  action: Action enum
  timeout_ms: Option<u64>

Response:
  id: String (matches request)
  result: RpcResult enum
```

## Noise Parameters

```text
Pattern:    XX (mutual authentication, no pre-shared keys)
DH:         25519 (Curve25519)
Cipher:     ChaChaPoly (ChaCha20-Poly1305)
Hash:       BLAKE2s
```

## Version Negotiation

- Version 1: Current protocol (this document)
- If version mismatch: connection closed with error frame
- Forward compatibility: unknown fields ignored in msgpack
