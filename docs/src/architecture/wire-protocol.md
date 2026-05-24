# Wire Protocol

RavenFabric uses a custom binary wire protocol optimized for security and efficiency.

## Frame Format

```text
┌──────────────┬─────────┬──────────────────────────────────┐
│  Magic (4B)  │ Ver (1B)│         Payload                  │
│  R V N F     │  0x01   │  [Noise handshake / frames]      │
└──────────────┴─────────┴──────────────────────────────────┘
```

### Magic Bytes

Every connection starts with `RVNF` (0x52 0x56 0x4E 0x46) followed by version byte (currently `0x01`). Invalid magic causes immediate disconnection — no error response, no negotiation.

## Handshake Phase

After magic + version validation, the Noise XX handshake begins:

```text
Noise_XX_25519_ChaChaPoly_BLAKE2s

Message 1: Initiator → Responder:  e
Message 2: Responder → Initiator:  e, ee, s, es
Message 3: Initiator → Responder:  s, se
```

After three messages, both sides have:

- Verified each other's static public key (mutual authentication)
- Established shared symmetric keys (forward secrecy)
- The initiator's identity is hidden until message 3 (identity protection)

## Encrypted Frame Format

After handshake, all data is encrypted:

```text
┌────────────────┬──────────────────────────────────────┐
│ Length (4B BE) │  Ciphertext + MAC (16B)              │
└────────────────┴──────────────────────────────────────┘
```

- **Length**: 4 bytes, big-endian, total ciphertext length including MAC
- **Ciphertext**: ChaCha20-Poly1305 encrypted payload
- **MAC**: 16-byte Poly1305 authentication tag
- **Max frame**: 64 KB per encrypted frame

Any MAC verification failure immediately terminates the connection and triggers tamper detection.

## Multiplexing

Yamux multiplexing runs over the SecureChannel, enabling concurrent operations:

```text
SecureChannel
  └── yamux
      ├── Stream 0: RPC request/response
      ├── Stream 1: Shell session I/O
      ├── Stream 2: Port forward data
      └── Stream N: additional concurrent streams
```

Each yamux stream is independent — a slow file transfer doesn't block command execution.

## RPC Encoding

RPC messages are encoded with msgpack (via `rmp-serde`):

```rust
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
}

pub enum Action {
    Execute { command, env, workdir },
    StreamExecute { command, env, workdir },
    Read { path },
    Write { path, data, mode },
    Shell { shell, rows, cols, env },
    Ping,
    Status,
    // ... and more (see RPC Reference)
}

pub struct Response {
    pub id: String,
    pub result: RpcResult,
}

pub enum RpcResult {
    Success { stdout, stderr, exit_code, duration_ms },
    Denied { reason, rule },
    Error { message },
    StreamChunk { stream, data },
    Pong { timestamp_ms },
    // ... (see RPC Reference)
}
```

## Transport Independence

The wire protocol is transport-agnostic. The same magic + handshake + encrypted frames work over:

- WebSocket (relay connections)
- QUIC (direct connections)
- WireGuard (peer-to-peer)
- Serial port (air-gapped)
- Any `AsyncRead + AsyncWrite` stream

The transport layer only needs to deliver ordered bytes — the protocol handles everything else.
