//! Steganographic and censorship-resistant transport definitions.
//!
//! These provide type-safe configuration for exotic transport channels
//! that disguise RavenFabric traffic as normal network activity or
//! use unconventional physical channels.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

// --- DNS Tunnel Encoding ---

/// DNS tunnel encoder/decoder.
///
/// Encodes binary data into DNS-safe labels (base32) and
/// decodes response TXT record payloads.
pub struct DnsTunnelCodec {
    /// Domain suffix for queries.
    domain: String,
    /// Encoding scheme.
    encoding: DnsTunnelEncoding,
    /// Max data per DNS label (63 bytes max, encoding overhead).
    max_label_data: usize,
}

/// DNS tunnel encoding scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsTunnelEncoding {
    /// Base32 (RFC 4648, no padding, case-insensitive).
    Base32,
    /// Hex encoding (lowercase).
    Hex,
}

impl DnsTunnelCodec {
    /// Create a new DNS tunnel codec.
    pub fn new(domain: String, encoding: DnsTunnelEncoding) -> Self {
        let max_label_data = match encoding {
            // 63 char label limit. base32: 5 bits per char → 63 chars = 39 bytes.
            DnsTunnelEncoding::Base32 => 39,
            // hex: 4 bits per char → 63 chars = 31 bytes.
            DnsTunnelEncoding::Hex => 31,
        };
        Self {
            domain,
            encoding,
            max_label_data,
        }
    }

    /// Encode binary data into a DNS query name.
    /// Returns a list of DNS query names (one per fragment).
    pub fn encode_queries(&self, data: &[u8], query_id: u16) -> Vec<String> {
        let mut queries = Vec::new();
        for (i, chunk) in data.chunks(self.max_label_data).enumerate() {
            let encoded = match self.encoding {
                DnsTunnelEncoding::Base32 => base32_encode(chunk),
                DnsTunnelEncoding::Hex => hex_encode(chunk),
            };
            // Format: <encoded>.<seq>.<query_id>.<domain>
            queries.push(format!("{}.{}.{}.{}", encoded, i, query_id, self.domain));
        }
        queries
    }

    /// Decode a DNS TXT record response payload.
    pub fn decode_response(&self, txt_data: &str) -> Option<Vec<u8>> {
        match self.encoding {
            DnsTunnelEncoding::Base32 => base32_decode(txt_data),
            DnsTunnelEncoding::Hex => hex_decode(txt_data),
        }
    }

    /// Number of fragments needed for a payload.
    pub fn fragment_count(&self, data_len: usize) -> usize {
        if data_len == 0 {
            return 0;
        }
        data_len.div_ceil(self.max_label_data)
    }

    /// Max data per fragment.
    pub fn max_fragment_size(&self) -> usize {
        self.max_label_data
    }
}

/// Simple base32 encoder (RFC 4648, no padding, lowercase).
fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            result.push(ALPHABET[((buffer >> bits) & 0x1F) as usize] as char);
        }
    }
    if bits > 0 {
        buffer <<= 5 - bits;
        result.push(ALPHABET[(buffer & 0x1F) as usize] as char);
    }
    result
}

/// Simple base32 decoder (RFC 4648, no padding, case-insensitive).
fn base32_decode(encoded: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer: u64 = 0;
    let mut bits = 0;

    for c in encoded.chars() {
        let val = match c {
            'a'..='z' => c as u64 - 'a' as u64,
            'A'..='Z' => c as u64 - 'A' as u64,
            '2'..='7' => c as u64 - '2' as u64 + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | val;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
        }
    }
    Some(result)
}

/// Simple hex encoder.
fn hex_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    data.iter()
        .fold(String::with_capacity(data.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Simple hex decoder.
fn hex_decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.len() % 2 != 0 {
        return None;
    }
    (0..encoded.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).ok())
        .collect()
}

// --- ICMP Tunnel Framing ---

/// ICMP tunnel frame for embedding data in echo request/reply payloads.
#[derive(Debug, Clone)]
pub struct IcmpFrame {
    /// ICMP type (8 = echo request, 0 = echo reply).
    pub icmp_type: u8,
    /// Identifier (for session multiplexing).
    pub identifier: u16,
    /// Sequence number.
    pub sequence: u16,
    /// Payload data.
    pub payload: Vec<u8>,
}

/// ICMP tunnel framer.
pub struct IcmpTunnelFramer {
    /// Session identifier.
    identifier: u16,
    /// Next sequence number.
    next_seq: u16,
    /// Max payload per ICMP packet (typically limited to ~1400 bytes).
    max_payload: usize,
}

