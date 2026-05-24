# RavenFabric — Connectivity Value Chain

Complete reference for the end-to-end connectivity lifecycle: from identity genesis
to encrypted, policy-validated data flowing between two nodes. Each phase has
multiple implementation alternatives, ordered by priority.

---

## Value Chain Overview

```text
┌──────────────────────────────────────────────────────────────────┐
│  Phase 0: IDENTITY GENESIS                                       │
│  Who is the node? How do we know it's authentic?                 │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 1: ENROLLMENT / BOOTSTRAP                                 │
│  How does a new node become known to the fabric?                 │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 2: DISCOVERY                                              │
│  How do two nodes find each other?                               │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 3: RENDEZVOUS                                             │
│  How do they meet? Where do they exchange endpoint information?  │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 4: NAT / REACHABILITY ASSESSMENT                          │
│  What network are we in? What traversal is needed?               │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 5: PATH SELECTION                                         │
│  Which transports are available? Which should be attempted?      │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 6: NAT TRAVERSAL / TUNNEL ESTABLISHMENT                   │
│  Actually opening a packet-carrying channel between A and B      │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 7: BROKER / RELAY DECISION                                │
│  Direct? Via broker? Hybrid? How do we upgrade later?            │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 8: CRYPTOGRAPHIC HANDSHAKE                                │
│  Mutual auth + key establishment (transport-independent)         │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 9: SESSION ESTABLISHMENT                                  │
│  Multiplexing, flow control, stream allocation                   │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 10: PATH UPGRADE / MIGRATION                              │
│  Switch transport mid-session without losing data                │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 11: HEALTH MONITORING & FAILOVER                          │
│  Continuous probing, automatic fallback                          │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Phase 12: GRACEFUL TEARDOWN                                     │
│  Draining, audit-flush, key rotation                             │
└──────────────────────────────────────────────────────────────────┘
```

---

## Phase 0 — Identity Genesis

Identity is the foundation. Everything else builds on this.

### Implementation Alternatives

| Method | Description | Use in RavenFabric |
|--------|-------------|-------------------|
| **Curve25519 keypair** | Locally generated, 32-byte private/public | **Primary** — basis for Noise XX |
| **Ed25519 signing key** | Signing separate from DH key | Audit logger, capability tokens |
| **Hybrid PQ keypair** | X25519 + ML-KEM-768 (Kyber) | v0.6+ — harvest-now-decrypt-later resistance |
| **TPM-bound key** | Private key cannot be extracted from hardware | Enterprise mode, attestation |
| **YubiKey/PKCS#11** | Smart-card-bound identity | Operator identities, not agents |
| **SPIFFE SVID** | Workload identity independent of host | Kubernetes mode, SPIRE integration |
| **Content-addressed identity** | ID = hash(public_key) | Reticulum-style — address = key |
| **Hierarchically derived** | HKDF from master-secret per agent | Fleet deployment, central key hierarchy |

### Identity Forms

```text
┌─────────────────────────────────────────────────────────────────┐
│  Static Identity (long-lived)                                   │
│  ├── Agent identity key   (Curve25519, 5+ year lifetime)        │
│  ├── Audit signing key    (Ed25519, separate for accountability)│
│  └── Recovery key         (offline, disaster recovery only)     │
├─────────────────────────────────────────────────────────────────┤
│  Session Identity (short-lived)                                 │
│  ├── Ephemeral DH key     (Curve25519, per session)             │
│  └── PQ-KEM ephemeral     (ML-KEM, per session, hybrid)        │
├─────────────────────────────────────────────────────────────────┤
│  Capability Tokens (per-action)                                 │
│  ├── Biscuit tokens       (contextualized permissions)          │
│  └── Time-bound caveats   (expiry, scope, attenuation)         │
└─────────────────────────────────────────────────────────────────┘
```

### Filesystem Layout

```text
/etc/ravenfabric/
├── identity/
│   ├── agent.key         (0600, mlock'd, zeroed-on-drop)
│   ├── agent.pub         (0644)
│   ├── audit.key         (0600)
│   └── recovery.key.gpg  (offline-encrypted backup)
├── known_peers/
│   └── <agent-id>.pub    (TOFU-cache of peers)
└── policy/
    └── ...
```

---

## Phase 1 — Enrollment / Bootstrap

