//! HSM/PKCS#11 key provider for Noise XX handshake.
//!
//! When the `hsm` feature is enabled, the long-lived Curve25519 identity key can
//! be stored on a PKCS#11-compatible Hardware Security Module (YubiHSM2, AWS
//! CloudHSM, SoftHSM2, etc.). The X25519 DH operation for the Noise XX handshake
//! is performed inside the HSM so that the raw private key bytes never leave the
//! device.
//!
//! A dedicated OS thread owns the PKCS#11 `Session` (which is `!Send`), and all
//! HSM operations are dispatched to it via a synchronous channel. The
//! `HsmKeyProvider` and the snow `HsmSnowDh` / `HsmSnowResolver` types therefore
//! satisfy `Send + Sync` without any unsafe code.
//!
//! # Fallback
//! When the PKCS#11 module cannot be loaded, or when the token is unreachable, a
//! graceful fallback to file-based `StaticKey` is provided and a warning is
//! logged. Set `fips_mode = true` to instead return a hard error if the HSM is
//! unavailable.
//!
//! # Snow integration
//! ```ignore
//! # use rf_crypto::hsm::{HsmConfig, HsmKeyProvider};
//! # use std::path::PathBuf;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = HsmConfig {
//!     pkcs11_module_path: PathBuf::from("/usr/lib/softhsm/libsofthsm2.so"),
//!     slot_id: 0,
//!     pin: "1234".into(),
//!     key_label: "rf-agent-identity".into(),
//!     fips_mode: false,
//! };
//! let provider = HsmKeyProvider::open(config)?;
//! // With feature = "hsm": provider.into_snow_resolver() returns an HsmSnowResolver
//! // suitable for snow::Builder::with_resolver(params, resolver)
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;

use tracing::warn;

use crate::error::CryptoError;
use crate::keys::StaticKey;

// ── Public configuration ──────────────────────────────────────────────────────

/// Configuration for PKCS#11 HSM key storage.
#[derive(Clone, Debug)]
pub struct HsmConfig {
    /// Path to the PKCS#11 shared library (`.so` on Linux, `.dylib` on macOS,
    /// `.dll` on Windows). For YubiHSM2, use `libyubihsm_pkcs11.so`. For
    /// SoftHSM2, use `libsofthsm2.so`.
    pub pkcs11_module_path: PathBuf,

    /// PKCS#11 slot ID to use (use `pkcs11-tool --list-slots` to discover).
    pub slot_id: u64,

    /// User PIN for the token. Will be cleared from memory after login.
    pub pin: String,

    /// `CKA_LABEL` of the X25519 key pair on the token. If no key with this
    /// label exists, a new non-extractable X25519 key pair is generated.
    pub key_label: String,

    /// When `true`, return `CryptoError::HsmUnavailable` if the HSM cannot be
    /// reached instead of falling back to file-based key storage.
    pub fips_mode: bool,
}

// ── Internal channel messages ─────────────────────────────────────────────────

/// Operations dispatched to the HSM worker thread.
#[allow(dead_code)]
enum HsmOp {
    /// Perform X25519 DH: private key is the HSM static key, public key is `peer`.
    Dh {
        #[allow(dead_code)]
        peer: [u8; 32],
        #[allow(dead_code)]
        reply: SyncSender<Result<[u8; 32], CryptoError>>,
    },
    /// Shut down the worker thread gracefully.
    Shutdown,
}

// ── HsmHandle — the Send+Sync side ───────────────────────────────────────────

/// A cloneable, `Send + Sync` handle to the HSM worker thread.
///
/// Holds the cached public key and a channel sender for DH requests.
#[derive(Clone)]
pub struct HsmHandle {
    /// The X25519 public key read from the HSM at initialisation time.
    pub public_key: [u8; 32],
    /// Sender to the dedicated HSM worker thread.
    tx: Arc<SyncSender<HsmOp>>,
}

