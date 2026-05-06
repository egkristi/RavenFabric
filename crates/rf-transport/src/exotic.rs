//! Steganographic and censorship-resistant transport definitions.
//!
//! These provide type-safe configuration for exotic transport channels
//! that disguise RavenFabric traffic as normal network activity or
//! use unconventional physical channels.

use rand::RngCore;
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
/// Uses ChaCha20-Poly1305 AEAD with counter-derived nonces for
/// authenticated encryption matching Shadowsocks wire format.
#[derive(Debug, Clone)]
pub struct MimicryFrame {
    /// AEAD-encrypted length field (2 bytes + 16 byte tag).
    pub length_block: Vec<u8>,
    /// AEAD-encrypted payload (N bytes + 16 byte tag).
    pub payload_block: Vec<u8>,
}

/// Protocol mimicry encoder/decoder using ChaCha20-Poly1305 AEAD.
pub struct MimicryCodec {
    /// 32-byte pre-shared key for ChaCha20-Poly1305.
    psk: Vec<u8>,
    /// Protocol being mimicked.
    protocol: MimicProtocol,
    /// Counter for nonce derivation (monotonically increasing).
    counter: u64,
}

impl MimicryCodec {
    /// Tag size for AEAD (16 bytes for Poly1305).
    const TAG_SIZE: usize = 16;

    /// Create a new mimicry codec.
    pub fn new(psk: Vec<u8>, protocol: MimicProtocol) -> Self {
        Self {
            psk,
            protocol,
            counter: 0,
        }
    }

    /// Derive a 12-byte nonce from counter and a sub-key index.
    fn derive_nonce(counter: u64, sub: u8) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[0] = sub;
        // Padding byte at [1..3] stays zero.
        nonce[4..12].copy_from_slice(&counter.to_be_bytes());
        nonce
    }

    /// Encode a payload into an AEAD-encrypted frame.
    pub fn encode(&mut self, payload: &[u8]) -> MimicryFrame {
        use chacha20poly1305::aead::Aead;
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

        let key = chacha20poly1305::Key::from_slice(&self.psk[..32]);
        let cipher = ChaCha20Poly1305::new(key);

        // Encrypt length (2 bytes).
        let len_bytes = (payload.len() as u16).to_be_bytes();
        let len_nonce = Self::derive_nonce(self.counter, 0);
        let length_block = cipher
            .encrypt(Nonce::from_slice(&len_nonce), len_bytes.as_ref())
            .expect("length encryption should not fail");

        // Encrypt payload.
        let payload_nonce = Self::derive_nonce(self.counter, 1);
        let payload_block = cipher
            .encrypt(Nonce::from_slice(&payload_nonce), payload)
            .expect("payload encryption should not fail");

        self.counter += 1;

        MimicryFrame {
            length_block,
            payload_block,
        }
    }

    /// Decode an AEAD-encrypted frame back to plaintext.
    /// Returns None if authentication fails (tampered frame).
    pub fn decode(&self, frame: &MimicryFrame) -> Option<Vec<u8>> {
        self.decode_at(frame, self.counter.saturating_sub(1))
    }

    /// Decode a frame using a specific counter value (for out-of-order).
    pub fn decode_at(&self, frame: &MimicryFrame, counter: u64) -> Option<Vec<u8>> {
        use chacha20poly1305::aead::Aead;
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};

        if frame.length_block.len() < 2 + Self::TAG_SIZE {
            return None;
        }

        let key = chacha20poly1305::Key::from_slice(&self.psk[..32]);
        let cipher = ChaCha20Poly1305::new(key);

        // Decrypt length.
        let len_nonce = Self::derive_nonce(counter, 0);
        let len_bytes = cipher
            .decrypt(Nonce::from_slice(&len_nonce), frame.length_block.as_ref())
            .ok()?;
        let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;

        if frame.payload_block.len() < len + Self::TAG_SIZE {
            return None;
        }

        // Decrypt payload.
        let payload_nonce = Self::derive_nonce(counter, 1);
        let payload = cipher
            .decrypt(
                Nonce::from_slice(&payload_nonce),
                frame.payload_block.as_ref(),
            )
            .ok()?;

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