How a completely new node joins the fabric.

### Implementation Alternatives

| Method | Security | Use case |
|--------|----------|----------|
| **OTP / one-time token** | High (TTL, single-use, hash-stored) | **Primary** — admin-issued |
| **Pre-shared key (PSK)** | Medium | Lab/dev only, not production |
| **Cloud-init / metadata service** | Medium-high | AWS/Azure/GCP instance identity |
| **Kubernetes ServiceAccount JWT** | High | K8s deployments, projected token |
| **Hardware attestation (TPM/SEV/TDX)** | Very high | Confidential computing, regulated |
| **Cloud provider IID** | High | AWS Instance Identity Document, Azure IMDS |
| **QR-code + manual** | High (out-of-band) | Edge devices, physical access |
| **NFC tap-to-enroll** | High | IoT, physical proximity |
| **OAuth Device Flow** | High | Operator bootstrap (not agent) |
| **Sneakernet enrollment** | Maximum (offline) | Air-gapped via USB/QR |

### Bootstrap Flow

```text
┌──────────┐                    ┌──────────┐                    ┌──────────┐
│  Admin   │                    │  Broker  │                    │  Agent   │
└────┬─────┘                    └────┬─────┘                    └────┬─────┘
     │                               │                               │
     │  1. generate OTP              │                               │
     │ ─────────────────────────────►│                               │
     │       hash(otp), TTL, scope   │                               │
     │                               │                               │
     │  2. deliver token (out-of-band)                               │
     │ ──────────────────────────────────────────────────────────────►│
     │       SSH / cloud-init / QR / NFC                             │
     │                               │                               │
     │                               │   3. generate keypair locally │
     │                               │       (private NEVER leaves)  │
     │                               │                               │
     │                               │   4. POST /bootstrap          │
     │                               │ ◄─────────────────────────────│
     │                               │       {token, agent_id, pubkey}│
     │                               │                               │
     │                               │   5. validate + register      │
     │                               │       mark token used         │
     │                               │                               │
     │                               │   6. return relay endpoints + │
     │                               │      controller pubkey        │
     │                               │ ─────────────────────────────►│
     │                               │                               │
     │                               │   7. all future: Noise XX     │
     │                               │       (bootstrap path closed) │
```

---

## Phase 2 — Discovery

When a node needs to communicate, how does it find out who's out there?

### Implementation Alternatives

| Method | Scales to | Censorship resistance | Use |
|--------|-----------|----------------------|-----|
| **Central broker directory** | 100k+ nodes | Low | Standard enterprise |
| **DNS SRV records** | 10k+ | Low-medium | If DNS control exists |
| **mDNS / DNS-SD** | LAN scope | N/A (local) | Same-subnet bootstrap |
| **DHT (Kademlia)** | Global | High | P2P mode, BitTorrent-style |
| **Gossip (SWIM/HyParView)** | 10k+ | High | Self-organizing fleet |
| **Consul/etcd integration** | 10k+ | Low | Existing infrastructure |
| **Kubernetes API** | Cluster scope | Low | K8s-native deployment |
| **Reticulum announce-flood** | Mesh scope | High | Air-gap, LoRa mesh |
| **Yggdrasil DHT** | Global | Medium | IPv6 overlay |
| **Bluetooth LE advertise** | Proximity | N/A (local) | Mobile/IoT |
| **Static config file** | N/A | N/A | Air-gap, small deployments |
| **Verifiable signed records** | 100k+ | High | DNSSEC-style signed endpoints |

### Hybrid Discovery (recommended)

