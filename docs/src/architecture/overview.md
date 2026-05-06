# Architecture Overview

RavenFabric is organized as a layered architecture with strict dependency boundaries.

## Layers

```
┌──────────────────────────────────┐
│        Application Layer         │  rf-cli, rf-agent, rf-relay, rf-mcp-server
│  (binaries, user-facing tools)   │
├──────────────────────────────────┤
│         Executor Layer           │  rf-executor
│  (command execution, streaming)  │
├──────────────────────────────────┤
│          Policy Layer            │  rf-policy
│  (deny-by-default enforcement)  │
├──────────────────────────────────┤
│         Audit Layer              │  rf-audit
│  (structured logging, anomaly)  │
├──────────────────────────────────┤
│           RPC Layer              │  rf-rpc
│  (message types, codec, mux)    │
├──────────────────────────────────┤
│        Transport Layer           │  rf-transport
│  (drivers, connection mgmt)     │
├──────────────────────────────────┤
│          Crypto Layer            │  rf-crypto
│  (Noise XX, key management)     │
└──────────────────────────────────┘
```

## Crates

| Crate | Purpose | LOC | Tests |
|-------|---------|-----|-------|
| `rf-crypto` | Noise XX handshake, SecureChannel, key management, PQ hybrid KEM | ~1,600 | 35 |
| `rf-transport` | Driver trait, WebSocket/QUIC/Memory/WireGuard/Vsock/Unix/Stdio, NAT traversal, mesh, auto-selection, MASQUE, ECH | ~15,700 | 318 |
| `rf-rpc` | Message types, msgpack codec, yamux mux, DTN, routing, controller API | ~5,800 | 106 |
| `rf-audit` | Structured JSON-lines audit logging, anomaly event integration | ~650 | 14 |
| `rf-policy` | Policy enforcement, RBAC, capabilities, CRDT convergence, anomaly detection, injection detection | ~4,500 | 97 |
| `rf-executor` | Command execution, streaming, orchestration, PTY, plugins | ~6,500 | 105 |
| `rf-bootstrap` | OTP enrollment, TrustStore, relay pairing | ~430 | 11 |
| `rf-relay` | Stateless encrypted relay broker with per-IP rate limiting | ~390 | 7 |
| `rf-agent` | Agent binary (connects to relay, executes RPC, reconnect with backoff) | ~370 | — |
| `rf-cli` | CLI client `rf` (exec, dev, status, policy, completions) | ~1,080 | — |
| `rf-mcp-server` | MCP server for AI agent integration (Claude, Cursor, Aider) | ~2,500 | 34 |
| `rf-integration-tests` | End-to-end integration tests | ~240 | 2 |

**Total: ~40,700 LOC | 740 tests | 0 clippy warnings**

## Data Flow

```
Client (rf CLI)
  │
  │ Noise XX handshake
  │ ↕ mutual authentication
  │
  ├── SecureChannel (E2E encrypted)
  │   │
  │   │ yamux multiplexed
  │   │
  │   ├── RPC stream (msgpack)
  │   │   ├── Request → Policy check → Execute → Audit → Response
  │   │   └── Streaming stdout/stderr
  │   │
  │   └── Control stream
  │       ├── Heartbeat
  │       └── Metrics
  │
  └── Transport (WebSocket / QUIC / Memory / ...)
      │
      └── Relay (opaque forwarding, never decrypts)
          │
          └── Agent (rf-agent)
              ├── Policy engine (final authority)
              ├── Executor (sandboxed)
              └── Audit log (append-only)
```

## Design Principles

1. **Security is non-negotiable** — No command executes without policy check
2. **Agent is final authority** — Orchestrator cannot override agent policy
3. **Zero trust** — Every connection mutually authenticated
4. **Audit everything** — Every action logged, no exceptions
5. **Network agnostic** — Any byte-moving channel is a valid transport
6. **Single binary** — No runtime dependencies
