//! Multi-key invitation tokens (ROADMAP R0.6 / findings F5, F9).
//!
//! The legacy meet-token format is a single global HMAC secret over
//! `<payload>.<hex_mac>` — no per-invitee revocation, no expiry. This module
//! adds a keyring-based format so "invite only" is per-invitee rather than a
//! shared password:
//!
//! ```text
//! <kid>.<b64url(payload)>.<hex_mac>
//!
//! payload = { "sub": "...", "exp": <unix_secs>, "nonce": "...", "forward_hops": <n> }
//! ```
//!
//! The `kid` selects a key from a keyring loaded from a TOML file; `exp` is
//! enforced with a configurable ceiling; `nonce` enables single-use when
//! requested. The legacy two-segment format is still accepted when a legacy
//! global secret is configured.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// A single keyring entry, keyed by `kid`.
#[derive(Debug, Clone, Deserialize)]
pub struct KeyEntry {
    /// Raw secret bytes for HMAC (hex-encoded in the TOML file).
    pub secret: String,
    /// Human-readable label (used in logs).
    #[serde(default)]
    pub label: Option<String>,
    /// Whether this key is currently enabled. Disabled keys fail verification.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// A keyring mapping `kid` → `KeyEntry`, loaded from a TOML file.
///
/// The TOML file format is:
///
/// ```toml
/// [keys.alice]
/// secret = "<hex>"
/// label = "Alice's laptop"
/// enabled = true
/// ```
#[derive(Debug, Clone, Default)]
pub struct TokenKeyring {
    entries: HashMap<String, KeyEntry>,
}

impl TokenKeyring {
    /// Load a keyring from a TOML file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Self::parse(&contents)
    }

    /// Parse a keyring from TOML text.
    pub fn parse(toml_text: &str) -> anyhow::Result<Self> {
        let value: toml::Value = toml::from_str(toml_text)?;
        let Some(keys_table) = value.get("keys").and_then(|v| v.as_table()) else {
            return Ok(Self::default());
        };

        let mut entries = HashMap::new();
        for (kid, entry_val) in keys_table {
            let entry: KeyEntry = entry_val
                .clone()
                .try_into()
                .map_err(|e| anyhow::anyhow!("invalid key entry for '{kid}': {e}"))?;
            entries.insert(kid.clone(), entry);
        }

        Ok(Self { entries })
    }

    /// Get a key entry by `kid`.
    pub fn get(&self, kid: &str) -> Option<&KeyEntry> {
        self.entries.get(kid)
    }

    /// Number of entries (including disabled).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the keyring is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over `(kid, enabled)` pairs for logging.
    pub fn enabled_ids(&self) -> Vec<(&str, bool)> {
        self.entries
            .iter()
            .map(|(kid, e)| (kid.as_str(), e.enabled))
            .collect()
    }
}

/// The decoded, verified claims of a token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject (invitee name).
    #[serde(default)]
    pub sub: String,
    /// Expiry as Unix seconds. `None` = no expiry.
    #[serde(default)]
    pub exp: Option<u64>,
    /// Nonce for single-use enforcement. `None` = reusable.
    #[serde(default)]
    pub nonce: Option<String>,
    /// Forwarding hop budget. `None` = no budget.
    #[serde(default)]
    pub forward_hops: Option<u32>,
}

/// Options governing token verification.
#[derive(Debug, Clone)]
pub struct TokenVerifier {
    /// Keyring for the multi-key format.
    pub keyring: Arc<Mutex<TokenKeyring>>,
    /// Legacy global secret (fallback for the two-segment format). `None` = disabled.
    pub legacy_secret: Option<String>,
    /// Maximum allowed `exp` (seconds). `0` = no ceiling.
    pub max_token_age_secs: u64,
    /// Enforce single-use via a nonce set.
    pub enforce_single_use: bool,
    /// Seen nonces (bounded), shared across clones.
    seen_nonces: Arc<Mutex<HashSet<String>>>,
}

/// Verification outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Token is valid.
    Valid,
    /// Unknown `kid` (multi-key format).
    UnknownKid,
    /// Key is disabled.
    KeyDisabled,
    /// HMAC mismatch.
    BadMac,
    /// Token expired.
    Expired,
    /// Token reused (nonce already seen, single-use enforced).
    Reused,
    /// Malformed token.
    Malformed,
}