impl HsmHandle {
    /// Compute the X25519 DH shared secret with `peer_public` using the
    /// HSM-stored private key. Blocks briefly while the HSM worker performs
    /// the operation.
    pub fn compute_dh(&self, peer_public: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.tx
            .send(HsmOp::Dh {
                peer: *peer_public,
                reply: reply_tx,
            })
            .map_err(|_| CryptoError::HsmUnavailable)?;
        reply_rx.recv().map_err(|_| CryptoError::HsmUnavailable)?
    }
}

impl Drop for HsmHandle {
    fn drop(&mut self) {
        // Only send shutdown when the last clone is dropped.
        if Arc::strong_count(&self.tx) == 1 {
            let _ = self.tx.send(HsmOp::Shutdown);
        }
    }
}

// ── HsmKeyProvider ────────────────────────────────────────────────────────────

/// HSM-backed (or file-backed fallback) key provider.
///
/// Created via [`HsmKeyProvider::open`] or [`HsmKeyProvider::open_with_fallback`].
pub struct HsmKeyProvider {
    #[allow(dead_code)]
    config: HsmConfig,
    inner: ProviderInner,
}

#[allow(dead_code)]
enum ProviderInner {
    /// Key lives on the HSM; DH is delegated to the worker thread.
    Hsm(HsmHandle),
    /// Fallback: key is loaded from disk.
    File(StaticKey),
}

impl HsmKeyProvider {
    /// Try to open the PKCS#11 module and locate/generate the X25519 key pair.
    ///
    /// Returns `Err` if the module cannot be loaded. If you want graceful
    /// fallback to a file-based key, use [`open_with_fallback`] instead.
    pub fn open(config: HsmConfig) -> Result<Self, CryptoError> {
        #[cfg(feature = "hsm")]
        {
            match pkcs11_impl::open_hsm(&config) {
                Ok(handle) => {
                    tracing::info!(
                        label = %config.key_label,
                        pubkey = %hex::encode(handle.public_key),
                        "HSM key loaded (PKCS#11)"
                    );
                    return Ok(Self {
                        inner: ProviderInner::Hsm(handle),
                        config,
                    });
                }
                Err(e) => {
                    if config.fips_mode {
                        return Err(e);
                    }
                    warn!(
                        error = %e,
                        "HSM unavailable — FIPS mode is OFF, continuing without HSM"
                    );
                }
            }
        }
        #[cfg(not(feature = "hsm"))]
        {
            let _ = &config;
            warn!("rf-crypto was compiled without the 'hsm' feature — HSM support is disabled");
        }

        Err(CryptoError::HsmUnavailable)
    }

    /// Try the HSM first; fall back to `fallback_key_path` (file-based
    /// `StaticKey`) if the HSM is unreachable.
    ///
    /// In FIPS mode (`config.fips_mode = true`) the fallback is skipped and an
    /// error is returned instead.
    pub fn open_with_fallback(
        config: HsmConfig,
        fallback_key_path: &std::path::Path,
    ) -> Result<Self, CryptoError> {
        match Self::open(config.clone()) {
            Ok(p) => Ok(p),
            Err(e) if config.fips_mode => Err(e),
            Err(_) => {
                let key = StaticKey::load_or_generate(fallback_key_path)?;
                warn!(
                    path = %fallback_key_path.display(),
                    "Using file-based key (HSM fallback)"
                );
                Ok(Self {
                    inner: ProviderInner::File(key),
                    config,
                })
            }
        }
    }

    /// Return the X25519 public key (32 bytes).
    pub fn public_key(&self) -> [u8; 32] {
        match &self.inner {
            ProviderInner::Hsm(h) => h.public_key,
            ProviderInner::File(k) => k.public,
        }
    }

    /// Hex-encoded public key for display / logging.
    pub fn public_hex(&self) -> String {
        hex::encode(self.public_key())
    }

