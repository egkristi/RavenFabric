# Mesh VPN & Delay-Tolerant Networking

RavenFabric includes a built-in mesh VPN and a delay-tolerant networking (DTN) layer for nodes that are intermittently connected, air-gapped, or reachable only via exotic transports.

## Mesh VPN

### What It Provides

- Direct peer-to-peer encrypted tunnels between agents without a central relay
- MagicDNS: automatic `<agent-id>.raven` hostname resolution across the mesh
- NAT traversal via STUN hole-punching
- Automatic fallback to relay-forwarded traffic when direct paths are unavailable

### Enabling the Mesh

```toml
# raven.toml
[mesh]
enabled = true
listen = "0.0.0.0:51820"          # WireGuard-compatible UDP port

[mesh.dns]
enabled = true
domain = "raven"                   # agents resolve as <id>.raven
```

Once enabled, agents discover each other through the relay's coordination channel and establish direct tunnels. No configuration is needed per agent pair.

### MagicDNS

With MagicDNS enabled, every enrolled agent is reachable by hostname:

```bash
# From any agent or connected client:
ping web-01.raven
ssh deploy@db-01.raven
curl https://api-gw.raven/status
```

The mesh DNS resolver is injected into the system resolver on Linux (`/etc/resolv.conf` or `systemd-resolved`) and macOS (DNS suffix).

### Mesh Routing

The mesh selects the best available path for each agent pair:

| Priority | Path |
|----------|------|
| 1 | Direct WireGuard tunnel (lowest latency) |
| 2 | QUIC direct |
| 3 | Relay-forwarded (encrypted, relay is content-blind) |
| 4 | DTN store-carry-forward (for offline nodes) |

Path selection is automatic and transparent. Failover occurs within one keep-alive interval (default: 25 seconds).

---

## Transport Diversity

RavenFabric supports 30+ transport backends. The agent probes available transports in parallel and selects the best path:

| Category | Transports |
|----------|-----------|
| Standard | WebSocket, QUIC, TCP |
| Overlay | WireGuard, Tor/onion, MASQUE, ECH |
| Radio | LoRa, BLE, AX.25 (amateur radio), acoustic modem |
| Satellite | Iridium, Starlink serial, VSAT serial |
| Direct | Serial port, USB serial, UNIX socket, stdio pipe |
| Mesh | Reticulum, delay-tolerant (DTN) |

Configure the probe order and strategy:

```toml
[transport]
strategy = "race"               # race | ordered | failover
probe_interval_seconds = 30
drivers = [
  "wireguard",
  "quic",
  "websocket",
  "tor",
  "serial:/dev/ttyUSB0",
]
```

### Tamper Detection and Path Migration

If the agent detects interference or tampering on the active transport (unexpected RST, decryption failures, abnormal latency), it automatically migrates to an alternative path without dropping the session:

```
[transport] wireguard-direct: decryption error — abandoning path
[transport] quic-direct: connecting...
[transport] quic-direct: session resumed (0 RPC frames lost)
```

This is a security property: a compromised transport cannot disrupt operations.

---

## Delay-Tolerant Networking (DTN)

DTN is designed for nodes that are offline for extended periods — remote sensors, field equipment, mobile assets — and sync opportunistically when connectivity becomes available.

### How DTN Works

```
Controller                    DTN Network               Agent
     │                            │                        │
     │── enqueue operation ──────►│                        │
     │   (job_id, payload)        │  (offline)             │
     │                            │  ...time passes...     │
     │                            │  (connectivity)        │
     │                            │──── deliver ──────────►│
     │                            │                        │── execute ──
     │◄─── result ────────────────┼────────────────────────│
```

Operations, files, and policy updates are queued in the DTN layer. When the agent comes online (via any available transport), pending operations are delivered in order.

### Enabling DTN

```toml
[agent.dtn]
enabled = true
store_path = "/var/lib/ravenfabric/dtn-store"
max_bundle_size_mb = 50
custody_transfer = true         # relay holds bundles until confirmed received
ttl_hours = 168                 # discard bundles older than 7 days
```

### Custody Transfer

With `custody_transfer = true`, the relay (or an intermediate DTN node) takes responsibility for delivering a bundle. If a bundle delivery fails, the custody node retries. The controller receives a custody acknowledgement when a node accepts responsibility.

### DTN-Aware Commands

Commands sent to offline agents are queued automatically when DTN is enabled:

```bash
# This command will be queued if the agent is offline
rf exec --token <TOKEN> --dtn "apt-get update && apt-get upgrade -y"
```

Output when agent is offline:
```
Agent field-sensor-07 is currently unreachable.
DTN queue accepted: job_id=c4d5e6f7
Bundle will be delivered when connectivity is available.
```

Check queue status:

```bash
rf dtn status --token <TOKEN>
```

### Store-and-Forward via Radio

For field deployments using LoRa or AX.25, the agent automatically uses the serial/radio transport for DTN delivery. No configuration change is needed — the transport layer handles routing.

---

## Air-Gap Operation

For systems with no persistent network connection, RavenFabric supports manual "sneakernet" operation:

```bash
# On the connected machine: export a bundle
rf export --token <TOKEN> --output bundle.rvnf "apt-get update"

# Transfer bundle.rvnf to the air-gapped machine (USB, floppy, etc.)

# On the air-gapped machine: import and execute
rf-agent --import bundle.rvnf
# Executes the queued command; result written to response.rvnf

# Transfer response.rvnf back; import result on connected machine
rf import-result response.rvnf
```

Bundles are signed and encrypted — tampering with a bundle file is detected on import.

---

## See Also

- [Architecture: Transport Layer](../architecture/transport.md) — Technical transport internals
- [Use Cases: Air-Gapped Industrial Systems](../use-cases/airgapped-ics.md) — DTN and air-gap patterns
- [Use Cases: Maritime & Offshore](../use-cases/maritime-offshore.md) — Satellite and radio transports
- [Configuration Reference](../reference/config.md) — Full mesh and DTN config options
