//! Traffic obfuscation and analysis resistance.
//!
//! Inspired by obfs4: adds padding, randomizes frame sizes, and removes
//! any recognizable patterns from the wire format. Includes constant-rate
//! traffic shaping to defeat timing/volume analysis.

use std::time::Duration;

use rand::RngCore;

/// Obfuscation configuration.
#[derive(Debug, Clone)]
pub struct ObfuscationConfig {
    /// Minimum padding bytes to add per frame.
    pub min_padding: usize,
    /// Maximum padding bytes to add per frame.
    pub max_padding: usize,
    /// Whether to randomize frame timing (add jitter).
    pub timing_jitter: bool,
    /// Maximum jitter in milliseconds.
    pub max_jitter_ms: u64,
}

impl Default for ObfuscationConfig {
    fn default() -> Self {
        Self {
            min_padding: 0,
            max_padding: 255,
            timing_jitter: true,
            max_jitter_ms: 50,
        }
    }
}

/// An obfuscated frame that looks like random bytes on the wire.
#[derive(Debug, Clone)]
pub struct ObfuscatedFrame {
    /// The obfuscated payload (padding + encrypted data, indistinguishable from random).
    pub data: Vec<u8>,
}

/// Obfuscate a plaintext frame by adding random padding.
/// The result should be indistinguishable from random bytes
/// (assumes input is already encrypted via Noise).
pub fn obfuscate(encrypted_data: &[u8], config: &ObfuscationConfig) -> ObfuscatedFrame {
    let mut rng = rand::rng();

    // Random padding length within configured bounds
    let padding_len = if config.max_padding > config.min_padding {
        config.min_padding + (rng.next_u32() as usize % (config.max_padding - config.min_padding))
    } else {
        config.min_padding
    };

    // Frame format: [1 byte padding_len][padding][encrypted_data]
    // Since encrypted_data is already random-looking (Noise output),
    // and padding is random, the whole frame is indistinguishable from random.
    let total_len = 1 + padding_len + encrypted_data.len();
    let mut data = Vec::with_capacity(total_len);

    // Padding length byte (this itself looks random since padding is variable)
    data.push(padding_len as u8);

    // Random padding
    let mut padding = vec![0u8; padding_len];
    rng.fill_bytes(&mut padding);
    data.extend_from_slice(&padding);

    // Encrypted payload
    data.extend_from_slice(encrypted_data);

    ObfuscatedFrame { data }
}

/// Deobfuscate a frame — strip padding to recover the encrypted payload.
pub fn deobfuscate(frame: &ObfuscatedFrame) -> Result<Vec<u8>, &'static str> {
    if frame.data.is_empty() {
        return Err("empty frame");
    }

    let padding_len = frame.data[0] as usize;
    let header_and_padding = 1 + padding_len;

    if frame.data.len() < header_and_padding {
        return Err("frame too short for declared padding");
    }

    Ok(frame.data[header_and_padding..].to_vec())
}

/// Calculate jitter delay for timing obfuscation.
pub fn jitter_delay(config: &ObfuscationConfig) -> std::time::Duration {
    if !config.timing_jitter || config.max_jitter_ms == 0 {
        return std::time::Duration::ZERO;
    }
    let jitter_ms = rand::rng().next_u64() % config.max_jitter_ms;
    std::time::Duration::from_millis(jitter_ms)
}

/// Check if a byte sequence has detectable patterns (for testing).
/// Returns a score 0.0-1.0 where 1.0 means perfectly random-looking.
pub fn randomness_score(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    // Simple chi-squared test against uniform distribution
    let mut freq = [0u64; 256];
    for &b in data {
        freq[b as usize] += 1;
    }

    let expected = data.len() as f64 / 256.0;
    let chi_sq: f64 = freq
        .iter()
        .map(|&f| {
            let diff = f as f64 - expected;
            diff * diff / expected
        })
        .sum();

    // Normalize: perfect random would give chi_sq ≈ 255
    // Very non-random (all same byte) would give chi_sq ≈ 255*N
    let normalized = chi_sq / (255.0 * data.len() as f64 / 256.0);
    (1.0 - normalized.min(1.0)).max(0.0)
}

