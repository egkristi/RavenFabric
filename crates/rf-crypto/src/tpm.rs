//! TPM 2.0 key storage for the RavenFabric identity key.
//!
//! When the `tpm` feature is enabled, this module wraps `tss-esapi` to provide:
//!
//! - **Key sealing** — bind a `StaticKey`'s private bytes to the current PCR
//!   measurement bank so they can only be unsealed on the same machine in the
//!   same measured boot state.
//! - **Key unsealing** — verify PCR state then recover the private key bytes
//!   and reconstruct the `StaticKey`.
//! - **Remote attestation** — produce a TPM2_Quote over a caller-specified set
//!   of PCR indices bound to a freshness nonce.
//! - **Measured boot verification** — check PCR extensions against expected
//!   values to assert that only authorised firmware and OS components booted.
//!
//! # Security properties
//! - The private key bytes are encrypted by the TPM's Storage Root Key (SRK)
//!   so they cannot be recovered without the TPM and the correct PCR state.
//! - Changing BIOS firmware, bootloader, or kernel (any component measured
//!   into the sealed PCRs) invalidates the seal.
//! - The attestation quote is signed by the TPM's Attestation Key (AK) and
//!   includes the freshness nonce to prevent replay attacks.
//!
//! # TPM connection
//! Requires a reachable TPM device:
//! - Linux: `/dev/tpm0` (raw) or `/dev/tpmrm0` (resource-managed, preferred)
//! - Windows: TPM via the in-kernel driver (TCTI)
//! - Simulator: `--tcti simulator:port=2321` for testing with `tpm2-simulator`

use crate::error::CryptoError;
use crate::keys::StaticKey;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for TPM-backed key sealing.
#[derive(Clone, Debug)]
pub struct TpmConfig {
    /// PCR bank (SHA-256 recommended) indices to include in the sealing policy.
    ///
    /// Typical choices:
    /// - `[0]` — firmware (BIOS/UEFI)
    /// - `[0, 1, 2, 3]` — full firmware + config block
    /// - `[0, 7]` — firmware + Secure Boot state
    /// - `[0, 7, 11, 14]` — firmware + Secure Boot + bootloader + shim
    pub pcr_list: Vec<u32>,

    /// NV index to use for storing TPM-sealed key metadata.
    /// Must be in the range `0x01000000..=0x01FFFFFF`.
    pub nv_index: u32,

    /// Optional label for display / audit logging.
    pub key_label: String,

    /// Preferred TCTI connection string (e.g. `"device:/dev/tpmrm0"` or
    /// `"tabrmd:"` for the resource manager daemon). If `None`, the library
    /// default is used.
    pub tcti: Option<String>,
}

impl Default for TpmConfig {
    fn default() -> Self {
        Self {
            pcr_list: vec![0, 7],
            nv_index: 0x01000001,
            key_label: "rf-agent-identity".into(),
            tcti: None,
        }
    }
}

// ── Sealed key blob ───────────────────────────────────────────────────────────

/// An opaque blob representing a TPM-sealed identity key.
///
/// The blob is produced by [`TpmKeyStore::seal`] and consumed by
/// [`TpmKeyStore::unseal`]. It can be stored in a file (e.g. alongside the
/// agent configuration).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SealedKeyBlob {
    /// Serialised TPM2B_PUBLIC structure (base64-encoded).
    pub public_blob: Vec<u8>,
    /// Serialised TPM2B_PRIVATE structure (base64-encoded, encrypted by SRK).
    pub private_blob: Vec<u8>,
    /// PCR indices that were active when the key was sealed.
    pub pcr_list: Vec<u32>,
}

// ── TpmKeyStore ───────────────────────────────────────────────────────────────

/// Provides TPM-backed sealing and unsealing of the RavenFabric identity key.
pub struct TpmKeyStore {
    #[allow(dead_code)]
    config: TpmConfig,
}

