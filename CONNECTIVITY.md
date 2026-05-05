# RavenFabric — Connectivity Value Chain

> Komplett oversikt over verdikjeden fra to noder vet om hverandres eksistens
> til en kryptert, policy-validert datapakke flyter mellom dem. Hver fase har
> flere realistiske implementeringsalternativer.

---

## Verdikjede-oversikt

```
┌──────────────────────────────────────────────────────────────────┐
│  Fase 0: IDENTITY GENESIS                                        │
│  Hvem er noden? Hvordan vet vi at den er ekte?                   │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 1: ENROLLMENT / BOOTSTRAP                                  │
│  Hvordan blir en ny node kjent for fabric-en?                    │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 2: DISCOVERY                                               │
│  Hvordan finner to noder hverandre?                              │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 3: RENDEZVOUS                                              │
│  Hvordan møtes de første gang? Hvor utveksler de endepunkt-info? │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 4: NAT/REACHABILITY ASSESSMENT                             │
│  Hvilket nett er vi i? Hva slags traversering trengs?            │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 5: PATH SELECTION                                          │
│  Hvilke transporter er tilgjengelige? Hvilke skal prøves?        │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 6: NAT TRAVERSAL / TUNNEL ESTABLISHMENT                    │
│  Faktisk åpning av en pakkebar kanal mellom A og B               │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 7: BROKER / RELAY DECISION                                 │
│  Direkte? Via broker? Hybridt? Hvordan oppgraderes det senere?   │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 8: CRYPTOGRAPHIC HANDSHAKE                                 │
│  Mutual auth + nøkkel-etablering (uavhengig av transport)        │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 9: SESSION ESTABLISHMENT                                   │
│  Multipleksing, flow control, channel allocation                 │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 10: PATH UPGRADE / MIGRATION                               │
│  Bytte transport mid-session uten å miste data                   │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 11: HEALTH MONITORING & FAILOVER                           │
│  Kontinuerlig probing, automatisk fallback                       │
└──────────────────────────────────────────────────────────────────┘
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Fase 12: GRACEFUL TEARDOWN                                      │
│  Drenering, audit-flush, nøkkel-rotasjon                         │
└──────────────────────────────────────────────────────────────────┘
```

---

## Fase 0 — Identity Genesis

Identitet er fundamentet. Alt annet bygger på dette.

### Implementeringsalternativer

| Metode | Beskrivelse | Bruk i RavenFabric |
|--------|-------------|--------------------|
| **Curve25519 keypair** | Lokal generert, 32-byte privat/offentlig nøkkel | **Primær** — basis for Noise XX |
| **Ed25519 signing key** | Signering separat fra DH-nøkkel | Audit-logger, capability-tokens |
| **Hybrid PQ keypair** | X25519 + ML-KEM-768 (Kyber) | v0.6+ — harvest-now-decrypt-later-motstand |
| **TPM-bundet nøkkel** | Privat nøkkel kan ikke ekstraheres fra hardware | Enterprise-modus, attestasjon |
| **YubiKey/PKCS#11** | Smart-card-bundet identitet | Operatør-identiteter, ikke agent |
| **SPIFFE SVID** | Workload identity uavhengig av host | Kubernetes-modus, integrasjon med eksisterende SPIRE |
| **Content-addressed identity** | ID = hash(public_key) | Reticulum-stil — adresse = nøkkel |
| **Hierarkisk avledet** | HKDF fra master-secret per agent | Flåte-deployment, sentral nøkkelhierarki |

### Identity Forms

```
┌─────────────────────────────────────────────────────────────────┐
│  Static Identity (langlivet)                                    │
│  ├── Agent identity key   (Curve25519, 5+ år)                   │
│  ├── Audit signing key    (Ed25519, separat for accountability) │
│  └── Recovery key         (offline, kun for disaster recovery)  │
├─────────────────────────────────────────────────────────────────┤
│  Session Identity (kortlivet)                                   │
│  ├── Ephemeral DH key     (Curve25519, per session)             │
│  └── PQ-KEM ephemeral     (ML-KEM, per session, hybrid)         │
├─────────────────────────────────────────────────────────────────┤
│  Capability Tokens (per-handling)                               │
│  ├── Macaroons / Biscuits (kontekstualiserte tillatelser)       │
│  └── Time-bound caveats   (utløp, scope, attenuering)           │
└─────────────────────────────────────────────────────────────────┘
```

### Filsystem-layout (eksempel)

```
/etc/ravenfabric/
├── identity/
│   ├── agent.key         (0600, mlock'd, zeroed-on-drop)
│   ├── agent.pub         (0644)
│   ├── audit.key         (0600)
│   └── recovery.key.gpg  (offline-encrypted backup)
├── known_peers/
│   └── <agent-id>.pub    (TOFU-cache av peers)
└── policy/
    └── ...
```

---

## Fase 1 — Enrollment / Bootstrap

Hvordan blir en helt ny node lagt til i fabric-en?

### Implementeringsalternativer

| Metode | Sikkerhet | Bruksområde |
|--------|-----------|-------------|
| **OTP / one-time token** | Høy (TTL, single-use, hash-stored) | **Primær** — manuell admin-utstedelse |
| **Pre-shared key (PSK)** | Medium | Lab/dev, ikke produksjon |
| **Cloud-init / metadata service** | Medium-høy | AWS/Azure/GCP, instans-identitet |
| **Kubernetes ServiceAccount JWT** | Høy | K8s-deployments, projected token |
| **mTLS bootstrap** | Høy | Eksisterende PKI |
| **Hardware attestation (TPM/SEV/TDX)** | Svært høy | Confidential computing, regulert sektor |
| **Cloud provider IID** | Høy | AWS Instance Identity Document, Azure IMDS |
| **QR-kode + manuell** | Høy (out-of-band) | Edge-devices, fysisk tilgang |
| **NFC tap-to-enroll** | Høy | IoT, fysisk proximity |
| **DNS-validated** | Medium | Hvis agent eier DNS-record |
| **OAuth Device Flow** | Høy | Operatør-bootstrap (ikke agent) |
| **Sneakernet enrollment** | Maks (offline) | Air-gapped via USB/QR |