impl IcmpTunnelFramer {
    /// Create a new ICMP tunnel framer.
    pub fn new(identifier: u16, max_payload: usize) -> Self {
        Self {
            identifier,
            next_seq: 0,
            max_payload,
        }
    }

    /// Wrap data into ICMP echo request frames.
    pub fn encode_request(&mut self, data: &[u8]) -> Vec<IcmpFrame> {
        let mut frames = Vec::new();
        for chunk in data.chunks(self.max_payload) {
            frames.push(IcmpFrame {
                icmp_type: 8, // Echo request.
                identifier: self.identifier,
                sequence: self.next_seq,
                payload: chunk.to_vec(),
            });
            self.next_seq = self.next_seq.wrapping_add(1);
        }
        frames
    }

    /// Check if a frame belongs to this session.
    pub fn is_our_frame(&self, frame: &IcmpFrame) -> bool {
        frame.identifier == self.identifier
    }

    /// Serialize an ICMP frame to bytes (simplified — real ICMP has checksum).
    pub fn serialize_frame(frame: &IcmpFrame) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + frame.payload.len());
        buf.push(frame.icmp_type);
        buf.push(0); // Code.
        buf.extend_from_slice(&[0, 0]); // Checksum placeholder.
        buf.extend_from_slice(&frame.identifier.to_be_bytes());
        buf.extend_from_slice(&frame.sequence.to_be_bytes());
        buf.extend_from_slice(&frame.payload);
        buf
    }

    /// Deserialize bytes to an ICMP frame.
    pub fn deserialize_frame(data: &[u8]) -> Option<IcmpFrame> {
        if data.len() < 8 {
            return None;
        }
        Some(IcmpFrame {
            icmp_type: data[0],
            identifier: u16::from_be_bytes([data[4], data[5]]),
            sequence: u16::from_be_bytes([data[6], data[7]]),
            payload: data[8..].to_vec(),
        })
    }
}

// --- Serial Port Framing ---

/// Serial port frame with sync bytes, length, CRC, and escape handling.
///
/// Frame format: [SYNC: 2] [LENGTH: 2] [PAYLOAD: N] [CRC16: 2]
/// SYNC = 0x7E 0x7E
/// LENGTH = big-endian u16 (payload length)
/// CRC16 = CRC-CCITT of payload
#[derive(Debug, Clone)]
pub struct SerialFrame {
    /// Frame payload.
    pub payload: Vec<u8>,
}

/// Serial port framer.
pub struct SerialFramer {
    /// Maximum frame payload size.
    max_payload: usize,
}

impl SerialFramer {
    /// Sync byte pattern.
    pub const SYNC: [u8; 2] = [0x7E, 0x7E];

    /// Create a new serial framer.
    pub fn new(max_payload: usize) -> Self {
        Self { max_payload }
    }

    /// Encode a payload into a serial frame (with sync, length, CRC).
    pub fn encode(&self, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.len() > self.max_payload {
            return None;
        }
        let len = payload.len() as u16;
        let crc = crc16_ccitt(payload);

        let mut frame = Vec::with_capacity(6 + payload.len());
        frame.extend_from_slice(&Self::SYNC);
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&crc.to_be_bytes());
        Some(frame)
    }

    /// Decode a serial frame. Returns the payload if valid.
    pub fn decode(&self, data: &[u8]) -> Option<SerialFrame> {
        if data.len() < 6 {
            return None;
        }
        if data[0..2] != Self::SYNC {
            return None;
        }
        let len = u16::from_be_bytes([data[2], data[3]]) as usize;
        if data.len() < 4 + len + 2 {
            return None;
        }
        let payload = &data[4..4 + len];
        let expected_crc = u16::from_be_bytes([data[4 + len], data[4 + len + 1]]);
        let actual_crc = crc16_ccitt(payload);
        if expected_crc != actual_crc {
            return None; // CRC mismatch.
        }
        Some(SerialFrame {
            payload: payload.to_vec(),
        })
    }

    /// Find frame boundaries in a byte stream (sync word scanning).
    pub fn find_frame_start(data: &[u8]) -> Option<usize> {
        data.windows(2).position(|w| w == Self::SYNC)
    }
}

/// CRC-16/CCITT (polynomial 0x1021, init 0xFFFF).
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// --- Domain Fronting ---

