# RPC Protocol

RavenFabric uses a custom RPC protocol over encrypted channels. All communication is E2E encrypted — the relay never sees message content.

## Transport Stack

```
┌──────────────────────────────┐
│ RPC (Request / Response)     │  ← msgpack-encoded
├──────────────────────────────┤
│ yamux (multiplexing)         │  ← concurrent streams
├──────────────────────────────┤
│ SecureChannel (Noise XX)     │  ← E2E encrypted frames
├──────────────────────────────┤
│ Transport Driver             │  ← WebSocket / QUIC / etc.
└──────────────────────────────┘
```

## Request

```rust
pub struct Request {
    pub id: String,
    pub action: Action,
    pub timeout_ms: Option<u64>,
}
```

## Action Types

| Action | Description | Fields |
|--------|-------------|--------|
| `Execute` | Run a command | `command`, `env`, `workdir` |
| `StreamExecute` | Run with streaming output | `command`, `env`, `workdir` |
| `BackgroundExec` | Run in background | `command`, `env`, `workdir` |
| `JobQuery` | Query background job status | `job_id` |
| `JobWait` | Wait for background job | `job_id` |
| `Read` | Read a file | `path` |
| `Write` | Write a file | `path`, `data`, `mode` |
| `List` | List directory | `path` |
| `Metrics` | Collect system metrics | — |
| `Status` | Agent status/version | — |
| `Ping` | Liveness check | — |
| `Signal` | Send signal to process | `pid`, `signal` |
| `Shell` | Open PTY shell session | `shell`, `rows`, `cols`, `env` |
| `ShellInput` | Send input to shell | `session_id`, `data` |
| `ShellResize` | Resize shell terminal | `session_id`, `rows`, `cols` |
| `ShellClose` | Close shell session | `session_id` |
| `PortForward` | Start local forward | `bind_addr`, `target_addr` |
| `PortForwardClose` | Stop forward | `forward_id` |
| `RemoteForward` | Start remote forward | `bind_addr`, `target_addr` |
| `Socks5Forward` | Start SOCKS5 proxy | `bind_addr` |
| `Socks5Close` | Stop SOCKS5 proxy | `forward_id` |
| `HealthCheck` | Run health probe | `probe_type`, `target`, `timeout_ms` |
| `TailLog` | Tail a log file | `path`, `lines` |

## Response

```rust
pub struct Response {
    pub id: String,
    pub result: RpcResult,
}
```

## Result Types

| Result | When | Fields |
|--------|------|--------|
| `Success` | Command completed | `stdout`, `stderr`, `exit_code`, `duration_ms` |
| `Denied` | Policy rejected | `reason`, `rule` |
| `Error` | Runtime error | `message` |
| `StatusInfo` | Status response | `agent_id`, `version`, `uptime_seconds` |
| `StreamChunk` | Streaming output | `stream` (Stdout/Stderr), `data` |
| `StreamEnd` | Stream complete | `exit_code`, `duration_ms` |
| `JobStarted` | Background job began | `job_id`, `pid` |
| `JobStatus` | Job query result | `job_id`, `running`, `exit_code`, `stdout`, `stderr` |
| `Pong` | Ping response | `timestamp_ms` |
| `ShellOpened` | Shell ready | `session_id` |
| `ShellOutput` | Shell output data | `session_id`, `data` |
| `ShellExited` | Shell closed | `session_id`, `exit_code` |
| `ForwardStarted` | Forward active | `forward_id`, `bind_addr` |
| `ForwardStopped` | Forward closed | `forward_id` |
| `HealthCheckResult` | Probe result | `success`, `latency_ms`, `error` |
| `TailOutput` | Log lines | `lines`, `path` |

## Encoding

All messages use **msgpack** (via `rmp-serde`):
- Compact binary encoding (smaller than JSON)
- Schema-flexible (fields can be added without breaking)
- Length-delimited frames: `[4 bytes BE length][msgpack payload]`

## Multiplexing

**yamux** provides multiplexed streams over a single SecureChannel:
- Multiple concurrent RPC requests without head-of-line blocking
- Stream-level flow control
- Lightweight — minimal overhead per stream

## Streaming Protocol

For `StreamExecute`, the response is a sequence of messages:
1. Multiple `StreamChunk` results (stdout/stderr as they arrive)
2. Final `StreamEnd` with exit code

This enables real-time output display without buffering.

## DTN (Delay-Tolerant Networking)

For disconnected/intermittent agents, requests can be wrapped in DTN bundles:

```rust
pub struct DtnBundle {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub payload: Vec<u8>,
    pub priority: Priority,
    pub ttl_seconds: u64,
    pub hop_limit: u8,
    pub content_hash: Option<String>,
}
```

Bundles support:
- Priority ordering (Critical > High > Normal > Low > Bulk)
- TTL expiration (except Critical which never expires)
- Content-addressed integrity verification
- Hop-limited forwarding
- Store-carry-forward delivery