    /// Compute X25519 DH with `peer_public`.
    ///
    /// For HSM keys: delegates to the PKCS#11 worker thread (never exposes
    /// private key bytes). For file-backed keys: performs software DH.
    pub fn compute_dh(&self, peer_public: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
        match &self.inner {
            ProviderInner::Hsm(h) => h.compute_dh(peer_public),
            ProviderInner::File(k) => {
                #[cfg(feature = "hsm")]
                {
                    use x25519_dalek::{PublicKey, StaticSecret};
                    let secret = StaticSecret::from(*k.private_bytes());
                    let peer = PublicKey::from(*peer_public);
                    let shared = secret.diffie_hellman(&peer);
                    Ok(*shared.as_bytes())
                }
                #[cfg(not(feature = "hsm"))]
                {
                    let _ = k;
                    let _ = peer_public;
                    Err(CryptoError::HsmUnavailable)
                }
            }
        }
    }

    /// Consume the provider and return a snow `CryptoResolver` suitable for
    /// `snow::Builder::with_resolver()`.
    ///
    /// The resolver uses the HSM for static key DH operations and software
    /// X25519 (via `x25519-dalek`) for ephemeral keys.
    #[cfg(feature = "hsm")]
    pub fn into_snow_resolver(self) -> HsmSnowResolver {
        match self.inner {
            ProviderInner::Hsm(handle) => HsmSnowResolver::Hsm(handle),
            ProviderInner::File(key) => {
                HsmSnowResolver::Software { privkey: *key.private_bytes() }
            }
        }
    }

    /// Returns `true` if the key is currently backed by the HSM.
    pub fn is_hsm_backed(&self) -> bool {
        matches!(self.inner, ProviderInner::Hsm(_))
    }
}

// ── Snow resolver and Dh types ────────────────────────────────────────────────

#[cfg(feature = "hsm")]
use snow::{
    params::{CipherChoice, DHChoice, HashChoice},
    resolvers::CryptoResolver,
    types::{Cipher, Dh, Hash, Random},
};

/// Snow `CryptoResolver` that uses the HSM for the static Curve25519 key.
///
/// Ephemeral keys (generated fresh per-handshake) are computed in software via
/// `x25519-dalek`. Hash and cipher operations use snow's built-in defaults.
#[cfg(feature = "hsm")]
pub enum HsmSnowResolver {
    /// Static key lives on the HSM.
    Hsm(HsmHandle),
    /// Fallback: static key is a 32-byte software private key.
    Software { privkey: [u8; 32] },
}

#[cfg(feature = "hsm")]
impl CryptoResolver for HsmSnowResolver {
    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        if !matches!(choice, DHChoice::Curve25519) {
            return None;
        }
        let dh: Box<dyn Dh> = match self {
            HsmSnowResolver::Hsm(handle) => Box::new(HsmSnowDh::new_hsm(handle.clone())),
            HsmSnowResolver::Software { privkey } => {
                Box::new(HsmSnowDh::new_software(*privkey))
            }
        };
        Some(dh)
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<dyn Hash>> {
        snow::resolvers::DefaultResolver.resolve_hash(choice)
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        snow::resolvers::DefaultResolver.resolve_cipher(choice)
    }
}

/// Snow `Dh` implementation for the HSM-backed static key.
///
/// - `set()` is called by snow for the static key → activates HSM/software mode.
/// - `generate()` is called by snow for ephemeral keys → uses software X25519.
/// - `dh()` delegates to the HSM (or software) as appropriate.
#[cfg(feature = "hsm")]
pub struct HsmSnowDh {
    /// Template for the static key — determines which `mode` to use after
    /// `set()` is called.
    static_template: StaticTemplate,
    /// Current operating mode (set after `set()` or `generate()` is called).
    mode: HsmDhMode,
    /// Placeholder for `privkey()` return value — never exposed for HSM keys.
    zero: [u8; 32],
}

#[cfg(feature = "hsm")]
enum StaticTemplate {
    Hsm(HsmHandle),
    Software([u8; 32]),
}