// --- HTTP/3 MASQUE Transport ---

/// HTTP/3 MASQUE proxy transport (RFC 9298 CONNECT-UDP / RFC 9484 CONNECT-IP).
///
/// Tunnels RavenFabric traffic through an HTTP/3 proxy using CONNECT-UDP
/// or CONNECT-IP methods, making it look like standard HTTP/3 proxy traffic.
pub struct MasqueTransport {
    /// HTTP/3 proxy endpoint URL.
    proxy_endpoint: String,
    /// Target host:port to reach through the proxy.
    target: String,
    /// MASQUE method (CONNECT-UDP or CONNECT-IP).
    method: MasqueMethod,
    /// Session ID for multiplexing.
    session_id: u64,
    /// Frames sent counter.
    frames_sent: u64,
    /// Frames received counter.
    frames_received: u64,
}

/// A MASQUE capsule (RFC 9297 HTTP Datagrams).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasqueCapsule {
    /// Capsule type (0x00 = DATAGRAM, 0x01 = CLOSE).
    pub capsule_type: MasqueCapsuleType,
    /// Session context ID (quarter-stream ID).
    pub context_id: u64,
    /// Payload data.
    pub payload: Vec<u8>,
}

/// MASQUE capsule types per RFC 9297.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasqueCapsuleType {
    /// DATAGRAM capsule — carries tunneled data.
    Datagram,
    /// CLOSE capsule — signals session teardown.
    Close,
    /// ADDRESS_ASSIGN — assign address (CONNECT-IP).
    AddressAssign,
    /// ADDRESS_REQUEST — request address (CONNECT-IP).
    AddressRequest,
    /// ROUTE_ADVERTISEMENT — advertise routes (CONNECT-IP).
    RouteAdvertisement,
}

impl MasqueCapsuleType {
    /// Wire type ID.
    pub fn type_id(self) -> u64 {
        match self {
            Self::Datagram => 0x00,
            Self::Close => 0x01,
            Self::AddressAssign => 0x01,
            Self::AddressRequest => 0x02,
            Self::RouteAdvertisement => 0x03,
        }
    }

    /// Parse from type ID.
    pub fn from_type_id(id: u64, method: &MasqueMethod) -> Option<Self> {
        match (method, id) {
            (MasqueMethod::ConnectUdp, 0x00) => Some(Self::Datagram),
            (MasqueMethod::ConnectUdp, 0x01) => Some(Self::Close),
            (MasqueMethod::ConnectIp, 0x00) => Some(Self::Datagram),
            (MasqueMethod::ConnectIp, 0x01) => Some(Self::AddressAssign),
            (MasqueMethod::ConnectIp, 0x02) => Some(Self::AddressRequest),
            (MasqueMethod::ConnectIp, 0x03) => Some(Self::RouteAdvertisement),
            _ => None,
        }
    }
}

impl MasqueTransport {
    /// Create a new MASQUE transport.
    pub fn new(proxy_endpoint: String, target: String, method: MasqueMethod) -> Self {
        Self {
            proxy_endpoint,
            target,
            method,
            session_id: rand::random::<u64>() & 0x3FFF_FFFF_FFFF_FFFF, // 62-bit
            frames_sent: 0,
            frames_received: 0,
        }
    }

    /// Generate an HTTP/3 CONNECT request for the MASQUE session.
    ///
    /// Returns the serialized HTTP/3 CONNECT headers as bytes.
    pub fn connect_request(&self) -> Vec<u8> {
        let method_str = match self.method {
            MasqueMethod::ConnectUdp => "connect-udp",
            MasqueMethod::ConnectIp => "connect-ip",
        };

        // HTTP/3 extended CONNECT pseudo-headers (RFC 9220)
        let request = format!(
            ":method: CONNECT\r\n\
             :protocol: {method_str}\r\n\
             :authority: {proxy}\r\n\
             :path: /.well-known/masque/{method_str}/{target}/\r\n\
             capsule-protocol: ?1\r\n\
             \r\n",
            proxy = self.proxy_endpoint,
            target = self.target,
        );
        request.into_bytes()
    }