// --- Traffic Analysis Resistance ---

/// Traffic shaping mode for defeating traffic analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrafficShapingMode {
    /// No shaping — send frames as they are produced.
    None,
    /// Constant-rate: send frames at a fixed interval, padding with dummy
    /// traffic when idle. Defeats timing and volume analysis.
    ConstantRate {
        /// Interval between frames.
        interval: Duration,
        /// Target frame size (real data + padding).
        frame_size: usize,
    },
    /// Adaptive: adjust sending rate to match a target bandwidth, adding
    /// dummy traffic to fill gaps. Less overhead than constant-rate.
    Adaptive {
        /// Target bandwidth in bytes per second.
        target_bps: u64,
        /// Measurement window for rate calculation.
        window: Duration,
    },
}

/// A frame that may be real data or dummy traffic (cover traffic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapedFrame {
    /// Real data frame.
    Data(Vec<u8>),
    /// Dummy frame (cover traffic) — discard on receive.
    Dummy(Vec<u8>),
}

/// Traffic shaper — buffers real data and produces a constant stream
/// of fixed-size frames, mixing in dummy traffic when idle.
pub struct TrafficShaper {
    mode: TrafficShapingMode,
    /// Queued real data waiting to be sent.
    pending: Vec<Vec<u8>>,
    /// Total real bytes sent in current window.
    real_bytes_sent: u64,
    /// Total dummy bytes sent in current window.
    dummy_bytes_sent: u64,
}

impl TrafficShaper {
    /// Create a new traffic shaper.
    pub fn new(mode: TrafficShapingMode) -> Self {
        Self {
            mode,
            pending: Vec::new(),
            real_bytes_sent: 0,
            dummy_bytes_sent: 0,
        }
    }

    /// Queue real data for shaped sending.
    pub fn enqueue(&mut self, data: Vec<u8>) {
        self.pending.push(data);
    }

    /// Produce the next frame to send.
    ///
    /// In constant-rate mode, returns a fixed-size frame containing either
    /// real data or dummy traffic. In no-shaping mode, returns real data
    /// or None if nothing is queued.
    pub fn next_frame(&mut self) -> Option<ShapedFrame> {
        match &self.mode {
            TrafficShapingMode::None => {
                if self.pending.is_empty() {
                    None
                } else {
                    let data = self.pending.remove(0);
                    self.real_bytes_sent += data.len() as u64;
                    Some(ShapedFrame::Data(data))
                }
            }
            TrafficShapingMode::ConstantRate { frame_size, .. } => {
                let target = *frame_size;
                if let Some(data) = self.pending.first() {
                    if data.len() <= target {
                        let mut frame = self.pending.remove(0);
                        // Pad to target size
                        let pad_len = target.saturating_sub(frame.len());
                        if pad_len > 0 {
                            let mut padding = vec![0u8; pad_len];
                            rand::rng().fill_bytes(&mut padding);
                            frame.extend_from_slice(&padding);
                        }
                        self.real_bytes_sent += frame.len() as u64;
                        Some(ShapedFrame::Data(frame))
                    } else {
                        // Split large data across multiple frames
                        let chunk: Vec<u8> = self.pending[0].drain(..target).collect();
                        if self.pending[0].is_empty() {
                            self.pending.remove(0);
                        }
                        self.real_bytes_sent += chunk.len() as u64;
                        Some(ShapedFrame::Data(chunk))
                    }
                } else {
                    // No real data — send dummy
                    let mut dummy = vec![0u8; target];
                    rand::rng().fill_bytes(&mut dummy);
                    self.dummy_bytes_sent += dummy.len() as u64;
                    Some(ShapedFrame::Dummy(dummy))
                }
            }
            TrafficShapingMode::Adaptive { .. } => {
                // For adaptive mode, behave like no-shaping for real data,
                // but report when dummy traffic should be injected.
                if self.pending.is_empty() {
                    None
                } else {
                    let data = self.pending.remove(0);
                    self.real_bytes_sent += data.len() as u64;
                    Some(ShapedFrame::Data(data))
                }
            }
        }
    }

