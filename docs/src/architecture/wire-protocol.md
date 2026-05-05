# Wire Protocol

RavenFabric uses a custom binary wire protocol optimized for security and efficiency.

## Frame Format

```
┌──────────────┬─────────┬──────────────────────────────────┐
│  Magic (4B)  │ Ver (1B)│         Payload                  │
│  R V N F     │  0x01   │  [Noise handshake / frames]      │
└──────────────┴─────────┴──────────────────────────────────┘
```

### Magic Bytes

Every connection starts with `RVNF` (0x52 0x56 0x4E 0x46) followed by version byte (currently `0x01`). Invalid magic causes immediate disconnection.

## Handshake Phase

After magic + version validation, the Noise XX handshake begins:

```
Noise_XX_25519_ChaChaPoly_BLAKE2s

Message 1: Initiator → Responder:  e
Message 2: Responder → Initiator:  e, ee, s, es
Message 3: Initiator → Responder:  s, se
```

## Encrypted Frame Format

After handshake completion, all data is encrypted:

```
┌────────────────┬──────────────────────────────────────┐
│ Length (4B BE)  │  Ciphertext + MAC (16B)              │
└────────────────┴──────────────────────────────────────┘
```

- **Length**: 4 bytes, big-endian, total ciphertext length including MAC
- **Ciphertext**: ChaCha20-Poly1305 encrypted payload
- **MAC**: 16-byte Poly1305 authentication tag

## Multiplexing

Yamux multiplexing runs over the SecureChannel, allowing concurrent RPC streams:

```
SecureChannel
  └── yamux
      ├── Stream 0: RPC requests/responses
      ├── Stream 1: stdout streaming
      ├── Stream 2: stderr streaming
      └── Stream N: additional streams
```

## RPC Encoding

RPC messages are encoded with msgpack (MessagePack):

```rust
enum Request {
    Exec { command: String },
    FileRead { path: String },
    FileWrite { path: String, data: Vec<u8> },
    Status,
    Heartbeat,
}

enum Response {
    ExecResult { exit_code: i32, stdout: Vec<u8>, stderr: Vec<u8> },
    FileData { data: Vec<u8> },
    FileWritten { bytes: u64 },
    StatusInfo { ... },
    Pong,
    Error { message: String },
}
```

## Transport Independence

The wire protocol is transport-agnostic. The same framing works over:

- WebSocket (primary)
- QUIC
- TCP
- Unix sockets
- Memory channels (testing)
- Any `AsyncRead + AsyncWrite` implementation
