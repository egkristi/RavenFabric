//! NAT type detection and ICE candidate gathering.
//!
//! Implements types for STUN-based NAT detection (RFC 5780), ICE candidate
//! collection, and coordinated hole punching.

use std::net::SocketAddr;
use std::time::Duration;

/// NAT type classification per RFC 3489/5780.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatType {
    /// No NAT — public IP directly reachable.
    Open,
    /// Full cone — any external host can send to the mapped address.
    FullCone,
    /// Restricted cone — only hosts the internal endpoint has sent to can reach it.
    RestrictedCone,
    /// Port-restricted cone — like restricted, but port must also match.
    PortRestrictedCone,
    /// Symmetric — different mapping for each destination.
    Symmetric,
    /// Could not determine (e.g., UDP blocked).
    Unknown,
}

impl NatType {
    /// Whether direct P2P connectivity is likely achievable.
    pub fn supports_hole_punch(&self) -> bool {
        matches!(
            self,
            NatType::Open
                | NatType::FullCone
                | NatType::RestrictedCone
                | NatType::PortRestrictedCone
        )
    }

    /// Whether relay is required for connectivity.
    pub fn requires_relay(&self) -> bool {
        matches!(self, NatType::Symmetric | NatType::Unknown)
    }
}

/// A STUN server endpoint for NAT detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StunServer {
    /// Server address.
    pub addr: SocketAddr,
    /// Optional secondary address (for change-request tests).
    pub alt_addr: Option<SocketAddr>,
}

/// Result of a STUN binding request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StunBinding {
    /// Local address used for the request.
    pub local_addr: SocketAddr,
    /// Server-reflexive address returned by STUN.
    pub mapped_addr: SocketAddr,
    /// STUN server that responded.
    pub server: SocketAddr,
    /// Round-trip time of the request.
    pub rtt: Duration,
}

/// ICE candidate type per RFC 8445.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateType {
    /// Host candidate — local interface address.
    Host,
    /// Server reflexive — discovered via STUN.
    ServerReflexive,
    /// Peer reflexive — discovered during connectivity checks.
    PeerReflexive,
    /// Relayed — traffic goes through a TURN relay.
    Relayed,
}

/// Transport protocol for an ICE candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateTransport {
    Udp,
    Tcp,
}

/// An ICE candidate for connectivity establishment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCandidate {
    /// Candidate type.
    pub candidate_type: CandidateType,
    /// Transport protocol.
    pub transport: CandidateTransport,
    /// Address of this candidate.
    pub addr: SocketAddr,
    /// Related address (e.g., base for srflx).
    pub related_addr: Option<SocketAddr>,
    /// Priority (higher = preferred).
    pub priority: u32,
    /// Foundation string (for pairing).
    pub foundation: String,
}

impl IceCandidate {
    /// Compute priority per RFC 8445 formula.
    /// priority = (2^24) * type_pref + (2^8) * local_pref + (256 - component_id)
    pub fn compute_priority(type_pref: u32, local_pref: u32, component_id: u32) -> u32 {
        (1 << 24) * type_pref + (1 << 8) * local_pref + (256 - component_id)
    }

    /// Default type preference values per RFC 8445.
    pub fn type_preference(candidate_type: &CandidateType) -> u32 {
        match candidate_type {
            CandidateType::Host => 126,
            CandidateType::PeerReflexive => 110,
            CandidateType::ServerReflexive => 100,
            CandidateType::Relayed => 0,
        }
    }
}

/// State of an ICE candidate pair check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    /// Waiting to be checked.
    Waiting,
    /// Check in progress.
    InProgress,
    /// Check succeeded.
    Succeeded,
    /// Check failed.
    Failed,
    /// Pair frozen (waiting for another pair to complete first).
    Frozen,
}

/// An ICE candidate pair (local + remote).
#[derive(Debug, Clone)]
pub struct CandidatePair {
    /// Local candidate.
    pub local: IceCandidate,
    /// Remote candidate.
    pub remote: IceCandidate,
    /// Pair priority (computed from both candidates).
    pub priority: u64,
    /// Current check state.
    pub state: PairState,
    /// Measured RTT if check succeeded.
    pub rtt: Option<Duration>,
}

impl CandidatePair {
    /// Compute pair priority per RFC 8445.
    /// pair_priority = 2^32 * MIN(G,D) + 2 * MAX(G,D) + (G>D ? 1 : 0)
    pub fn compute_pair_priority(controlling_prio: u32, controlled_prio: u32) -> u64 {
        let g = controlling_prio as u64;
        let d = controlled_prio as u64;
        let min = g.min(d);
        let max = g.max(d);
        (1u64 << 32) * min + 2 * max + if g > d { 1 } else { 0 }
    }
}

