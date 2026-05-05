//! Decentralized policy distribution types.
//!
//! Implements CRDT-based desired-state convergence, append-only signed
//! policy logs, and content-addressed distribution.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

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

// --- CRDT Implementations ---

/// A grow-only set (G-Set) — elements can only be added, never removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GSet<T: Eq + std::hash::Hash + Clone> {
    elements: HashSet<T>,
}

impl<T: Eq + std::hash::Hash + Clone> GSet<T> {
    /// Create an empty G-Set.
    pub fn new() -> Self {
        Self {
            elements: HashSet::new(),
        }
    }

    /// Insert an element.
    pub fn insert(&mut self, value: T) {
        self.elements.insert(value);
    }

    /// Check if the set contains an element.
    pub fn contains(&self, value: &T) -> bool {
        self.elements.contains(value)
    }

    /// Merge another G-Set into this one (union).
    pub fn merge(&mut self, other: &GSet<T>) {
        for elem in &other.elements {
            self.elements.insert(elem.clone());
        }
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl<T: Eq + std::hash::Hash + Clone> Default for GSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A last-writer-wins register (LWW-Register).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwRegister<T: Clone> {
    value: T,
    timestamp: u64,
    author: String,
}

impl<T: Clone> LwwRegister<T> {
    /// Create a new LWW register.
    pub fn new(value: T, timestamp: u64, author: String) -> Self {
        Self {
            value,
            timestamp,
            author,
        }
    }

    /// Get the current value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get the timestamp.
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Get the author.
    pub fn author(&self) -> &str {
        &self.author
    }

    /// Update the value (only succeeds if the new timestamp is greater).
    pub fn update(&mut self, value: T, timestamp: u64, author: String) -> bool {
        if timestamp > self.timestamp {
            self.value = value;
            self.timestamp = timestamp;
            self.author = author;
            true
        } else {
            false
        }
    }

    /// Merge another register (last-writer-wins by timestamp).
    pub fn merge(&mut self, other: &LwwRegister<T>) {
        if other.timestamp > self.timestamp {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.author = other.author.clone();
        }
    }
}

/// Observed-Remove Set (OR-Set) — supports both add and remove with tombstones.
///
/// Each replica must use a distinct `replica_id` to ensure tag uniqueness.
/// Tags are derived as `replica_id * TAG_SPACE + local_counter`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrSet {
    /// Active elements: element → set of unique tags.
    elements: HashMap<String, HashSet<u64>>,
    /// Tombstoned tags (removed).
    tombstones: HashSet<u64>,
    /// Replica ID (ensures tag uniqueness across replicas).
    replica_id: u64,
    /// Next local counter.
    next_counter: u64,
}

impl OrSet {
    /// Tag space per replica (allows billions of ops per replica).
    const TAG_SPACE: u64 = 1_000_000_000;

    /// Create an empty OR-Set for a given replica.
    pub fn with_replica(replica_id: u64) -> Self {
        Self {
            elements: HashMap::new(),
            tombstones: HashSet::new(),
            replica_id,
            next_counter: 0,
        }
    }

    /// Create an empty OR-Set (replica 0).
    pub fn new() -> Self {
        Self::with_replica(0)
    }

    /// Add an element. Returns the tag assigned.
    pub fn add(&mut self, element: String) -> u64 {
        let tag = self.replica_id * Self::TAG_SPACE + self.next_counter;
        self.next_counter += 1;
        self.elements.entry(element).or_default().insert(tag);
        tag
    }

    /// Remove an element (tombstones all its current tags).
    pub fn remove(&mut self, element: &str) -> bool {
        if let Some(tags) = self.elements.remove(element) {
            for tag in tags {
                self.tombstones.insert(tag);
            }
            true
        } else {
            false
        }
    }

    /// Check if the set contains an element (has non-tombstoned tags).
    pub fn contains(&self, element: &str) -> bool {
        self.elements
            .get(element)
            .is_some_and(|tags| tags.iter().any(|t| !self.tombstones.contains(t)))
    }