impl TpmKeyStore {
    /// Open a connection to the TPM.
    ///
    /// On Linux this typically requires read access to `/dev/tpmrm0`. Run the
    /// agent as a member of the `tss` group, or with `CAP_DAC_OVERRIDE`.
    pub fn new(config: TpmConfig) -> Result<Self, CryptoError> {
        #[cfg(feature = "tpm")]
        {
            // Verify the TPM is reachable by opening and immediately closing a
            // context. Actual operations open their own context per call to
            // avoid holding a long-lived resource.
            let _ctx = tpm_impl::open_context(&config)?;
            tracing::info!(
                label = %config.key_label,
                pcrs = ?config.pcr_list,
                "TPM 2.0 key store initialised"
            );
        }
        #[cfg(not(feature = "tpm"))]
        {
            let _ = &config;
            return Err(CryptoError::Tpm(
                "rf-crypto compiled without the 'tpm' feature".into(),
            ));
        }
        #[cfg(feature = "tpm")]
        {
            return Ok(Self { config });
        }

        // Safety: one of the cfg blocks above always returns.
        #[allow(unreachable_code)]
        Err(CryptoError::Tpm("unreachable".into()))
    }

    /// Seal `key` to the current PCR state. Returns a [`SealedKeyBlob`] that
    /// can be persisted alongside the agent configuration.
    ///
    /// **Security**: the `key.private` bytes are encrypted by the TPM's SRK and
    /// will only be decryptable on the same TPM in the same PCR state.
    pub fn seal(&self, key: &StaticKey) -> Result<SealedKeyBlob, CryptoError> {
        #[cfg(feature = "tpm")]
        {
            tpm_impl::seal_key(&self.config, key.private_bytes())
        }
        #[cfg(not(feature = "tpm"))]
        {
            let _ = key;
            Err(CryptoError::Tpm(
                "rf-crypto compiled without the 'tpm' feature".into(),
            ))
        }
    }

    /// Unseal a previously sealed key. Fails with [`CryptoError::TpmUnsealFailed`]
    /// if the PCR state has changed since sealing (e.g. different firmware).
    pub fn unseal(&self, blob: &SealedKeyBlob) -> Result<StaticKey, CryptoError> {
        #[cfg(feature = "tpm")]
        {
            let private_bytes = tpm_impl::unseal_key(&self.config, blob)?;
            let key = StaticKey::from_private_bytes(&private_bytes)?;
            tracing::info!(
                pubkey = %key.public_hex(),
                "TPM unseal succeeded"
            );
            Ok(key)
        }
        #[cfg(not(feature = "tpm"))]
        {
            let _ = blob;
            Err(CryptoError::Tpm(
                "rf-crypto compiled without the 'tpm' feature".into(),
            ))
        }
    }
}

// ── TpmAttestation ────────────────────────────────────────────────────────────

/// Provides TPM 2.0 remote attestation services.
pub struct TpmAttestation {
    #[allow(dead_code)]
    config: TpmConfig,
}

impl TpmAttestation {
    /// Create an attestation provider using the same TPM configuration as the
    /// key store.
    pub fn new(config: TpmConfig) -> Self {
        Self { config }
    }

    /// Produce a TPM2_Quote over the specified `pcr_list` bound to `nonce`.
    ///
    /// The returned bytes are a serialised `TPMS_ATTEST` + signature pair that
    /// can be sent to a verifier for remote attestation.
    ///
    /// `nonce` must be fresh (e.g. random bytes provided by the verifier) to
    /// prevent replay attacks. Maximum length is 64 bytes (TPM2 maximum for
    /// qualifying data).
    pub fn quote(&self, pcr_list: &[u32], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if nonce.len() > 64 {
            return Err(CryptoError::Tpm(
                "nonce too long: max 64 bytes for TPM qualifying data".into(),
            ));
        }
        #[cfg(feature = "tpm")]
        {
            tpm_impl::tpm_quote(&self.config, pcr_list, nonce)
        }
        #[cfg(not(feature = "tpm"))]
        {
            let _ = (pcr_list, nonce);
            Err(CryptoError::Tpm(
                "rf-crypto compiled without the 'tpm' feature".into(),
            ))
        }
    }