```text
┌─────────────────────────────────────────────────────────────────┐
│  PRIORITY 1: Cached known peers                                 │
│              (fastest, no round-trip)                            │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 2: mDNS/DNS-SD (LAN scope)                           │
│              (low latency, no broker required)                  │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 3: Broker directory query                             │
│              (authoritative, requires internet)                 │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 4: Gossip from connected peers                        │
│              (eventual consistency, robust)                     │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 5: DHT lookup                                         │
│              (censorship-resistant, slow bootstrap)             │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 6: Reticulum announce / mesh broadcast                │
│              (offline, slow, last resort)                       │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase 3 — Rendezvous

Two nodes know each other exists. Now they must exchange enough information
to attempt a connection — candidate endpoints, transport preferences, etc.

### Implementation Alternatives

| Method | Latency | Privacy | Use |
|--------|---------|---------|-----|
| **Broker as rendezvous point** | Low | Low (broker sees metadata) | **Primary** for fabric mode |
| **STUN-style server-reflexive discovery** | Low | Medium | Classic WebRTC pattern |
| **DHT-stored endpoint records** | Medium | High | Signed records in Kademlia |
| **Out-of-band exchange (QR/NFC)** | N/A (manual) | Maximum | Initial pairing |
| **libp2p Circuit Relay v2 reservation** | Low | Medium | DCUtR pattern |
| **Email-based rendezvous** | High | High | NNCP-style offline rendezvous |

### Rendezvous Payload

```yaml
# Exchanged at rendezvous, signed with agent's identity key:
peer_id: "agent-prod-1"
public_key: "32-bytes-hex"
endpoints:
  - transport: wireguard
    address: "203.0.113.42:51820"
    priority: 1
  - transport: wireguard
    address: "[2001:db8::42]:51820"
    priority: 1
  - transport: quic
    address: "agent-prod-1.relay.example.com:443"
    priority: 2
  - transport: websocket
    address: "wss://relay.example.com/agent-prod-1"
    priority: 3
  - transport: reticulum
    destination_hash: "abc123..."
    priority: 99
capabilities:
  - "exec.shell"
  - "fs.read"
  - "metrics.collect"
nat_type: "port-restricted"
issued_at: 1714824000
expires_at: 1714827600
signature: "ed25519-signature-bytes"
```

---

## Phase 4 — NAT / Reachability Assessment

Before choosing transport: what kind of network are we *in*?

### Probes

| Probe | What it reveals | Cost |
|-------|----------------|------|
| **STUN binding request** | Public IP, port mapping behavior | Low |
| **STUN behavior tests (RFC 5780)** | NAT type (full cone, restricted, symmetric) | Low |
| **UPnP / NAT-PMP / PCP query** | Port forwarding possible? | Low |
| **IPv6 reachability test** | Direct IPv6 availability | Low |
| **Captive portal detection** | Hotel/airport WiFi? | Low |
| **HTTP/HTTPS reachability** | At least port 443 works? | Low |
| **DNS-over-HTTPS test** | DNS traffic filtered? | Low |
| **TCP fingerprint analysis** | DPI/proxy in the way? | Medium |
| **SOCKS/HTTP proxy detection** | Corporate proxy configured? | Low |
| **Per-relay latency probe** | Which relay is closest? | Medium |
| **Bandwidth estimation** | Practical throughput? | High |

### Network Environment Classification

```rust
struct NetworkProbe {
    nat_type: NatType,                     // Open, FullCone, Restricted, PortRestricted, Symmetric
    ipv4_available: bool,
    ipv6_available: bool,
    udp_blocked: bool,
    tcp_443_open: bool,
    udp_443_open: bool,
    captive_portal: Option<CaptivePortal>,
    relay_latencies: HashMap<RelayId, Duration>,
    has_corporate_proxy: bool,
    proxy_endpoint: Option<ProxyConfig>,
    dns_filtered: bool,
    ipv6_only: bool,
    nat64_present: bool,
    available_drivers: Vec<DriverId>,
    egress_class: EgressClass,
}

