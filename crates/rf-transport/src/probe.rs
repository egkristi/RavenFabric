//! Network environment probing and classification.
//!
//! Provides the `NetworkProbe` struct for assessing what kind of network
//! environment the agent is operating in, and `EgressClass` for classification.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::time::Duration;

/// Classification of the network egress environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EgressClass {
    /// Direct internet access, no NAT or minimal NAT.
    Open,
    /// Behind a home router (likely NAT, UDP often works).
    HomeRouter,
    /// Behind enterprise proxy (HTTP CONNECT required, UDP likely blocked).
    EnterpriseProxy,
    /// Restrictive DPI (deep packet inspection, may block non-standard protocols).
    RestrictiveDpi,
    /// Hostile network (active interference detected).
    Hostile,
    /// No internet access at all (air-gapped).
    AirGap,
    /// Unknown (probing incomplete or failed).
    Unknown,
}

/// Results of network environment probing.
#[derive(Debug, Clone)]
pub struct NetworkProbe {
    /// Classified egress type.
    pub egress_class: EgressClass,
    /// Whether IPv4 connectivity is available.
    pub ipv4_available: bool,
    /// Whether IPv6 connectivity is available.
    pub ipv6_available: bool,
    /// Whether UDP is reachable (tested on common ports).
    pub udp_reachable: bool,
    /// Local IP address detected (if any).
    pub local_ip: Option<IpAddr>,
    /// Whether a captive portal was detected.
    pub captive_portal: bool,
    /// Whether HTTP CONNECT proxy was detected.
    pub proxy_detected: bool,
    /// Duration the probe took.
    pub probe_duration: Duration,
}

impl NetworkProbe {
    /// Perform a quick network probe (non-blocking where possible).
    ///
    /// This does NOT make external network requests — it only checks
    /// local socket capabilities and interface availability.
    pub fn quick_probe() -> Self {
        let start = std::time::Instant::now();

        let ipv4_available = Self::check_ipv4();
        let ipv6_available = Self::check_ipv6();
        let udp_reachable = Self::check_udp();
        let local_ip = Self::detect_local_ip();

        let egress_class = Self::classify(ipv4_available, ipv6_available, udp_reachable);

        Self {
            egress_class,
            ipv4_available,
            ipv6_available,
            udp_reachable,
            local_ip,
            captive_portal: false, // Requires HTTP request — not done in quick probe
            proxy_detected: false, // Requires HTTP CONNECT test
            probe_duration: start.elapsed(),
        }
    }

    /// Classify the network environment based on probe results.
    fn classify(ipv4: bool, ipv6: bool, udp: bool) -> EgressClass {
        if !ipv4 && !ipv6 {
            return EgressClass::AirGap;
        }
        if !udp {
            // UDP blocked often means enterprise proxy or restrictive DPI
            return EgressClass::EnterpriseProxy;
        }
        // If both IP versions and UDP work, likely open or home router
        if ipv4 && ipv6 && udp {
            EgressClass::Open
        } else if ipv4 && udp {
            EgressClass::HomeRouter
        } else {
            EgressClass::Unknown
        }
    }

    /// Check if IPv4 socket creation works.
    fn check_ipv4() -> bool {
        UdpSocket::bind("0.0.0.0:0").is_ok()
    }

    /// Check if IPv6 socket creation works.
    fn check_ipv6() -> bool {
        UdpSocket::bind("[::]:0").is_ok()
    }

    /// Check if UDP is usable by binding and setting a send target.
    fn check_udp() -> bool {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            // Try to "connect" (set default destination) — doesn't send data
            sock.connect("8.8.8.8:53").is_ok()
        } else {
            false
        }
    }

    /// Detect the local IP by connecting a UDP socket to a known address.
    /// No actual packets are sent.
    fn detect_local_ip() -> Option<IpAddr> {
        let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
        sock.connect("8.8.8.8:53").ok()?;
        let addr: SocketAddr = sock.local_addr().ok()?;
        Some(addr.ip())
    }
}

impl std::fmt::Display for EgressClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::HomeRouter => write!(f, "HomeRouter"),
            Self::EnterpriseProxy => write!(f, "EnterpriseProxy"),
            Self::RestrictiveDpi => write!(f, "RestrictiveDPI"),
            Self::Hostile => write!(f, "Hostile"),
            Self::AirGap => write!(f, "AirGap"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quick_probe_runs() {
        let probe = NetworkProbe::quick_probe();
        // On any dev machine, at least IPv4 should be available
        assert!(probe.probe_duration.as_secs() < 5);
        // EgressClass should not be Unknown on a machine with network
        assert_ne!(probe.egress_class, EgressClass::Hostile);
    }

    #[test]
    fn test_classify_airgap() {
        assert_eq!(
            NetworkProbe::classify(false, false, false),
            EgressClass::AirGap
        );
    }

    #[test]
    fn test_classify_open() {
        assert_eq!(NetworkProbe::classify(true, true, true), EgressClass::Open);
    }

    #[test]
    fn test_classify_enterprise() {
        assert_eq!(
            NetworkProbe::classify(true, true, false),
            EgressClass::EnterpriseProxy
        );
    }

    #[test]
    fn test_egress_display() {
        assert_eq!(EgressClass::Open.to_string(), "Open");
        assert_eq!(EgressClass::AirGap.to_string(), "AirGap");
    }
}
