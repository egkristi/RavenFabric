//! Audio modem transport driver.
//!
//! Encodes data as audio frequencies for transmission over acoustic channels
//! (speaker/microphone). Uses frequency-shift keying (FSK) modulation within
//! the audible or near-ultrasonic range (18-20 kHz for inaudible mode).
//!
//! # Use Cases
//!
//! - Air-gapped environments where no network/radio is available
//! - Short-range device pairing (similar to Chirp/Google Tone)
//! - Covert communication over phone calls or ambient audio
//!
//! # Protocol
//!
//! - Modulation: 2-FSK (two frequencies per bit)
//! - Preamble: 8 alternating tones for synchronization
//! - Frame: [preamble][length: 2 bytes][payload][CRC-16]
//! - Symbol rate: configurable (default: 100 baud)
//! - Frequencies: mark=18000 Hz, space=19000 Hz (near-ultrasonic)

use crate::driver::{AsyncStream, Driver, DriverConfig, Listener, Target};
use crate::error::TransportError;

/// Default mark frequency (Hz) — represents binary 1.
const DEFAULT_MARK_FREQ: u32 = 18000;

/// Default space frequency (Hz) — represents binary 0.
const DEFAULT_SPACE_FREQ: u32 = 19000;

/// Default sample rate (Hz).
const DEFAULT_SAMPLE_RATE: u32 = 44100;

/// Default symbol rate (baud).
const DEFAULT_BAUD_RATE: u32 = 100;

/// Preamble length (number of alternating symbols).
const PREAMBLE_LENGTH: usize = 8;

/// Maximum frame payload size.
const MAX_PAYLOAD_SIZE: usize = 256;

/// Audio modem configuration.
#[derive(Debug, Clone)]
pub struct AudioModemConfig {
    /// Mark frequency (binary 1).
    pub mark_freq: u32,
    /// Space frequency (binary 0).
    pub space_freq: u32,
    /// Audio sample rate.
    pub sample_rate: u32,
    /// Symbol rate (baud).
    pub baud_rate: u32,
}

impl Default for AudioModemConfig {
    fn default() -> Self {
        Self {
            mark_freq: DEFAULT_MARK_FREQ,
            space_freq: DEFAULT_SPACE_FREQ,
            sample_rate: DEFAULT_SAMPLE_RATE,
            baud_rate: DEFAULT_BAUD_RATE,
        }
    }
}

/// Transport driver that encodes data as audio for acoustic transmission.
pub struct AudioModemDriver {
    /// Modem configuration.
    config: AudioModemConfig,
    /// Audio device proxy address (TCP bridge to audio hardware).
    proxy_addr: String,
}

impl AudioModemDriver {
    /// Create a new audio modem driver with default settings (near-ultrasonic).
    pub fn new() -> Self {
        Self {
            config: AudioModemConfig::default(),
            proxy_addr: "127.0.0.1:7720".to_string(),
        }
    }

    /// Create with custom frequencies (e.g., audible range for debugging).
    pub fn with_frequencies(mark: u32, space: u32) -> Self {
        Self {
            config: AudioModemConfig {
                mark_freq: mark,
                space_freq: space,
                ..AudioModemConfig::default()
            },
            proxy_addr: "127.0.0.1:7720".to_string(),
        }
    }

    /// Create with custom proxy address.
    pub fn with_proxy(addr: impl Into<String>) -> Self {
        Self {
            config: AudioModemConfig::default(),
            proxy_addr: addr.into(),
        }
    }

    /// Calculate samples per symbol.
    pub fn samples_per_symbol(&self) -> u32 {
        self.config.sample_rate / self.config.baud_rate
    }

    /// Encode a single bit as audio samples (simplified: square wave).
    pub fn encode_bit(&self, bit: bool) -> Vec<i16> {
        let freq = if bit {
            self.config.mark_freq
        } else {
            self.config.space_freq
        };
        let samples = self.samples_per_symbol() as usize;
        let mut buf = Vec::with_capacity(samples);
        for i in 0..samples {
            // Generate sine wave using floating-point phase
            let t = i as f32 / self.config.sample_rate as f32;
            let sample = (t * freq as f32 * 2.0 * std::f32::consts::PI).sin() * 16000.0;
            buf.push(sample as i16);
        }
        buf
    }

