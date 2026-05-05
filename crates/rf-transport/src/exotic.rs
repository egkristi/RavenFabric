//! Steganographic and censorship-resistant transport definitions.
//!
//! These provide type-safe configuration for exotic transport channels
//! that disguise RavenFabric traffic as normal network activity or
//! use unconventional physical channels.

use serde::{Deserialize, Serialize};

/// A steganographic or censorship-resistant transport type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StegTransport {
    /// DNS tunneling — encode frames in DNS queries (TXT/CNAME/NULL records).
    DnsTunnel {
        /// Authoritative DNS domain (e.g., "t.example.com").
        domain: String,
        /// Upstream DNS resolver (or direct to authoritative NS).
        resolver: String,
        /// Encoding: base32, base64, hex.
        encoding: String,
        /// Max data per query (bytes, limited by DNS packet size).
        max_payload: u16,
    },
    /// ICMP tunneling — data in echo request/reply payloads.
    IcmpTunnel {
        /// Target IP that echoes our payloads.
        target: String,
        /// Max payload per ICMP packet.
        max_payload: u16,
        /// Sequence number for ordering.
        initial_seq: u16,
    },
    /// Domain fronting — TLS SNI differs from HTTP Host header.
    DomainFronting {
        /// CDN domain for TLS SNI (e.g., "cdn.googleapis.com").
        sni_domain: String,
        /// Actual target domain in HTTP Host header.
        target_domain: String,
        /// CDN to route through.
        cdn: CdnProvider,
    },
    /// HTTP/3 MASQUE — CONNECT-UDP or CONNECT-IP through HTTP/3.
    Masque {
        /// HTTP/3 proxy endpoint.
        proxy_endpoint: String,
        /// Target to reach through the proxy.
        target: String,
        /// Protocol (connect-udp or connect-ip).
        method: MasqueMethod,
    },
    /// Shadowsocks/Trojan-style protocol mimicry.
    ProtocolMimicry {
        /// Target that accepts the mimicked protocol.
        endpoint: String,
        /// Which protocol to mimic.
        protocol: MimicProtocol,
        /// Pre-shared key for the mimicry layer.
        psk: String,
    },
    /// Tor hidden service (.onion endpoint).
    TorHiddenService {
        /// .onion address to connect to.
        onion_addr: String,
        /// Local SOCKS5 proxy for Tor (usually 127.0.0.1:9050).
        socks_proxy: String,
    },
    /// Encrypted Client Hello (ECH) for WebSocket TLS.
    EncryptedClientHello {
        /// Target WebSocket endpoint.
        ws_endpoint: String,
        /// ECH config (base64 encoded).
        ech_config: String,
    },
}

/// CDN provider for domain fronting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdnProvider {
    Cloudflare,
    Fastly,
    Akamai,
    Azure,
    Aws,
    Gcp,
}

/// MASQUE method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MasqueMethod {
    ConnectUdp,
    ConnectIp,
}

/// Protocol to mimic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MimicProtocol {
    /// Look like HTTPS browsing traffic.
    Https,
    /// Look like Shadowsocks protocol.
    Shadowsocks,
    /// Look like Trojan protocol (TLS + password-based routing).
    Trojan,
    /// Look like WebRTC data channel.
    Webrtc,
}

/// Physical/exotic transport channel definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PhysicalTransport {
    /// Serial port (RS-232/USB).
    Serial {
        /// Device path (e.g., "/dev/ttyUSB0", "COM3").
        device: String,
        /// Baud rate.
        baud_rate: u32,
        /// Data bits (5, 6, 7, 8).
        data_bits: u8,
        /// Parity: none, odd, even.
        parity: String,
    },
    /// Bluetooth/BLE proximity mesh.
    Bluetooth {
        /// Service UUID for discovery.
        service_uuid: String,
        /// Prefer BLE (low energy) over classic.
        prefer_ble: bool,
        /// Maximum connections.
        max_peers: u8,
    },
    /// Wi-Fi Direct ad-hoc.
    WifiDirect {
        /// Group name for discovery.
        group_name: String,
        /// PSK for the group.
        psk: String,
    },
    /// LoRa/Meshtastic sub-GHz radio.
    Lora {
        /// Frequency in Hz (e.g., 915_000_000 for US ISM).
        frequency_hz: u32,
        /// Spreading factor (7-12).
        spreading_factor: u8,
        /// Bandwidth in Hz.
        bandwidth_hz: u32,
        /// Transmit power in dBm.
        tx_power_dbm: i8,
    },
    /// AX.25 packet radio.
    PacketRadio {
        /// Callsign (e.g., "N0CALL-1").
        callsign: String,
        /// TNC device or KISS interface.
        tnc_device: String,
        /// Baud rate for the radio link.
        baud_rate: u32,
    },
    /// Audio modem (data over sound).
    AudioModem {
        /// Modulation scheme.
        modulation: AudioModulation,
        /// Sample rate in Hz.
        sample_rate: u32,
        /// Frequency band start (Hz).
        freq_start: u32,
        /// Frequency band end (Hz).
        freq_end: u32,
    },
    /// QR-stream visual channel.
    QrStream {
        /// Maximum data per QR code (bytes).
        max_per_frame: u16,
        /// Frames per second.
        fps: u8,
        /// Error correction level.
        error_correction: QrErrorCorrection,
    },
    /// Satellite link (Iridium/Starlink).
    Satellite {
        /// Satellite network.
        network: SatelliteNetwork,
        /// Modem device or API endpoint.
        interface: String,
        /// Expected one-way latency (ms).
        latency_ms: u32,
    },
    /// NNCP-style physical media (USB/SD card file transfer).
    PhysicalMedia {
        /// Mount path to watch for incoming bundles.
        mount_path: String,
        /// Output directory for outgoing bundles.
        outbox_path: String,
        /// Polling interval in seconds.
        poll_interval_secs: u32,
    },
}