### Bootstrap Flow Pattern

```
┌──────────┐                    ┌──────────┐                    ┌──────────┐
│  Admin   │                    │  Broker  │                    │  Agent   │
└────┬─────┘                    └────┬─────┘                    └────┬─────┘
     │                                │                                │
     │  1. generate OTP               │                                │
     │ ──────────────────────────────►│                                │
     │       hash(otp), TTL, scope    │                                │
     │                                │                                │
     │  2. deliver token (out-of-band)│                                │
     │ ─────────────────────────────────────────────────────────────► │
     │       SSH / cloud-init / QR / NFC                               │
     │                                │                                │
     │                                │   3. generate keypair locally  │
     │                                │       (private NEVER leaves)   │
     │                                │                                │
     │                                │   4. POST /bootstrap           │
     │                                │ ◄──────────────────────────────│
     │                                │       {token, agent_id, pubkey}│
     │                                │                                │
     │                                │   5. validate + register       │
     │                                │       mark token used          │
     │                                │                                │
     │                                │   6. return relay endpoints +  │
     │                                │      controller pubkey         │
     │                                │ ──────────────────────────────►│
     │                                │                                │
     │                                │   7. agent caches identity     │
     │                                │       /etc/ravenfabric/        │
     │                                │                                │
     │                                │   8. all future: Noise XX      │
     │                                │       (bootstrap path closed)  │
```

---

## Fase 2 — Discovery

Når en node skal kommunisere, hvordan finner den ut hvem som er der?

### Implementeringsalternativer

| Metode | Skalerer til | Sensur-resistens | Bruk |
|--------|--------------|------------------|------|
| **Sentral broker-direktorat** | 100k+ noder | Lav | Standard enterprise |
| **DNS SRV records** | 10k+ | Lav-medium | Hvis kontroll over DNS |
| **mDNS / DNS-SD** | LAN-scope | N/A (lokalt) | Same-subnet bootstrap |
| **DHT (Kademlia)** | Globalt | Høy | P2P-modus, BitTorrent-stil |
| **Gossip (SWIM/HyParView)** | 10k+ | Høy | Self-organizing fleet |
| **Consul/etcd service discovery** | 10k+ | Lav | Eksisterende infrastruktur |
| **Kubernetes API** | Cluster-scope | Lav | K8s-native deployment |
| **Tailscale-stil tailnet map** | 100k+ | Lav | Sentral kontrollplan med push |
| **IPFS/IPNS lookup** | Globalt | Høy | Sensur-resistent rendezvous |
| **Reticulum announce-flood** | Mesh-scope | Høy | Air-gap, LoRa-mesh |
| **Yggdrasil DHT** | Globalt | Medium | IPv6-overlay |
| **Bluetooth LE advertise** | Proximity | N/A (lokalt) | Mobil/IoT |
| **Wi-Fi Direct discovery** | Proximity | N/A | Ad-hoc møter |
| **Static config file** | N/A | N/A | Air-gap, små deployments |
| **Verifiable signed records** | 100k+ | Høy | DNSSEC-stil signerte endepunkter |

### Hybrid Discovery (anbefalt for RavenFabric)

```
┌─────────────────────────────────────────────────────────────────┐
│  PRIORITY 1: Cached known peers                                 │
│              (raskest, ingen rundtur)                           │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 2: mDNS/DNS-SD (LAN-scope)                            │
│              (lav latens, ingen broker nødvendig)               │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 3: Broker directory query                             │
│              (autoritativt, krever internett)                   │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 4: Gossip from connected peers                        │
│              (eventual consistency, robust)                     │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 5: DHT lookup                                         │
│              (sensur-resistent, treig bootstrap)                │
├─────────────────────────────────────────────────────────────────┤
│  PRIORITY 6: Reticulum announce / mesh broadcast                │
│              (offline, langsomt, sist resort)                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Fase 3 — Rendezvous

To noder vet at den andre eksisterer. Nå må de utveksle nok info til å faktisk
forsøke en forbindelse — kandidat-endepunkter, transport-preferanser, osv.

### Implementeringsalternativer

| Metode | Latens | Privacy | Bruk |
|--------|--------|---------|------|
| **Broker as rendezvous point** | Lav | Lav (broker ser metadata) | **Primær** for fabric-modus |
| **STUN-style server-reflexive discovery** | Lav | Medium | Klassisk WebRTC-mønster |
| **DHT-stored endpoint records** | Medium | Høy | Signerte records i Kademlia |
| **Out-of-band exchange (QR/NFC)** | N/A (manuelt) | Maks | Initial pairing |
| **IPFS PubSub topics** | Medium | Medium | Topic = hash(peer_id) |
| **libp2p Circuit Relay v2 reservation** | Lav | Medium | DCUtR-mønster |
| **Tailscale-stil koordinator-push** | Lav | Lav | Sentral kontrollplan |
| **Matrix room as rendezvous** | Medium | Medium | Federert chat-protokoll som signaling |
| **Email-based rendezvous** | Høy | Høy | NNCP-stil offline rendezvous |
| **Blockchain anchored rendezvous** | Høy | Maks | Sensur-resistent men dyrt |

### Rendezvous Payload (eksempel)

```yaml
# Det som utveksles ved rendezvous, signert med agentens identity key:
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