    /// Verify that the current PCR values match the expected set.
    ///
    /// Fails with [`CryptoError::TpmUnsealFailed`] if any PCR value differs
    /// from the expected, indicating tampered firmware or an unrecognised boot
    /// path.
    pub fn verify_measured_boot(
        &self,
        expected_pcrs: &[(u32, [u8; 32])],
    ) -> Result<(), CryptoError> {
        #[cfg(feature = "tpm")]
        {
            tpm_impl::verify_pcrs(&self.config, expected_pcrs)
        }
        #[cfg(not(feature = "tpm"))]
        {
            let _ = expected_pcrs;
            Err(CryptoError::Tpm(
                "rf-crypto compiled without the 'tpm' feature".into(),
            ))
        }
    }
}

// ── tss-esapi implementation (tpm feature only) ───────────────────────────────

#[cfg(feature = "tpm")]
mod tpm_impl {
    use super::{SealedKeyBlob, TpmConfig};
    use crate::error::CryptoError;

    use tss_esapi::{
        Context,
        attributes::{ObjectAttributesBuilder, SessionAttributesBuilder},
        constants::SessionType,
        handles::PcrHandle,
        interface_types::{
            algorithm::{HashingAlgorithm, PublicAlgorithm, SymmetricMode},
            resource_handles::Hierarchy,
            session_handles::PolicySession,
        },
        structures::{
            Digest, MaxBuffer, PcrSelectionListBuilder, PublicBuilder, SensitiveCreate,
            SensitiveData, SymmetricDefinitionObject,
        },
        tcti_ldr::TctiNameConf,
    };

    use std::str::FromStr;

    /// Open a TPM context using the configured TCTI.
    pub(super) fn open_context(config: &TpmConfig) -> Result<Context, CryptoError> {
        let tcti = if let Some(s) = &config.tcti {
            TctiNameConf::from_str(s)
                .map_err(|e| CryptoError::Tpm(format!("invalid TCTI string '{s}': {e}")))?
        } else {
            TctiNameConf::from_str("device:/dev/tpmrm0")
                .unwrap_or_else(|_| TctiNameConf::from_str("tabrmd:").unwrap())
        };
        Context::new(tcti).map_err(|e| CryptoError::Tpm(format!("TPM context open failed: {e}")))
    }

    /// Seal `private_key_bytes` (32 bytes) under a PCR policy.
    pub(super) fn seal_key(
        config: &TpmConfig,
        private_key_bytes: &[u8; 32],
    ) -> Result<SealedKeyBlob, CryptoError> {
        let mut ctx = open_context(config)?;

        // Build the PCR selection for the policy.
        let pcr_selection = build_pcr_selection(&config.pcr_list)?;

        // Create a trial policy session to calculate the policy digest.
        let trial_session = ctx
            .start_auth_session(
                None,
                None,
                None,
                SessionType::Trial,
                tss_esapi::structures::SymmetricDefinition::AES_128_CFB,
                HashingAlgorithm::Sha256,
            )
            .map_err(|e| CryptoError::Tpm(format!("trial session failed: {e}")))?;

        let trial_session_handle = PolicySession::try_from(trial_session)
            .map_err(|e| CryptoError::Tpm(format!("trial session cast failed: {e}")))?;

        ctx.policy_pcr(
            trial_session_handle,
            &Digest::default(),
            pcr_selection.clone(),
        )
        .map_err(|e| CryptoError::Tpm(format!("policy_pcr failed: {e}")))?;

        let policy_digest = ctx
            .policy_get_digest(trial_session_handle)
            .map_err(|e| CryptoError::Tpm(format!("policy_get_digest failed: {e}")))?;

        // Create the primary key (SRK equivalent) under Endorsement hierarchy.
        let primary = ctx
            .create_primary(
                Hierarchy::Owner,
                srk_public_template()?,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| CryptoError::Tpm(format!("create_primary failed: {e}")))?;

        // Seal the private key bytes.
        let sensitive_data = SensitiveData::try_from(private_key_bytes.to_vec())
            .map_err(|e| CryptoError::Tpm(format!("sensitive data construction failed: {e}")))?;

        let sealed_template = sealed_object_template(policy_digest)?;

        let (sealed_public, sealed_private, _, _, _) = ctx
            .create(
                primary.key_handle,
                sealed_template,
                None,
                Some(&SensitiveCreate::new(sensitive_data, None)),
                None,
                None,
            )
            .map_err(|e| CryptoError::TpmSealFailed)?;

        Ok(SealedKeyBlob {
            public_blob: sealed_public.marshal().unwrap_or_default(),
            private_blob: sealed_private.to_vec(),
            pcr_list: config.pcr_list.clone(),
        })
    }