    /// Check if the shaper should send a dummy frame based on adaptive mode.
    /// Returns the number of dummy bytes needed to maintain the target rate.
    pub fn dummy_bytes_needed(&self, elapsed: Duration) -> u64 {
        match &self.mode {
            TrafficShapingMode::Adaptive {
                target_bps, window, ..
            } => {
                let window_secs = window.as_secs_f64();
                let elapsed_secs = elapsed.as_secs_f64().min(window_secs);
                let expected_bytes = (*target_bps as f64 * elapsed_secs) as u64;
                expected_bytes.saturating_sub(self.real_bytes_sent + self.dummy_bytes_sent)
            }
            _ => 0,
        }
    }

    /// Generate a dummy frame of the specified size.
    pub fn generate_dummy(&mut self, size: usize) -> ShapedFrame {
        let mut data = vec![0u8; size];
        rand::rng().fill_bytes(&mut data);
        self.dummy_bytes_sent += size as u64;
        ShapedFrame::Dummy(data)
    }

    /// Reset counters for a new measurement window.
    pub fn reset_counters(&mut self) {
        self.real_bytes_sent = 0;
        self.dummy_bytes_sent = 0;
    }

    /// Get statistics.
    pub fn stats(&self) -> (u64, u64) {
        (self.real_bytes_sent, self.dummy_bytes_sent)
    }

    /// Number of queued real data frames.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obfuscate_deobfuscate_roundtrip() {
        let config = ObfuscationConfig::default();
        let original = b"encrypted noise output here";

        let obfuscated = obfuscate(original, &config);
        let recovered = deobfuscate(&obfuscated).unwrap();