## Fase 4 — NAT / Reachability Assessment

Før man velger transport: hva slags nett er vi *i*?

### Implementeringsalternativer

| Probe | Hva det avdekker | Cost |
|-------|------------------|------|
| **STUN binding request** | Public IP, port mapping behavior | Lav |
| **STUN behavior tests (RFC 5780)** | NAT type (full cone, restricted, symmetric) | Lav |
| **UPnP / NAT-PMP / PCP query** | Mulighet for port forwarding | Lav |
| **IPv6 reachability test** | Direkte IPv6-tilgjengelighet | Lav |
| **Captive portal detection** | Er vi i hotell/flyplass-WiFi? | Lav |
| **MTU discovery** | Path MTU mellom peers | Medium |
| **HTTP/HTTPS reachability** | Funker ihvertfall port 443? | Lav |
| **DNS-over-HTTPS test** | Er DNS-trafikk filtrert? | Lav |
| **TCP fingerprint analysis** | Er det DPI/proxy i veien? | Medium |
| **Socks/HTTP proxy detection** | Bedrifts-proxy konfigurert? | Lav |
| **Per-relay latency probe** | Hvilken relay er nærmest? | Medium |
| **Bandwidth estimation** | Hva er praktisk throughput? | Høy |
| **Time-of-day correlation** | Er det tidssone-baserte filtre? | Lang sikt |

### Tailscale netcheck-mønster (utvidet)

```
NetworkProbe {
    nat_type: NATType,                     // Open, FullCone, Restricted, PortRestricted, Symmetric
    ipv4_available: bool,
    ipv6_available: bool,
    udp_blocked: bool,
    tcp_443_open: bool,
    udp_443_open: bool,
    captive_portal: Option<CaptivePortal>,
    mtu_v4: u16,
    mtu_v6: u16,
    relay_latencies: HashMap<RelayId, Duration>,
    has_corporate_proxy: bool,
    proxy_endpoint: Option<ProxyConfig>,
    dns_filtered: bool,
    ipv6_only: bool,
    nat64_present: bool,
    available_drivers: Vec<DriverId>,        // Hvilke transport-drivers kan teoretisk fungere
    egress_filter_class: EgressClass,        // Open / EnterpriseProxy / Hostile / AirGap
}
```

### Egress Klassifisering

```
EgressClass:
  Open              → alle transporter mulig
  HomeRouter        → de fleste mulig, NAT i veien
  EnterpriseProxy   → kun HTTP/HTTPS gjennom proxy
  RestrictiveDPI    → må gjemme seg som HTTPS (MASQUE/domain fronting)
  Hostile           → må kamufleres aktivt (obfs4, Shadowsocks)
  AirGap            → ingen IP-nett, kun fysisk/radio
  Sneakernet        → kun USB/QR/manual transport
```

---

## Fase 5 — Path Selection

Med kjent nett-miljø: hvilke transporter skal vi prøve, og i hvilken rekkefølge?

### Komplett transport-katalog for RavenFabric

#### Tier 1: Direct Connectivity (lavest latens)

| Transport | NAT-traversering | Internett | Kommentar |
|-----------|------------------|-----------|-----------|
| **WireGuard direct (IPv4)** | Åpent nett | Ja | Lavest latens, krever direkte rute |
| **WireGuard direct (IPv6)** | IPv6 (ofte ingen NAT) | Ja | Foretrukket hvis tilgjengelig |
| **QUIC direct** | Åpent nett | Ja | Connection migration, 0-RTT resumption |
| **TCP direct (Noise-over-TCP)** | Åpent nett | Ja | Fallback når UDP blokkert |
| **SCTP direct** | Åpent nett | Ja | Eksotisk, men multistreaming innebygd |

#### Tier 2: NAT Traversal (krever koordinering)

| Transport | Metode | Hit rate |
|-----------|--------|----------|
| **WireGuard + STUN** | Server-reflexive endpoint | ~70% |
| **WireGuard + UDP hole punching** | Symmetric coordination via broker | ~85% |
| **QUIC + ICE** | Full ICE-rammeverk | ~95% |
| **libp2p DCUtR** | "Direct Connection Upgrade through Relay" | ~90% |
| **TCP simultaneous open** | RFC 5128 hole punching for TCP | ~60% |
| **Birthday-paradox port prediction** | For symmetric NAT | ~40% |

#### Tier 3: Brokered Transports (alltid funker hvis broker er nådd)

| Transport | Båndbredde | Latens | Kommentar |
|-----------|------------|--------|-----------|
| **WebSocket via broker (port 443)** | Høy | Medium | Funker overalt med HTTPS |
| **QUIC via broker** | Høy | Lav | Med 0-RTT resumption |
| **HTTP/3 + MASQUE** | Høy | Lav | Skjuler seg som HTTPS |
| **TCP-relay (TURN-stil)** | Medium | Medium | Klassisk fallback |
| **WebRTC DataChannel via TURN** | Medium | Medium | Bra for nettleser-klienter |

#### Tier 4: Overlay Networks

| Transport | Sensur-resistens | Kommentar |
|-----------|------------------|-----------|
| **Yggdrasil overlay** | Medium | IPv6 mesh, self-routing |
| **CJDNS** | Medium | Mer veteran enn Yggdrasil |
| **Tor hidden service (.onion)** | Høy | Anonymitet inkludert |
| **I2P** | Høy | Garlic routing, intern fokus |
| **libp2p mesh** | Medium | Modulært, mange transporter |

#### Tier 5: Censorship-Resistant / Hostile Networks