enum EgressClass {
    Open,              // All transports possible
    HomeRouter,        // Most transports work, NAT in the way
    EnterpriseProxy,   // Only HTTP/HTTPS through proxy
    RestrictiveDpi,    // Must disguise as HTTPS (MASQUE/domain fronting)
    Hostile,           // Must actively camouflage (obfs4, Shadowsocks)
    AirGap,            // No IP network, physical/radio only
    Sneakernet,        // Only USB/QR/manual transport
}
```

---

## Phase 5 — Path Selection

With known network environment: which transports to try, in what order?

### Transport Catalog

#### Tier 1: Direct Connectivity (lowest latency)

| Transport | NAT requirement | Notes |
|-----------|----------------|-------|
| **WireGuard direct (IPv6)** | None (often no NAT) | Preferred if available |
| **WireGuard direct (IPv4)** | Open network | Lowest latency |
| **QUIC direct** | Open network | Connection migration, 0-RTT |
| **TCP direct (Noise-over-TCP)** | Open network | Fallback when UDP blocked |

#### Tier 2: NAT Traversal (requires coordination)

| Transport | Method | Success rate |
|-----------|--------|--------------|
| **WireGuard + STUN** | Server-reflexive endpoint | ~70% |
| **WireGuard + UDP hole punch** | Symmetric coordination via broker | ~85% |
| **QUIC + ICE** | Full ICE framework | ~95% |
| **DCUtR pattern** | Direct Connection Upgrade through Relay | ~90% |
| **TCP simultaneous open** | RFC 5128 hole punching | ~60% |
| **Birthday paradox port prediction** | For symmetric NAT | ~40% |

#### Tier 3: Brokered Transports (always works if broker reachable)

| Transport | Bandwidth | Latency | Notes |
|-----------|-----------|---------|-------|
| **WebSocket via broker (port 443)** | High | Medium | Works everywhere |
| **QUIC via broker** | High | Low | With 0-RTT resumption |
| **HTTP/3 + MASQUE** | High | Low | Indistinguishable from HTTPS |
| **TCP relay (TURN-style)** | Medium | Medium | Classic fallback |

#### Tier 4: Overlay Networks

| Transport | Censorship resistance | Notes |
|-----------|----------------------|-------|
| **Yggdrasil overlay** | Medium | IPv6 mesh, self-routing, key-derived addresses |
| **Tor hidden service (.onion)** | High | Anonymity included |
| **I2P** | High | Garlic routing, internal focus |
| **Veilid** | High | DHT-based, onion-routed by default |

#### Tier 5: Censorship-Resistant / Hostile Networks

| Transport | What it conceals |
|-----------|-----------------|
| **HTTP/3 MASQUE (CONNECT-UDP)** | All traffic inside HTTPS |
| **ECH (Encrypted Client Hello)** | SNI information |
| **Domain fronting (CDN)** | Actual destination |
| **Traffic obfuscation (obfs4-style)** | Protocol fingerprint |
| **Shadowsocks / Trojan-GFW** | Looks like standard HTTPS |

#### Tier 6: Out-of-Band (offline / air-gap)

| Transport | Bandwidth | Latency | Use |
|-----------|-----------|---------|-----|
| **Reticulum (TCP backbone)** | Low | Medium | Mesh over anything |
| **Reticulum (LoRa)** | < 11 kbps | Seconds–minutes | Long range, low power |
| **Reticulum (packet radio AX.25)** | Low | Seconds | HF/VHF/UHF, global |
| **Serial (RS-232/USB)** | Variable | Low | Direct cable, true air-gap |
| **NNCP (sneakernet)** | Variable | Hours–days | USB stick, physical mail |
| **DNS tunnel (DoH/DoT)** | Very low | High | Last resort |
| **ICMP tunnel** | Low | Medium | Works when only ping allowed |
| **HF radio (Winlink-style)** | Very low | Minutes | Global, no commercial infra |
| **Satellite (Iridium/Starlink)** | Low–high | ms–seconds | Global coverage |
| **Audio modem** | Very low | N/A | Between devices with mic/speaker |
| **QR-stream (visual)** | Low | N/A | Air-gap import/export |

### Path Selection Strategies

| Strategy | Behavior | Best for |
|----------|----------|----------|
| **Sequential** | Try one at a time in priority order | Battery-saving, mobile |
| **Race (Happy Eyeballs)** | Start all in parallel, use first responder | Latency-critical |
| **Parallel** | Establish ALL simultaneously, keep active | Mission-critical, redundancy |
| **Tiered race** | Race tier 1; if all fail, race tier 2 | Balanced performance |
| **Policy-driven** | Policy determines allowed transports per command | Security — sensitive ops via WG only |
| **Adaptive** | Heuristics based on historical success | Mobile nodes, varying networks |

---

## Phase 6 — NAT Traversal / Tunnel Establishment

Actually opening a packet-carrying channel.

### Techniques

| Technique | NAT types it works for | Notes |
|-----------|----------------------|-------|
| **Direct connect** | Open / IPv6 | No traversal needed |
| **STUN binding (RFC 8489)** | Cone NAT | Server-reflexive endpoint |
| **UDP hole punching** | Restricted, port-restricted | Requires simultaneous signaling |
| **TCP hole punching (RFC 5128)** | Most | Harder than UDP |
| **UPnP / NAT-PMP / PCP** | Home routers | Often disabled in enterprise |
| **Birthday paradox attack** | Symmetric | Statistical port guessing |
| **TURN relay** | All | 100% success, but relay in path |
| **DCUtR (libp2p pattern)** | Most | Start relay, upgrade to direct |
| **ICE (RFC 8445)** | All | Complete framework, tries all methods |
| **MASQUE CONNECT-UDP** | All | UDP traversal inside HTTPS |

### UDP Hole Punching Sequence

```text
┌──────────┐                ┌──────────┐                ┌──────────┐
│  Peer A  │                │  Broker  │                │  Peer B  │
│ (NAT)    │                │ (public) │                │ (NAT)    │
└────┬─────┘                └────┬─────┘                └────┬─────┘
     │                           │                           │
     │  1. STUN binding          │                           │
     │ ─────────────────────────►│                           │
     │  ◄────────────────────────│                           │
     │  Public: 198.51.100.1:443 │                           │
     │                           │                           │
     │                           │  2. STUN binding          │
     │                           │ ◄─────────────────────────│
     │                           │ ─────────────────────────►│
     │                           │  Public: 203.0.113.7:31443│
     │                           │                           │
     │  3. Request to talk to B  │                           │
     │ ─────────────────────────►│                           │
     │                           │  4. Exchange endpoints    │
     │  ◄────────────────────────│──────────────────────────►│
     │                           │                           │
     │  5. SIMULTANEOUS PUNCH                                │
     │ ──────────────────────────┼──────────────────────────►│
     │ ◄─────────────────────────┼───────────────────────────│
     │                           │                           │
     │  6. Direct path established                           │
     │ ◄────────────────────────────────────────────────────►│