/// Maximum number of nonces retained for single-use enforcement. Exceeding this
/// evicts the oldest entries (approximate LRU via `Vec` index 0).
const MAX_SEEN_NONCES: usize = 10_000;

impl TokenVerifier {
    /// Create a verifier with the given keyring and options.
    pub fn new(
        keyring: Arc<Mutex<TokenKeyring>>,
        legacy_secret: Option<String>,
        max_token_age_secs: u64,
        enforce_single_use: bool,
    ) -> Self {
        Self {
            keyring,
            legacy_secret,
            max_token_age_secs,
            enforce_single_use,
            seen_nonces: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Verify a meet token. Returns the outcome.
    pub fn verify(&self, token: &str) -> VerifyOutcome {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Count segments: 3 = <kid>.<payload>.<mac>, 2 = legacy <payload>.<mac>.
        let segments: Vec<&str> = token.split('.').collect();
        match segments.as_slice() {
            [kid, payload, mac_hex] => {
                let keyring = self.keyring.lock().unwrap();
                let Some(entry) = keyring.get(kid) else {
                    return VerifyOutcome::UnknownKid;
                };
                if !entry.enabled {
                    return VerifyOutcome::KeyDisabled;
                }
                // The keyring secret is hex-encoded; decode before HMAC.
                let Ok(secret) = hex::decode(&entry.secret) else {
                    return VerifyOutcome::Malformed;
                };
                if !verify_hmac(payload, mac_hex, &secret) {
                    return VerifyOutcome::BadMac;
                }

                // Decode payload claims.
                let claims: TokenClaims = match decode_claims(payload) {
                    Some(c) => c,
                    None => return VerifyOutcome::Malformed,
                };

                if let Some(exp) = claims.exp {
                    if now >= exp {
                        return VerifyOutcome::Expired;
                    }
                    if self.max_token_age_secs > 0
                        && exp.saturating_sub(now) > self.max_token_age_secs
                    {
                        // exp too far in the future — reject as suspicious.
                        return VerifyOutcome::Expired;
                    }
                }

                if self.enforce_single_use {
                    if let Some(nonce) = &claims.nonce {
                        if !self.mark_seen(nonce) {
                            return VerifyOutcome::Reused;
                        }
                    }
                }

                VerifyOutcome::Valid
            }
            [payload, mac_hex] => {
                let Some(secret) = &self.legacy_secret else {
                    return VerifyOutcome::UnknownKid;
                };
                if !verify_hmac(payload, mac_hex, secret.as_bytes()) {
                    return VerifyOutcome::BadMac;
                }
                VerifyOutcome::Valid
            }
            _ => VerifyOutcome::Malformed,
        }
    }

    /// Record a nonce as seen. Returns `false` if it was already present.
    fn mark_seen(&self, nonce: &str) -> bool {
        let mut seen = self.seen_nonces.lock().unwrap();
        if seen.contains(nonce) {
            return false;
        }
        // Bound the set: evict oldest entries if over capacity.
        if seen.len() >= MAX_SEEN_NONCES {
            // Evict an arbitrary (oldest-by-insertion is not tracked) element.
            if let Some(k) = seen.iter().next().cloned() {
                seen.remove(&k);
            }
        }
        seen.insert(nonce.to_string());
        true
    }
}

/// Build a token (issue). `secret` is hex-encoded. Used by tests and the CLI.
pub fn issue_token(secret_hex: &str, kid: &str, claims: &TokenClaims) -> anyhow::Result<String> {
    let secret = hex::decode(secret_hex).map_err(|e| anyhow::anyhow!("invalid secret hex: {e}"))?;
    let payload_json = serde_json::to_vec(claims)?;
    let payload_b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&payload_json);

    let mut mac = HmacSha256::new_from_slice(&secret)?;
    mac.update(payload_b64url.as_bytes());
    let mac_hex = hex::encode(mac.finalize().into_bytes());

    Ok(format!("{kid}.{payload_b64url}.{mac_hex}"))
}

/// HMAC-verify a payload against a hex MAC. `secret` is raw key bytes.
fn verify_hmac(payload: &str, mac_hex: &str, secret: &[u8]) -> bool {
    let Ok(mac_bytes) = hex::decode(mac_hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret) else {
        return false;
    };
    mac.update(payload.as_bytes());
    mac.verify_slice(&mac_bytes).is_ok()
}

/// Decode a base64url payload into `TokenClaims`.
fn decode_claims(payload_b64url: &str) -> Option<TokenClaims> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64url)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyring() -> TokenKeyring {
        TokenKeyring::parse(
            r#"
[keys.alice]
secret = "000102030405060708090a0b0c0d0e0f"
label = "Alice"
enabled = true

[keys.bob]
secret = "101112131415161718191a1b1c1d1e1f"
label = "Bob"
enabled = false
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_keyring_parse() {
        let kr = keyring();
        assert_eq!(kr.len(), 2);
        assert!(kr.get("alice").unwrap().enabled);
        assert!(!kr.get("bob").unwrap().enabled);
        assert!(kr.get("carol").is_none());
    }

    #[test]
    fn test_verify_multi_key_valid() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, false);
        let claims = TokenClaims {
            sub: "alice".into(),
            exp: None,
            nonce: None,
            forward_hops: None,
        };
        let token = issue_token("000102030405060708090a0b0c0d0e0f", "alice", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::Valid);
    }

    #[test]
    fn test_verify_unknown_kid() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, false);
        let claims = TokenClaims {
            sub: "carol".into(),
            exp: None,
            nonce: None,
            forward_hops: None,
        };
        let token = issue_token("000102030405060708090a0b0c0d0e0f", "carol", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::UnknownKid);
    }

    #[test]
    fn test_verify_disabled_kid() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, false);
        let claims = TokenClaims {
            sub: "bob".into(),
            exp: None,
            nonce: None,
            forward_hops: None,
        };
        let token = issue_token("101112131415161718191a1b1c1d1e1f", "bob", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::KeyDisabled);
    }

    #[test]
    fn test_verify_expired() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, false);
        let claims = TokenClaims {
            sub: "alice".into(),
            exp: Some(1), // long expired
            nonce: None,
            forward_hops: None,
        };
        let token = issue_token("000102030405060708090a0b0c0d0e0f", "alice", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::Expired);
    }

    #[test]
    fn test_verify_bad_mac() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, false);
        let claims = TokenClaims {
            sub: "alice".into(),
            exp: None,
            nonce: None,
            forward_hops: None,
        };
        // Sign with a different key than what "alice" is registered with.
        let token = issue_token(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "alice",
            &claims,
        )
        .unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::BadMac);
    }

    #[test]
    fn test_verify_legacy_format() {
        // Legacy <payload>.<mac> with a global secret still works.
        let mut mac = HmacSha256::new_from_slice(b"legacy-secret").unwrap();
        mac.update(b"payload");
        let legacy = format!("payload.{}", hex::encode(mac.finalize().into_bytes()));

        let v = TokenVerifier::new(
            Arc::new(Mutex::new(keyring())),
            Some("legacy-secret".into()),
            0,
            false,
        );
        assert_eq!(v.verify(&legacy), VerifyOutcome::Valid);
    }

    #[test]
    fn test_single_use_nonce() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 0, true);
        let claims = TokenClaims {
            sub: "alice".into(),
            exp: None,
            nonce: Some("nonce-1".into()),
            forward_hops: None,
        };
        let token = issue_token("000102030405060708090a0b0c0d0e0f", "alice", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::Valid);
        // Reusing the same nonce must be rejected.
        assert_eq!(v.verify(&token), VerifyOutcome::Reused);
    }

    #[test]
    fn test_max_token_age_ceiling() {
        let v = TokenVerifier::new(Arc::new(Mutex::new(keyring())), None, 3600, false);
        let far_future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 7200; // 2 hours out, over the 1h ceiling
        let claims = TokenClaims {
            sub: "alice".into(),
            exp: Some(far_future),
            nonce: None,
            forward_hops: None,
        };
        let token = issue_token("000102030405060708090a0b0c0d0e0f", "alice", &claims).unwrap();
        assert_eq!(v.verify(&token), VerifyOutcome::Expired);
    }
}