/// Domain fronting request rewriter.
///
/// Rewrites TLS SNI to the CDN domain while setting the HTTP Host
/// header to the real target, bypassing SNI-based censorship.
pub struct DomainFronter {
    /// CDN domain for TLS SNI.
    sni_domain: String,
    /// Real target domain for HTTP Host header.
    target_domain: String,
    /// CDN provider.
    cdn: CdnProvider,
}

impl DomainFronter {
    /// Create a new domain fronter.
    pub fn new(sni_domain: String, target_domain: String, cdn: CdnProvider) -> Self {
        Self {
            sni_domain,
            target_domain,
            cdn,
        }
    }

    /// Rewrite an HTTP request for domain fronting.
    /// Returns (tls_sni, http_host, http_path).
    pub fn rewrite_request(&self, path: &str) -> (String, String, String) {
        (
            self.sni_domain.clone(),
            self.target_domain.clone(),
            path.to_string(),
        )
    }

    /// Generate HTTP CONNECT-style tunneling request.
    pub fn tunnel_request(&self, data: &[u8]) -> Vec<u8> {
        let mut request = format!(
            "POST / HTTP/1.1\r\n\
             Host: {}\r\n\
             Content-Length: {}\r\n\
             Content-Type: application/octet-stream\r\n\
             X-Forwarded-Host: {}\r\n\
             \r\n",
            self.target_domain,
            data.len(),
            self.sni_domain,
        )
        .into_bytes();
        request.extend_from_slice(data);
        request
    }

    /// Parse a tunnel response (extract body from HTTP response).
    pub fn parse_response(data: &[u8]) -> Option<Vec<u8>> {
        let s = std::str::from_utf8(data).ok()?;
        let body_start = s.find("\r\n\r\n")? + 4;
        Some(data[body_start..].to_vec())
    }

    /// SNI domain.
    pub fn sni_domain(&self) -> &str {
        &self.sni_domain
    }

    /// Target domain.
    pub fn target_domain(&self) -> &str {
        &self.target_domain
    }

    /// CDN provider.
    pub fn cdn(&self) -> &CdnProvider {
        &self.cdn
    }
}

// --- Protocol Mimicry (Shadowsocks-style) ---

/// Obfuscated protocol frame.
///
/// Mimics Shadowsocks AEAD framing:
/// [encrypted_length: 2 + 16 tag] [encrypted_payload: N + 16 tag]
///
/// Without actual crypto, this provides the framing structure
/// that real protocol integration would fill with AEAD ciphertext.
#[derive(Debug, Clone)]
pub struct MimicryFrame {
    /// Obfuscated length field (2 bytes + 16 byte tag).
    pub length_block: Vec<u8>,
    /// Obfuscated payload (N bytes + 16 byte tag).
    pub payload_block: Vec<u8>,
}

/// Protocol mimicry encoder/decoder.
pub struct MimicryCodec {
    /// Pre-shared key (for XOR obfuscation in stub; real impl uses AEAD).
    psk: Vec<u8>,
    /// Protocol being mimicked.
    protocol: MimicProtocol,
    /// Counter for nonce derivation.
    counter: u64,
}

impl MimicryCodec {
    /// Tag size for AEAD (16 bytes for Poly1305/GCM).
    const TAG_SIZE: usize = 16;

    /// Create a new mimicry codec.
    pub fn new(psk: Vec<u8>, protocol: MimicProtocol) -> Self {
        Self {
            psk,
            protocol,
            counter: 0,
        }
    }

    /// Encode a payload into an obfuscated frame.
    pub fn encode(&mut self, payload: &[u8]) -> MimicryFrame {
        let len_bytes = (payload.len() as u16).to_be_bytes();
        let mut length_block = Vec::with_capacity(2 + Self::TAG_SIZE);
        // XOR with PSK for length (stub — real impl uses AEAD).
        for (i, &b) in len_bytes.iter().enumerate() {
            length_block.push(b ^ self.psk[i % self.psk.len()]);
        }
        // Fake tag (deterministic from counter).
        for i in 0..Self::TAG_SIZE {
            length_block.push(
                self.psk[(i + 2) % self.psk.len()] ^ (self.counter as u8).wrapping_add(i as u8),
            );
        }

        let mut payload_block = Vec::with_capacity(payload.len() + Self::TAG_SIZE);
        // XOR with PSK for payload (stub).
        for (i, &b) in payload.iter().enumerate() {
            payload_block.push(b ^ self.psk[(i + 4) % self.psk.len()]);
        }
        // Fake tag.
        for i in 0..Self::TAG_SIZE {
            payload_block.push(
                self.psk[(i + 6) % self.psk.len()]
                    ^ (self.counter as u8).wrapping_add(i as u8 + 16),
            );
        }

        self.counter += 1;

        MimicryFrame {
            length_block,
            payload_block,
        }
    }

