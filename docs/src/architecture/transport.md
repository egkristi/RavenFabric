# Transport Layer

The transport layer provides network-agnostic connectivity. Any channel that can move bytes is a valid transport.

## Driver Trait

All transports implement the `Driver` trait:

```rust
#[async_trait]
pub trait Driver: Send + Sync {
    /// Establish a connection to a target.
    async fn dial(&self, target: &Target, config: &DriverConfig) -> Result<Box<dyn AsyncStream>, TransportError>;
    /// Listen for incoming connections.
    async fn listen(&self, config: &DriverConfig) -> Result<Box<dyn Listener>, TransportError>;
}
```

The `AsyncStream` trait combines `AsyncRead + AsyncWrite + Send + Unpin`, making every transport interchangeable.

## Built-in Transports

| Transport | Protocol | Use Case | Feature Flag |
|-----------|----------|----------|--------------|
| **WebSocket** | TCP/TLS | Relay connections, firewall traversal | default |
| **QUIC** | UDP | Low-latency, multiplexed, 0-RTT | `quic` |
| **WireGuard** | UDP | Direct peer-to-peer on open networks | `wireguard` |
| **Memory** | In-process | Testing (uses `tokio::io::duplex`) | default |

### WebSocket (Primary)

Default transport for relay connections. Works through firewalls, HTTP proxies, and CDNs.

```toml
[transport]
driver = "websocket"
```

### QUIC

UDP-based transport with built-in multiplexing and 0-RTT connection resumption.

```toml
[transport]
driver = "quic"
```

### WireGuard

Userspace WireGuard for direct peer-to-peer connections. Full `WgTunnel` with UDP socket, key handling, and peer management.

### Memory

In-process transport for testing. Uses `tokio::io::duplex` — no real network required.

## Local IPC Transports (Planned)

For same-host communication — AI agent access, MCP server connections, sidecar patterns — without touching the network stack. All local transports go through the same Noise XX handshake and policy engine as network transports. Local does not mean trusted.

| Transport | Platform | Use Case | Socket Path / Address |
|-----------|----------|----------|----------------------|
| **UNIX domain socket** | Linux, macOS, FreeBSD | Primary local IPC — AI agents, MCP server, sidecar processes | `/var/run/ravenfabric/local.sock` |
| **Abstract namespace socket** | Linux only | No filesystem cleanup, container-friendly | `@ravenfabric/<session-id>` |
| **Named pipe** | Windows | Windows-native local IPC equivalent | `\\.\pipe\ravenfabric` |
| **Stdio pipe** | All | Parent-child process communication (MCP stdio transport) | stdin/stdout FDs |
| **Vsock** | Linux (VM guests) | VM-to-hypervisor communication (firecracker, QEMU) | CID + port |

### Why local transports matter

The AI agent access use case (Claude Code, Cursor, Aider, MCP servers) requires same-host communication between the agent runtime and the RavenFabric policy engine. Network loopback (`127.0.0.1`) works but has drawbacks:

- **Port conflicts** — multiple agents or users competing for ports
- **No peer identity** — TCP cannot verify which process connected
- **Firewall interference** — host firewalls may block loopback in hardened environments
- **Overhead** — full TCP/IP stack for same-host communication

UNIX domain sockets solve all of these: filesystem permissions control access, `SO_PEERCRED` / `LOCAL_PEERCRED` verify the connecting process identity (UID/PID), no port allocation is needed, and kernel-level transfer avoids IP stack overhead.

### Automatic driver selection

When `rf exec local` is invoked, the CLI automatically selects the fastest available local transport:

```text
vsock (if in VM) > unix socket > named pipe (Windows) > loopback TCP
```

### Socket activation

On systemd (Linux) and launchd (macOS), the agent supports socket activation — the OS creates the socket and starts the agent on first connection. This means zero idle resource usage.

```ini
# systemd example: /etc/systemd/system/ravenfabric.socket
[Socket]
ListenStream=/var/run/ravenfabric/local.sock
SocketMode=0660
SocketGroup=ravenfabric

[Install]
WantedBy=sockets.target
```

## Censorship-Resistant Transports

Implemented codecs and framers for hostile network traversal:

| Transport | Implementation | Tests |
|-----------|---------------|-------|
| DNS tunneling | `DnsTunnelCodec` — base32 encoding, query fragmentation | 5 tests |
| ICMP tunneling | `IcmpTunnelFramer` — echo request framing, session mux | 3 tests |
| Domain fronting | `DomainFronter` — SNI/Host rewriting | 3 tests |
| Serial port | `SerialFramer` — sync bytes, CRC-16/CCITT | 5 tests |
| Protocol mimicry | `MimicryCodec` — ChaCha20-Poly1305 AEAD envelope | 4 tests |
| Traffic obfuscation | Padding/depadding layer | Functional |

## Overlay Networks (Planned)

Transport driver enum variants are defined for:

- Reticulum Network Stack
- Yggdrasil (self-configuring IPv6 mesh)
- I2P (garlic routing)
- Veilid (DHT-based, onion-routed)
- Tor hidden service

These are scaffolded as enum variants but do not yet implement protocol integration.

## Connection Management

The `ConnectionRunner` orchestrates:

- **Happy Eyeballs** (RFC 8305) — parallel connection attempts with staggered starts
- **Automatic reconnection** — exponential backoff with jitter
- **Multipath scheduling** — 5 algorithms (RoundRobin, LowestLatency, LatencyWeighted, Redundant, BandwidthWeighted)
- **Transport migration** — automatic path switching on tamper detection
- **Proxy detection** — HTTP CONNECT probing with auth detection
- **Interface migration** — auto-migrate on network change events (Wi-Fi → cellular)

## NAT Traversal

ICE-style connectivity establishment:

- **STUN client** — real UDP binding requests (RFC 5389/8489)
- **STUN server** — XOR-MAPPED-ADDRESS responses for self-hosted infrastructure
- **TURN relay** — UDP allocations with permissions and capacity limits
- **UDP/TCP hole punching** — probe/ACK protocol with concurrent punch
- **Birthday paradox port prediction** — deterministic PRNG candidate generation
- **NAT type detection** — comparison across multiple STUN servers

## Peer Discovery

Multiple discovery mechanisms for finding agents without central registries:

- **mDNS/DNS-SD** — LAN discovery via UDP broadcast
- **DHT (Kademlia-style)** — 256 k-buckets with XOR distance routing
- **Gossip (SWIM/HyParView)** — UDP health propagation
- **BLE beacon** — RSSI-based proximity discovery
- **Announce-flood** — gossip with dedup and rate limiting

## Censorship Escalation

The `CensorshipEscalation` state machine automatically escalates through transport tiers when interference is detected:

1. **Standard** — WebSocket/QUIC (normal connectivity)
2. **Obfuscated** — Traffic shaping, padding
3. **Domain-fronted** — CDN-routed connections
4. **Tunneled** — DNS/ICMP/serial encapsulation
5. **Physical** — Store-carry-forward, NNCP-style media transport

Tamper detection triggers immediate escalation. De-escalation is blocked after confirmed tampering.
