//! Session migration — transport upgrade/downgrade without dropping the logical session.
//!
//! Implements:
//! - Background transport upgrade (relay → direct)
//! - Session ticket for resumption on new transport
//! - Atomic swap (make-before-break with overlap window)

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
}