/// Audio modulation scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioModulation {
    /// Frequency-shift keying.
    Fsk,
    /// Orthogonal frequency-division multiplexing (like chirp/quietnet).
    Ofdm,
    /// Chirp spread spectrum.
    Chirp,
}

/// QR error correction level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum QrErrorCorrection {
    L,
    M,
    Q,
    H,
}

/// Satellite network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SatelliteNetwork {
    Iridium,
    Starlink,
    Inmarsat,
    Globalstar,
}

/// Capabilities and constraints of a transport channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportCapabilities {
    /// Maximum payload per frame/packet (bytes).
    pub max_frame_size: u32,
    /// Expected bandwidth (bytes/sec).
    pub bandwidth_bps: u64,
    /// Expected one-way latency (ms).
    pub latency_ms: u32,
    /// Whether the transport is bidirectional.
    pub bidirectional: bool,
    /// Whether the transport provides ordering guarantees.
    pub ordered: bool,
    /// Whether the transport is reliable (retransmits).
    pub reliable: bool,
    /// Estimated operational cost (0 = free, higher = more expensive).
    pub cost: u8,
    /// Whether this transport is censorship-resistant.
    pub censorship_resistant: bool,
    /// Whether this transport works in air-gapped environments.
    pub air_gap_capable: bool,
}

// --- Signed DNS Records ---

/// DNS record type for relay discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnsRecordType {
    /// SRV record for relay endpoint.
    Srv,
    /// TXT record for agent metadata.
    Txt,
    /// TLSA record for DANE validation.
    Tlsa,
}

/// A signed DNS record for cryptographic relay discovery.
///
/// Agents discover relays and verify their authenticity through
/// DNSSEC-signed SRV/TXT/TLSA records under the ravenfabric domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedDnsRecord {
    /// Domain name (e.g., "_ravenfabric._tcp.example.com").
    pub name: String,
    /// Record type.
    pub record_type: DnsRecordType,
    /// Record data (type-dependent).
    pub data: String,
    /// TTL in seconds.
    pub ttl_secs: u32,
    /// Whether this record was validated via DNSSEC.
    pub dnssec_validated: bool,
    /// DANE TLSA selector (0=full cert, 1=SubjectPublicKeyInfo).
    pub tlsa_selector: Option<u8>,
    /// DANE TLSA matching type (0=exact, 1=SHA-256, 2=SHA-512).
    pub tlsa_matching_type: Option<u8>,
}

/// DNS-based relay discovery — resolves SRV records and validates via DANE.
pub struct DnsRelayDiscovery {
    /// Domain to query for relay SRV records.
    domain: String,
    /// Resolved records (cached).
    records: Vec<SignedDnsRecord>,
    /// Whether to require DNSSEC validation.
    require_dnssec: bool,
}

impl DnsRelayDiscovery {
    /// Create a new DNS relay discovery resolver.
    pub fn new(domain: String, require_dnssec: bool) -> Self {
        Self {
            domain,
            records: Vec::new(),
            require_dnssec,
        }
    }

    /// Add a discovered record.
    pub fn add_record(&mut self, record: SignedDnsRecord) -> bool {
        if self.require_dnssec && !record.dnssec_validated {
            return false; // Reject unsigned records
        }
        self.records.push(record);
        true
    }

