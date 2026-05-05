# Transport Layer

The transport layer provides network-agnostic connectivity. Any channel that can move bytes is a valid transport.

## Driver Trait

All transports implement the `Driver` trait:

```rust
#[async_trait]
pub trait Driver: Send + Sync {
    async fn connect(&self, addr: &str) -> Result<Connection>;
    async fn listen(&self, addr: &str) -> Result<Listener>;
}
```

## Built-in Transports

### WebSocket (Primary)

Default transport for relay connections. Works through firewalls, proxies, and CDNs.

```toml
[transport]
driver = "websocket"
```

### QUIC

UDP-based transport with built-in encryption. Faster connection establishment.

```toml
[transport]
driver = "quic"
```

### Memory

In-process transport for testing. Uses `tokio::io::duplex`.

### WireGuard

Userspace WireGuard for direct peer-to-peer connections on open networks.

## Exotic Transports

RavenFabric supports steganographic and physical transports for censorship resistance and air-gapped environments:

### Steganographic
- DNS tunneling
- ICMP tunneling
- Domain fronting (via CDN)
- MASQUE (HTTP/3 proxy)
- Protocol mimicry (looks like normal HTTPS/SSH)
- Tor hidden service
- Encrypted Client Hello (ECH)

### Physical
- Serial port (RS-232/USB)
- Bluetooth/BLE
- Wi-Fi Direct
- LoRa/Meshtastic (10+ km range)
- AX.25 packet radio
- Audio modem (data over sound)
- QR-stream (animated QR codes)
- Satellite (Iridium/Starlink)
- Physical media (USB/SD card, NNCP-style)

## Overlay Networks

- Reticulum Network Stack
- Yggdrasil (self-configuring IPv6 mesh)
- I2P (garlic routing)
- Veilid (DHT-based, onion-routed)
- Mixnet (Nym/Loopix)

## Connection Management

The `ConnectionManager` handles:
- Happy Eyeballs (parallel connection attempts)
- Automatic reconnection with exponential backoff
- Multipath scheduling (round-robin, latency-weighted)
- Transport migration on tampering detection
- Proxy detection and CONNECT tunneling

## NAT Traversal

ICE-style hole punching with:
- STUN binding discovery
- Candidate gathering (host, server-reflexive, relay)
- Birthday paradox port prediction for symmetric NAT
- TURN relay fallback
