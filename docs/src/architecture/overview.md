# Architecture Overview

RavenFabric is organized as a layered architecture with strict dependency boundaries.

## Layers

```
┌──────────────────────────────────┐
│        Application Layer         │  rf-cli, rf-agent, rf-relay
│  (binaries, user-facing tools)   │
├──────────────────────────────────┤
│         Executor Layer           │  rf-executor
│  (command execution, streaming)  │
├──────────────────────────────────┤
│          Policy Layer            │  rf-policy
│  (deny-by-default enforcement)  │
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
| `rf-crypto` | Noise XX handshake, SecureChannel, key management, post-quantum KEM | ~1,300 | 25 |
| `rf-transport` | Driver trait, WebSocket/QUIC/Memory, NAT, mesh, WireGuard, overlays | ~5,300 | 121 |
| `rf-rpc` | Message types, msgpack codec, yamux mux, DTN, routing, controller | ~2,900 | 61 |
| `rf-audit` | Structured JSON-lines audit logging | 53 | — |
| `rf-policy` | Policy enforcement, RBAC, capabilities, distributed policy | ~1,500 | 31 |
| `rf-executor` | Command execution, streaming, orchestration, PTY, plugins | ~3,600 | 48 |
| `rf-bootstrap` | OTP enrollment, TrustStore | ~380 | 11 |

**Total: ~16,700 LOC | 336 tests | 0 clippy warnings**

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