    /// Decode an obfuscated frame back to plaintext.
    pub fn decode(&self, frame: &MimicryFrame) -> Option<Vec<u8>> {
        if frame.length_block.len() < 2 + Self::TAG_SIZE {
            return None;
        }
        // Decode length.
        let len_bytes: Vec<u8> = frame.length_block[..2]
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.psk[i % self.psk.len()])
            .collect();
        let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;

        if frame.payload_block.len() < len + Self::TAG_SIZE {
            return None;
        }

        // Decode payload.
        let payload: Vec<u8> = frame.payload_block[..len]
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ self.psk[(i + 4) % self.psk.len()])
            .collect();

        Some(payload)
    }

    /// Serialize a frame to bytes (wire format).
    pub fn serialize(frame: &MimicryFrame) -> Vec<u8> {
        let mut out = Vec::with_capacity(frame.length_block.len() + frame.payload_block.len());
        out.extend_from_slice(&frame.length_block);
        out.extend_from_slice(&frame.payload_block);
        out
    }

    /// Protocol being mimicked.
    pub fn protocol(&self) -> &MimicProtocol {
        &self.protocol
    }

    /// Traffic statistics for fingerprint resistance.
    pub fn stats(&self) -> HashMap<String, u64> {
        let mut m = HashMap::new();
        m.insert("frames_sent".into(), self.counter);
        m.insert(
            "protocol".into(),
            match self.protocol {
                MimicProtocol::Https => 1,
                MimicProtocol::Shadowsocks => 2,
                MimicProtocol::Trojan => 3,
                MimicProtocol::Webrtc => 4,
            },
        );
        m
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

    // --- DNS Tunnel Tests ---

    #[test]
    fn test_dns_tunnel_base32_roundtrip() {
        let data = b"Hello, RavenFabric!";
        let encoded = base32_encode(data);
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(&decoded[..data.len()], data);
    }

    #[test]
    fn test_dns_tunnel_hex_roundtrip() {
        let data = b"\x00\xFF\x42\xAB";
        let encoded = hex_encode(data);
        assert_eq!(encoded, "00ff42ab");
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_dns_tunnel_encode_queries() {
        let codec = DnsTunnelCodec::new("t.example.com".into(), DnsTunnelEncoding::Hex);
        let data = vec![0xAA; 10];
        let queries = codec.encode_queries(&data, 42);
        assert_eq!(queries.len(), 1); // 10 bytes < 31 max.
        assert!(queries[0].ends_with(".t.example.com"));
        assert!(queries[0].contains(".0.42.")); // seq.query_id
    }

    #[test]
    fn test_dns_tunnel_fragmentation() {
        let codec = DnsTunnelCodec::new("t.example.com".into(), DnsTunnelEncoding::Hex);
        let data = vec![0xFF; 100];
        let count = codec.fragment_count(100);
        assert_eq!(count, 4); // ceil(100/31) = 4
        let queries = codec.encode_queries(&data, 1);
        assert_eq!(queries.len(), 4);
    }

    #[test]
    fn test_dns_tunnel_decode_response() {
        let codec = DnsTunnelCodec::new("t.example.com".into(), DnsTunnelEncoding::Hex);
        let decoded = codec.decode_response("48656c6c6f").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    // --- ICMP Tunnel Tests ---

    #[test]
    fn test_icmp_frame_encode_decode() {
        let mut framer = IcmpTunnelFramer::new(0x1234, 1400);
        let data = b"test payload";
        let frames = framer.encode_request(data);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].identifier, 0x1234);
        assert_eq!(frames[0].sequence, 0);
        assert_eq!(frames[0].payload, data);
        assert!(framer.is_our_frame(&frames[0]));
    }

    #[test]
    fn test_icmp_serialize_deserialize() {
        let frame = IcmpFrame {
            icmp_type: 8,
            identifier: 0xABCD,
            sequence: 42,
            payload: vec![1, 2, 3, 4],
        };
        let bytes = IcmpTunnelFramer::serialize_frame(&frame);
        let decoded = IcmpTunnelFramer::deserialize_frame(&bytes).unwrap();
        assert_eq!(decoded.icmp_type, 8);
        assert_eq!(decoded.identifier, 0xABCD);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_icmp_fragmentation() {
        let mut framer = IcmpTunnelFramer::new(1, 10);
        let data = vec![0u8; 25];
        let frames = framer.encode_request(&data);
        assert_eq!(frames.len(), 3); // 10 + 10 + 5
        assert_eq!(frames[0].sequence, 0);
        assert_eq!(frames[1].sequence, 1);
        assert_eq!(frames[2].sequence, 2);
    }

    // --- Serial Frame Tests ---

    #[test]
    fn test_serial_frame_roundtrip() {
        let framer = SerialFramer::new(1024);
        let payload = b"RavenFabric serial data";
        let encoded = framer.encode(payload).unwrap();
        let decoded = framer.decode(&encoded).unwrap();
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_serial_frame_crc_check() {
        let framer = SerialFramer::new(1024);
        let mut encoded = framer.encode(b"data").unwrap();
        // Corrupt the payload.
        encoded[4] ^= 0xFF;
        assert!(framer.decode(&encoded).is_none()); // CRC mismatch.
    }

    #[test]
    fn test_serial_frame_too_large() {
        let framer = SerialFramer::new(10);
        assert!(framer.encode(&[0u8; 11]).is_none());
    }

    #[test]
    fn test_serial_find_frame_start() {
        let data = [0x00, 0x00, 0x7E, 0x7E, 0x01, 0x02];
        assert_eq!(SerialFramer::find_frame_start(&data), Some(2));
    }

    #[test]
    fn test_crc16_ccitt() {
        let crc = crc16_ccitt(b"123456789");
        assert_eq!(crc, 0x29B1); // Known CRC-CCITT for "123456789".
    }

    // --- Domain Fronting Tests ---

    #[test]
    fn test_domain_fronting_rewrite() {
        let fronter = DomainFronter::new(
            "cdn.googleapis.com".into(),
            "secret.example.com".into(),
            CdnProvider::Gcp,
        );
        let (sni, host, path) = fronter.rewrite_request("/api/v1/data");
        assert_eq!(sni, "cdn.googleapis.com");
        assert_eq!(host, "secret.example.com");
        assert_eq!(path, "/api/v1/data");
    }

    #[test]
    fn test_domain_fronting_tunnel_request() {
        let fronter = DomainFronter::new(
            "cdn.example.com".into(),
            "target.example.com".into(),
            CdnProvider::Cloudflare,
        );
        let request = fronter.tunnel_request(b"payload");
        let s = String::from_utf8_lossy(&request);
        assert!(s.contains("Host: target.example.com"));
        assert!(s.contains("Content-Length: 7"));
        assert!(s.contains("payload"));
    }

    #[test]
    fn test_domain_fronting_parse_response() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let body = DomainFronter::parse_response(response).unwrap();
        assert_eq!(body, b"hello");
    }

    // --- Protocol Mimicry Tests ---

    #[test]
    fn test_mimicry_encode_decode() {
        let psk = vec![0x42u8; 32];
        let mut codec = MimicryCodec::new(psk.clone(), MimicProtocol::Shadowsocks);
        let plaintext = b"secret message";
        let frame = codec.encode(plaintext);

        let codec2 = MimicryCodec::new(psk, MimicProtocol::Shadowsocks);
        let decoded = codec2.decode(&frame).unwrap();
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn test_mimicry_frame_structure() {
        let mut codec = MimicryCodec::new(vec![0x01; 32], MimicProtocol::Https);
        let frame = codec.encode(b"data");
        // Length block: 2 bytes + 16 tag = 18.
        assert_eq!(frame.length_block.len(), 18);
        // Payload block: 4 bytes + 16 tag = 20.
        assert_eq!(frame.payload_block.len(), 20);
    }

    #[test]
    fn test_mimicry_stats() {
        let mut codec = MimicryCodec::new(vec![0x01; 32], MimicProtocol::Shadowsocks);
        codec.encode(b"a");
        codec.encode(b"b");
        let stats = codec.stats();
        assert_eq!(stats["frames_sent"], 2);
    }

    #[test]
    fn test_mimicry_serialize() {
        let mut codec = MimicryCodec::new(vec![0xFF; 32], MimicProtocol::Trojan);
        let frame = codec.encode(b"test");
        let bytes = MimicryCodec::serialize(&frame);
        assert_eq!(bytes.len(), 18 + 20); // length_block + payload_block
    }
}
