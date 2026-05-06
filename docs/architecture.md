# Architecture

## Overview

RavenFabric is a Cargo workspace with 11 crates organized in a strict dependency hierarchy.
Each crate has a single responsibility and minimal coupling.

## Crate Map

```
┌─────────────────────────────────────────────────────────┐
│                      Binaries                           │
│  rf-agent          rf-relay          rf-cli            │
└────────┬──────────────┬──────────────┬─────────────────┘
         │              │              │
┌────────┴──────────────┴──────────────┴─────────────────┐
│                    Libraries                           │
│  rf-executor   rf-rpc   rf-policy   rf-bootstrap      │
└────────┬─────────┬────────┬──────────┬─────────────────┘
         │         │        │          │
┌────────┴─────────┴────────┴──────────┴─────────────────┐
│                   Foundation                           │
│  rf-crypto      rf-transport      rf-audit            │
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│                   Testing                              │
│  rf-integration-tests (end-to-end validation)         │
└────────────────────────────────────────────────────────┘
```

## Data Flow

### Command Execution (happy path)

```
Client (rf-cli)
  │
  ├── Noise XX handshake → mutual auth
  │
  ├── yamux stream open
  │
  ├── Request { Execute { "systemctl status nginx" } }
  │        │
  │  ┌─────┴──────────────────────────────────┐
  │  │ Agent (rf-agent)                       │
  │  │  1. Policy check (rf-policy)           │
  │  │  2. Audit log "allowed" (rf-audit)     │
  │  │  3. Execute with timeout (rf-executor) │
  │  │  4. Audit log result                   │
  │  │  5. Response { Success { stdout, ... }}│
  │  └────────────────────────────────────────┘
  │
  └── Display result
```

### Relay-mediated Connection

```
Agent ──WSS──→ Relay ←──WSS── Client
                 │
                 │ (meet protocol)
                 │ 1. Agent registers with ID
                 │ 2. Client connects with target ID
                 │ 3. Relay pairs streams (no decryption)
                 │ 4. End-to-end Noise XX through relay
                 │
         Relay NEVER sees plaintext
```

## Key Traits

| Trait | Crate | Purpose |
|-------|-------|---------|
| `Driver` | rf-transport | Pluggable transport backends |
| `AuditLogger` | rf-audit | Audit log destinations |
| `AsyncStream` | rf-transport | Combined AsyncRead + AsyncWrite |

## Concurrency Model

- **SecureChannel**: Split reader/writer behind independent Mutexes (concurrent send/recv)
- **Executor**: `Arc<RwLock<RpcPolicy>>` for hot-reloading policy without restart
- **Relay**: One task per connection pair, no shared mutable state
- **Agent**: Single tokio runtime, yamux multiplexer for concurrent RPC streams