| Transport | Hva det skjuler |
|-----------|-----------------|
| **Domain fronting (CDN)** | Faktisk destinasjon |
| **ECH (Encrypted Client Hello)** | SNI-info |
| **MASQUE (CONNECT-UDP)** | All trafikk inni HTTPS |
| **obfs4 / meek / snowflake** | Trafikk-mønster (Tor pluggable transports) |
| **Shadowsocks** | Stealth ut fra Kina-stil DPI |
| **Trojan-GFW** | Ser ut som vanlig HTTPS |
| **V2Ray VMess / VLESS** | Multi-protocol obfuscation |
| **Hysteria2 / TUIC** | QUIC-basert med ekstra obfuskering |

#### Tier 6: Out-of-Band (offline / air-gap)

| Transport | Båndbredde | Latens | Bruk |
|-----------|------------|--------|------|
| **Reticulum (TCP backbone)** | Lav | Medium | Mesh over hva som helst |
| **Reticulum (LoRa)** | < 11 kbps | Sekunder-minutter | Lang rekkevidde, lavt strømforbruk |
| **Reticulum (BLE)** | Medium | Medium | Proximity, lokal |
| **Reticulum (packet radio AX.25)** | Lav | Sekunder | HF/VHF/UHF, global rekkevidde |
| **Serial (RS-232/USB)** | Variabel | Lav | Direkte kabel, true air-gap |
| **NNCP (sneakernet)** | Variabel | Timer-dager | USB-stick, brevpost |
| **SMTP/IMAP tunnel** | Lav | Minutter | E-post som transport |
| **DNS tunnel (DoH/DoT)** | Veldig lav | Høy | Sist resort |
| **ICMP tunnel** | Lav | Medium | Funker når kun ping er tillatt |
| **HF radio (Winlink-stil)** | Veldig lav | Minutter | Globalt, ingen infrastruktur |
| **Iridium SBD** | Veldig lav | Minutter | Satellitt, globalt |
| **Starlink** | Høy | Lav | Satellitt-bredbånd |
| **Audio modem (lyd)** | Veldig lav | N/A | Mellom enheter med mikrofon |
| **QR-stream (visuelt)** | Lav | N/A | Air-gap exfil/import |
| **NFC** | Lav | N/A | Proximity bootstrap |

### Path Selection Strategier

```
┌─────────────────────────────────────────────────────────────────┐
│  Strategi: SEQUENTIAL                                           │
│  Prøv én av gangen i prioritert rekkefølge.                     │
│  Bra for: Batteri-sparing, mobile noder                         │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Strategi: RACE (Happy Eyeballs)                                │
│  Start alle parallelt, bruk første som svarer.                  │
│  Bra for: Latency-kritisk, bruker mer båndbredde                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Strategi: PARALLEL                                             │
│  Etabler ALLE samtidig, hold dem oppe.                          │
│  Bra for: Mission-critical, multipath, redundans                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Strategi: TIERED RACE                                          │
│  Prøv tier 1 parallelt. Hvis alle feiler, tier 2 parallelt.     │
│  Bra for: Balansert ytelse vs båndbredde                        │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Strategi: POLICY-DRIVEN                                        │
│  Policy bestemmer tillatte transporter per kommando-type.       │
│  Bra for: Sikkerhet — sensitive operasjoner kun via direkte WG. │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Strategi: ADAPTIVE                                             │
│  ML/heuristikk basert på historisk suksess i samme nett.        │
│  Bra for: Noder som flytter mye (mobile, laptop)                │
└─────────────────────────────────────────────────────────────────┘
```

---

## Fase 6 — NAT Traversal / Tunnel Establishment

Selve åpningen av en pakkebar kanal.

### Teknikker

| Teknikk | NAT-type det funker for | Kommentar |
|---------|-------------------------|-----------|
| **Direct connect** | Open / IPv6 | Ingen traversering nødvendig |
| **STUN binding (RFC 8489)** | Cone NAT | Server-reflexive endpoint |
| **UDP hole punching** | Restricted, port-restricted | Krever simultan signaling |
| **TCP hole punching (RFC 5128)** | De fleste | Vanskeligere enn UDP |
| **TCP simultaneous open** | Spesifikke implementasjoner | OS-avhengig |
| **UPnP IGD** | Hjemme-rutere | Ofte deaktivert i bedrift |
| **NAT-PMP / PCP** | Apple-rutere, moderne | Mer pålitelig enn UPnP |
| **Birthday paradox attack** | Symmetric | Statistisk port-gjetting |
| **Port prediction** | Sequential symmetric | Forutsigbar portallokering |
| **TURN relay** | Alle | 100% suksess, men relay i veien |
| **DCUtR (libp2p)** | De fleste | Smart "start relay, oppgrader direkte" |
| **ICE (RFC 8445)** | Alle | Komplett rammeverk, prøver alle metoder |
| **Trickle ICE** | Alle | Inkrementell kandidat-utveksling |
| **MASQUE CONNECT-UDP** | Alle | UDP-traversering inni HTTPS |

### Hole Punching Sekvens (UDP)

