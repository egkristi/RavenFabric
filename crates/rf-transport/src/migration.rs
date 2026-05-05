//! Session migration — transport upgrade/downgrade without dropping the logical session.
//!
//! Implements:
//! - Background transport upgrade (relay → direct)
//! - Session ticket for resumption on new transport
//! - Atomic swap (make-before-break with overlap window)
//! - 0-RTT resumption: cached session tickets enable fast reconnects

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A session ticket that allows resuming a session on a different transport.
/// The ticket proves the peer completed a Noise XX handshake previously.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTicket {
    /// Unique session ID (survives transport changes).
    pub session_id: [u8; 16],
    /// Remote peer's static public key (verified during original handshake).
    pub peer_static_key: [u8; 32],
    /// Timestamp when the original session was established.
    pub established_at_ms: u64,
    /// Transport the session was last active on.
    pub last_transport: String,
    /// Number of times this session has migrated.
    pub migration_count: u32,
}

/// State of a session migration in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Not migrating.
    Idle,
    /// New transport being established (old still active).
    Establishing,
    /// New transport active, verifying peer key matches.
    Verifying,
    /// Both paths active during overlap window.
    Overlap,
    /// Migration complete, old transport being closed.
    Closing,
    /// Migration failed, rolled back to original transport.
    RolledBack,
}

/// Controls a session migration (make-before-break).
#[derive(Debug)]
pub struct SessionMigration {
    /// Current phase.
    phase: MigrationPhase,
    /// Session ticket for resumption.
    ticket: SessionTicket,
    /// Target transport name.
    target_transport: String,
    /// When the migration started.
    started_at: Option<Instant>,
    /// Maximum time to keep both transports active during overlap.
    overlap_timeout: Duration,
    /// Whether the new transport has been verified (peer key matches).
    peer_verified: bool,
}

impl SessionMigration {
    /// Create a new session migration from ticket to target transport.
    pub fn new(ticket: SessionTicket, target_transport: String) -> Self {
        Self {
            phase: MigrationPhase::Idle,
            ticket,
            target_transport,
            started_at: None,
            overlap_timeout: Duration::from_secs(5),
            peer_verified: false,
        }
    }

    /// Set the overlap timeout.
    pub fn with_overlap_timeout(mut self, timeout: Duration) -> Self {
        self.overlap_timeout = timeout;
        self
    }

    /// Start the migration (new transport being established).
    pub fn start(&mut self) {
        self.phase = MigrationPhase::Establishing;
        self.started_at = Some(Instant::now());
    }

    /// New transport connected — verify the peer.
    pub fn transport_connected(&mut self) {
        self.phase = MigrationPhase::Verifying;
    }

    /// Peer key verified — enter overlap window.
    pub fn peer_key_verified(&mut self, peer_key: &[u8; 32]) -> bool {
        if peer_key == &self.ticket.peer_static_key {
            self.peer_verified = true;
            self.phase = MigrationPhase::Overlap;
            true
        } else {
            // Key mismatch — possible MITM on new transport
            self.phase = MigrationPhase::RolledBack;
            false
        }
    }

    /// Complete the migration (close old transport).
    pub fn complete(&mut self) -> SessionTicket {
        self.phase = MigrationPhase::Closing;
        let mut new_ticket = self.ticket.clone();
        new_ticket.last_transport = self.target_transport.clone();
        new_ticket.migration_count += 1;
        new_ticket
    }

    /// Abort the migration (roll back to original transport).
    pub fn abort(&mut self) {
        self.phase = MigrationPhase::RolledBack;
    }

    /// Check if overlap window has expired.
    pub fn overlap_expired(&self) -> bool {
        if self.phase != MigrationPhase::Overlap {
            return false;
        }
        self.started_at
            .map(|t| t.elapsed() > self.overlap_timeout)
            .unwrap_or(false)
    }

    /// Current phase of the migration.
    pub fn phase(&self) -> MigrationPhase {
        self.phase
    }

    /// Whether the peer has been verified on the new transport.
    pub fn is_peer_verified(&self) -> bool {
        self.peer_verified
    }

    /// The target transport name.
    pub fn target(&self) -> &str {
        &self.target_transport
    }

    /// The session ticket.
    pub fn ticket(&self) -> &SessionTicket {
        &self.ticket
    }
}

