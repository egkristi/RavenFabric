# RPC Protocol Reference

## Overview

RavenFabric RPC uses msgpack encoding over yamux-multiplexed Noise XX secure channels. All communication is end-to-end encrypted — the relay never sees plaintext.

## Transport Stack

```
┌─────────────────────────────────┐
│ RPC Messages (msgpack)          │
├─────────────────────────────────┤
│ yamux Multiplexing              │  Multiple concurrent streams
├─────────────────────────────────┤
│ SecureChannel (Noise XX)        │  E2E encryption + authentication
├─────────────────────────────────┤
│ Transport Driver                │  WebSocket / QUIC / Memory / ...
└─────────────────────────────────┘
```

## Request Format

```rust
struct Request {
    id: u64,              // Unique request ID (monotonically increasing)
    action: Action,       // What to do
}
```

## Response Format

```rust
struct Response {
    id: u64,              // Matches the request ID
    result: RpcResult,    // Outcome
}
```

## Action Types

| Action | Description | Fields |
|--------|-------------|--------|
| `Exec` | Execute a command | `command: String` |
| `ExecBackground` | Execute without waiting | `command: String` → returns `job_id` |
| `FileRead` | Read a file | `path: String` |
| `FileWrite` | Write a file | `path: String, data: Vec<u8>` |
| `FileList` | List directory | `path: String` |
| `Status` | Query agent status | — |
| `Metrics` | Collect system metrics | — |
| `Heartbeat` | Keep-alive ping | — |
| `Shell` | Open PTY session | `cols: u16, rows: u16` |
| `ShellInput` | Send input to PTY | `data: Vec<u8>` |
| `ShellResize` | Resize PTY | `cols: u16, rows: u16` |
| `ShellClose` | Close PTY session | — |
| `PortForward` | Start port forward | `remote_addr: String, remote_port: u16` |
| `PortForwardClose` | Stop port forward | `id: u64` |
| `RemoteForward` | Agent-side listener | `bind_addr: String, bind_port: u16` |

## Response Types

| Type | Description | Fields |
|------|-------------|--------|
| `ExecResult` | Command output | `exit_code: i32, stdout: Vec<u8>, stderr: Vec<u8>` |
| `FileData` | File contents | `data: Vec<u8>` |
| `FileWritten` | Write confirmation | `bytes: u64` |
| `FileEntries` | Directory listing | `entries: Vec<FileEntry>` |
| `StatusInfo` | Agent status | `id: String, uptime_secs: u64, version: String` |
| `MetricsData` | System metrics | `cpu: f64, memory_used: u64, ...` |
| `Pong` | Heartbeat response | — |
| `JobStarted` | Background job ID | `job_id: u64` |
| `Error` | Error response | `code: u16, message: String` |

## Error Codes

| Code | Meaning |
|------|---------|
| 1000 | Policy denied |
| 1001 | Command not found |
| 1002 | Execution timeout |
| 1003 | Output limit exceeded |
| 2000 | File not found |
| 2001 | Permission denied (filesystem) |
| 2002 | Path outside allowed scope |
| 3000 | Internal error |
| 3001 | Agent busy (max concurrent reached) |

## Encoding

All messages use MessagePack (msgpack) via `rmp-serde`:

```rust
// Serialize request
let bytes = rmp_serde::to_vec(&request)?;

// Frame: [length: 4 bytes BE][msgpack payload]
stream.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
stream.write_all(&bytes).await?;

// Deserialize response
let response: Response = rmp_serde::from_slice(&payload)?;
```

## Multiplexing (yamux)

Multiple RPC requests run concurrently over a single SecureChannel using [yamux](https://github.com/hashicorp/yamux/blob/master/spec.md):

- Each RPC request opens a new yamux stream
- Streams are lightweight (minimal overhead per stream)
- Backpressure via yamux flow control
- Server-initiated streams for push notifications

## Streaming Output

For `Exec` with `mode: streaming`, stdout/stderr are sent as incremental frames:

```
StreamChunk {
    request_id: u64,      // Matches original exec request
    channel: Channel,     // Stdout or Stderr
    data: Vec<u8>,        // Chunk of output
    final: bool,          // True on last chunk
}
```

The client receives output in real-time without waiting for command completion.

## DTN (Delay-Tolerant Networking)

For air-gapped or intermittent connectivity, requests are wrapped as DTN bundles:

```rust
struct DtnBundle {
    id: Uuid,                    // Unique bundle ID
    destination: String,         // Target agent ID
    priority: Priority,          // Low, Normal, High, Critical
    ttl: Duration,               // Time-to-live (expires after)
    payload: Vec<u8>,            // Encrypted RPC request
    content_hash: [u8; 32],      // SHA-256 for integrity
    idempotency_key: Option<String>,  // Deduplication
}
```

Properties:
- **Priority ordering** — Critical bundles delivered first
- **TTL expiration** — Stale bundles discarded automatically
- **Custody transfer** — Reliable hop-by-hop delivery with acknowledgments
- **Content-addressed** — Tamper detection via SHA-256
- **Idempotent** — Duplicate delivery is safe (dedup by key)

## Wire Format Summary

```
Connection establishment:
  1. TCP/WebSocket/QUIC connect to relay
  2. Magic bytes: RVNF (4 bytes) + version (1 byte)
  3. Noise XX handshake (3 messages)
  4. SecureChannel established

Encrypted frame:
  [length: 4 bytes BE][ciphertext + 16-byte Poly1305 MAC]

Inside SecureChannel:
  yamux frames → individual streams → msgpack RPC messages
```