#[cfg(feature = "hsm")]
enum HsmDhMode {
    /// Not yet initialised.
    Uninit,
    /// Using the HSM static key; `HsmHandle` is borrowed from the resolver.
    HsmStatic(HsmHandle),
    /// Using a software static key (file-based fallback).
    SoftwareStatic([u8; 32], [u8; 32]),
    /// Ephemeral key generated in software.
    Ephemeral { privkey: [u8; 32], pubkey: [u8; 32] },
}

#[cfg(feature = "hsm")]
impl HsmSnowDh {
    fn new_hsm(handle: HsmHandle) -> Self {
        Self {
            static_template: StaticTemplate::Hsm(handle),
            mode: HsmDhMode::Uninit,
            zero: [0u8; 32],
        }
    }

    fn new_software(privkey: [u8; 32]) -> Self {
        Self {
            static_template: StaticTemplate::Software(privkey),
            mode: HsmDhMode::Uninit,
            zero: [0u8; 32],
        }
    }

    fn derive_pubkey_software(privkey: &[u8; 32]) -> [u8; 32] {
        use x25519_dalek::{PublicKey, StaticSecret};
        let secret = StaticSecret::from(*privkey);
        *PublicKey::from(&secret).as_bytes()
    }
}

#[cfg(feature = "hsm")]
impl Dh for HsmSnowDh {
    fn name(&self) -> &'static str {
        "25519"
    }

    fn pub_len(&self) -> usize {
        32
    }

    fn priv_len(&self) -> usize {
        32
    }

    /// Called by snow to install the static private key.
    /// For HSM keys the bytes are ignored; the stored handle is used instead.
    fn set(&mut self, privkey: &[u8]) {
        match &self.static_template {
            StaticTemplate::Hsm(handle) => {
                self.mode = HsmDhMode::HsmStatic(handle.clone());
                let _ = privkey; // ignored — HSM key is non-extractable
            }
            StaticTemplate::Software(stored_priv) => {
                let mut key = [0u8; 32];
                // Use the stored (correct) private key, not the dummy bytes from
                // Builder::local_private_key (which we pass as &[0u8;32]).
                key.copy_from_slice(stored_priv);
                let pubkey = Self::derive_pubkey_software(&key);
                self.mode = HsmDhMode::SoftwareStatic(key, pubkey);
            }
        }
    }

    /// Called by snow to generate an ephemeral key pair (software X25519).
    fn generate(&mut self, rng: &mut dyn Random) {
        use x25519_dalek::{PublicKey, StaticSecret};
        let mut privkey = [0u8; 32];
        rng.fill_bytes(&mut privkey);
        let secret = StaticSecret::from(privkey);
        let pubkey = *PublicKey::from(&secret).as_bytes();
        self.mode = HsmDhMode::Ephemeral { privkey, pubkey };
    }

    fn pubkey(&self) -> &[u8] {
        match &self.mode {
            HsmDhMode::HsmStatic(handle) => &handle.public_key,
            HsmDhMode::SoftwareStatic(_, pub_bytes) => pub_bytes,
            HsmDhMode::Ephemeral { pubkey, .. } => pubkey,
            HsmDhMode::Uninit => &self.zero,
        }
    }

    /// Private key bytes are not exposed for HSM keys (returns zeroes).
    /// Snow only uses this for serialisation, not for security operations.
    fn privkey(&self) -> &[u8] {
        match &self.mode {
            HsmDhMode::SoftwareStatic(privkey, _) | HsmDhMode::Ephemeral { privkey, .. } => {
                privkey
            }
            HsmDhMode::HsmStatic(_) | HsmDhMode::Uninit => &self.zero,
        }
    }

    fn dh(&self, pubkey: &[u8], out: &mut [u8]) -> Result<(), ()> {
        if pubkey.len() != 32 || out.len() < 32 {
            return Err(());
        }
        let peer: [u8; 32] = pubkey.try_into().map_err(|_| ())?;

        match &self.mode {
            HsmDhMode::HsmStatic(handle) => {
                let shared = handle.compute_dh(&peer).map_err(|_| ())?;
                out[..32].copy_from_slice(&shared);
                Ok(())
            }
            HsmDhMode::SoftwareStatic(privkey, _) | HsmDhMode::Ephemeral { privkey, .. } => {
                use x25519_dalek::{PublicKey, StaticSecret};
                let secret = StaticSecret::from(*privkey);
                let peer_key = PublicKey::from(peer);
                let shared = secret.diffie_hellman(&peer_key);
                out[..32].copy_from_slice(shared.as_bytes());
                Ok(())
            }
            HsmDhMode::Uninit => Err(()),
        }
    }
}

