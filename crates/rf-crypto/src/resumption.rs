//! 0-RTT session resumption for known peers.
//!
//! When a peer has been previously authenticated via Noise XX, subsequent
//! connections can use a cached session ticket to skip the full handshake.
//! The ticket contains the peer's static key and session metadata, allowing
//! the transport to verify identity before completing the full Noise handshake.

use serde::{Deserialize, Serialize};

/// A session ticket for 0-RTT resumption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResumptionTicket {
    /// Peer's static public key (32 bytes, hex-encoded).
    pub peer_static_key: String,
    /// When the original session was established (Unix timestamp ms).
    pub established_at_ms: u64,
    /// When this ticket expires (Unix timestamp ms).
    pub expires_at_ms: u64,
    /// Application data to restore (e.g., agent ID, roles).
    pub metadata: TicketMetadata,
    /// Ticket nonce — prevents replay.
    pub nonce: [u8; 16],
}

/// Metadata embedded in a resumption ticket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketMetadata {
    /// Peer's agent ID.
    pub agent_id: String,
    /// Last known transport used.
    pub last_transport: String,
    /// Number of previous resumptions.
    pub resumption_count: u32,
}

/// Result of validating a resumption ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketValidation {
    /// Ticket is valid — can proceed with 0-RTT.
    Valid,
    /// Ticket has expired.
    Expired,
    /// Peer key doesn't match current connection.
    KeyMismatch,
    /// Ticket has been revoked.
    Revoked,
    /// Nonce was already used (replay attempt).
    Replay,
}

/// Ticket store for managing resumption tickets.
#[derive(Debug)]
pub struct TicketStore {
    /// Tickets keyed by peer static key (hex).
    tickets: std::collections::HashMap<String, ResumptionTicket>,
    /// Revoked ticket nonces.
    revoked_nonces: std::collections::HashSet<[u8; 16]>,
    /// Used nonces (replay protection).
    used_nonces: std::collections::HashSet<[u8; 16]>,
    /// Maximum ticket lifetime in milliseconds.
    max_lifetime_ms: u64,
}

impl TicketStore {
    /// Create a new ticket store with the given max ticket lifetime.
    pub fn new(max_lifetime_ms: u64) -> Self {
        Self {
            tickets: std::collections::HashMap::new(),
            revoked_nonces: std::collections::HashSet::new(),
            used_nonces: std::collections::HashSet::new(),
            max_lifetime_ms,
        }
    }

    /// Issue a new resumption ticket for a peer.
    pub fn issue(
        &mut self,
        peer_key: &str,
        agent_id: &str,
        transport: &str,
        now_ms: u64,
        nonce: [u8; 16],
    ) -> ResumptionTicket {
        let ticket = ResumptionTicket {
            peer_static_key: peer_key.to_string(),
            established_at_ms: now_ms,
            expires_at_ms: now_ms + self.max_lifetime_ms,
            metadata: TicketMetadata {
                agent_id: agent_id.to_string(),
                last_transport: transport.to_string(),
                resumption_count: self
                    .tickets
                    .get(peer_key)
                    .map(|t| t.metadata.resumption_count + 1)
                    .unwrap_or(0),
            },
            nonce,
        };
        self.tickets.insert(peer_key.to_string(), ticket.clone());
        ticket
    }

    /// Validate a ticket for resumption.
    pub fn validate(&mut self, ticket: &ResumptionTicket, now_ms: u64) -> TicketValidation {
        // Check replay
        if self.used_nonces.contains(&ticket.nonce) {
            return TicketValidation::Replay;
        }

        // Check revocation
        if self.revoked_nonces.contains(&ticket.nonce) {
            return TicketValidation::Revoked;
        }

        // Check expiry
        if now_ms > ticket.expires_at_ms {
            return TicketValidation::Expired;
        }

        // Check that peer key matches stored ticket
        if let Some(stored) = self.tickets.get(&ticket.peer_static_key) {
            if stored.nonce != ticket.nonce {
                return TicketValidation::KeyMismatch;
            }
        }

        // Mark nonce as used
        self.used_nonces.insert(ticket.nonce);

        TicketValidation::Valid
    }

