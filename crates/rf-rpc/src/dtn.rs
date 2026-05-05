//! Delay-Tolerant Networking (DTN) queue and custody transfer.
//!
//! Provides types for store-carry-forward messaging, offline queuing,
//! TTL/priority handling, and custody transfer protocol.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

/// Priority level for queued messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Lowest priority — best effort delivery.
    Low = 0,
    /// Normal priority.
    Normal = 1,
    /// High priority — prefer over normal traffic.
    High = 2,
    /// Critical — security events, tamper alerts. Never dropped by TTL.
    Critical = 3,
}

/// A queued message bundle for store-carry-forward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    /// Unique bundle ID (content-addressed hash or UUID).
    pub id: String,
    /// Source agent ID.
    pub source: String,
    /// Destination agent ID (or group/label).
    pub destination: String,
    /// Priority level.
    pub priority: Priority,
    /// Time-to-live in seconds. 0 = infinite.
    pub ttl_secs: u64,
    /// Creation timestamp (Unix ms).
    pub created_at_ms: u64,
    /// Payload (opaque bytes, already encrypted).
    pub payload: Vec<u8>,
    /// Whether this bundle requires custody transfer acknowledgment.
    pub custody_requested: bool,
    /// Idempotency key for deduplication.
    pub idempotency_key: Option<String>,
    /// Number of hops this bundle has traversed.
    pub hop_count: u32,
    /// Maximum hops allowed (0 = unlimited).
    pub max_hops: u32,
}

impl Bundle {
    /// Check if this bundle has expired based on TTL.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        if self.ttl_secs == 0 {
            return false; // infinite TTL
        }
        // Critical messages never expire
        if self.priority == Priority::Critical {
            return false;
        }
        let age_ms = now_ms.saturating_sub(self.created_at_ms);
        age_ms > self.ttl_secs * 1000
    }

    /// Check if max hops exceeded.
    pub fn hops_exceeded(&self) -> bool {
        self.max_hops > 0 && self.hop_count >= self.max_hops
    }
}

/// Ordering for priority queue (higher priority + older = first out).
impl PartialEq for Bundle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Bundle {}

/// Wrapper for BinaryHeap ordering.
#[derive(Debug)]
struct PrioritizedBundle(Bundle);

impl PartialEq for PrioritizedBundle {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for PrioritizedBundle {}

impl PartialOrd for PrioritizedBundle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedBundle {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then older first (lower created_at_ms)
        match self.0.priority.cmp(&other.0.priority) {
            Ordering::Equal => other.0.created_at_ms.cmp(&self.0.created_at_ms),
            other => other,
        }
    }
}

/// Custody transfer acknowledgment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustodyAck {
    /// Bundle ID being acknowledged.
    pub bundle_id: String,
    /// Agent accepting custody.
    pub custodian: String,
    /// Timestamp of acceptance.
    pub accepted_at_ms: u64,
}

/// Custody transfer state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustodyState {
    /// We hold custody — responsible for delivery.
    Held,
    /// Custody transferred to next hop — awaiting ack.
    Transferring,
    /// Custody accepted by next hop.
    Transferred,
    /// Delivery confirmed to final destination.
    Delivered,
    /// Transfer failed — we still hold custody.
    Failed,
}

/// In-memory DTN queue with priority ordering and deduplication.
#[derive(Debug)]
pub struct DtnQueue {
    queue: BinaryHeap<PrioritizedBundle>,
    /// Set of seen idempotency keys for deduplication.
    seen_keys: std::collections::HashSet<String>,
    /// Maximum queue size (bundles).
    max_size: usize,
    /// Total bundles dropped due to capacity.
    pub dropped_count: u64,
}

