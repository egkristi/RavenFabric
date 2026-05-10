# Transport Showcase Demo

Demonstrates RavenFabric's transport diversity — the same encrypted protocol running over fundamentally different byte channels. Every transport uses identical Noise XX mutual authentication and ChaCha20-Poly1305 encryption; only the underlying byte transport changes.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Noise XX + ChaCha20-Poly1305                │
│                     (identical on every transport)              │
├──────────┬──────────┬──────────┬──────────┬─────────────────────┤
│ WebSocket│   QUIC   │  UNIX    │  Stdio   │   Memory            │
│  (TCP)   │  (UDP)   │  Socket  │  Pipe    │   (in-process)      │
│  :9090   │  :9443   │ /tmp/... │ stdin/   │   tokio::io::duplex │
│          │          │          │  stdout  │                     │
└──────────┴──────────┴──────────┴──────────┴─────────────────────┘
```

All traffic is end-to-end encrypted. The transport is just a byte pipe — authentication and encryption happen above it. If you can move bytes, RavenFabric can run over it.

## Prerequisites

- Rust toolchain (1.88+)
- Docker (for the containerized demo)
- The `rf` CLI binary (`cargo build --release -p rf-cli`)

## Scenarios

| # | Scenario | Script | What It Shows |
|---|----------|--------|---------------|
| 01 | WebSocket (TCP) | `scenarios/01-websocket.sh` | Default relay transport — relay + agent + CLI over WebSocket |
| 02 | QUIC (UDP) | `scenarios/02-quic.sh` | UDP-based transport with built-in multiplexing and 0-RTT |
| 03 | UNIX Socket | `scenarios/03-unix-socket.sh` | Same-host IPC — zero network, filesystem socket |
| 04 | Stdio Pipe | `scenarios/04-stdio-pipe.sh` | Parent spawns agent as child process, communicates over stdin/stdout |
| 05 | Memory (In-Process) | `scenarios/05-memory.sh` | In-process duplex — both sides in same Tokio runtime |
| 06 | All Transports | `scenarios/06-all-transports.sh` | Runs all 5 transports sequentially and compares results |

## Quick Start

```bash
cd demos/transport-showcase

# Run all transport scenarios
./scenarios/06-all-transports.sh

# Or run individual transports
./scenarios/01-websocket.sh
./scenarios/03-unix-socket.sh
```

## How It Works

Each scenario uses a small Rust integration test binary (`transport-showcase-test`) that:

1. **Creates a transport** — WebSocket listener, QUIC endpoint, UNIX socket, stdio pipe, or memory channel
2. **Performs Noise XX handshake** — both sides prove identity via Curve25519 static keys
3. **Establishes SecureChannel** — encrypted frame transport with 16-byte MAC on every frame
4. **Sends an RPC request** — msgpack-encoded command execution request
5. **Receives encrypted response** — decrypted and verified at the other end
6. **Verifies** — identical behavior regardless of transport

The point: the transport layer is completely interchangeable. The same command, the same encryption, the same policy enforcement — only the bytes-on-wire layer changes.

## Transport Comparison

| Transport | Protocol | Latency | Use Case |
|-----------|----------|---------|----------|
| **WebSocket** | TCP | ~1 ms (LAN) | Default. Works through proxies, firewalls, CDNs |
| **QUIC** | UDP | ~0.5 ms (LAN) | Multiplexed streams, 0-RTT reconnect, mobile-friendly |
| **UNIX Socket** | IPC | ~0.1 ms | Same-host sidecar, container-to-container |
| **Stdio Pipe** | Process | ~0.05 ms | MCP server, embedded agent, subprocess isolation |
| **Memory** | In-process | ~0.01 ms | Testing, `rf dev` mode, embedded scenarios |

## Beyond This Demo

RavenFabric also supports transports that require specialized hardware or network infrastructure:

| Transport | Requires | Use Case |
|-----------|----------|----------|
| LoRa (Meshtastic) | LoRa radio + serial/TCP | Off-grid mesh, disaster response |
| BLE | Bluetooth adapter | Proximity management, IoT |
| AX.25 | Amateur radio TNC | Emergency comms, HF/VHF |
| Satellite | Iridium/Starlink modem | Maritime, remote sites |
| Tor | Tor daemon | Censorship-resistant access |
| I2P | I2P router | Anonymous overlay |
| WireGuard | WG kernel module | VPN tunnel encapsulation |
| Vsock | VM hypervisor | Firecracker/QEMU guest ↔ host |
| QR Stream | Camera + display | Air-gap bridging |
| Audio Modem | Microphone + speaker | Extreme last-resort channel |

These all use the same Noise XX handshake and ChaCha20-Poly1305 encryption. The transport is just a pipe.