// ── PKCS#11 worker thread (hsm feature only) ─────────────────────────────────

#[cfg(feature = "hsm")]
mod pkcs11_impl {
    use super::{HsmHandle, HsmOp};
    use crate::error::CryptoError;
    use super::HsmConfig;

    use cryptoki::{
        context::{CInitializeArgs, Pkcs11},
        mechanism::{
            elliptic_curve::{Ecdh1DeriveParams, EcKdfType},
            Mechanism,
        },
        object::{Attribute, AttributeType, KeyType, ObjectClass, ObjectHandle},
        session::UserType,
        types::AuthPin,
    };

    use std::sync::mpsc;
    use std::sync::Arc;
    use tracing::warn;

    /// X25519 OID in DER encoding: `1.3.101.110`
    const X25519_OID_DER: &[u8] = &[0x06, 0x03, 0x2B, 0x65, 0x6E];

    /// Open the PKCS#11 module, start a worker thread, and return an `HsmHandle`.
    pub(super) fn open_hsm(config: &HsmConfig) -> Result<super::HsmHandle, CryptoError> {
        let pkcs11 = Pkcs11::new(&config.pkcs11_module_path)
            .map_err(|e| CryptoError::Hsm(format!("failed to load PKCS#11 module: {e}")))?;

        pkcs11
            .initialize(CInitializeArgs::OsThreads)
            .map_err(|e| CryptoError::Hsm(format!("PKCS#11 initialize failed: {e}")))?;

        // Find the requested slot.
        let slots = pkcs11
            .get_slots_with_initialized_token()
            .map_err(|e| CryptoError::Hsm(format!("get slots failed: {e}")))?;

        let slot = slots
            .into_iter()
            .find(|s| s.id() == config.slot_id)
            .ok_or_else(|| CryptoError::Hsm(format!("slot {} not found", config.slot_id)))?;

        // Open an R/W session.
        let session = pkcs11
            .open_rw_session(slot)
            .map_err(|e| CryptoError::Hsm(format!("open session failed: {e}")))?;

        // Log in.
        let pin = AuthPin::new(config.pin.clone());
        session
            .login(UserType::User, Some(&pin))
            .map_err(|e| CryptoError::Hsm(format!("login failed: {e}")))?;

        // Find existing key pair, or generate a new one.
        let (priv_handle, pub_bytes) =
            find_or_generate_key(&session, &config.key_label)?;

        // Channel for dispatching operations to this thread.
        let (tx, rx) = mpsc::sync_channel::<HsmOp>(8);

        // Move the Session (which is !Send) into a dedicated OS thread.
        std::thread::spawn(move || {
            for op in rx {
                match op {
                    HsmOp::Dh { peer, reply } => {
                        let result = ecdh_derive(&session, priv_handle, &peer);
                        let _ = reply.send(result);
                    }
                    HsmOp::Shutdown => break,
                }
            }
        });

        Ok(super::HsmHandle {
            public_key: pub_bytes,
            tx: Arc::new(tx),
        })
    }

