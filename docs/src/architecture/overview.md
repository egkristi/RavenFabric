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
| `rf-crypto` | Noise XX handshake, SecureChannel, key management, PQ hybrid KEM | ~1,800 | 42 |
| `rf-transport` | Driver trait, WebSocket/QUIC/Memory/WireGuard/Vsock/Unix/Stdio, NAT traversal, mesh, auto-selection, LoRa, BLE, AX.25, satellite, mixnet, MASQUE, ECH | ~21,900 | 551 |
| `rf-rpc` | Message types, msgpack codec, yamux mux, DTN, routing, controller API | ~5,800 | 114 |
| `rf-audit` | Structured JSON-lines audit logging, anomaly event integration | ~650 | 14 |
| `rf-policy` | Policy enforcement, RBAC, capabilities, CRDT convergence, anomaly detection, injection detection | ~4,500 | 97 |
| `rf-executor` | Command execution, streaming, orchestration, PTY, plugins, desired-state convergence | ~9,900 | 165 |
| `rf-bootstrap` | OTP enrollment, TrustStore, relay pairing | ~430 | 11 |
| `rf-relay` | Stateless encrypted relay broker with per-IP rate limiting | ~400 | 7 |
| `rf-agent` | Agent binary (connects to relay, executes RPC, reconnect with backoff) | ~380 | — |
| `rf-cli` | CLI client `rf` (exec, dev, status, shell, forward, playbook, policy, completions) | ~1,200 | — |
| `rf-mcp-server` | MCP server for AI agent integration (Claude, Cursor, Aider) | ~3,300 | 50 |
| `rf-mcp-client` | MCP client SDK (Rust library for building MCP-aware applications) | ~720 | 14 |
| `rf-integration-tests` | End-to-end integration tests | ~1,700 | 28 |

**Total: ~52,800 LOC | 1,093 tests | 0 clippy warnings**

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