    /// List all active elements.
    pub fn elements(&self) -> Vec<&str> {
        self.elements
            .iter()
            .filter(|(_, tags)| tags.iter().any(|t| !self.tombstones.contains(t)))
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// Merge another OR-Set into this one.
    pub fn merge(&mut self, other: &OrSet) {
        // Merge tombstones first.
        for tag in &other.tombstones {
            self.tombstones.insert(*tag);
        }
        // Merge elements — union of tags, minus tombstones.
        for (elem, tags) in &other.elements {
            let entry = self.elements.entry(elem.clone()).or_default();
            for tag in tags {
                if !self.tombstones.contains(tag) {
                    entry.insert(*tag);
                }
            }
        }
        // Clean up our existing entries: remove tombstoned tags.
        for tags in self.elements.values_mut() {
            tags.retain(|t| !self.tombstones.contains(t));
        }
        self.elements.retain(|_, tags| !tags.is_empty());
        self.next_counter = self.next_counter.max(other.next_counter);
    }

    /// Number of active elements.
    pub fn len(&self) -> usize {
        self.elements().len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for OrSet {
    fn default() -> Self {
        Self::new()
    }
}

/// CRDT-based policy state — combines multiple CRDTs for convergent policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCrdt {
    /// Allowed commands (OR-Set — can add and remove rules).
    pub allowed_commands: OrSet,
    /// Denied commands (G-Set — deny rules can never be removed).
    pub denied_commands: GSet<String>,
    /// Rule priorities (LWW per rule_id).
    pub priorities: HashMap<String, LwwRegister<u32>>,
    /// Operations log for audit.
    pub ops: Vec<PolicyCrdtOp>,
}

impl PolicyCrdt {
    /// Create a new empty policy CRDT.
    pub fn new() -> Self {
        Self {
            allowed_commands: OrSet::new(),
            denied_commands: GSet::new(),
            priorities: HashMap::new(),
            ops: Vec::new(),
        }
    }

    /// Apply a CRDT operation.
    pub fn apply(&mut self, op: PolicyCrdtOp) {
        match &op {
            PolicyCrdtOp::Add { rule_id, rule, .. } => {
                self.allowed_commands.add(rule.clone());
                // Record priority if not already set.
                if !self.priorities.contains_key(rule_id) {
                    self.priorities
                        .insert(rule_id.clone(), LwwRegister::new(0, 0, String::new()));
                }
            }
            PolicyCrdtOp::Remove { rule_id, .. } => {
                // Only remove from allowed — denied rules are permanent.
                self.allowed_commands.remove(rule_id);
            }
            PolicyCrdtOp::SetPriority {
                rule_id,
                priority,
                timestamp,
                author,
            } => {
                if let Some(reg) = self.priorities.get_mut(rule_id) {
                    reg.update(*priority, *timestamp, author.clone());
                } else {
                    self.priorities.insert(
                        rule_id.clone(),
                        LwwRegister::new(*priority, *timestamp, author.clone()),
                    );
                }
            }
        }
        self.ops.push(op);
    }

    /// Merge another PolicyCrdt into this one (convergent).
    pub fn merge(&mut self, other: &PolicyCrdt) {
        self.allowed_commands.merge(&other.allowed_commands);
        self.denied_commands.merge(&other.denied_commands);
        for (rule_id, reg) in &other.priorities {
            if let Some(existing) = self.priorities.get_mut(rule_id) {
                existing.merge(reg);
            } else {
                self.priorities.insert(rule_id.clone(), reg.clone());
            }
        }
    }

    /// Check if a command is allowed (allow set minus deny set).
    pub fn is_allowed(&self, command: &str) -> bool {
        if self.denied_commands.contains(&command.to_string()) {
            return false; // Deny always wins.
        }
        self.allowed_commands.contains(command)
    }
}

impl Default for PolicyCrdt {
    fn default() -> Self {
        Self::new()
    }
}

/// Append-only policy log with hash-chain integrity.
pub struct PolicyLog {
    entries: Vec<PolicyLogEntry>,
    /// Author → latest sequence number.
    sequences: HashMap<String, u64>,
}

impl PolicyLog {
    /// Create a new empty log.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sequences: HashMap::new(),
        }
    }

