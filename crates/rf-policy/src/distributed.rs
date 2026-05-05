//! Decentralized policy distribution types.
//!
//! Implements CRDT-based desired-state convergence, append-only signed
//! policy logs, and content-addressed distribution.

use serde::{Deserialize, Serialize};

/// CRDT operation for policy convergence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PolicyCrdtOp {
    /// Add a policy rule.
    Add {
        rule_id: String,
        rule: String,
        timestamp: u64,
        author: String,
    },
    /// Remove a policy rule (tombstone).
    Remove {
        rule_id: String,
        timestamp: u64,
        author: String,
    },
    /// Update priority/ordering.
    SetPriority {
        rule_id: String,
        priority: u32,
        timestamp: u64,
        author: String,
    },
}

/// Append-only signed policy log entry (Scuttlebutt-inspired).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyLogEntry {
    /// Sequence number (monotonically increasing per author).
    pub sequence: u64,
    /// Author public key.
    pub author: String,
    /// Previous entry hash (forms hash chain).
    pub previous: Option<String>,
    /// Content hash (SHA-256 of payload).
    pub content_hash: String,
    /// CRDT operation.
    pub operation: PolicyCrdtOp,
    /// Ed25519 signature of (sequence || previous || content_hash).
    pub signature: String,
    /// Timestamp (Unix seconds).
    pub timestamp: u64,
}

/// Content-addressed policy distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAddressedPolicy {
    /// SHA-256 hash of the policy content.
    pub hash: String,
    /// Policy content (YAML).
    pub content: String,
    /// Version number.
    pub version: u32,
    /// Signatures from authorized signers.
    pub signatures: Vec<PolicySignature>,
    /// Minimum signatures required for validity.
    pub quorum: u8,
}

/// Policy signature from an authorized signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySignature {
    /// Signer public key.
    pub signer: String,
    /// Ed25519 signature.
    pub signature: String,
    /// Signed at timestamp.
    pub signed_at: u64,
}

/// Verify content-addressed policy has sufficient signatures.
pub fn verify_quorum(policy: &ContentAddressedPolicy) -> bool {
    policy.signatures.len() >= policy.quorum as usize
}

/// Policy sync state between agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Fully synchronized.
    Synced,
    /// Have newer entries to send.
    Ahead { entries: u64 },
    /// Missing entries from peer.
    Behind { entries: u64 },
    /// Diverged (both have unique entries).
    Diverged { local: u64, remote: u64 },
}

/// Merge strategy for conflicting policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Last-write-wins (by timestamp).
    LastWriteWins,
    /// Most restrictive policy wins (security-first).
    MostRestrictive,
    /// Require quorum agreement.
    QuorumAgreement { threshold: u8 },
    /// Manual resolution required.
    ManualResolve,
}

/// SPIFFE-style workload identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpiffeIdentity {
    /// SPIFFE ID (e.g., "spiffe://ravenfabric.io/agent/web-01").
    pub spiffe_id: String,
    /// Trust domain.
    pub trust_domain: String,
    /// Workload path.
    pub path: String,
    /// Identity attestation method.
    pub attestation: AttestationMethod,
}

/// How the workload identity was attested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationMethod {
    /// Node agent attestation (agent key).
    NodeAgent,
    /// Kubernetes service account token.
    K8sSat {
        namespace: String,
        service_account: String,
    },
    /// AWS IAM role.
    AwsIam { role_arn: String },
    /// Unix process ID (local only).
    UnixPid { uid: u32, gid: u32 },
    /// Join token (bootstrap).
    JoinToken,
}

/// Named Data Networking concepts for policy distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NdnPolicy {
    /// Interest: request policy by content name.
    pub name_prefix: String,
    /// Whether to cache satisfied interests.
    pub cacheable: bool,
    /// Freshness period (seconds).
    pub freshness_secs: u32,
    /// Verification: content must be signed by trusted key.
    pub required_signer: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crdt_op_serde() {
        let op = PolicyCrdtOp::Add {
            rule_id: "rule-1".into(),
            rule: "allow command:ls".into(),
            timestamp: 1000,
            author: "key-abc".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("add"));
        let parsed: PolicyCrdtOp = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, op);
    }

    #[test]
    fn test_policy_log_chain() {
        let entry1 = PolicyLogEntry {
            sequence: 1,
            author: "author-key".into(),
            previous: None,
            content_hash: "hash1".into(),
            operation: PolicyCrdtOp::Add {
                rule_id: "r1".into(),
                rule: "allow *".into(),
                timestamp: 100,
                author: "author-key".into(),
            },
            signature: "sig1".into(),
            timestamp: 100,
        };
        let entry2 = PolicyLogEntry {
            sequence: 2,
            author: "author-key".into(),
            previous: Some("hash1".into()),
            content_hash: "hash2".into(),
            operation: PolicyCrdtOp::Remove {
                rule_id: "r1".into(),
                timestamp: 200,
                author: "author-key".into(),
            },
            signature: "sig2".into(),
            timestamp: 200,
        };
        assert_eq!(entry2.previous.as_deref(), Some("hash1"));
        assert_eq!(entry2.sequence, entry1.sequence + 1);
    }

    #[test]
    fn test_content_addressed_quorum() {
        let policy = ContentAddressedPolicy {
            hash: "abc".into(),
            content: "spec: {}".into(),
            version: 1,
            signatures: vec![
                PolicySignature {
                    signer: "key1".into(),
                    signature: "sig1".into(),
                    signed_at: 100,
                },
                PolicySignature {
                    signer: "key2".into(),
                    signature: "sig2".into(),
                    signed_at: 101,
                },
            ],
            quorum: 2,
        };
        assert!(verify_quorum(&policy));

        let insufficient = ContentAddressedPolicy {
            quorum: 3,
            ..policy
        };
        assert!(!verify_quorum(&insufficient));
    }

    #[test]
    fn test_sync_state() {
        let states = [
            SyncState::Synced,
            SyncState::Ahead { entries: 5 },
            SyncState::Behind { entries: 3 },
            SyncState::Diverged {
                local: 2,
                remote: 4,
            },
        ];
        for s in &states {
            let json = serde_json::to_string(s).unwrap();
            let parsed: SyncState = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn test_spiffe_identity() {
        let id = SpiffeIdentity {
            spiffe_id: "spiffe://ravenfabric.io/agent/web-01".into(),
            trust_domain: "ravenfabric.io".into(),
            path: "/agent/web-01".into(),
            attestation: AttestationMethod::NodeAgent,
        };
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("spiffe://"));
        assert!(json.contains("node_agent"));
    }

    #[test]
    fn test_merge_strategy() {
        let strategies = [
            MergeStrategy::LastWriteWins,
            MergeStrategy::MostRestrictive,
            MergeStrategy::QuorumAgreement { threshold: 3 },
            MergeStrategy::ManualResolve,
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let parsed: MergeStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, s);
        }
    }

    #[test]
    fn test_ndn_policy() {
        let ndn = NdnPolicy {
            name_prefix: "/ravenfabric/policy/v1".into(),
            cacheable: true,
            freshness_secs: 3600,
            required_signer: Some("root-key".into()),
        };
        let json = serde_json::to_string(&ndn).unwrap();
        assert!(json.contains("/ravenfabric/policy"));
    }
}