    /// Parse a CONNECT response. Returns true if the proxy accepted.
    pub fn parse_connect_response(data: &[u8]) -> bool {
        if let Ok(s) = std::str::from_utf8(data) {
            // HTTP/3 returns status via :status pseudo-header
            // HTTP/1.1 returns "HTTP/1.1 200"
            s.contains(":status: 200") || s.contains("200")
        } else {
            false
        }
    }

    /// Encode a data payload into a MASQUE capsule (RFC 9297).
    ///
    /// Format: [context_id: varint] [payload]
    /// The capsule itself is framed by HTTP/3 DATA frames.
    pub fn encode_capsule(&mut self, data: &[u8]) -> MasqueCapsule {
        self.frames_sent += 1;
        MasqueCapsule {
            capsule_type: MasqueCapsuleType::Datagram,
            context_id: 0, // Default context
            payload: data.to_vec(),
        }
    }

    /// Encode a close capsule to signal session teardown.
    pub fn encode_close(&mut self) -> MasqueCapsule {
        self.frames_sent += 1;
        MasqueCapsule {
            capsule_type: MasqueCapsuleType::Close,
            context_id: 0,
            payload: Vec::new(),
        }
    }

    /// Serialize a capsule to wire format.
    ///
    /// Wire format: [capsule_type: varint][length: varint][context_id: varint][payload]
    pub fn serialize_capsule(capsule: &MasqueCapsule) -> Vec<u8> {
        let mut out = Vec::new();
        // Capsule type (varint)
        encode_varint(capsule.capsule_type.type_id(), &mut out);
        // Capsule length (context_id varint + payload)
        let context_len = varint_len(capsule.context_id);
        let total_len = context_len + capsule.payload.len();
        encode_varint(total_len as u64, &mut out);
        // Context ID (varint)
        encode_varint(capsule.context_id, &mut out);
        // Payload
        out.extend_from_slice(&capsule.payload);
        out
    }

    /// Deserialize a capsule from wire format.
    pub fn deserialize_capsule(&mut self, data: &[u8]) -> Option<MasqueCapsule> {
        let mut offset = 0;

        // Capsule type
        let (type_id, consumed) = decode_varint(&data[offset..])?;
        offset += consumed;

        // Length
        let (length, consumed) = decode_varint(&data[offset..])?;
        offset += consumed;

        if data.len() < offset + length as usize {
            return None; // Incomplete
        }

        // Context ID
        let (context_id, consumed) = decode_varint(&data[offset..])?;
        offset += consumed;

        // Payload (rest up to length)
        let payload_len = length as usize - varint_len(context_id);
        if data.len() < offset + payload_len {
            return None;
        }
        let payload = data[offset..offset + payload_len].to_vec();

        let capsule_type = MasqueCapsuleType::from_type_id(type_id, &self.method)?;

        self.frames_received += 1;

        Some(MasqueCapsule {
            capsule_type,
            context_id,
            payload,
        })
    }

    /// Session ID.
    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    /// Proxy endpoint.
    pub fn proxy_endpoint(&self) -> &str {
        &self.proxy_endpoint
    }

    /// Target.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Method.
    pub fn method(&self) -> &MasqueMethod {
        &self.method
    }

    /// Frames sent.
    pub fn frames_sent(&self) -> u64 {
        self.frames_sent
    }

    /// Frames received.
    pub fn frames_received(&self) -> u64 {
        self.frames_received
    }
}

// --- Encrypted Client Hello (ECH) ---

/// Encrypted Client Hello configuration and handler (RFC 9460 / draft-ietf-tls-esni).
///
/// ECH encrypts the ClientHello's SNI extension so that network observers
/// cannot determine the true destination server. This defeats SNI-based
/// censorship and traffic analysis.
pub struct EchTransport {
    /// Target WebSocket endpoint (the real server).
    target_endpoint: String,
    /// Public-facing server name (the outer SNI, e.g., "cloudflare-ech.com").
    public_name: String,
    /// ECH config list (base64-encoded, from DNS HTTPS record).
    ech_config_list: String,
    /// HPKE cipher suite for ECH encryption.
    cipher_suite: EchCipherSuite,
    /// Whether GREASE ECH should be used as fallback when config is unavailable.
    grease_on_failure: bool,
}