    /// Compute SHA-256 hash for a log entry.
    fn compute_hash(entry: &PolicyLogEntry) -> String {
        let mut hasher = Sha256::new();
        hasher.update(entry.sequence.to_be_bytes());
        hasher.update(entry.author.as_bytes());
        if let Some(prev) = &entry.previous {
            hasher.update(prev.as_bytes());
        }
        hasher.update(serde_json::to_vec(&entry.operation).unwrap_or_default());
        format!("{:x}", hasher.finalize())
    }

    /// Append an operation to the log. Returns the entry's content hash.
    pub fn append(&mut self, author: String, operation: PolicyCrdtOp) -> String {
        let seq = self.sequences.get(&author).copied().unwrap_or(0) + 1;
        let previous = self.entries.last().map(|e| e.content_hash.clone());
        let timestamp = operation.timestamp();

        let mut entry = PolicyLogEntry {
            sequence: seq,
            author: author.clone(),
            previous,
            content_hash: String::new(), // Computed below.
            operation,
            signature: String::new(), // Placeholder — real impl would Ed25519 sign.
            timestamp,
        };
        entry.content_hash = Self::compute_hash(&entry);
        let hash = entry.content_hash.clone();
        self.sequences.insert(author, seq);
        self.entries.push(entry);
        hash
    }

    /// Verify the hash chain integrity (all entries link correctly).
    pub fn verify_chain(&self) -> bool {
        for (i, entry) in self.entries.iter().enumerate() {
            // Verify content hash.
            let expected = Self::compute_hash(entry);
            if entry.content_hash != expected {
                return false;
            }
            // Verify previous link.
            if i > 0 {
                if entry.previous.as_deref() != Some(&self.entries[i - 1].content_hash) {
                    return false;
                }
            } else if entry.previous.is_some() {
                return false; // First entry should have no previous.
            }
        }
        true
    }

    /// Get the sync state relative to a peer's known sequence numbers.
    pub fn sync_state(&self, peer_sequences: &HashMap<String, u64>) -> SyncState {
        let mut local_ahead = 0u64;
        let mut remote_ahead = 0u64;

        for (author, &local_seq) in &self.sequences {
            let remote_seq = peer_sequences.get(author).copied().unwrap_or(0);
            if local_seq > remote_seq {
                local_ahead += local_seq - remote_seq;
            }
        }
        for (author, &remote_seq) in peer_sequences {
            let local_seq = self.sequences.get(author).copied().unwrap_or(0);
            if remote_seq > local_seq {
                remote_ahead += remote_seq - local_seq;
            }
        }

        match (local_ahead, remote_ahead) {
            (0, 0) => SyncState::Synced,
            (a, 0) => SyncState::Ahead { entries: a },
            (0, b) => SyncState::Behind { entries: b },
            (a, b) => SyncState::Diverged {
                local: a,
                remote: b,
            },
        }
    }

    /// Get entries a peer is missing (entries after their known sequence).
    pub fn entries_since(&self, peer_sequences: &HashMap<String, u64>) -> Vec<&PolicyLogEntry> {
        self.entries
            .iter()
            .filter(|e| {
                let peer_seq = peer_sequences.get(&e.author).copied().unwrap_or(0);
                e.sequence > peer_seq
            })
            .collect()
    }

    /// Number of entries in the log.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all entries.
    pub fn entries(&self) -> &[PolicyLogEntry] {
        &self.entries
    }
}

impl Default for PolicyLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to extract timestamp from a PolicyCrdtOp.
impl PolicyCrdtOp {
    /// Get the timestamp of this operation.
    pub fn timestamp(&self) -> u64 {
        match self {
            PolicyCrdtOp::Add { timestamp, .. } => *timestamp,
            PolicyCrdtOp::Remove { timestamp, .. } => *timestamp,
            PolicyCrdtOp::SetPriority { timestamp, .. } => *timestamp,
        }
    }