    /// Revoke a ticket by nonce.
    pub fn revoke(&mut self, nonce: [u8; 16]) {
        self.revoked_nonces.insert(nonce);
    }

    /// Remove expired tickets (garbage collection).
    pub fn prune(&mut self, now_ms: u64) {
        self.tickets.retain(|_, t| t.expires_at_ms > now_ms);
    }

    /// Get a stored ticket for a peer.
    pub fn get(&self, peer_key: &str) -> Option<&ResumptionTicket> {
        self.tickets.get(peer_key)
    }

    /// Number of stored tickets.
    pub fn ticket_count(&self) -> usize {
        self.tickets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_nonce() -> [u8; 16] {
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    }

    #[test]
    fn test_issue_ticket() {
        let mut store = TicketStore::new(3_600_000); // 1 hour
        let ticket = store.issue("abcd1234", "web-01", "websocket", 1000, test_nonce());

        assert_eq!(ticket.peer_static_key, "abcd1234");
        assert_eq!(ticket.metadata.agent_id, "web-01");
        assert_eq!(ticket.established_at_ms, 1000);
        assert_eq!(ticket.expires_at_ms, 3_601_000);
        assert_eq!(ticket.metadata.resumption_count, 0);
    }

    #[test]
    fn test_validate_valid() {
        let mut store = TicketStore::new(3_600_000);
        let ticket = store.issue("peer-key", "agent-1", "ws", 1000, test_nonce());

        let result = store.validate(&ticket, 2000);
        assert_eq!(result, TicketValidation::Valid);
    }

    #[test]
    fn test_validate_expired() {
        let mut store = TicketStore::new(1000); // 1 second lifetime
        let ticket = store.issue("peer-key", "agent-1", "ws", 1000, test_nonce());

        let result = store.validate(&ticket, 5000); // Well past expiry
        assert_eq!(result, TicketValidation::Expired);
    }

    #[test]
    fn test_validate_replay() {
        let mut store = TicketStore::new(3_600_000);
        let ticket = store.issue("peer-key", "agent-1", "ws", 1000, test_nonce());

        // First use: valid
        let result = store.validate(&ticket, 2000);
        assert_eq!(result, TicketValidation::Valid);

        // Second use: replay
        let result = store.validate(&ticket, 3000);
        assert_eq!(result, TicketValidation::Replay);
    }

    #[test]
    fn test_validate_revoked() {
        let mut store = TicketStore::new(3_600_000);
        let nonce = test_nonce();
        let ticket = store.issue("peer-key", "agent-1", "ws", 1000, nonce);

        store.revoke(nonce);
        let result = store.validate(&ticket, 2000);
        assert_eq!(result, TicketValidation::Revoked);
    }

    #[test]
    fn test_prune() {
        let mut store = TicketStore::new(1000);
        store.issue("peer-1", "a1", "ws", 1000, [1; 16]);
        store.issue("peer-2", "a2", "ws", 2000, [2; 16]);
        store.issue("peer-3", "a3", "ws", 5000, [3; 16]);

        assert_eq!(store.ticket_count(), 3);
        store.prune(4000); // peer-1 and peer-2 expired
        assert_eq!(store.ticket_count(), 1);
    }

    #[test]
    fn test_resumption_count_increments() {
        let mut store = TicketStore::new(3_600_000);
        store.issue("peer-key", "agent", "ws", 1000, [1; 16]);
        let ticket2 = store.issue("peer-key", "agent", "ws", 2000, [2; 16]);
        assert_eq!(ticket2.metadata.resumption_count, 1);

        let ticket3 = store.issue("peer-key", "agent", "ws", 3000, [3; 16]);
        assert_eq!(ticket3.metadata.resumption_count, 2);
    }
}