    /// Get relay addresses from cached SRV records.
    pub fn relay_addresses(&self) -> Vec<&str> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::Srv)
            .map(|r| r.data.as_str())
            .collect()
    }

    /// Get TLSA records for DANE validation.
    pub fn tlsa_records(&self) -> Vec<&SignedDnsRecord> {
        self.records
            .iter()
            .filter(|r| r.record_type == DnsRecordType::Tlsa)
            .collect()
    }

    /// Domain being queried.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Number of cached records.
    pub fn record_count(&self) -> usize {
        self.records.len()
    }
}

// --- BLE Beacon Discovery ---

/// BLE beacon advertisement for proximity mesh discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleBeacon {
    /// Service UUID used for RavenFabric discovery.
    pub service_uuid: String,
    /// Advertising node ID (truncated public key hash).
    pub node_id_short: [u8; 8],
    /// Signal strength indicator (RSSI, dBm).
    pub rssi: i8,
    /// Whether the beacon is connectable (GATT service available).
    pub connectable: bool,
    /// Capabilities advertised in beacon payload.
    pub capabilities: u8,
}

/// BLE beacon discovery controller.
pub struct BleDiscovery {
    /// Our service UUID.
    service_uuid: String,
    /// Discovered beacons: node_id_short → beacon.
    discovered: std::collections::HashMap<[u8; 8], BleBeacon>,
    /// RSSI threshold for "in range" (dBm, e.g., -80).
    rssi_threshold: i8,
}

impl BleDiscovery {
    /// Create a new BLE discovery controller.
    pub fn new(service_uuid: String, rssi_threshold: i8) -> Self {
        Self {
            service_uuid,
            discovered: std::collections::HashMap::new(),
            rssi_threshold,
        }
    }

    /// Process a discovered beacon.
    /// Returns true if this is a new peer in range.
    pub fn on_beacon(&mut self, beacon: BleBeacon) -> bool {
        if beacon.service_uuid != self.service_uuid {
            return false;
        }
        if beacon.rssi < self.rssi_threshold {
            return false; // Too far away
        }

        let is_new = !self.discovered.contains_key(&beacon.node_id_short);
        self.discovered.insert(beacon.node_id_short, beacon);
        is_new
    }

    /// Get all in-range peers.
    pub fn in_range_peers(&self) -> Vec<&BleBeacon> {
        self.discovered
            .values()
            .filter(|b| b.rssi >= self.rssi_threshold)
            .collect()
    }

    /// Prune out-of-range peers.
    pub fn prune_out_of_range(&mut self) {
        self.discovered.retain(|_, b| b.rssi >= self.rssi_threshold);
    }

    /// Number of discovered peers.
    pub fn peer_count(&self) -> usize {
        self.discovered.len()
    }

    /// Our service UUID.
    pub fn service_uuid(&self) -> &str {
        &self.service_uuid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dns_tunnel_config() {
        let transport = StegTransport::DnsTunnel {
            domain: "t.example.com".into(),
            resolver: "8.8.8.8:53".into(),
            encoding: "base32".into(),
            max_payload: 200,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("dns_tunnel"));
        let parsed: StegTransport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, transport);
    }

    #[test]
    fn test_domain_fronting_config() {
        let transport = StegTransport::DomainFronting {
            sni_domain: "cdn.googleapis.com".into(),
            target_domain: "secret.example.com".into(),
            cdn: CdnProvider::Gcp,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("domain_fronting"));
    }

    #[test]
    fn test_serial_transport() {
        let transport = PhysicalTransport::Serial {
            device: "/dev/ttyUSB0".into(),
            baud_rate: 115200,
            data_bits: 8,
            parity: "none".into(),
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("serial"));
        assert!(json.contains("115200"));
    }

    #[test]
    fn test_lora_transport() {
        let transport = PhysicalTransport::Lora {
            frequency_hz: 915_000_000,
            spreading_factor: 10,
            bandwidth_hz: 125_000,
            tx_power_dbm: 14,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("lora"));
    }

    #[test]
    fn test_audio_modem() {
        let transport = PhysicalTransport::AudioModem {
            modulation: AudioModulation::Ofdm,
            sample_rate: 44100,
            freq_start: 1000,
            freq_end: 8000,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("audio_modem"));
        assert!(json.contains("ofdm"));
    }

    #[test]
    fn test_qr_stream() {
        let transport = PhysicalTransport::QrStream {
            max_per_frame: 1024,
            fps: 15,
            error_correction: QrErrorCorrection::M,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("qr_stream"));
    }

