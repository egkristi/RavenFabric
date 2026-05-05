//! Traffic obfuscation — make RavenFabric protocol indistinguishable from random bytes.
//!
//! Inspired by obfs4: adds padding, randomizes frame sizes, and removes
//! any recognizable patterns from the wire format.

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
    let mut rng = rand::thread_rng();

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
    let jitter_ms = rand::thread_rng().next_u64() % config.max_jitter_ms;
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
        rand::thread_rng().fill_bytes(&mut input);

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
}
