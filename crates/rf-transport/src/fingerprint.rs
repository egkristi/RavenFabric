//! Protocol fingerprint verification — detect DPI/downgrade attacks.
//!
//! Validates that the RavenFabric wire protocol header matches expected
//! values, detecting interception or protocol manipulation.

/// Expected wire protocol magic bytes.
pub const PROTOCOL_MAGIC: &[u8; 4] = b"RVNF";

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

/// Minimum valid frame size (4-byte magic + 1-byte version + minimum payload).
pub const MIN_HEADER_SIZE: usize = 5;

/// Result of fingerprint verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingerprintResult {
    /// Protocol matches expected fingerprint.
    Valid,
    /// Magic bytes don't match — likely DPI injection or wrong protocol.
    InvalidMagic { got: [u8; 4] },
    /// Version mismatch — possible downgrade attack.
    VersionMismatch { expected: u8, got: u8 },
    /// Header too short — truncation or injection.
    Truncated { len: usize },
    /// Unexpected padding or trailing data in header.
    ExtraData,
}

/// Verify protocol fingerprint on incoming data.
pub fn verify_fingerprint(data: &[u8]) -> FingerprintResult {
    if data.len() < MIN_HEADER_SIZE {
        return FingerprintResult::Truncated { len: data.len() };
    }

    let magic = &data[0..4];
    if magic != PROTOCOL_MAGIC {
        let mut got = [0u8; 4];
        got.copy_from_slice(magic);
        return FingerprintResult::InvalidMagic { got };
    }

    let version = data[4];
    if version != PROTOCOL_VERSION {
        return FingerprintResult::VersionMismatch {
            expected: PROTOCOL_VERSION,
            got: version,
        };
    }

    FingerprintResult::Valid
}

/// Check if data looks like a DPI-injected response (common patterns).
pub fn detect_dpi_injection(data: &[u8]) -> bool {
    // HTTP response injection (common DPI technique)
    if data.starts_with(b"HTTP/1.") || data.starts_with(b"HTTP/2") {
        return true;
    }
    // TLS ClientHello injection (downgrade to TLS for inspection)
    if data.len() >= 3 && data[0] == 0x16 && data[1] == 0x03 {
        return true;
    }
    // TCP RST pattern (connection reset injection)
    // (This would be at TCP layer, not payload, but check for common payload patterns)
    if data.starts_with(b"<html") || data.starts_with(b"<!DOCTYPE") {
        return true;
    }
    false
}

/// Classify the severity of a fingerprint failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatLevel {
    /// Likely benign (version negotiation needed).
    Low,
    /// Suspicious (possible downgrade attempt).
    Medium,
    /// Critical (active interception/injection detected).
    Critical,
}

/// Assess threat level from a fingerprint result.
pub fn assess_threat(result: &FingerprintResult) -> ThreatLevel {
    match result {
        FingerprintResult::Valid => ThreatLevel::Low,
        FingerprintResult::VersionMismatch { .. } => ThreatLevel::Medium,
        FingerprintResult::Truncated { .. } => ThreatLevel::Medium,
        FingerprintResult::InvalidMagic { .. } => ThreatLevel::Critical,
        FingerprintResult::ExtraData => ThreatLevel::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_fingerprint() {
        let data = b"RVNF\x01some_payload_here";
        assert_eq!(verify_fingerprint(data), FingerprintResult::Valid);
    }

    #[test]
    fn test_invalid_magic() {
        let data = b"HTTP\x01hello";
        let result = verify_fingerprint(data);
        assert!(matches!(result, FingerprintResult::InvalidMagic { got } if &got == b"HTTP"));
    }

    #[test]
    fn test_version_mismatch() {
        let data = b"RVNF\x02payload";
        let result = verify_fingerprint(data);
        assert_eq!(
            result,
            FingerprintResult::VersionMismatch {
                expected: 1,
                got: 2
            }
        );
    }

    #[test]
    fn test_truncated() {
        let data = b"RVN";
        let result = verify_fingerprint(data);
        assert_eq!(result, FingerprintResult::Truncated { len: 3 });
    }

    #[test]
    fn test_detect_dpi_http() {
        assert!(detect_dpi_injection(b"HTTP/1.1 200 OK\r\n"));
        assert!(detect_dpi_injection(b"HTTP/2 403 Forbidden"));
    }

    #[test]
    fn test_detect_dpi_tls() {
        // TLS record layer: ContentType=Handshake(0x16), Version=TLS 1.2 (0x0303)
        assert!(detect_dpi_injection(&[0x16, 0x03, 0x03, 0x00, 0x05]));
    }

    #[test]
    fn test_detect_dpi_html() {
        assert!(detect_dpi_injection(b"<html><body>blocked</body></html>"));
        assert!(detect_dpi_injection(b"<!DOCTYPE html>"));
    }

    #[test]
    fn test_no_dpi_on_valid_protocol() {
        assert!(!detect_dpi_injection(b"RVNF\x01data"));
    }

    #[test]
    fn test_threat_assessment() {
        assert_eq!(assess_threat(&FingerprintResult::Valid), ThreatLevel::Low);
        assert_eq!(
            assess_threat(&FingerprintResult::InvalidMagic { got: *b"HTTP" }),
            ThreatLevel::Critical
        );
        assert_eq!(
            assess_threat(&FingerprintResult::VersionMismatch {
                expected: 1,
                got: 2
            }),
            ThreatLevel::Medium
        );
    }
}