```

---

## Phase 7 — Broker / Relay Decision

When and how the broker (rf-relay) is used.

### Broker Roles

```text
┌─────────────────────────────────────────────────────────────────┐
│  RavenFabric Broker (rf-relay)                                  │
│                                                                 │
│  Roles:                                                         │
│  ├── Discovery directory     (who exists)                       │
│  ├── Rendezvous facilitator  (exchange endpoints)               │
│  ├── Hole-punch coordinator  (synchronize punch packets)        │
│  ├── Fallback data relay     (forward ciphertext when direct    │
│  │                            fails)                            │
│  ├── Session metadata logger (metadata only, not content)       │
│  └── Health reporter         (uptime, latency, capacity)        │
│                                                                 │
│  NEVER roles:                                                   │
│  ├── Decryption (has no keys)                                   │
│  ├── Policy evaluation (agent's responsibility)                 │
│  ├── Identity issuer (only validates OTP)                       │
│  └── Audit storage (audit stays on agent)                       │
└─────────────────────────────────────────────────────────────────┘
```

### Connection Modes

| Mode | Description | When |
|------|-------------|------|
| **Broker-assisted, direct data** | Broker helps with rendezvous, then out of the way | Preferred — lowest latency |
| **Broker-relayed** | All data through broker (encrypted, opaque) | When direct fails |
| **Hybrid (relay → upgrade)** | Start relay, upgrade to direct in background | Default for first connection |
| **Brokerless** | Only local/cached endpoint info, no broker | Air-gap, mDNS, pre-configured |
| **Multi-broker (anycast)** | Client uses geographically nearest broker | Geo-distributed deployments |

### Broker Threat Model

```text
┌─────────────────────────────────────────────────────────────────┐
│  ASSUMPTION: BROKER IS POTENTIALLY COMPROMISED                  │
│                                                                 │
│  Broker MUST NOT be able to:                                    │
│  ✗  Read command content                                        │
│  ✗  Read file content                                           │
│  ✗  Read audit events                                           │
│  ✗  Modify messages without detection                           │
│  ✗  Impersonate either party                                    │
│  ✗  Decrypt after the fact (forward secrecy)                    │
│                                                                 │
│  Broker CAN see:                                                │
│  •  Which peer IDs are communicating (metadata)                 │
│  •  Timing and volume (traffic analysis)                        │
│  •  IP addresses of endpoints                                   │
│                                                                 │
│  Mitigations for metadata leakage:                              │
│  •  Padding to fixed frame sizes                                │
│  •  Cover traffic / dummy packets (high-paranoia mode)          │
│  •  Mixnet routing for control plane (v0.5+)                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Phase 8 — Cryptographic Handshake

Mutual authentication + key establishment, completely independent of transport.

### Protocol Options

| Protocol | Properties | Use in RavenFabric |
|----------|-----------|-------------------|
| **Noise XX** | Mutual auth, forward secrecy, no PKI | **Primary (v0.1)** |
| **Noise IK** | Initiator knows responder pubkey | Resumption / known peers |
| **Noise NK** | Responder anonymous | Client → broker without client cert |
| **Noise XX + ML-KEM hybrid** | Post-quantum | **v0.6+** |
| **WireGuard (Noise IK variant)** | Built into WG protocol | For WireGuard driver |
| **PQXDH (Signal)** | Hybrid PQ for async messaging | Async/store-and-forward |
| **MLS (RFC 9420)** | Group key agreement | Multi-party sessions |

### Noise XX Pattern (primary)

```text
Pattern: Noise_XX_25519_ChaChaPoly_BLAKE2s

  -> e                        (initiator ephemeral)
  <- e, ee, s, es            (responder authenticates)
  -> s, se                   (initiator authenticates)

Properties:
  ✓ Mutual authentication
  ✓ Forward secrecy (ephemeral keys)
  ✓ Identity hiding (initiator's static key encrypted)
  ✓ Replay resistance
  ✓ KCI resistance (key compromise impersonation)
  ✓ 1.5 RTT setup
```

### Hybrid Post-Quantum (v0.6 design)

```text
Pattern: Noise_XXhfs_25519+ML-KEM-768_ChaChaPoly_BLAKE2s

  -> e, e1                        (e = X25519, e1 = ML-KEM encaps)
  <- e, ee, ekem1, s, es
  -> s, se

Forward secrecy: protected by both classical AND PQ-KEM.
Harvest-now-decrypt-later: defeated.
```

---

## Phase 9 — Session Establishment

After handshake: how the channel is structured.

### Multiplexing Options

| Protocol | Properties | Use |
|----------|-----------|-----|
| **yamux** | Battle-tested (libp2p), per-stream flow control | **Primary (v0.1)** |
| **QUIC streams** | Native when transport = QUIC | QUIC driver |
| **Custom length-delimited** | Simplest possible | Air-gap, low-bandwidth |

### Stream Allocation

```text
┌─────────────────────────────────────────────────────────────────┐
│  One Noise XX session carries many yamux streams:               │
│                                                                 │
│  Stream 0:  Control plane (heartbeat, capability negotiation)   │
│  Stream 1:  RPC requests/responses                              │
│  Stream 2:  Bulk file transfer                                  │
│  Stream 3:  Live shell PTY (full duplex)                        │
│  Stream 4:  Streaming logs (agent → controller)                 │
│  Stream 5:  Metrics push                                        │
│  Stream 6:  Tunnel: localhost:8080 → agent:80                   │
│  Stream 7:  Tunnel: SOCKS5 dynamic forward                      │
│  ...                                                            │
│                                                                 │
│  Each stream has independent flow control.                      │
│  Each stream closes independently.                              │
│  Per-stream policy possible (audit different streams differently)│
└─────────────────────────────────────────────────────────────────┘
```

### Wire Protocol

```text
Outer frame (transport-independent):
┌──────────┬──────────┬──────────────────────────────────┐
│ Magic    │ Length   │ Noise ciphertext + 16B MAC       │
│ "RVNF"  │ u32 BE  │ ...                              │
└──────────┴──────────┴──────────────────────────────────┘

Inner (after Noise decrypt) — yamux frame:
┌────────┬─────┬───────┬────────────┬────────┬────────────────┐
│Version │Type │ Flags │ Stream ID  │ Length │ Payload        │
│ u8     │ u8  │ u16   │ u32        │ u32    │ ...            │
└────────┴─────┴───────┴────────────┴────────┴────────────────┘

Payload (msgpack):
  Request, Response, ShellInput, ShellOutput, FileChunk, MetricsBatch, ...
```

---

## Phase 10 — Path Upgrade / Migration

An established session can switch underlying transport without losing data.

### Cross-Protocol Upgrade (unique to RavenFabric)

```text
TIMELINE:
═══════════════════════════════════════════════════════════════════

t=0:    Initial connect via WebSocket relay (port 443)
        ┌─────┐         ┌──────┐         ┌─────┐
        │  A  │ ◄═════► │relay │ ◄═════► │  B  │
        └─────┘         └──────┘         └─────┘

t=1s:   Background race: try direct WireGuard
        ┌─────┐         ┌──────┐         ┌─────┐
        │  A  │ ◄═════► │relay │ ◄═════► │  B  │
        │     │ ·····························► │     │  ← attempting
        └─────┘                           └─────┘

t=3s:   Direct WireGuard succeeds, validate peer key
        ┌─────┐                           ┌─────┐
        │  A  │ ◄════════════════════════► │  B  │  ← new path
        │     │ ◄═════► relay ◄═════►     │     │  ← old path warm
        └─────┘                           └─────┘

t=3.1s: Atomic switch — same session, new transport
        Outstanding RPCs transferred via session ID continuity.
        Audit entry: "transport upgraded ws-relay → wireguard-direct"

t=3.5s: Old WebSocket gracefully closed (after drain timeout)
        ┌─────┐                           ┌─────┐
        │  A  │ ◄════════════════════════► │  B  │
        └─────┘                           └─────┘
```

### Migration Techniques

| Technique | How |
|-----------|-----|
| **QUIC connection migration** | Native — connection ID survives IP change |
| **Session ticket resumption** | Re-handshake on new transport, same session ID |
| **0-RTT resumption** | Pre-shared parameters, zero RTT |
| **Background race + atomic swap** | Establish new path, switch atomically when ready |
| **Make-before-break** | Hold both paths up during overlap window |

---

## Phase 11 — Health Monitoring & Failover

Continuous monitoring of path health.

### Health Indicators

| Indicator | How measured | Threshold |
|-----------|-------------|-----------|
| **Round-trip time** | Periodic ping | > 2x baseline = degraded |
| **Packet loss** | Sequence number gaps | > 1% sustained = degraded |
| **Connection liveness** | Heartbeat | Miss 3 = failed |
| **Network change** | OS event (route table, default gw) | Re-probe all drivers |
| **Captive portal appearance** | Detection probe | Pause + alert |

### Failover Logic

```text
ACTIVE PATH HEALTH CHECK (every 5s)
         │
         ▼
    Path healthy? ─── No ───► Already racing secondary?
         │                         │ Yes      │ No
         │ Yes                     ▼          ▼
         ▼                    Promote     Start race +
    Continue                  secondary   use relay as bridge
```

### Sticky vs Adaptive

| Mode | Behavior | Use case |
|------|----------|----------|
| **Sticky** | Hold chosen path until hard failure | Stability, low variability |
| **Adaptive** | Continuously re-evaluate, switch if better | Mobile nodes, varying network |
| **Hybrid** | Sticky within segment, re-evaluate on network change | Default recommended |

---

## Phase 12 — Graceful Teardown

Clean shutdown is as important as clean startup.

### Teardown Sequence

```text
1. Stop accepting new RPC requests on session
2. Wait for in-flight requests to complete (with timeout)
3. Flush audit log to durable storage
4. Send Noise close-notify
5. Close yamux streams gracefully
6. Close transport (TCP FIN, QUIC CONNECTION_CLOSE, etc.)
7. Zeroize session keys in memory
8. Update peer state: Connected → Disconnected (clean)
9. Cache last-known-good endpoint for fast reconnect
```

### Reconnect Strategy

| Strategy | Backoff | Use |
|----------|---------|-----|
| **Immediate retry** | None | Transient network glitch |
| **Exponential backoff** | 1s, 2s, 4s, ..., max 60s | Standard |
| **Exponential + jitter** | Full jitter | Prevent thundering herd |
| **Network-aware** | Wait for network event | Mobile, lid-close |
| **Scheduled** | Cron-style | Air-gap rendezvous windows |

---

## End-to-End Example

```text
═══════════════════════════════════════════════════════════════════
  rf exec prod-server-1 "systemctl status nginx"
═══════════════════════════════════════════════════════════════════

[0]  IDENTITY
     CLI loads operator key from ~/.config/ravenfabric/operator.key

[1]  ENROLLMENT — already done (operator enrolled previously)

[2]  DISCOVERY
     CLI checks local cache: prod-server-1 → endpoints?
     ✓ Cache hit: 4 endpoints (ws-relay, quic-relay, wg-direct, reticulum)

[3]  RENDEZVOUS
     CLI verifies cache freshness via broker (signed record)
     ✓ Endpoints valid (issued 12 min ago)

[4]  REACHABILITY ASSESSMENT
     NetworkProbe:
     - IPv6 available: yes
     - UDP 443 open: yes
     - Corporate proxy: no
     - NAT type: full cone

[5]  PATH SELECTION (race strategy)
     Tier 1 candidates (parallel):
     - WireGuard direct (IPv6)
     - QUIC direct
     Tier 2 fallback:
     - WebSocket via relay

[6]  TUNNEL ESTABLISHMENT
     t+0ms:   Start QUIC + WireGuard races
     t+12ms:  WireGuard direct connects (IPv6)
     t+18ms:  QUIC connects (cancel — slower)

[7]  BROKER NOT NEEDED
     Direct path established. Broker only for initial signaling.

[8]  CRYPTOGRAPHIC HANDSHAKE
     t+12ms:  Noise XX -> e
     t+24ms:  Noise XX <- e, ee, s, es
     t+36ms:  Noise XX -> s, se
     ✓ Mutual auth complete

[9]  SESSION ESTABLISHMENT
     t+36ms:  yamux session over Noise channel
     t+37ms:  Open stream 1 (RPC)

[10] BACKGROUND MONITORING
     QUIC kept warm as standby

[11] EXECUTE
     t+38ms:  Send RPC request (msgpack, Noise-sealed)
     t+50ms:  Agent receives, policy check
     t+52ms:  Policy ALLOW — execute "systemctl status nginx"
     t+103ms: Output streamed back
     t+105ms: CLI displays output

[12] TEARDOWN (or keep-alive)
     Single-shot: tear down after response
     Interactive: keep session warm 30s for next command

═══════════════════════════════════════════════════════════════════
TOTAL: ~105ms cold, <50ms warm
═══════════════════════════════════════════════════════════════════
```

---

## Extended Driver Trait Design

To support the full transport catalog, the Driver trait must be rich:

```rust
#[async_trait]
pub trait TransportDriver: Send + Sync {
    /// Unique identifier (e.g. "wireguard-direct", "websocket-relay")
    fn id(&self) -> &'static str;

    /// Tier classification (1 = direct, 2 = NAT-traversal, 3 = relay, ...)
    fn tier(&self) -> Tier;

    /// Probe whether this driver can work in the current network
    async fn probe(&self, env: &NetworkEnvironment) -> ProbeResult;

    /// Establish a transport-level connection (no crypto yet)
    async fn dial(&self, target: &Target, ctx: &DialContext) -> Result<Box<dyn AsyncStream>>;

    /// Listen for incoming connections (relay/server-side)
    async fn listen(&self, addr: &ListenAddr) -> Result<Box<dyn TransportListener>>;

    /// Capabilities of this transport
    fn capabilities(&self) -> Capabilities;

    /// Health check for an established connection
    async fn health(&self, conn: &dyn AsyncStream) -> Health;

    /// Support migration of session state to this transport
    async fn accept_migration(&self, token: SessionToken) -> Result<Box<dyn AsyncStream>>;
}

struct Capabilities {
    bidirectional: bool,
    reliable: bool,
    ordered: bool,
    max_bandwidth_bps: Option<u64>,
    typical_latency_ms: Option<u32>,
    supports_migration: bool,
    delay_tolerant: bool,
}
```

---

## Architectural Implications

This connectivity value chain establishes six core properties unique to RavenFabric:

1. **The Driver trait is the kernel.** Everything else builds on pluggable transports.
2. **Identity is independent of transport.** Same Noise XX over WireGuard, LoRa, serial, or USB stick.
3. **Broker is never privileged.** It facilitates connections, never controls them.
4. **Path selection is policy-driven.** Not just "fastest path" — "permitted path for this command type."
5. **Migration is first-class.** Sessions outlive any individual transport path.
6. **Air-gap is not a special case.** It's just another tier of driver.
7. **PQ-hybrid is designed in from day one** (implementation in v0.6).

No other system spans from direct WireGuard on datacenter LAN to Reticulum over
LoRa in arctic wilderness — under the same policy engine, same audit log,
and same identity.