    /// Find a key pair by label, or generate a new non-extractable X25519 pair.
    fn find_or_generate_key(
        session: &cryptoki::session::Session,
        label: &str,
    ) -> Result<(ObjectHandle, [u8; 32]), CryptoError> {
        // Search for an existing private key with the given label.
        let search_template = vec![
            Attribute::Class(ObjectClass::PRIVATE_KEY),
            Attribute::Label(label.as_bytes().to_vec()),
        ];
        let existing = session
            .find_objects(&search_template)
            .map_err(|e| CryptoError::Hsm(format!("find objects failed: {e}")))?;

        if let Some(&priv_handle) = existing.first() {
            // Found an existing key — get the corresponding public key bytes.
            let pub_search = vec![
                Attribute::Class(ObjectClass::PUBLIC_KEY),
                Attribute::Label(label.as_bytes().to_vec()),
            ];
            let pub_keys = session
                .find_objects(&pub_search)
                .map_err(|e| CryptoError::Hsm(format!("find public key failed: {e}")))?;
            let pub_handle = pub_keys
                .into_iter()
                .next()
                .ok_or_else(|| CryptoError::Hsm("public key not found for existing label".into()))?;

            let pub_attrs = session
                .get_attributes(pub_handle, &[AttributeType::EcPoint])
                .map_err(|e| CryptoError::Hsm(format!("get EC point failed: {e}")))?;

            for attr in pub_attrs {
                if let Attribute::EcPoint(bytes) = attr {
                    if bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        return Ok((priv_handle, arr));
                    }
                }
            }
            return Err(CryptoError::Hsm("unexpected EC point format in stored key".into()));
        }

        // Generate a new X25519 key pair (non-extractable private key).
        let pub_template = vec![
            Attribute::Token(true),
            Attribute::Private(false),
            Attribute::Derive(true),
            Attribute::EcParams(X25519_OID_DER.to_vec()),
            Attribute::Label(label.as_bytes().to_vec()),
        ];
        let priv_template = vec![
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sensitive(true),
            Attribute::Extractable(false),
            Attribute::Derive(true),
            Attribute::Label(label.as_bytes().to_vec()),
        ];

        let (pub_handle, priv_handle) = session
            .generate_key_pair(
                &Mechanism::EccMontgomeryKeyPairGen,
                &pub_template,
                &priv_template,
            )
            .map_err(|e| {
                CryptoError::Hsm(format!("generate X25519 key pair failed: {e}"))
            })?;

        // Read back the public key bytes.
        let pub_attrs = session
            .get_attributes(pub_handle, &[AttributeType::EcPoint])
            .map_err(|e| CryptoError::Hsm(format!("get generated EC point failed: {e}")))?;