/// HPKE cipher suite selection for ECH.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EchCipherSuite {
    /// X25519 + HKDF-SHA256 + AES-128-GCM (recommended).
    X25519HkdfSha256Aes128Gcm,
    /// X25519 + HKDF-SHA256 + ChaCha20-Poly1305.
    X25519HkdfSha256ChaCha20,
}

/// Parsed ECH config entry (from HTTPS DNS record or well-known URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EchConfig {
    /// Config version (0xFE0D for draft-13+).
    pub version: u16,
    /// Config ID (unique per server rotation).
    pub config_id: u8,
    /// HPKE KEM ID (0x0020 = DHKEM X25519).
    pub kem_id: u16,
    /// HPKE public key (raw bytes).
    pub public_key: Vec<u8>,
    /// Public name (outer SNI).
    pub public_name: String,
    /// Maximum name length (for padding).
    pub max_name_len: u8,
    /// Supported cipher suites.
    pub cipher_suites: Vec<(u16, u16)>, // (KDF ID, AEAD ID)
}

/// Result of ECH config parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EchConfigParseResult {
    /// Successfully parsed config list.
    Success(Vec<EchConfig>),
    /// Config format not recognized (use GREASE).
    UnknownVersion,
    /// Invalid config data.
    Invalid(String),
}

impl EchTransport {
    /// Create a new ECH transport.
    pub fn new(
        target_endpoint: String,
        public_name: String,
        ech_config_list: String,
        cipher_suite: EchCipherSuite,
    ) -> Self {
        Self {
            target_endpoint,
            public_name,
            ech_config_list,
            cipher_suite,
            grease_on_failure: true,
        }
    }

    /// Disable GREASE fallback (fail hard if ECH config is invalid).
    pub fn disable_grease_fallback(&mut self) {
        self.grease_on_failure = false;
    }

    /// Parse the base64-encoded ECH config list.
    ///
    /// ECH config list format:
    /// [total_length: 2][config1][config2]...
    /// Each config: [version: 2][length: 2][contents...]
    pub fn parse_config_list(config_b64: &str) -> EchConfigParseResult {
        let data = match base64_decode(config_b64) {
            Some(d) => d,
            None => return EchConfigParseResult::Invalid("invalid base64".into()),
        };

        if data.len() < 2 {
            return EchConfigParseResult::Invalid("too short".into());
        }

        let total_len = u16::from_be_bytes([data[0], data[1]]) as usize;
        if data.len() < 2 + total_len {
            return EchConfigParseResult::Invalid("length mismatch".into());
        }

        let mut configs = Vec::new();
        let mut offset = 2;

        while offset < 2 + total_len {
            if offset + 4 > data.len() {
                break;
            }
            let version = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let config_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;

            if version != 0xFE0D {
                // Unknown version — skip this config
                offset += config_len;
                continue;
            }

            if offset + config_len > data.len() {
                return EchConfigParseResult::Invalid("config truncated".into());
            }

            let config_data = &data[offset..offset + config_len];
            if let Some(config) = Self::parse_single_config(config_data) {
                configs.push(config);
            }
            offset += config_len;
        }

        if configs.is_empty() {
            EchConfigParseResult::UnknownVersion
        } else {
            EchConfigParseResult::Success(configs)
        }
    }

    /// Parse a single ECH config contents.
    fn parse_single_config(data: &[u8]) -> Option<EchConfig> {
        if data.len() < 7 {
            return None;
        }

        let mut offset = 0;

        // Config ID (1 byte)
        let config_id = data[offset];
        offset += 1;

        // KEM ID (2 bytes)
        let kem_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        // Public key length (2 bytes) + public key
        let pk_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        if offset + pk_len > data.len() {
            return None;
        }
        let public_key = data[offset..offset + pk_len].to_vec();
        offset += pk_len;

        // Cipher suites length (2 bytes)
        if offset + 2 > data.len() {
            return None;
        }
        let suites_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        let mut cipher_suites = Vec::new();
        let suites_end = offset + suites_len;
        while offset + 4 <= suites_end && offset + 4 <= data.len() {
            let kdf_id = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let aead_id = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
            cipher_suites.push((kdf_id, aead_id));
            offset += 4;
        }
        offset = suites_end.min(data.len());

        // Max name length (1 byte)
        if offset >= data.len() {
            return None;
        }
        let max_name_len = data[offset];
        offset += 1;

        // Public name length (1 byte) + public name
        if offset >= data.len() {
            return None;
        }
        let name_len = data[offset] as usize;
        offset += 1;
        if offset + name_len > data.len() {
            return None;
        }
        let public_name = String::from_utf8_lossy(&data[offset..offset + name_len]).into_owned();

        Some(EchConfig {
            version: 0xFE0D,
            config_id,
            kem_id,
            public_key,
            public_name,
            max_name_len,
            cipher_suites,
        })
    }