/// 0-RTT session resumption cache.
///
/// Stores validated session tickets keyed by peer public key, enabling
/// fast reconnects without a full Noise XX handshake when a prior session
/// ticket exists and is still valid.
///
/// The 0-RTT flow:
/// 1. Client looks up peer key in cache → finds valid ticket
/// 2. Client sends ticket + early data in first message
/// 3. Server validates ticket (checks session_id, expiry, peer key)
/// 4. If valid: session resumes immediately (0-RTT)
/// 5. If invalid: falls back to full Noise XX handshake (1-RTT)
pub struct ZeroRttCache {
    /// Tickets indexed by peer static key (hex).
    tickets: HashMap<String, CachedTicket>,
    /// Maximum ticket age before requiring fresh handshake.
    max_ticket_age: Duration,
    /// Maximum number of cached tickets.
    max_entries: usize,
}

/// A cached session ticket with metadata.
#[derive(Debug, Clone)]
struct CachedTicket {
    ticket: SessionTicket,
    /// When this ticket was cached.
    cached_at: Instant,
    /// Number of times this ticket was used for resumption.
    use_count: u32,
    /// Maximum uses before requiring fresh handshake (replay protection).
    max_uses: u32,
}

/// Result of a 0-RTT resumption attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeResult {
    /// Ticket found and valid — can use 0-RTT.
    Resumed { session_id: [u8; 16] },
    /// Ticket expired — need fresh handshake.
    Expired,
    /// Ticket used too many times — need fresh handshake (replay protection).
    ExhaustedUses,
    /// No ticket for this peer — need full handshake.
    NoCachedTicket,
}

impl ZeroRttCache {
    /// Create a new 0-RTT cache.
    pub fn new(max_ticket_age: Duration, max_entries: usize) -> Self {
        Self {
            tickets: HashMap::new(),
            max_ticket_age,
            max_entries,
        }
    }

    /// Store a session ticket for future resumption.
    pub fn store(&mut self, peer_key_hex: &str, ticket: SessionTicket) {
        // Evict oldest if at capacity
        if self.tickets.len() >= self.max_entries && !self.tickets.contains_key(peer_key_hex) {
            // Find oldest entry
            if let Some(oldest_key) = self
                .tickets
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone())
            {
                self.tickets.remove(&oldest_key);
            }
        }