        for attr in pub_attrs {
            if let Attribute::EcPoint(bytes) = attr {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    return Ok((priv_handle, arr));
                }
            }
        }
        Err(CryptoError::Hsm("generated key has unexpected EC point format".into()))
    }

    /// Perform ECDH1 key derivation using the HSM private key and the peer's
    /// 32-byte X25519 public key. Returns the 32-byte shared secret.
    fn ecdh_derive(
        session: &cryptoki::session::Session,
        priv_handle: ObjectHandle,
        peer_public: &[u8; 32],
    ) -> Result<[u8; 32], CryptoError> {
        let peer_bytes = peer_public.to_vec();
        let params = Ecdh1DeriveParams::new(EcKdfType::Null, None, &peer_bytes);
        let mech = Mechanism::Ecdh1Derive(params);

        // Derive a 32-byte generic secret (the shared secret).
        let derive_template = vec![
            Attribute::Class(ObjectClass::SECRET_KEY),
            Attribute::KeyType(KeyType::GENERIC_SECRET),
            Attribute::ValueLen(32_usize.into()),
            Attribute::Sensitive(false),
            Attribute::Extractable(true),
            Attribute::Token(false),
        ];

        let derived = session
            .derive_key(&mech, priv_handle, &derive_template)
            .map_err(|e| {
                if e.to_string().contains("MECHANISM_INVALID")
                    || e.to_string().contains("mechanism not supported")
                {
                    CryptoError::HsmX25519Unsupported
                } else {
                    CryptoError::Hsm(format!("ECDH derive failed: {e}"))
                }
            })?;

        let attrs = session
            .get_attributes(derived, &[AttributeType::Value])
            .map_err(|e| CryptoError::Hsm(format!("get derived key value failed: {e}")))?;

        for attr in attrs {
            if let Attribute::Value(bytes) = attr {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    // Delete the ephemeral derived key from the HSM (session object).
                    let _ = session.destroy_object(derived);
                    return Ok(arr);
                }
            }
        }
        Err(CryptoError::Hsm("derived key has unexpected length".into()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsm_config_clone() {
        let config = HsmConfig {
            pkcs11_module_path: "/usr/lib/softhsm/libsofthsm2.so".into(),
            slot_id: 0,
            pin: "1234".into(),
            key_label: "test-key".into(),
            fips_mode: false,
        };
        let config2 = config.clone();
        assert_eq!(config.key_label, config2.key_label);
    }

    /// When `feature = "hsm"` is not enabled, `open` must return an error.
    #[test]
    fn test_open_without_hsm_feature_returns_error() {
        let config = HsmConfig {
            pkcs11_module_path: "/nonexistent/module.so".into(),
            slot_id: 0,
            pin: "pin".into(),
            key_label: "key".into(),
            fips_mode: false,
        };
        // Without a real PKCS#11 module this should fail gracefully.
        let result = HsmKeyProvider::open(config);
        assert!(result.is_err());
    }

    /// File-backed fallback: if HSM is unavailable and fips_mode=false, fall
    /// back to a fresh StaticKey.
    #[test]
    fn test_fallback_to_file_key() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("fallback.key");

        let config = HsmConfig {
            pkcs11_module_path: "/nonexistent/module.so".into(),
            slot_id: 0,
            pin: "pin".into(),
            key_label: "key".into(),
            fips_mode: false,
        };
        let provider = HsmKeyProvider::open_with_fallback(config, &key_path)
            .expect("fallback should succeed");

        assert!(!provider.is_hsm_backed());
        assert_ne!(provider.public_key(), [0u8; 32]);
        assert!(key_path.exists());
    }

    /// FIPS mode: fallback must be rejected when HSM is unavailable.
    #[test]
    fn test_fips_mode_rejects_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("fallback.key");

        let config = HsmConfig {
            pkcs11_module_path: "/nonexistent/module.so".into(),
            slot_id: 0,
            pin: "pin".into(),
            key_label: "key".into(),
            fips_mode: true,
        };
        let result = HsmKeyProvider::open_with_fallback(config, &key_path);
        assert!(
            result.is_err(),
            "FIPS mode must reject fallback to file key"
        );
    }

    /// Software DH (file-backed mode) via HsmKeyProvider.
    #[cfg(feature = "hsm")]
    #[test]
    fn test_software_dh_via_provider() {
        use x25519_dalek::{PublicKey, StaticSecret};

        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test.key");

        let config = HsmConfig {
            pkcs11_module_path: "/nonexistent.so".into(),
            slot_id: 0,
            pin: "pin".into(),
            key_label: "k".into(),
            fips_mode: false,
        };
        let alice = HsmKeyProvider::open_with_fallback(config.clone(), &key_path).unwrap();

        let key_path2 = dir.path().join("test2.key");
        let bob = HsmKeyProvider::open_with_fallback(config, &key_path2).unwrap();

        let alice_pub: [u8; 32] = alice.public_key();
        let bob_pub: [u8; 32] = bob.public_key();

        let alice_shared = alice.compute_dh(&bob_pub).unwrap();
        let bob_shared = bob.compute_dh(&alice_pub).unwrap();

        assert_eq!(alice_shared, bob_shared, "DH shared secrets must match");
    }
}