    #[test]
    fn test_physical_media() {
        let transport = PhysicalTransport::PhysicalMedia {
            mount_path: "/mnt/usb".into(),
            outbox_path: "/mnt/usb/outbox".into(),
            poll_interval_secs: 5,
        };
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("physical_media"));
    }

    #[test]
    fn test_capabilities() {
        let caps = TransportCapabilities {
            max_frame_size: 250,
            bandwidth_bps: 300,
            latency_ms: 5000,
            bidirectional: true,
            ordered: false,
            reliable: false,
            cost: 1,
            censorship_resistant: true,
            air_gap_capable: true,
        };
        assert!(caps.censorship_resistant);
        assert!(caps.air_gap_capable);
    }

    #[test]
    fn test_signed_dns_record() {
        let record = SignedDnsRecord {
            name: "_ravenfabric._tcp.example.com".into(),
            record_type: DnsRecordType::Srv,
            data: "0 10 9090 relay.example.com".into(),
            ttl_secs: 300,
            dnssec_validated: true,
            tlsa_selector: None,
            tlsa_matching_type: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("srv"));
        assert!(json.contains("dnssec_validated"));
    }

    #[test]
    fn test_dns_relay_discovery() {
        let mut disc = DnsRelayDiscovery::new("_ravenfabric._tcp.example.com".into(), true);

        // Reject unsigned record
        let unsigned = SignedDnsRecord {
            name: disc.domain().to_string(),
            record_type: DnsRecordType::Srv,
            data: "relay.bad.com:9090".into(),
            ttl_secs: 60,
            dnssec_validated: false,
            tlsa_selector: None,
            tlsa_matching_type: None,
        };
        assert!(!disc.add_record(unsigned));
        assert_eq!(disc.record_count(), 0);

        // Accept signed record
        let signed = SignedDnsRecord {
            name: disc.domain().to_string(),
            record_type: DnsRecordType::Srv,
            data: "relay.good.com:9090".into(),
            ttl_secs: 300,
            dnssec_validated: true,
            tlsa_selector: None,
            tlsa_matching_type: None,
        };
        assert!(disc.add_record(signed));
        assert_eq!(disc.record_count(), 1);
        assert_eq!(disc.relay_addresses(), vec!["relay.good.com:9090"]);
    }

    #[test]
    fn test_dns_discovery_tlsa() {
        let mut disc = DnsRelayDiscovery::new("example.com".into(), false);
        disc.add_record(SignedDnsRecord {
            name: "_443._tcp.relay.example.com".into(),
            record_type: DnsRecordType::Tlsa,
            data: "abcdef1234567890".into(),
            ttl_secs: 3600,
            dnssec_validated: true,
            tlsa_selector: Some(1),
            tlsa_matching_type: Some(1),
        });
        let tlsa = disc.tlsa_records();
        assert_eq!(tlsa.len(), 1);
        assert_eq!(tlsa[0].tlsa_selector, Some(1));
    }

    #[test]
    fn test_ble_beacon_discovery() {
        let uuid = "12345678-1234-1234-1234-123456789abc".to_string();
        let mut disc = BleDiscovery::new(uuid.clone(), -80);

        let beacon = BleBeacon {
            service_uuid: uuid.clone(),
            node_id_short: [1, 2, 3, 4, 5, 6, 7, 8],
            rssi: -65,
            connectable: true,
            capabilities: 0x03,
        };
        assert!(disc.on_beacon(beacon));
        assert_eq!(disc.peer_count(), 1);

        // Same node again — not new
        let beacon2 = BleBeacon {
            service_uuid: uuid.clone(),
            node_id_short: [1, 2, 3, 4, 5, 6, 7, 8],
            rssi: -70,
            connectable: true,
            capabilities: 0x03,
        };
        assert!(!disc.on_beacon(beacon2));
        assert_eq!(disc.peer_count(), 1);
    }

    #[test]
    fn test_ble_beacon_out_of_range() {
        let uuid = "12345678-1234-1234-1234-123456789abc".to_string();
        let mut disc = BleDiscovery::new(uuid.clone(), -80);

        let far_beacon = BleBeacon {
            service_uuid: uuid,
            node_id_short: [1, 2, 3, 4, 5, 6, 7, 8],
            rssi: -95, // Too far
            connectable: true,
            capabilities: 0,
        };
        assert!(!disc.on_beacon(far_beacon));
        assert_eq!(disc.peer_count(), 0);
    }

    #[test]
    fn test_ble_wrong_service_uuid() {
        let mut disc = BleDiscovery::new("our-uuid".into(), -80);

        let beacon = BleBeacon {
            service_uuid: "other-uuid".into(),
            node_id_short: [1, 2, 3, 4, 5, 6, 7, 8],
            rssi: -50,
            connectable: true,
            capabilities: 0,
        };
        assert!(!disc.on_beacon(beacon));
    }
}
