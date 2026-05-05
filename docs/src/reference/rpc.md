# RPC Protocol Reference

## Overview

RavenFabric RPC uses msgpack encoding over yamux-multiplexed Noise XX secure channels.

## Request Types

| Action | Description | Fields |
|--------|-------------|--------|
| `Exec` | Execute a command | `command: String` |
| `FileRead` | Read a file | `path: String` |
| `FileWrite` | Write a file | `path: String, data: Vec<u8>` |
| `Status` | Query agent status | — |
| `Heartbeat` | Keep-alive ping | — |
| `Shell` | Open PTY session | `cols: u16, rows: u16` |

## Response Types

| Type | Description | Fields |
|------|-------------|--------|
| `ExecResult` | Command result | `exit_code: i32, stdout: Vec<u8>, stderr: Vec<u8>` |
| `FileData` | File contents | `data: Vec<u8>` |
| `FileWritten` | Write confirmation | `bytes: u64` |
| `StatusInfo` | Agent status | `id: String, uptime: u64, ...` |
| `Pong` | Heartbeat response | — |
| `Error` | Error response | `message: String` |

## Message Format

```
Request {
    id: u64,          // Unique request ID
    action: Action,   // Enum variant
}

Response {
    id: u64,          // Matching request ID
    result: Result,   // Enum variant
}
```

## Encoding

All messages use msgpack (MessagePack) via `rmp-serde`:

```rust
// Serialize
let bytes = rmp_serde::to_vec(&request)?;

// Deserialize
let response: Response = rmp_serde::from_slice(&bytes)?;
```

## Streaming

For long-running commands, stdout/stderr are streamed over separate yamux streams. The client receives output in real-time without waiting for command completion.

## DTN (Delay-Tolerant Networking)

For air-gapped or high-latency environments, requests can be queued as DTN bundles with:

- Priority levels (Low, Normal, High, Critical)
- TTL-based expiration
- Custody transfer (reliable delivery)
- Idempotency keys (deduplication)