/// Hole punch coordination message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HolePunchMessage {
    /// Request relay to coordinate hole punch with peer.
    InitiateRequest {
        peer_id: String,
        candidates: Vec<IceCandidate>,
    },
    /// Relay forwards candidates to peer.
    CandidateExchange {
        from_peer: String,
        candidates: Vec<IceCandidate>,
    },
    /// Connectivity check probe.
    ConnectivityCheck { transaction_id: [u8; 12] },
    /// Response to connectivity check.
    ConnectivityCheckResponse {
        transaction_id: [u8; 12],
        mapped_addr: SocketAddr,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_nat_type_hole_punch() {
        assert!(NatType::Open.supports_hole_punch());
        assert!(NatType::FullCone.supports_hole_punch());
        assert!(NatType::RestrictedCone.supports_hole_punch());
        assert!(NatType::PortRestrictedCone.supports_hole_punch());
        assert!(!NatType::Symmetric.supports_hole_punch());
        assert!(!NatType::Unknown.supports_hole_punch());
    }

    #[test]
    fn test_nat_type_requires_relay() {
        assert!(!NatType::Open.requires_relay());
        assert!(!NatType::FullCone.requires_relay());
        assert!(NatType::Symmetric.requires_relay());
        assert!(NatType::Unknown.requires_relay());
    }

    #[test]
    fn test_ice_priority() {
        // Host candidate, high local pref, component 1
        let prio = IceCandidate::compute_priority(126, 65535, 1);
        assert!(prio > 0);

        // Server reflexive should be lower than host
        let srflx_prio = IceCandidate::compute_priority(100, 65535, 1);
        assert!(prio > srflx_prio);
    }

    #[test]
    fn test_type_preference() {
        assert!(
            IceCandidate::type_preference(&CandidateType::Host)
                > IceCandidate::type_preference(&CandidateType::ServerReflexive)
        );
        assert!(
            IceCandidate::type_preference(&CandidateType::ServerReflexive)
                > IceCandidate::type_preference(&CandidateType::Relayed)
        );
    }

    #[test]
    fn test_pair_priority() {
        let prio = CandidatePair::compute_pair_priority(100, 50);
        assert!(prio > 0);

        // Symmetric: same values should produce consistent result
        let prio2 = CandidatePair::compute_pair_priority(50, 100);
        // Order matters — controlling vs controlled
        assert_ne!(prio, prio2);
    }

    #[test]
    fn test_candidate_creation() {
        let candidate = IceCandidate {
            candidate_type: CandidateType::Host,
            transport: CandidateTransport::Udp,
            addr: SocketAddr::new(Ipv4Addr::new(192, 168, 1, 100).into(), 5000),
            related_addr: None,
            priority: IceCandidate::compute_priority(126, 65535, 1),
            foundation: "1".to_string(),
        };
        assert_eq!(candidate.candidate_type, CandidateType::Host);
        assert_eq!(candidate.addr.port(), 5000);
    }

    #[test]
    fn test_ipv6_candidate() {
        let candidate = IceCandidate {
            candidate_type: CandidateType::ServerReflexive,
            transport: CandidateTransport::Udp,
            addr: SocketAddr::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).into(), 5000),
            related_addr: Some(SocketAddr::new(
                Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1).into(),
                5000,
            )),
            priority: IceCandidate::compute_priority(100, 65535, 1),
            foundation: "2".to_string(),
        };
        assert_eq!(candidate.candidate_type, CandidateType::ServerReflexive);
        assert!(candidate.related_addr.is_some());
    }

    #[test]
    fn test_pair_states() {
        let pair = CandidatePair {
            local: IceCandidate {
                candidate_type: CandidateType::Host,
                transport: CandidateTransport::Udp,
                addr: SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 1000),
                related_addr: None,
                priority: 100,
                foundation: "1".to_string(),
            },
            remote: IceCandidate {
                candidate_type: CandidateType::ServerReflexive,
                transport: CandidateTransport::Udp,
                addr: SocketAddr::new(Ipv4Addr::new(203, 0, 113, 1).into(), 2000),
                related_addr: None,
                priority: 80,
                foundation: "2".to_string(),
            },
            priority: CandidatePair::compute_pair_priority(100, 80),
            state: PairState::Frozen,
            rtt: None,
        };
        assert_eq!(pair.state, PairState::Frozen);
        assert!(pair.rtt.is_none());
    }
}