    /// Unseal a `SealedKeyBlob` and return the 32-byte private key.
    pub(super) fn unseal_key(
        config: &TpmConfig,
        blob: &SealedKeyBlob,
    ) -> Result<[u8; 32], CryptoError> {
        let mut ctx = open_context(config)?;

        let pcr_selection = build_pcr_selection(&config.pcr_list)?;

        // Create a real policy session and satisfy PCR policy.
        let policy_session = ctx
            .start_auth_session(
                None,
                None,
                None,
                SessionType::Policy,
                tss_esapi::structures::SymmetricDefinition::AES_128_CFB,
                HashingAlgorithm::Sha256,
            )
            .map_err(|e| CryptoError::Tpm(format!("policy session failed: {e}")))?;

        let policy_session_handle = PolicySession::try_from(policy_session)
            .map_err(|e| CryptoError::Tpm(format!("policy session cast failed: {e}")))?;

        ctx.policy_pcr(policy_session_handle, &Digest::default(), pcr_selection)
            .map_err(|_| CryptoError::TpmUnsealFailed)?;

        // Load the sealed object.
        let primary = ctx
            .create_primary(
                Hierarchy::Owner,
                srk_public_template()?,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| CryptoError::Tpm(format!("create_primary for unseal failed: {e}")))?;

        let public = tss_esapi::structures::Public::unmarshal(&blob.public_blob)
            .map_err(|e| CryptoError::Tpm(format!("unmarshal public failed: {e}")))?;

        let private = tss_esapi::structures::Private::try_from(blob.private_blob.clone())
            .map_err(|e| CryptoError::Tpm(format!("private blob construction failed: {e}")))?;

        let loaded = ctx
            .load(primary.key_handle, private, public)
            .map_err(|_| CryptoError::TpmUnsealFailed)?;

        let sensitive = ctx
            .unseal(loaded.into())
            .map_err(|_| CryptoError::TpmUnsealFailed)?;

        let bytes = sensitive.to_vec();
        if bytes.len() != 32 {
            return Err(CryptoError::Tpm(format!(
                "unexpected unsealed data length: {} (expected 32)",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }

    /// Produce a TPM2_Quote attestation blob.
    pub(super) fn tpm_quote(
        config: &TpmConfig,
        pcr_list: &[u32],
        nonce: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        let mut ctx = open_context(config)?;
        let pcr_selection = build_pcr_selection(pcr_list)?;

        let qualifying = MaxBuffer::try_from(nonce.to_vec())
            .map_err(|e| CryptoError::Tpm(format!("nonce construction failed: {e}")))?;

        // Create an ephemeral Attestation Key for signing the quote.
        let ak = ctx
            .create_primary(
                Hierarchy::Endorsement,
                ak_public_template()?,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| CryptoError::Tpm(format!("create AK failed: {e}")))?;

        let (attest, signature) = ctx
            .quote(ak.key_handle, &qualifying, None, pcr_selection)
            .map_err(|e| CryptoError::Tpm(format!("TPM2_Quote failed: {e}")))?;

        // Serialise attest + signature into a single blob.
        let mut blob = Vec::new();
        blob.extend_from_slice(&attest.marshal().unwrap_or_default());
        blob.extend_from_slice(&signature.marshal().unwrap_or_default());
        Ok(blob)
    }

    /// Verify that the current TPM PCR values match `expected_pcrs`.
    pub(super) fn verify_pcrs(
        config: &TpmConfig,
        expected_pcrs: &[(u32, [u8; 32])],
    ) -> Result<(), CryptoError> {
        let mut ctx = open_context(config)?;

        let indices: Vec<u32> = expected_pcrs.iter().map(|(i, _)| *i).collect();
        let pcr_selection = build_pcr_selection(&indices)?;

        let (_update_counter, _selection, digest_list) = ctx
            .pcr_read(pcr_selection)
            .map_err(|e| CryptoError::Tpm(format!("pcr_read failed: {e}")))?;

        let digests: Vec<Vec<u8>> = digest_list.value().iter().map(|d| d.to_vec()).collect();

        for (i, (idx, expected)) in expected_pcrs.iter().enumerate() {
            let actual = digests
                .get(i)
                .ok_or_else(|| CryptoError::Tpm(format!("PCR {idx} not in read result")))?;
            if actual.as_slice() != expected.as_slice() {
                return Err(CryptoError::TpmUnsealFailed);
            }
        }
        Ok(())
    }

    // ── Template helpers ──────────────────────────────────────────────────────

    fn build_pcr_selection(
        pcr_list: &[u32],
    ) -> Result<tss_esapi::structures::PcrSelectionList, CryptoError> {
        let mut builder = PcrSelectionListBuilder::new();
        for &idx in pcr_list {
            let handle = PcrHandle::try_from(idx)
                .map_err(|_| CryptoError::Tpm(format!("invalid PCR index: {idx}")))?;
            builder = builder.with_selection(HashingAlgorithm::Sha256, &[handle]);
        }
        builder
            .build()
            .map_err(|e| CryptoError::Tpm(format!("PCR selection build failed: {e}")))
    }

    fn srk_public_template() -> Result<tss_esapi::structures::Public, CryptoError> {
        let obj_attrs = ObjectAttributesBuilder::new()
            .with_fixed_tpm(true)
            .with_fixed_parent(true)
            .with_sensitive_data_origin(true)
            .with_user_with_auth(true)
            .with_decrypt(true)
            .with_restricted(true)
            .build()
            .map_err(|e| CryptoError::Tpm(format!("object attributes build failed: {e}")))?;

        PublicBuilder::new()
            .with_public_algorithm(PublicAlgorithm::SymCipher)
            .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
            .with_object_attributes(obj_attrs)
            .with_symmetric_cipher_parameters(
                SymmetricDefinitionObject::try_from((
                    tss_esapi::interface_types::algorithm::SymmetricAlgorithm::Aes,
                    128,
                    SymmetricMode::Cfb,
                ))
                .map_err(|e| CryptoError::Tpm(format!("sym def failed: {e}")))?,
            )
            .with_symmetric_cipher_unique_identifier(Default::default())
            .build()
            .map_err(|e| CryptoError::Tpm(format!("public build failed: {e}")))
    }

    fn sealed_object_template(
        policy_digest: Digest,
    ) -> Result<tss_esapi::structures::Public, CryptoError> {
        let obj_attrs = ObjectAttributesBuilder::new()
            .with_user_with_auth(false)
            .with_admin_with_policy(true)
            .with_no_da(true)
            .build()
            .map_err(|e| CryptoError::Tpm(format!("sealed obj attrs failed: {e}")))?;

        PublicBuilder::new()
            .with_public_algorithm(PublicAlgorithm::KeyedHash)
            .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
            .with_object_attributes(obj_attrs)
            .with_auth_policy(policy_digest)
            .with_keyed_hash_parameters(Default::default())
            .with_keyed_hash_unique_identifier(Default::default())
            .build()
            .map_err(|e| CryptoError::Tpm(format!("sealed obj public build failed: {e}")))
    }

    fn ak_public_template() -> Result<tss_esapi::structures::Public, CryptoError> {
        let obj_attrs = ObjectAttributesBuilder::new()
            .with_fixed_tpm(true)
            .with_fixed_parent(true)
            .with_sensitive_data_origin(true)
            .with_user_with_auth(true)
            .with_sign_encrypt(true)
            .with_restricted(true)
            .build()
            .map_err(|e| CryptoError::Tpm(format!("AK attrs build failed: {e}")))?;

        use tss_esapi::interface_types::ecc::EccCurve;
        use tss_esapi::structures::{EccScheme, PublicEccParametersBuilder};

        PublicBuilder::new()
            .with_public_algorithm(PublicAlgorithm::Ecc)
            .with_name_hashing_algorithm(HashingAlgorithm::Sha256)
            .with_object_attributes(obj_attrs)
            .with_ecc_parameters(
                PublicEccParametersBuilder::new()
                    .with_ecc_scheme(EccScheme::Null)
                    .with_curve(EccCurve::NistP256)
                    .with_is_signing_key(true)
                    .with_is_restricted(true)
                    .build()
                    .map_err(|e| CryptoError::Tpm(format!("AK ECC params failed: {e}")))?,
            )
            .with_ecc_unique_identifier(Default::default())
            .build()
            .map_err(|e| CryptoError::Tpm(format!("AK public build failed: {e}")))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tpm_config_defaults() {
        let cfg = TpmConfig::default();
        assert!(!cfg.pcr_list.is_empty());
        assert!(cfg.nv_index >= 0x01000000);
        assert!(!cfg.key_label.is_empty());
    }

    #[test]
    fn test_tpm_config_custom_pcrs() {
        let cfg = TpmConfig {
            pcr_list: vec![0, 1, 2, 3, 7],
            nv_index: 0x01000002,
            key_label: "test".into(),
            tcti: None,
        };
        assert_eq!(cfg.pcr_list.len(), 5);
        assert!(cfg.pcr_list.contains(&7));
    }

    #[test]
    fn test_tpm_config_clone() {
        let cfg = TpmConfig::default();
        let cfg2 = cfg.clone();
        assert_eq!(cfg.pcr_list, cfg2.pcr_list);
        assert_eq!(cfg.key_label, cfg2.key_label);
    }

    /// Without a real TPM, `TpmKeyStore::new` must return an error (not panic).
    #[test]
    fn test_tpm_new_fails_without_device() {
        let cfg = TpmConfig {
            tcti: Some("device:/dev/nonexistent_tpm".into()),
            ..TpmConfig::default()
        };
        let result = TpmKeyStore::new(cfg);
        assert!(
            result.is_err(),
            "expected error when TPM device not available"
        );
    }

    /// Attestation nonce length validation.
    #[test]
    fn test_attestation_nonce_too_long() {
        let attestor = TpmAttestation::new(TpmConfig::default());
        let long_nonce = vec![0u8; 65];
        let result = attestor.quote(&[0, 7], &long_nonce);
        assert!(matches!(result, Err(CryptoError::Tpm(_))));
    }

    /// Attestation with valid nonce length (will fail on device, but must not
    /// fail on length check).
    #[test]
    fn test_attestation_nonce_valid_length() {
        let attestor = TpmAttestation::new(TpmConfig {
            tcti: Some("device:/dev/nonexistent_tpm".into()),
            ..TpmConfig::default()
        });
        let nonce = vec![0u8; 32];
        // The call will fail because no TPM is present, but it must not fail
        // due to nonce length.
        let result = attestor.quote(&[0], &nonce);
        if let Err(CryptoError::Tpm(msg)) = &result {
            assert!(!msg.contains("nonce too long"));
        }
    }

    #[test]
    fn test_sealed_blob_serialisation() {
        let blob = SealedKeyBlob {
            public_blob: vec![1, 2, 3],
            private_blob: vec![4, 5, 6],
            pcr_list: vec![0, 7],
        };
        let json = serde_json::to_string(&blob).unwrap();
        let decoded: SealedKeyBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(blob.public_blob, decoded.public_blob);
        assert_eq!(blob.pcr_list, decoded.pcr_list);
    }
}