    /// Generate a GREASE ECH extension (random, indistinguishable from real ECH).
    ///
    /// Used when no valid ECH config is available to maintain uniformity
    /// of traffic patterns — all clients send ECH-like extensions.
    pub fn generate_grease_ech() -> Vec<u8> {
        let mut rng = rand::rng();
        let mut grease = vec![0u8; 128]; // Typical ECH payload size
        rng.fill_bytes(&mut grease);
        // Set GREASE cipher suite indicator
        grease[0] = 0xDA;
        grease[1] = 0x0A; // GREASE value
        grease
    }

    /// Build the ClientHelloOuter SNI value.
    pub fn outer_sni(&self) -> &str {
        &self.public_name
    }

    /// Target endpoint.
    pub fn target_endpoint(&self) -> &str {
        &self.target_endpoint
    }

    /// ECH config (base64).
    pub fn ech_config_list(&self) -> &str {
        &self.ech_config_list
    }

    /// Cipher suite.
    pub fn cipher_suite(&self) -> EchCipherSuite {
        self.cipher_suite
    }

    /// Whether GREASE is used on config failure.
    pub fn grease_on_failure(&self) -> bool {
        self.grease_on_failure
    }
}

/// Simple base64 decoder (no external dependency needed).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Option<u8> {
        TABLE.iter().position(|&x| x == c).map(|p| p as u8)
    }

    let input = input.trim().as_bytes();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input {
        if byte == b'=' {
            break;
        }
        if byte == b'\n' || byte == b'\r' || byte == b' ' {
            continue;
        }
        let val = decode_char(byte)?;
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Encode a variable-length integer (QUIC varint encoding, RFC 9000 Section 16).
fn encode_varint(value: u64, buf: &mut Vec<u8>) {
    if value < 64 {
        buf.push(value as u8);
    } else if value < 16384 {
        buf.push(0x40 | (value >> 8) as u8);
        buf.push(value as u8);
    } else if value < 1_073_741_824 {
        buf.push(0x80 | (value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    } else {
        buf.push(0xC0 | (value >> 56) as u8);
        buf.push((value >> 48) as u8);
        buf.push((value >> 40) as u8);
        buf.push((value >> 32) as u8);
        buf.push((value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    }
}

/// Decode a QUIC varint. Returns (value, bytes_consumed).
fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    let len = 1 << (first >> 6);
    if data.len() < len {
        return None;
    }
    let value = match len {
        1 => u64::from(first & 0x3F),
        2 => {
            let v = u16::from_be_bytes([first & 0x3F, data[1]]);
            u64::from(v)
        }
        4 => {
            let v = u32::from_be_bytes([first & 0x3F, data[1], data[2], data[3]]);
            u64::from(v)
        }
        8 => u64::from_be_bytes([
            first & 0x3F,
            data[1],
            data[2],
            data[3],
            data[4],
            data[5],
            data[6],
            data[7],
        ]),
        _ => return None,
    };
    Some((value, len))
}

/// Length of a varint encoding.
fn varint_len(value: u64) -> usize {
    if value < 64 {
        1
    } else if value < 16384 {
        2
    } else if value < 1_073_741_824 {
        4
    } else {
        8
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

    // --- MASQUE transport tests ---

    #[test]
    fn test_masque_connect_request_udp() {
        let transport = MasqueTransport::new(
            "proxy.example.com".into(),
            "target.example.com:443".into(),
            MasqueMethod::ConnectUdp,
        );
        let req = transport.connect_request();
        let req_str = String::from_utf8(req).unwrap();
        assert!(req_str.contains(":method: CONNECT"));
        assert!(req_str.contains(":protocol: connect-udp"));
        assert!(req_str.contains("capsule-protocol: ?1"));
        assert!(req_str.contains("target.example.com:443"));
    }

    #[test]
    fn test_masque_connect_request_ip() {
        let transport = MasqueTransport::new(
            "proxy.example.com".into(),
            "10.0.0.1".into(),
            MasqueMethod::ConnectIp,
        );
        let req = transport.connect_request();
        let req_str = String::from_utf8(req).unwrap();
        assert!(req_str.contains(":protocol: connect-ip"));
        assert!(req_str.contains("/.well-known/masque/connect-ip/10.0.0.1/"));
    }

    #[test]
    fn test_masque_parse_connect_response() {
        assert!(MasqueTransport::parse_connect_response(b":status: 200\r\n"));
        assert!(MasqueTransport::parse_connect_response(
            b"HTTP/1.1 200 OK\r\n"
        ));
        assert!(!MasqueTransport::parse_connect_response(
            b":status: 403\r\n"
        ));
    }

    #[test]
    fn test_masque_capsule_roundtrip() {
        let mut transport = MasqueTransport::new(
            "proxy.example.com".into(),
            "target:443".into(),
            MasqueMethod::ConnectUdp,
        );
        let payload = b"hello masque";
        let capsule = transport.encode_capsule(payload);
        assert_eq!(capsule.capsule_type, MasqueCapsuleType::Datagram);
        assert_eq!(capsule.payload, payload);

        let serialized = MasqueTransport::serialize_capsule(&capsule);
        let deserialized = transport.deserialize_capsule(&serialized).unwrap();
        assert_eq!(deserialized.capsule_type, MasqueCapsuleType::Datagram);
        assert_eq!(deserialized.context_id, 0);
        assert_eq!(deserialized.payload, payload);
    }

    #[test]
    fn test_masque_close_capsule() {
        let mut transport = MasqueTransport::new(
            "proxy.example.com".into(),
            "target:443".into(),
            MasqueMethod::ConnectUdp,
        );
        let capsule = transport.encode_close();
        assert_eq!(capsule.capsule_type, MasqueCapsuleType::Close);
        assert!(capsule.payload.is_empty());

        let serialized = MasqueTransport::serialize_capsule(&capsule);
        let deserialized = transport.deserialize_capsule(&serialized).unwrap();
        assert_eq!(deserialized.capsule_type, MasqueCapsuleType::Close);
    }

    #[test]
    fn test_masque_frame_counters() {
        let mut transport = MasqueTransport::new(
            "p.example.com".into(),
            "t:443".into(),
            MasqueMethod::ConnectUdp,
        );
        assert_eq!(transport.frames_sent(), 0);
        assert_eq!(transport.frames_received(), 0);

        transport.encode_capsule(b"a");
        transport.encode_capsule(b"b");
        assert_eq!(transport.frames_sent(), 2);

        let capsule = transport.encode_capsule(b"c");
        let serialized = MasqueTransport::serialize_capsule(&capsule);
        transport.deserialize_capsule(&serialized);
        assert_eq!(transport.frames_sent(), 3);
        assert_eq!(transport.frames_received(), 1);
    }

    #[test]
    fn test_varint_encoding_roundtrip() {
        for value in [0u64, 1, 63, 64, 16383, 16384, 1_073_741_823, 1_073_741_824] {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);
            let (decoded, len) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, value);
            assert_eq!(len, buf.len());
        }
    }

    #[test]
    fn test_masque_connect_ip_capsule_types() {
        let mut transport = MasqueTransport::new(
            "proxy.example.com".into(),
            "10.0.0.0/24".into(),
            MasqueMethod::ConnectIp,
        );

        // Address assign capsule
        let capsule = MasqueCapsule {
            capsule_type: MasqueCapsuleType::AddressAssign,
            context_id: 0,
            payload: vec![0x04, 10, 0, 0, 1, 24], // IPv4 10.0.0.1/24
        };
        let serialized = MasqueTransport::serialize_capsule(&capsule);
        let deserialized = transport.deserialize_capsule(&serialized).unwrap();
        assert_eq!(deserialized.capsule_type, MasqueCapsuleType::AddressAssign);
        assert_eq!(deserialized.payload, vec![0x04, 10, 0, 0, 1, 24]);
    }

    // --- ECH transport tests ---

    #[test]
    fn test_ech_transport_creation() {
        let ech = EchTransport::new(
            "wss://target.example.com".into(),
            "cloudflare-ech.com".into(),
            "AAAAAA==".into(),
            EchCipherSuite::X25519HkdfSha256Aes128Gcm,
        );
        assert_eq!(ech.outer_sni(), "cloudflare-ech.com");
        assert_eq!(ech.target_endpoint(), "wss://target.example.com");
        assert!(ech.grease_on_failure());
    }

    #[test]
    fn test_ech_grease_generation() {
        let grease = EchTransport::generate_grease_ech();
        assert_eq!(grease.len(), 128);
        assert_eq!(grease[0], 0xDA);
        assert_eq!(grease[1], 0x0A);
    }

    #[test]
    fn test_ech_config_parse_too_short() {
        let result = EchTransport::parse_config_list("AA==");
        assert!(matches!(result, EchConfigParseResult::Invalid(_)));
    }

    #[test]
    fn test_ech_config_parse_valid() {
        // Construct a minimal valid ECH config list:
        // [config_id: 1][kem_id: 0x0020][pk_len: 32][pk: 32 bytes]
        // [suites_len: 4][kdf+aead][max_name_len][name_len][name]
        let mut config = Vec::new();
        config.push(0x01); // config_id
        config.extend_from_slice(&[0x00, 0x20]); // kem_id X25519
        config.extend_from_slice(&[0x00, 0x20]); // pk length 32
        config.extend_from_slice(&[0xAA; 32]); // public key
        config.extend_from_slice(&[0x00, 0x04]); // suites length
        config.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // HKDF-SHA256 + AES-128-GCM
        config.push(64); // max_name_len
        config.push(0x01); // name_len
        config.push(b'e'); // name

        // Wrap in config entry: [version: 0xFE0D][length]
        let mut entry = Vec::new();
        entry.extend_from_slice(&[0xFE, 0x0D]);
        entry.extend_from_slice(&(config.len() as u16).to_be_bytes());
        entry.extend_from_slice(&config);

        // Wrap in config list: [total_len]
        let mut list = Vec::new();
        list.extend_from_slice(&(entry.len() as u16).to_be_bytes());
        list.extend_from_slice(&entry);

        let b64 = base64_encode_test(&list);
        let result = EchTransport::parse_config_list(&b64);
        match result {
            EchConfigParseResult::Success(configs) => {
                assert_eq!(configs.len(), 1);
                assert_eq!(configs[0].version, 0xFE0D);
                assert_eq!(configs[0].config_id, 0x01);
                assert_eq!(configs[0].kem_id, 0x0020);
                assert_eq!(configs[0].public_key.len(), 32);
                assert_eq!(configs[0].cipher_suites, vec![(0x0001, 0x0001)]);
                assert_eq!(configs[0].public_name, "e");
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    #[test]
    fn test_ech_config_unknown_version() {
        let mut entry = Vec::new();
        entry.extend_from_slice(&[0x00, 0x01]); // unknown version
        entry.extend_from_slice(&[0x00, 0x04]); // length
        entry.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // dummy data

        let mut list = Vec::new();
        list.extend_from_slice(&(entry.len() as u16).to_be_bytes());
        list.extend_from_slice(&entry);

        let b64 = base64_encode_test(&list);
        let result = EchTransport::parse_config_list(&b64);
        assert!(matches!(result, EchConfigParseResult::UnknownVersion));
    }

    #[test]
    fn test_ech_disable_grease() {
        let mut ech = EchTransport::new(
            "wss://target.example.com".into(),
            "public.example.com".into(),
            "config".into(),
            EchCipherSuite::X25519HkdfSha256ChaCha20,
        );
        assert!(ech.grease_on_failure());
        ech.disable_grease_fallback();
        assert!(!ech.grease_on_failure());
    }

    /// Helper: minimal base64 encoder for tests.
    fn base64_encode_test(data: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(b2 & 0x3F) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}