```
┌──────────┐                ┌──────────┐                ┌──────────┐
│  Peer A  │                │  Broker  │                │  Peer B  │
│ (NAT)    │                │ (public) │                │ (NAT)    │
└────┬─────┘                └────┬─────┘                └────┬─────┘
     │                            │                            │
     │  1. STUN binding           │                            │
     │ ──────────────────────────►│                            │
     │  ◄──────────────────────── │                            │
     │  Public: 198.51.100.1:443  │                            │
     │                            │                            │
     │                            │   2. STUN binding          │
     │                            │ ◄──────────────────────────│
     │                            │ ──────────────────────────►│
     │                            │  Public: 203.0.113.7:31443 │
     │                            │                            │
     │  3. Request to talk to B   │                            │
     │ ──────────────────────────►│                            │
     │                            │   4. Forward A's endpoint  │
     │                            │ ──────────────────────────►│
     │                            │   B's endpoint to A        │
     │  ◄──────────────────────── │                            │
     │                            │                            │
     │  5. SIMULTANEOUS PUNCH                                  │
     │ ───────────────────────────┼───────────────────────────►│
     │ ◄──────────────────────────┼───────────────────────────  │
     │                            │                            │
     │  6. (Hopefully) direct path established                 │
     │ ────────────────────────────────────────────────────────►│
     │ ◄────────────────────────────────────────────────────── │
```

---

## Fase 7 — Broker / Relay Decision

Når og hvordan brukes broker-en?

### Broker-roller i RavenFabric

```
┌─────────────────────────────────────────────────────────────────┐
│  RavenFabric Broker (rf-relay)                                  │
│                                                                 │
│  Roller:                                                        │
│  ├── Discovery directory     (hvem finnes)                      │
│  ├── Rendezvous facilitator  (utveksle endepunkter)             │
│  ├── Hole-punch coordinator  (synkroniser punch-pakker)         │
│  ├── Fallback data relay     (bytte-pakker når direkte feiler)  │
│  ├── Session metadata logger (kun metadata, ikke innhold)       │
│  └── Health reporter         (oppetid, latens, kapasitet)       │
│                                                                 │
│  ALDRI roller:                                                  │
│  ├── Decryption                                                 │
│  ├── Policy evaluation                                          │
│  ├── Identity issuer (kun OTP-validering)                       │
│  └── Audit storage (audit ligger på agenten)                    │
└─────────────────────────────────────────────────────────────────┘
```

### Broker-arkitektur-mønstre

| Mønster | Eksempel | Trade-offs |
|---------|----------|------------|
| **Sentral broker** | Tailscale DERP | Enkelt, men single point of failure |
| **Geo-distribuert mesh av brokere** | Cloudflare-stil | Lav latens, kompleks |
| **Federerte brokere** | Matrix homeservers | Selvhosting per organisasjon |
| **Self-hosted only** | Headscale | Full kontroll, drift-byrde |
| **DHT-basert (ingen broker)** | BitTorrent, IPFS | Sensur-resistent, treig bootstrap |
| **Hybrid (sentral + DHT-fallback)** | Hyperswarm | Best av begge verdener |
| **P2P broker rotation** | Veilid | Hvilken som helst node kan være broker |

### Broker Connection Modes

```
┌─────────────────────────────────────────────────────────────────┐
│  Mode 1: BROKER-ASSISTED, DIRECT DATA                           │
│  ┌────────┐                ┌────────┐                ┌────────┐ │
│  │ Agent A│ ◄──signal───►  │ Broker │ ◄──signal───►  │ Agent B│ │
│  └────┬───┘                └────────┘                └───┬────┘ │
│       │                                                  │      │
│       └──────────────── direct data ──────────────────── ┘      │
│  Broker hjelper med rendezvous, så ute av veien.                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Mode 2: BROKER-RELAYED                                         │
│  ┌────────┐                ┌────────┐                ┌────────┐ │
│  │ Agent A│ ◄──data────►   │ Broker │ ◄──data────►   │ Agent B│ │
│  └────────┘                └────────┘                └────────┘ │
│  Hvis direkte feiler. Broker ser kun ciphertext.                │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Mode 3: HYBRID (broker-relay → direct upgrade)                 │
│  Start relay, oppgrader til direkte i bakgrunnen.               │
│  Pakker kan migreres mid-stream via session ID.                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Mode 4: BROKERLESS                                             │
│  Kun lokal/cached endpoint info. Ingen broker.                  │
│  For air-gap, mDNS, eller pre-configured peers.                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  Mode 5: MULTI-BROKER (anycast)                                 │
│  Klient bruker geografisk nærmeste broker, kan failover.        │
│  Alle brokere er synkroniserte (gossip).                        │
└─────────────────────────────────────────────────────────────────┘
```

### Broker Authentication

| Metode | Hva broker validerer | Bruk |
|--------|----------------------|------|
| **HMAC token** | Pre-shared secret per agent | Standard |
| **Certificate-based** | mTLS med agent-cert | Enterprise |
| **OTP for bootstrap** | Single-use token | Initial enrollment |
| **Signed challenge** | Agent signerer broker-utstedt nonce | Anti-replay |
| **No auth (federated)** | Kun rate-limiting | Public broker-mesh |

### Hva broker IKKE skal kunne (sikkerhetsegenskaper)