    /// Encode a byte as 8 audio symbols (MSB first).
    pub fn encode_byte(&self, byte: u8) -> Vec<i16> {
        let mut samples = Vec::new();
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1 == 1;
            samples.extend(self.encode_bit(bit));
        }
        samples
    }

    /// Generate preamble (alternating mark/space for sync).
    pub fn generate_preamble(&self) -> Vec<i16> {
        let mut samples = Vec::new();
        for i in 0..PREAMBLE_LENGTH {
            samples.extend(self.encode_bit(i % 2 == 0));
        }
        samples
    }

    /// Encode a complete frame with preamble, length, payload, and CRC.
    pub fn encode_frame(&self, payload: &[u8]) -> Result<Vec<i16>, TransportError> {
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(TransportError::Connection(format!(
                "payload too large for audio modem: {} > {}",
                payload.len(),
                MAX_PAYLOAD_SIZE
            )));
        }

        let mut samples = Vec::new();

        // Preamble
        samples.extend(self.generate_preamble());

        // Length (2 bytes, big-endian)
        let len = payload.len() as u16;
        samples.extend(self.encode_byte((len >> 8) as u8));
        samples.extend(self.encode_byte((len & 0xFF) as u8));

        // Payload
        for &byte in payload {
            samples.extend(self.encode_byte(byte));
        }

        // CRC-16
        let crc = crc16(payload);
        samples.extend(self.encode_byte((crc >> 8) as u8));
        samples.extend(self.encode_byte((crc & 0xFF) as u8));

        Ok(samples)
    }

    /// Detect the dominant frequency in a window of samples.
    pub fn detect_frequency(&self, samples: &[i16]) -> u32 {
        if samples.is_empty() {
            return 0;
        }
        // Simple zero-crossing rate frequency estimation
        let mut crossings = 0u32;
        for i in 1..samples.len() {
            if (samples[i] > 0 && samples[i - 1] <= 0)
                || (samples[i] <= 0 && samples[i - 1] > 0)
            {
                crossings += 1;
            }
        }
        // Frequency = crossings / 2 / duration
        let duration_secs = samples.len() as f32 / self.config.sample_rate as f32;
        if duration_secs > 0.0 {
            (crossings as f32 / 2.0 / duration_secs) as u32
        } else {
            0
        }
    }

    /// Decode a single symbol (bit) from audio samples.
    pub fn decode_bit(&self, samples: &[i16]) -> bool {
        let freq = self.detect_frequency(samples);
        let mark_dist = (freq as i64 - self.config.mark_freq as i64).unsigned_abs();
        let space_dist = (freq as i64 - self.config.space_freq as i64).unsigned_abs();
        mark_dist < space_dist
    }
}

impl Default for AudioModemDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Driver for AudioModemDriver {
    fn name(&self) -> &str {
        "audio-modem"
    }

    fn available(&self) -> bool {
        // Audio modem available if proxy address is set
        !self.proxy_addr.is_empty()
    }

    async fn dial(
        &self,
        _target: &Target,
        _config: &DriverConfig,
    ) -> Result<Box<dyn AsyncStream>, TransportError> {
        // Connect to audio proxy daemon
        let stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to audio modem proxy at {}: {e}",
                    self.proxy_addr
                ))
            })?;
        Ok(Box::new(stream))
    }

    async fn listen(&self, _addr: &str) -> Result<Box<dyn Listener>, TransportError> {
        let _stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!(
                    "failed to connect to audio modem proxy at {} for listen: {e}",
                    self.proxy_addr
                ))
            })?;
        Ok(Box::new(AudioModemListener {
            proxy_addr: self.proxy_addr.clone(),
        }))
    }
}

/// Listener for incoming audio modem connections.
struct AudioModemListener {
    proxy_addr: String,
}

#[async_trait::async_trait]
impl Listener for AudioModemListener {
    async fn accept(&self) -> Result<Box<dyn AsyncStream>, TransportError> {
        let stream = tokio::net::TcpStream::connect(&self.proxy_addr)
            .await
            .map_err(|e| {
                TransportError::Connection(format!("audio modem accept failed: {e}"))
            })?;
        Ok(Box::new(stream))
    }
}