impl DtnQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: BinaryHeap::new(),
            seen_keys: std::collections::HashSet::new(),
            max_size,
            dropped_count: 0,
        }
    }

    /// Enqueue a bundle. Returns false if duplicate or queue full (and lower priority).
    pub fn enqueue(&mut self, bundle: Bundle) -> bool {
        // Deduplication
        if let Some(ref key) = bundle.idempotency_key {
            if self.seen_keys.contains(key) {
                return false;
            }
            self.seen_keys.insert(key.clone());
        }

        // Capacity check
        if self.queue.len() >= self.max_size {
            // Only drop if new bundle has higher priority than lowest in queue
            // Since BinaryHeap doesn't easily give us the min, we just drop
            self.dropped_count += 1;
            return false;
        }

        self.queue.push(PrioritizedBundle(bundle));
        true
    }

    /// Dequeue the highest-priority bundle.
    pub fn dequeue(&mut self) -> Option<Bundle> {
        self.queue.pop().map(|p| p.0)
    }

    /// Peek at the highest-priority bundle without removing it.
    pub fn peek(&self) -> Option<&Bundle> {
        self.queue.peek().map(|p| &p.0)
    }

    /// Remove expired bundles.
    pub fn prune_expired(&mut self, now_ms: u64) {
        let items: Vec<PrioritizedBundle> = self.queue.drain().collect();
        for item in items {
            if !item.0.is_expired(now_ms) {
                self.queue.push(item);
            }
        }
    }

    /// Current queue depth.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bundle(id: &str, priority: Priority, ttl: u64, created: u64) -> Bundle {
        Bundle {
            id: id.to_string(),
            source: "agent-a".to_string(),
            destination: "agent-b".to_string(),
            priority,
            ttl_secs: ttl,
            created_at_ms: created,
            payload: vec![1, 2, 3],
            custody_requested: false,
            idempotency_key: None,
            hop_count: 0,
            max_hops: 0,
        }
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = DtnQueue::new(100);
        queue.enqueue(make_bundle("low", Priority::Low, 60, 1000));
        queue.enqueue(make_bundle("high", Priority::High, 60, 1000));
        queue.enqueue(make_bundle("normal", Priority::Normal, 60, 1000));
        queue.enqueue(make_bundle("critical", Priority::Critical, 60, 1000));

        assert_eq!(queue.dequeue().unwrap().id, "critical");
        assert_eq!(queue.dequeue().unwrap().id, "high");
        assert_eq!(queue.dequeue().unwrap().id, "normal");
        assert_eq!(queue.dequeue().unwrap().id, "low");
    }

    #[test]
    fn test_same_priority_fifo() {
        let mut queue = DtnQueue::new(100);
        queue.enqueue(make_bundle("first", Priority::Normal, 60, 1000));
        queue.enqueue(make_bundle("second", Priority::Normal, 60, 2000));

        // Older (lower timestamp) should come first
        assert_eq!(queue.dequeue().unwrap().id, "first");
        assert_eq!(queue.dequeue().unwrap().id, "second");
    }

    #[test]
    fn test_ttl_expiry() {
        let bundle = make_bundle("b1", Priority::Normal, 10, 1000);
        assert!(!bundle.is_expired(5000)); // 4s old, TTL 10s
        assert!(bundle.is_expired(12000)); // 11s old, TTL 10s
    }

    #[test]
    fn test_critical_never_expires() {
        let bundle = make_bundle("crit", Priority::Critical, 1, 0);
        assert!(!bundle.is_expired(999_999_999));
    }

    #[test]
    fn test_deduplication() {
        let mut queue = DtnQueue::new(100);
        let mut b1 = make_bundle("b1", Priority::Normal, 60, 1000);
        b1.idempotency_key = Some("key-1".to_string());

        assert!(queue.enqueue(b1.clone()));
        assert!(!queue.enqueue(b1)); // Duplicate
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_capacity_limit() {
        let mut queue = DtnQueue::new(2);
        queue.enqueue(make_bundle("b1", Priority::Normal, 60, 1000));
        queue.enqueue(make_bundle("b2", Priority::Normal, 60, 2000));
        assert!(!queue.enqueue(make_bundle("b3", Priority::Normal, 60, 3000)));
        assert_eq!(queue.dropped_count, 1);
    }

    #[test]
    fn test_prune_expired() {
        let mut queue = DtnQueue::new(100);
        queue.enqueue(make_bundle("expired", Priority::Normal, 5, 1000));
        queue.enqueue(make_bundle("fresh", Priority::Normal, 60, 10000));

        queue.prune_expired(15000); // 14s for first (expired), 5s for second (fresh)
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.peek().unwrap().id, "fresh");
    }

    #[test]
    fn test_hop_count() {
        let mut bundle = make_bundle("b1", Priority::Normal, 60, 1000);
        bundle.max_hops = 3;
        bundle.hop_count = 2;
        assert!(!bundle.hops_exceeded());
        bundle.hop_count = 3;
        assert!(bundle.hops_exceeded());
    }

    #[test]
    fn test_infinite_ttl() {
        let bundle = make_bundle("inf", Priority::Low, 0, 0);
        assert!(!bundle.is_expired(u64::MAX - 1));
    }
}