```
┌─────────────────────────────────────────────────────────────────┐
│  RavenFabric Broker Threat Model                                │
│                                                                 │
│  Antakelse: BROKER ER POTENSIELT KOMPROMITTERT                  │
│                                                                 │
│  Broker MÅ IKKE kunne:                                          │
│  ✗  Lese kommando-innhold                                       │
│  ✗  Lese fil-innhold                                            │
│  ✗  Lese audit-events                                           │
│  ✗  Modifisere meldinger uten å bli oppdaget                    │
│  ✗  Impersonate enten part                                      │
│  ✗  Decrypte i ettertid (forward secrecy)                       │
│                                                                 │
│  Broker KAN se:                                                 │
│  •  Hvilke peer-IDs som snakker (metadata)                      │
│  •  Tidspunkt og volum (timing/size analysis)                   │
│  •  IP-adresser til endpunkter                                  │
│                                                                 │
│  Mitigering for metadata-leakage:                               │
│  •  Padding til faste rammestørrelser                           │
│  •  Cover traffic / dummy packets (high-paranoia mode)          │
│  •  Mixnet routing for kontroll-plan (v0.5+)                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## Fase 8 — Cryptographic Handshake

Mutual auth + nøkkel-etablering, helt uavhengig av transport.

### Implementeringsalternativer

| Protokoll | Egenskap | Bruk i RF |
|-----------|----------|-----------|
| **Noise XX** | Mutual auth, forward secrecy, no PKI | **Primær (v0.1)** |
| **Noise IK** | Initiator kjenner responder pubkey på forhånd | Resumption / kjente peers |
| **Noise NK** | Responder anonym | Klient → broker uten klient-cert |
| **Noise XX + ML-KEM hybrid** | Post-quantum | **v0.6+** |
| **TLS 1.3 + mTLS** | PKI-basert | Hvis interop med eksisterende systemer |
| **WireGuard (Noise IK-variant)** | Innebygd i WG-protokollen | For wireguard-driver |
| **CurveCP** | DJBs ChaCha-baserte | Eksotisk, men interessant |
| **PQXDH (Signal)** | Hybrid PQ for asynkron messaging | Async/store-and-forward |
| **MLS (RFC 9420)** | Group key agreement | Multi-party sessions |

### Noise XX Pattern (RavenFabric primær)

```
Pattern: Noise_XX_25519_ChaChaPoly_BLAKE2s

  -> e
  <- e, ee, s, es        (responder authenticates)
  -> s, se               (initiator authenticates)