/// CRC-16/CCITT calculation.
fn crc16(data: &[u8]) -> u16 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_name() {
        let driver = AudioModemDriver::new();
        assert_eq!(driver.name(), "audio-modem");
    }

    #[test]
    fn test_default_config() {
        let driver = AudioModemDriver::new();
        assert_eq!(driver.config.mark_freq, 18000);
        assert_eq!(driver.config.space_freq, 19000);
        assert_eq!(driver.config.sample_rate, 44100);
        assert_eq!(driver.config.baud_rate, 100);
    }

    #[test]
    fn test_samples_per_symbol() {
        let driver = AudioModemDriver::new();
        assert_eq!(driver.samples_per_symbol(), 441); // 44100 / 100
    }

    #[test]
    fn test_encode_bit_produces_correct_length() {
        let driver = AudioModemDriver::new();
        let samples = driver.encode_bit(true);
        assert_eq!(samples.len(), 441);
    }

    #[test]
    fn test_encode_byte_produces_8_symbols() {
        let driver = AudioModemDriver::new();
        let samples = driver.encode_byte(0xA5);
        assert_eq!(samples.len(), 441 * 8);
    }

    #[test]
    fn test_preamble_length() {
        let driver = AudioModemDriver::new();
        let preamble = driver.generate_preamble();
        assert_eq!(preamble.len(), 441 * PREAMBLE_LENGTH);
    }

    #[test]
    fn test_encode_frame_small_payload() {
        let driver = AudioModemDriver::new();
        let payload = b"Hi";
        let frame = driver.encode_frame(payload).unwrap();
        // preamble(8) + length(2 bytes) + payload(2 bytes) + crc(2 bytes) = 14 symbols
        let expected_symbols = PREAMBLE_LENGTH + 2 * 8 + 2 * 8 + 2 * 8;
        assert_eq!(frame.len(), expected_symbols * 441);
    }

    #[test]
    fn test_encode_frame_too_large() {
        let driver = AudioModemDriver::new();
        let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        let result = driver.encode_frame(&payload);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn test_crc16_known_value() {
        // CRC-16/CCITT of empty is 0xFFFF (initial value, no data processed loops)
        // Actually for empty data it stays 0xFFFF
        let crc = crc16(b"");
        assert_eq!(crc, 0xFFFF);
    }

    #[test]
    fn test_crc16_deterministic() {
        let crc1 = crc16(b"hello");
        let crc2 = crc16(b"hello");
        assert_eq!(crc1, crc2);
        let crc3 = crc16(b"world");
        assert_ne!(crc1, crc3);
    }

    #[test]
    fn test_decode_bit_mark() {
        let driver = AudioModemDriver::new();
        let samples = driver.encode_bit(true);
        let decoded = driver.decode_bit(&samples);
        assert!(decoded); // Should decode as mark (true)
    }

    #[test]
    fn test_decode_bit_space() {
        let driver = AudioModemDriver::new();
        let samples = driver.encode_bit(false);
        let decoded = driver.decode_bit(&samples);
        assert!(!decoded); // Should decode as space (false)
    }

    #[test]
    fn test_custom_frequencies() {
        let driver = AudioModemDriver::with_frequencies(1000, 2000);
        assert_eq!(driver.config.mark_freq, 1000);
        assert_eq!(driver.config.space_freq, 2000);
    }

    #[test]
    fn test_available() {
        let driver = AudioModemDriver::new();
        assert!(driver.available());
        let empty = AudioModemDriver::with_proxy("");
        assert!(!empty.available());
    }

    #[tokio::test]
    async fn test_dial_proxy_unavailable() {
        let driver = AudioModemDriver::with_proxy("127.0.0.1:19999");
        let target = Target {
            agent_id: "test".to_string(),
            relay_url: None,
            meet_token: None,
        };
        let config = DriverConfig::new();
        let result = driver.dial(&target, &config).await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to audio modem proxy")),
            Ok(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn test_listen_proxy_unavailable() {
        let driver = AudioModemDriver::with_proxy("127.0.0.1:19999");
        let result = driver.listen("test").await;
        match result {
            Err(e) => assert!(e.to_string().contains("failed to connect to audio modem proxy")),
            Ok(_) => panic!("expected error"),
        }
    }
}