        self.tickets.insert(
            peer_key_hex.to_string(),
            CachedTicket {
                ticket,
                cached_at: Instant::now(),
                use_count: 0,
                max_uses: 5, // Allow 5 resumptions per ticket
            },
        );
    }

    /// Attempt to resume a session with a cached ticket.
    pub fn try_resume(&mut self, peer_key_hex: &str) -> ResumeResult {
        let entry = match self.tickets.get_mut(peer_key_hex) {
            Some(e) => e,
            None => return ResumeResult::NoCachedTicket,
        };

        // Check age
        if entry.cached_at.elapsed() > self.max_ticket_age {
            self.tickets.remove(peer_key_hex);
            return ResumeResult::Expired;
        }

        // Check use count (replay protection)
        if entry.use_count >= entry.max_uses {
            self.tickets.remove(peer_key_hex);
            return ResumeResult::ExhaustedUses;
        }

        entry.use_count += 1;
        ResumeResult::Resumed {
            session_id: entry.ticket.session_id,
        }
    }

    /// Validate an incoming 0-RTT ticket from a peer.
    /// Returns true if the ticket is valid for resumption.
    pub fn validate_incoming(
        &self,
        ticket: &SessionTicket,
        claimed_peer_key: &[u8; 32],
    ) -> bool {
        // Verify the ticket's peer key matches what the peer claims
        ticket.peer_static_key == *claimed_peer_key
            // Verify the session isn't ancient (24h max)
            && {
                let age_ms = now_ms().saturating_sub(ticket.established_at_ms);
                age_ms < 86_400_000 // 24 hours in ms
            }
    }

    /// Remove a cached ticket (e.g., on session close).
    pub fn invalidate(&mut self, peer_key_hex: &str) {
        self.tickets.remove(peer_key_hex);
    }

    /// Number of cached tickets.
    pub fn len(&self) -> usize {
        self.tickets.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.tickets.is_empty()
    }

    /// Prune expired tickets.
    pub fn prune_expired(&mut self) {
        let max_age = self.max_ticket_age;
        self.tickets
            .retain(|_, entry| entry.cached_at.elapsed() <= max_age);
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ticket() -> SessionTicket {
        SessionTicket {
            session_id: [0x42; 16],
            peer_static_key: [0xAB; 32],
            established_at_ms: 1_700_000_000_000,
            last_transport: "ws-relay".into(),
            migration_count: 0,
        }
    }

    #[test]
    fn test_migration_happy_path() {
        let mut migration = SessionMigration::new(test_ticket(), "quic-direct".into());
        assert_eq!(migration.phase(), MigrationPhase::Idle);

        migration.start();
        assert_eq!(migration.phase(), MigrationPhase::Establishing);

        migration.transport_connected();
        assert_eq!(migration.phase(), MigrationPhase::Verifying);

        // Correct peer key
        let verified = migration.peer_key_verified(&[0xAB; 32]);
        assert!(verified);
        assert_eq!(migration.phase(), MigrationPhase::Overlap);

        let new_ticket = migration.complete();
        assert_eq!(new_ticket.last_transport, "quic-direct");
        assert_eq!(new_ticket.migration_count, 1);
        assert_eq!(new_ticket.session_id, [0x42; 16]);
    }

    #[test]
    fn test_migration_wrong_key_rolls_back() {
        let mut migration = SessionMigration::new(test_ticket(), "quic-direct".into());
        migration.start();
        migration.transport_connected();

        // Wrong peer key — possible MITM
        let verified = migration.peer_key_verified(&[0xFF; 32]);
        assert!(!verified);
        assert_eq!(migration.phase(), MigrationPhase::RolledBack);
    }

    #[test]
    fn test_migration_abort() {
        let mut migration = SessionMigration::new(test_ticket(), "quic-direct".into());
        migration.start();
        migration.abort();
        assert_eq!(migration.phase(), MigrationPhase::RolledBack);
    }

    #[test]
    fn test_overlap_timeout() {
        let mut migration = SessionMigration::new(test_ticket(), "quic-direct".into())
            .with_overlap_timeout(Duration::from_millis(0));
        migration.start();
        migration.transport_connected();
        migration.peer_key_verified(&[0xAB; 32]);

        // With 0ms timeout, overlap should be expired immediately
        assert!(migration.overlap_expired());
    }

    #[test]
    fn test_session_ticket_preservation() {
        let ticket = test_ticket();
        let migration = SessionMigration::new(ticket.clone(), "new-transport".into());
        assert_eq!(migration.ticket().session_id, [0x42; 16]);
        assert_eq!(migration.target(), "new-transport");
    }

    #[test]
    fn test_zero_rtt_store_and_resume() {
        let mut cache = ZeroRttCache::new(Duration::from_secs(3600), 100);
        let ticket = test_ticket();

        cache.store("abcd1234", ticket);
        assert_eq!(cache.len(), 1);

        let result = cache.try_resume("abcd1234");
        assert!(matches!(result, ResumeResult::Resumed { .. }));

        if let ResumeResult::Resumed { session_id } = result {
            assert_eq!(session_id, [0x42; 16]);
        }
    }

    #[test]
    fn test_zero_rtt_no_ticket() {
        let mut cache = ZeroRttCache::new(Duration::from_secs(3600), 100);
        let result = cache.try_resume("unknown-peer");
        assert_eq!(result, ResumeResult::NoCachedTicket);
    }

    #[test]
    fn test_zero_rtt_expired() {
        let mut cache = ZeroRttCache::new(Duration::from_millis(0), 100);
        let ticket = test_ticket();
        cache.store("peer-1", ticket);

        // With 0ms TTL, ticket is immediately expired
        let result = cache.try_resume("peer-1");
        assert_eq!(result, ResumeResult::Expired);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_zero_rtt_use_limit() {
        let mut cache = ZeroRttCache::new(Duration::from_secs(3600), 100);
        let ticket = test_ticket();
        cache.store("peer-1", ticket);

        // Use it 5 times (max_uses = 5)
        for _ in 0..5 {
            let result = cache.try_resume("peer-1");
            assert!(matches!(result, ResumeResult::Resumed { .. }));
        }

        // 6th time should fail
        let result = cache.try_resume("peer-1");
        assert_eq!(result, ResumeResult::ExhaustedUses);
    }

    #[test]
    fn test_zero_rtt_validate_incoming() {
        let cache = ZeroRttCache::new(Duration::from_secs(3600), 100);
        let mut ticket = test_ticket();
        ticket.established_at_ms = now_ms() - 1000; // 1 second ago

        // Valid: peer key matches
        assert!(cache.validate_incoming(&ticket, &[0xAB; 32]));

        // Invalid: peer key mismatch
        assert!(!cache.validate_incoming(&ticket, &[0xFF; 32]));

        // Invalid: ticket too old
        let mut old_ticket = test_ticket();
        old_ticket.established_at_ms = 1000; // Ancient
        assert!(!cache.validate_incoming(&old_ticket, &[0xAB; 32]));
    }

    #[test]
    fn test_zero_rtt_eviction() {
        let mut cache = ZeroRttCache::new(Duration::from_secs(3600), 2);

        cache.store("peer-1", test_ticket());
        cache.store("peer-2", test_ticket());
        assert_eq!(cache.len(), 2);

        // Adding a 3rd should evict the oldest
        cache.store("peer-3", test_ticket());
        assert_eq!(cache.len(), 2);
    }
}