Properties:
  ✓ Mutual authentication
  ✓ Forward secrecy (ephemeral keys)
  ✓ Identity hiding (initiator's static key encrypted)
  ✓ Replay resistance
  ✓ KCI resistance (key compromise impersonation)
  ✓ 1.5 RTT setup
```

### Hybrid Post-Quantum (v0.6 design)

```
Pattern: Noise_XXhfs_25519+ML-KEM-768_ChaChaPoly_BLAKE2s

  -> e, e1                       (e = X25519, e1 = ML-KEM ephemeral)
  <- e, ee, ekem1, s, es
  -> s, se

Forward secrecy: protected by both classical AND PQ-KEM.
Harvest-now-decrypt-later: defeated.
```

---

## Fase 9 — Session Establishment

Etter handshake: hvordan strukturerer vi kanal-en?

### Multipleksering

| Protokoll | Egenskap | Bruk |
|-----------|----------|------|
| **yamux** | Battle-tested (libp2p) | **Primær (v0.1)** |
| **HTTP/2 framing** | Standard, men HTTP-orientert | Hvis interop med web |
| **QUIC streams** | Native i QUIC | Når transport = QUIC |
| **mplex** | Enklere libp2p mux | For embedded/IoT |
| **SCTP streams** | Native | SCTP-transport |
| **Custom length-delimited** | Enkleste | Air-gap, low-bandwidth |

### Stream Allocation Patterns

```
┌─────────────────────────────────────────────────────────────────┐
│  En Noise XX session kan ha mange yamux-streams:                │
│                                                                 │
│  Stream 0:  Control plane (heartbeat, capability negotiation)   │
│  Stream 1:  RPC requests/responses                              │
│  Stream 2:  Bulk file transfer                                  │
│  Stream 3:  Live shell PTY (full duplex)                        │
│  Stream 4:  Streaming logs (agent → controller)                 │
│  Stream 5:  Metrics push                                        │
│  Stream 6:  Tunnel: localhost:8080 → agent:80                   │
│  Stream 7:  Tunnel: SOCKS5 dynamic forward                      │
│  ...                                                             │
│                                                                 │
│  Hver stream har independent flow control.                      │
│  Hver stream lukkes uavhengig av andre.                         │
│  Per-stream policy mulig (audit forskjellige streams ulikt).    │
└─────────────────────────────────────────────────────────────────┘
```

### Frame Format (RavenFabric wire-protokoll)

```
Outer frame (transport-uavhengig):
┌──────────┬──────────┬──────────────────────────────────┐
│ Magic    │ Length   │ Noise ciphertext + 16B MAC       │
│ "RVNF"   │ u32 BE   │ ...                              │
└──────────┴──────────┴──────────────────────────────────┘

Inner (etter Noise decrypt) — yamux-frame:
┌────────┬─────┬───────┬────────────┬────────┬────────────────┐
│Version │Type │ Flags │ Stream ID  │ Length │ Payload        │
│ u8     │ u8  │ u16   │ u32        │ u32    │ ...            │
└────────┴─────┴───────┴────────────┴────────┴────────────────┘

Payload (msgpack):
  RPC Request, RPC Response, ShellInput, ShellOutput, FileChunk, ...
```

---

## Fase 10 — Path Upgrade / Migration

En etablert session kan bytte underlag uten å miste data.

### Migration-mønstre

| Mønster | Hvordan |
|---------|---------|
| **QUIC connection migration** | Native — connection ID overlever IP-bytte |
| **MPTCP subflow add/remove** | Multipath TCP add/remove subflow |
| **Session ticket resumption** | Re-handshake på ny transport, samme session ID |
| **0-RTT resumption** | Forhåndsdelte parametre, ingen RTT |
| **Channel binding** | Session ID kryptografisk knyttet til Noise-state |
| **Background race + atomic swap** | Etabler ny path, switch atomisk når klar |
| **Make-before-break** | Hold begge paths oppe i overlap-vindu |

### Cross-Protocol Upgrade (RavenFabric unik egenskap)

```
TIMELINE:
═══════════════════════════════════════════════════════════════════

t=0:    Initial connect via WebSocket relay (port 443)
        ┌─────┐         ┌──────┐         ┌─────┐
        │  A  │ ◄═════► │relay │ ◄═════► │  B  │
        └─────┘         └──────┘         └─────┘

t=1s:   Background race: try direct WireGuard
        ┌─────┐         ┌──────┐         ┌─────┐
        │  A  │ ◄═════► │relay │ ◄═════► │  B  │
        │     │ ····················════► │     │  ← attempting
        └─────┘                            └─────┘

t=3s:   Direct WireGuard succeeds, validate peer key
        ┌─────┐                            ┌─────┐
        │  A  │ ◄══════════════════════►   │  B  │  ← new path verified
        │     │ ◄═════► relay ◄═════►      │     │  ← old path warm
        └─────┘                            └─────┘

t=3.1s: Atomic switch — same session, new transport
        Outstanding RPCs transferred via session ID continuity.
        Audit entry: "transport upgraded ws → wireguard-direct"

t=3.5s: Old WebSocket gracefully closed (after drain timeout)
        ┌─────┐                            ┌─────┐
        │  A  │ ◄══════════════════════►   │  B  │
        └─────┘                            └─────┘
```

---

## Fase 11 — Health Monitoring & Failover

Kontinuerlig overvåking av path-helse.

### Helse-indikatorer

| Indikator | Måles hvordan | Threshold |
|-----------|---------------|-----------|
| **Round-trip time** | Periodic ping | > 2x baseline = degraded |
| **Packet loss** | Sequence number gaps | > 1% sustained = degraded |
| **Throughput** | Active probing | < expected = investigate |
| **Connection liveness** | Heartbeat | Miss 3 = failed |
| **MTU changes** | DPLPMTUD | Trigger reconfigure |
| **Network change** | OS event (route table, default gw) | Re-probe alle drivers |
| **Captive portal appearance** | Detection probe | Pause + alert |

### Failover Logic

```
┌─────────────────────────────────────────────────────────────────┐
│  ACTIVE PATH HEALTH CHECK (every 5s)                            │
└────────────────┬────────────────────────────────────────────────┘
                 │
                 ▼
        ┌────────────────┐         No
        │ Path healthy?  │ ────────────►  ┌──────────────────────┐
        └────────┬───────┘                │ Trigger failover     │
                 │ Yes                    └──────────┬───────────┘
                 │                                   │
                 ▼                                   ▼
        ┌────────────────┐                 ┌────────────────────┐
        │ Continue       │                 │ Already racing     │
        └────────────────┘                 │ secondary path?    │
                                           └─────┬──────────┬───┘
                                                 │ Yes      │ No
                                                 ▼          ▼
                                        ┌──────────┐  ┌─────────────┐
                                        │ Promote  │  │ Start race  │
                                        │ secondary│  │ + use relay │
                                        └──────────┘  │ as bridge   │
                                                      └─────────────┘
```

### Sticky vs Adaptive

```
Sticky:    Hold valgt path til den feiler hardt.
           Bra for: Stabilitet, lav variabilitet

Adaptive:  Re-evaluer kontinuerlig, bytt hvis bedre finnes.
           Bra for: Mobile noder, varierende nett

Hybrid:    Sticky innen samme nettverkssegment,
           re-evaluer ved oppdaget nettverksskifte.
```

---

## Fase 12 — Graceful Teardown

Avslutning er like viktig som oppstart.

### Teardown-sekvens

```
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

### Reconnect Strategi

| Strategi | Backoff | Bruk |
|----------|---------|------|
| **Immediate retry** | None | Transient nettverks-glitch |
| **Exponential backoff** | 1s, 2s, 4s, 8s, ..., max 60s | Standard |
| **Exponential + jitter** | Fullt jitter | Forhindre thundering herd |
| **Adaptive (network-aware)** | Vent på nettverk-event | Mobile, lokk-laptop |
| **Scheduled** | Cron-stil | Air-gap rendezvous-vinduer |

---

## Komplett Verdikjede — Sammensatt Eksempel

```
═══════════════════════════════════════════════════════════════════
  rf exec prod-server-1 "systemctl status nginx"
═══════════════════════════════════════════════════════════════════

[0]  IDENTITY GENESIS
     CLI loads operator identity from ~/.config/ravenfabric/operator.key
     
[1]  ENROLLMENT — already done (operator enrolled previously)

[2]  DISCOVERY
     CLI checks local cache: prod-server-1 → endpoints?
     ✓ Cache hit, 4 endpoints (ws-relay, quic-relay, wg-direct, reticulum)

[3]  RENDEZVOUS
     CLI verifies cache freshness via broker (signed endpoint record)
     ✓ Endpoints still valid (issued 12 min ago)

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
     t+12ms:  Noise XX e (initiator)
     t+24ms:  Noise XX e, ee, s, es (responder)
     t+36ms:  Noise XX s, se (initiator)
     ✓ Mutual auth complete

[9]  SESSION ESTABLISHMENT
     t+36ms:  yamux session opened over Noise channel
     t+37ms:  Open stream 1 (RPC), capability negotiation

[10] BACKGROUND PATH MONITORING
     QUIC kept warm as standby (race won by WG, but QUIC OK)

[11] EXECUTE
     t+38ms:  Send RPC request (msgpack-encoded, Noise-sealed)
     t+50ms:  Agent receives, policy check
     t+52ms:  Policy ALLOW — execute "systemctl status nginx"
     t+103ms: Output streamed back via stream 1
     t+105ms: CLI displays output

[12] GRACEFUL TEARDOWN (or session kept alive)
     If single-shot: tear down after response
     If interactive: keep session warm 30s for next command

═══════════════════════════════════════════════════════════════════
TOTAL: ~105ms cold, <50ms warm
═══════════════════════════════════════════════════════════════════
```

---

## Roadmap-implikasjoner for RavenFabric

Basert på denne verdikjeden, her er en utvidet implementeringsplan:

### v0.1 — Foundation (allerede planlagt)
- [x] Identity (Curve25519 keypair)
- [x] Bootstrap (OTP)
- [x] Crypto (Noise XX)
- [x] Policy + audit
- [ ] **Discovery: cached + broker-directory** (ny)
- [ ] **Single transport: WebSocket via broker** (allerede planlagt)
- [ ] **Broker: stateless relay** (allerede planlagt)

### v0.2 — Multi-Transport
- [ ] **NetworkProbe / NAT detection** (Fase 4)
- [ ] **Path selection engine** (sequential strategy først)
- [ ] **WireGuard direct driver** (Fase 6 tier 1)
- [ ] **QUIC driver med connection migration** (Fase 6 tier 1)
- [ ] **STUN/hole punching koordinator** (Fase 6 tier 2)

### v0.3 — Path Diversity
- [ ] **Race + parallel strategier** (Fase 5)
- [ ] **Background transport upgrade** (Fase 10)
- [ ] **mDNS discovery** (LAN-scope)
- [ ] **MagicDNS / overlay IP** (allerede planlagt)

### v0.4 — Enterprise Networks
- [ ] **HTTP/HTTPS proxy support** (Fase 4 → 6)
- [ ] **MASQUE driver** (Fase 6 tier 5)
- [ ] **Federated brokers** (geo-distribuert)

### v0.5 — Air-Gap & Hostile Networks
- [ ] **Reticulum driver** (Fase 6 tier 6)
- [ ] **Tor hidden service driver** (Fase 6 tier 4)
- [ ] **Serial driver (RS-232/USB)** (Fase 6 tier 6)
- [ ] **NNCP driver (sneakernet)** (Fase 6 tier 6)
- [ ] **DNS tunnel driver** (sist resort)

### v0.6 — Post-Quantum & Advanced
- [ ] **Hybrid PQ Noise (X25519 + ML-KEM-768)** (Fase 8)
- [ ] **Capability tokens (biscuit/macaroon)** (Fase 0)
- [ ] **CRDT-baserte audit-logger** (eventual consistency)
- [ ] **Mixnet kontroll-plan (high-paranoia mode)** (metadata-beskyttelse)

### v0.7 — Mesh-Native Discovery
- [ ] **Gossip-based peer discovery** (Fase 2)
- [ ] **DHT discovery (Kademlia)** (Fase 2)
- [ ] **Brokerless mode** (Fase 7 mode 4)
- [ ] **Yggdrasil overlay driver** (Fase 6 tier 4)

### v1.0 — Adaptive Intelligence
- [ ] **ML-based path selection** (Fase 5 adaptive)
- [ ] **Predictive failover** (Fase 11)
- [ ] **Network change events (OS-integration)** (Fase 11)
- [ ] **Multi-path simultan (multipath QUIC)** (Fase 6 + 10)

---

## Driver Trait Design (utvidet)

For å støtte hele dette spekteret, må Driver-traiten være ekstremt fleksibel:

```rust
#[async_trait]
pub trait TransportDriver: Send + Sync {
    /// Unique identifier (e.g. "wireguard-direct", "websocket-relay")
    fn id(&self) -> &'static str;
    
    /// Tier (1 = direct, 2 = nat-traversal, 3 = relay, ...)
    fn tier(&self) -> Tier;
    
    /// Probe whether this driver can work in the current network
    async fn probe(&self, env: &NetworkEnvironment) -> ProbeResult;
    
    /// Establish a transport-level connection (no crypto yet)
    async fn dial(&self, target: &Target, ctx: DialContext) -> Result<Box<dyn AsyncStream>>;
    
    /// Listen for incoming connections (relay/server-side)
    async fn listen(&self, addr: &ListenAddr) -> Result<Box<dyn TransportListener>>;
    
    /// Capabilities of this transport
    fn capabilities(&self) -> Capabilities {
        // - bidirectional?
        // - reliable?
        // - ordered?
        // - max_bandwidth_estimate?
        // - typical_latency?
        // - supports_migration?
    }
    
    /// Health check for an established connection
    async fn health(&self, conn: &dyn AsyncStream) -> Health;
    
    /// Optional: support migration of session state to this transport
    async fn accept_migration(&self, session_token: SessionToken) -> Result<Box<dyn AsyncStream>>;
}
```

---

## Oppsummering — Hva dette betyr for RavenFabric

Denne verdikjeden viser at RavenFabric ikke er én ting, men en **abstraksjon
over et stort mulighetsrom**. De arkitektoniske valgene som gjør produktet
unikt er:

1. **Driver-traiten er kjernen.** Alt annet bygger på at "transport" er pluggable.
2. **Identitet er uavhengig av transport.** Samme Noise XX over alt — fra 
   WireGuard til Reticulum til serial.
3. **Broker er aldri privilegert.** Den er en bytteservice, ikke en kontroll-plan.
4. **Path selection er policy-styrt.** Ikke bare "raskeste path" — også 
   "tillatt path for denne kommando-typen".
5. **Migration er førsteklasses.** Session lever lenger enn enhver enkelt path.
6. **Air-gap er ikke en spesialcase.** Det er bare en annen tier av driver.
7. **PQ-hybrid fra dag 1 av designet** (selv om implementering venter til v0.6).

Dette gjør RavenFabric unik i markedet: ingen andre system jeg kjenner til
spenner fra direkte WireGuard på datasenter-LAN til Reticulum over LoRa i
arktisk villmark — under samme policy-engine, samme audit-logg, og samme
identitet.