    /// Get the author of this operation.
    pub fn author(&self) -> &str {
        match self {
            PolicyCrdtOp::Add { author, .. } => author,
            PolicyCrdtOp::Remove { author, .. } => author,
            PolicyCrdtOp::SetPriority { author, .. } => author,
        }
    }
}

/// Compute SHA-256 hash of policy content for content-addressed distribution.
pub fn compute_policy_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
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

    #[test]
    fn test_gset_merge() {
        let mut a = GSet::new();
        a.insert("rule1".to_string());
        a.insert("rule2".to_string());

        let mut b = GSet::new();
        b.insert("rule2".to_string());
        b.insert("rule3".to_string());

        a.merge(&b);
        assert_eq!(a.len(), 3);
        assert!(a.contains(&"rule1".to_string()));
        assert!(a.contains(&"rule2".to_string()));
        assert!(a.contains(&"rule3".to_string()));
    }

    #[test]
    fn test_gset_idempotent() {
        let mut a = GSet::new();
        a.insert("x".to_string());
        a.insert("x".to_string());
        assert_eq!(a.len(), 1);

        let b = a.clone();
        a.merge(&b);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn test_lww_register_merge() {
        let mut a = LwwRegister::new(10u32, 100, "alice".into());
        let b = LwwRegister::new(20u32, 200, "bob".into());

        a.merge(&b);
        assert_eq!(*a.value(), 20);
        assert_eq!(a.author(), "bob");
    }

    #[test]
    fn test_lww_register_older_loses() {
        let mut a = LwwRegister::new(10u32, 200, "alice".into());
        let b = LwwRegister::new(20u32, 100, "bob".into());

        a.merge(&b);
        assert_eq!(*a.value(), 10); // alice's value wins (newer timestamp)
    }

    #[test]
    fn test_lww_register_update() {
        let mut reg = LwwRegister::new(1u32, 100, "a".into());
        assert!(!reg.update(2, 50, "b".into())); // Older, rejected
        assert!(reg.update(3, 200, "c".into())); // Newer, accepted
        assert_eq!(*reg.value(), 3);
    }

    #[test]
    fn test_or_set_add_remove() {
        let mut set = OrSet::new();
        set.add("hello".into());
        set.add("world".into());
        assert!(set.contains("hello"));
        assert!(set.contains("world"));
        assert_eq!(set.len(), 2);

        set.remove("hello");
        assert!(!set.contains("hello"));
        assert!(set.contains("world"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_or_set_add_after_remove() {
        let mut set = OrSet::new();
        set.add("x".into());
        set.remove("x");
        assert!(!set.contains("x"));

        // Re-add with new tag.
        set.add("x".into());
        assert!(set.contains("x"));
    }

    #[test]
    fn test_or_set_merge() {
        let mut a = OrSet::with_replica(0);
        a.add("rule1".into());
        a.add("rule2".into());

        let mut b = OrSet::with_replica(1);
        b.add("rule2".into());
        b.add("rule3".into());
        b.remove("rule2");

        // After merge: rule1 (from a), rule2 (from a, not tombstoned there), rule3 (from b)
        // Actually rule2 is in a with tag 1, and in b removed with tag 0.
        // Merge unions tombstones, so b's tombstone for tag 0 shouldn't affect a's tag 1.
        a.merge(&b);
        assert!(a.contains("rule1"));
        assert!(a.contains("rule3"));
        // rule2: a has tag 1, b tombstoned tag 0 — tag 1 is NOT tombstoned
        assert!(a.contains("rule2"));
    }

    #[test]
    fn test_policy_crdt_apply() {
        let mut crdt = PolicyCrdt::new();
        crdt.apply(PolicyCrdtOp::Add {
            rule_id: "r1".into(),
            rule: "allow ls".into(),
            timestamp: 100,
            author: "admin".into(),
        });
        assert!(crdt.is_allowed("allow ls"));
        assert!(!crdt.is_allowed("rm -rf"));

        // Add a deny rule (permanent).
        crdt.denied_commands.insert("rm -rf".into());
        assert!(!crdt.is_allowed("rm -rf"));
    }

    #[test]
    fn test_policy_crdt_deny_wins() {
        let mut crdt = PolicyCrdt::new();
        crdt.apply(PolicyCrdtOp::Add {
            rule_id: "r1".into(),
            rule: "dangerous".into(),
            timestamp: 100,
            author: "a".into(),
        });
        crdt.denied_commands.insert("dangerous".into());
        // Deny always wins over allow.
        assert!(!crdt.is_allowed("dangerous"));
    }

    #[test]
    fn test_policy_crdt_merge() {
        let mut a = PolicyCrdt::new();
        a.apply(PolicyCrdtOp::Add {
            rule_id: "r1".into(),
            rule: "cmd1".into(),
            timestamp: 100,
            author: "x".into(),
        });

        let mut b = PolicyCrdt::new();
        b.apply(PolicyCrdtOp::Add {
            rule_id: "r2".into(),
            rule: "cmd2".into(),
            timestamp: 200,
            author: "y".into(),
        });
        b.denied_commands.insert("badcmd".into());

        a.merge(&b);
        assert!(a.is_allowed("cmd1"));
        assert!(a.is_allowed("cmd2"));
        assert!(!a.is_allowed("badcmd"));
    }

    #[test]
    fn test_policy_log_hash_chain() {
        let mut log = PolicyLog::new();
        log.append(
            "alice".into(),
            PolicyCrdtOp::Add {
                rule_id: "r1".into(),
                rule: "allow *".into(),
                timestamp: 100,
                author: "alice".into(),
            },
        );
        log.append(
            "alice".into(),
            PolicyCrdtOp::SetPriority {
                rule_id: "r1".into(),
                priority: 10,
                timestamp: 200,
                author: "alice".into(),
            },
        );

        assert_eq!(log.len(), 2);
        assert!(log.verify_chain());

        // Second entry links to first.
        assert!(log.entries()[1].previous.is_some());
    }

    #[test]
    fn test_policy_log_sync_state() {
        let mut log = PolicyLog::new();
        log.append(
            "alice".into(),
            PolicyCrdtOp::Add {
                rule_id: "r1".into(),
                rule: "x".into(),
                timestamp: 1,
                author: "alice".into(),
            },
        );
        log.append(
            "alice".into(),
            PolicyCrdtOp::Add {
                rule_id: "r2".into(),
                rule: "y".into(),
                timestamp: 2,
                author: "alice".into(),
            },
        );

        // Peer knows alice up to seq 1.
        let mut peer_seqs = std::collections::HashMap::new();
        peer_seqs.insert("alice".to_string(), 1u64);
        assert_eq!(log.sync_state(&peer_seqs), SyncState::Ahead { entries: 1 });

        // Peer has more entries from bob.
        peer_seqs.insert("bob".to_string(), 3);
        assert_eq!(
            log.sync_state(&peer_seqs),
            SyncState::Diverged {
                local: 1,
                remote: 3
            }
        );
    }

    #[test]
    fn test_policy_log_entries_since() {
        let mut log = PolicyLog::new();
        for i in 0..5 {
            log.append(
                "node".into(),
                PolicyCrdtOp::Add {
                    rule_id: format!("r{i}"),
                    rule: format!("cmd{i}"),
                    timestamp: i as u64,
                    author: "node".into(),
                },
            );
        }

        let mut peer_seqs = std::collections::HashMap::new();
        peer_seqs.insert("node".to_string(), 3u64);
        let missing = log.entries_since(&peer_seqs);
        assert_eq!(missing.len(), 2); // Entries 4 and 5
    }

    #[test]
    fn test_compute_policy_hash() {
        let hash1 = compute_policy_hash("spec: {}");
        let hash2 = compute_policy_hash("spec: {}");
        let hash3 = compute_policy_hash("spec: {different: true}");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn test_crdt_op_accessors() {
        let op = PolicyCrdtOp::Add {
            rule_id: "r1".into(),
            rule: "x".into(),
            timestamp: 42,
            author: "bob".into(),
        };
        assert_eq!(op.timestamp(), 42);
        assert_eq!(op.author(), "bob");
    }
}