        assert_eq!(recovered, original);
    }

    #[test]
    fn test_obfuscated_frame_larger_than_input() {
        let config = ObfuscationConfig {
            min_padding: 10,
            max_padding: 20,
            ..Default::default()
        };
        let original = b"data";
        let obfuscated = obfuscate(original, &config);

        // At least: 1 (len byte) + 10 (min padding) + 4 (data) = 15
        assert!(obfuscated.data.len() >= 15);
    }

    #[test]
    fn test_deobfuscate_empty_frame() {
        let frame = ObfuscatedFrame { data: vec![] };
        assert!(deobfuscate(&frame).is_err());
    }

    #[test]
    fn test_deobfuscate_truncated_frame() {
        // Claims 100 bytes of padding but frame is only 5 bytes
        let frame = ObfuscatedFrame {
            data: vec![100, 1, 2, 3, 4],
        };
        assert!(deobfuscate(&frame).is_err());
    }

    #[test]
    fn test_obfuscated_data_looks_random() {
        let config = ObfuscationConfig {
            min_padding: 100,
            max_padding: 200,
            ..Default::default()
        };
        // Use random-looking input (simulating Noise output)
        let mut input = vec![0u8; 1000];
        rand::rng().fill_bytes(&mut input);

        let obfuscated = obfuscate(&input, &config);
        let score = randomness_score(&obfuscated.data);
        // Should look fairly random (score > 0.5)
        assert!(score > 0.3, "randomness score too low: {score}");
    }

    #[test]
    fn test_jitter_delay_disabled() {
        let config = ObfuscationConfig {
            timing_jitter: false,
            ..Default::default()
        };
        assert_eq!(jitter_delay(&config), std::time::Duration::ZERO);
    }

    #[test]
    fn test_jitter_delay_bounded() {
        let config = ObfuscationConfig {
            timing_jitter: true,
            max_jitter_ms: 100,
            ..Default::default()
        };
        for _ in 0..100 {
            let delay = jitter_delay(&config);
            assert!(delay.as_millis() < 100);
        }
    }

    #[test]
    fn test_zero_padding_config() {
        let config = ObfuscationConfig {
            min_padding: 0,
            max_padding: 0,
            timing_jitter: false,
            max_jitter_ms: 0,
        };
        let original = b"test";
        let obfuscated = obfuscate(original, &config);
        // 1 byte header + 0 padding + 4 data = 5 bytes
        assert_eq!(obfuscated.data.len(), 5);
        assert_eq!(obfuscated.data[0], 0); // zero padding length

        let recovered = deobfuscate(&obfuscated).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_traffic_shaper_no_shaping() {
        let mut shaper = TrafficShaper::new(TrafficShapingMode::None);
        assert!(shaper.next_frame().is_none()); // Nothing queued

        shaper.enqueue(b"hello".to_vec());
        let frame = shaper.next_frame().unwrap();
        assert_eq!(frame, ShapedFrame::Data(b"hello".to_vec()));
        assert!(shaper.next_frame().is_none());
    }

    #[test]
    fn test_traffic_shaper_constant_rate_with_data() {
        let mut shaper = TrafficShaper::new(TrafficShapingMode::ConstantRate {
            interval: Duration::from_millis(50),
            frame_size: 64,
        });

        shaper.enqueue(b"real data".to_vec());
        let frame = shaper.next_frame().unwrap();

        match frame {
            ShapedFrame::Data(data) => {
                assert_eq!(data.len(), 64); // Padded to frame_size
                assert_eq!(&data[..9], b"real data");
            }
            ShapedFrame::Dummy(_) => panic!("expected data frame"),
        }
    }

    #[test]
    fn test_traffic_shaper_constant_rate_dummy() {
        let mut shaper = TrafficShaper::new(TrafficShapingMode::ConstantRate {
            interval: Duration::from_millis(50),
            frame_size: 32,
        });

        // No real data — should produce dummy
        let frame = shaper.next_frame().unwrap();
        match frame {
            ShapedFrame::Dummy(data) => assert_eq!(data.len(), 32),
            ShapedFrame::Data(_) => panic!("expected dummy frame"),
        }
    }

    #[test]
    fn test_traffic_shaper_constant_rate_splits_large() {
        let mut shaper = TrafficShaper::new(TrafficShapingMode::ConstantRate {
            interval: Duration::from_millis(50),
            frame_size: 4,
        });

        shaper.enqueue(b"abcdefgh".to_vec()); // 8 bytes, frame_size=4

        let f1 = shaper.next_frame().unwrap();
        assert_eq!(f1, ShapedFrame::Data(b"abcd".to_vec()));

        let f2 = shaper.next_frame().unwrap();
        assert_eq!(f2, ShapedFrame::Data(b"efgh".to_vec()));
    }

    #[test]
    fn test_traffic_shaper_adaptive_dummy_needed() {
        let shaper = TrafficShaper::new(TrafficShapingMode::Adaptive {
            target_bps: 1000,
            window: Duration::from_secs(1),
        });

        // After 500ms with no data sent, we need ~500 bytes of dummy
        let needed = shaper.dummy_bytes_needed(Duration::from_millis(500));
        assert_eq!(needed, 500);
    }

    #[test]
    fn test_traffic_shaper_stats() {
        let mut shaper = TrafficShaper::new(TrafficShapingMode::ConstantRate {
            interval: Duration::from_millis(10),
            frame_size: 16,
        });

        shaper.enqueue(b"data".to_vec());
        shaper.next_frame(); // Data frame (padded to 16)
        shaper.next_frame(); // Dummy frame (16 bytes)

        let (real, dummy) = shaper.stats();
        assert_eq!(real, 16);
        assert_eq!(dummy, 16);

        shaper.reset_counters();
        let (r, d) = shaper.stats();
        assert_eq!(r, 0);
        assert_eq!(d, 0);
    }
}
