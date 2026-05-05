//! Schedule-aware routing for intermittent connectivity.
//!
//! Models contact windows (when nodes are reachable), satellite passes,
//! and opportunistic sync for DTN-style delivery.

use serde::{Deserialize, Serialize};

/// A recurring or one-time contact window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactWindow {
    /// Peer/relay this window applies to.
    pub peer_id: String,
    /// Transport to use during this window.
    pub transport: String,
    /// Window start (Unix timestamp ms, or time-of-day for recurring).
    pub start_ms: u64,
    /// Window duration in milliseconds.
    pub duration_ms: u64,
    /// Recurrence pattern.
    pub recurrence: Recurrence,
    /// Expected bandwidth during this window (bytes/sec, 0 = unknown).
    pub bandwidth_bps: u64,
    /// Confidence level (0.0 - 1.0) that this window will actually be available.
    pub confidence: f64,
}

/// Recurrence pattern for contact windows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Recurrence {
    /// One-time window (e.g., satellite pass).
    Once,
    /// Daily at the same time.
    Daily,
    /// Weekly on specific days (0=Sun, 1=Mon, ...).
    Weekly { days: Vec<u8> },
    /// Orbital period (e.g., 90-minute LEO pass).
    Orbital { period_ms: u64 },
    /// Opportunistic — no schedule, sync when available.
    Opportunistic,
}

/// Check if a contact window is active at a given time.
pub fn is_window_active(window: &ContactWindow, now_ms: u64) -> bool {
    match &window.recurrence {
        Recurrence::Once => {
            now_ms >= window.start_ms && now_ms < window.start_ms + window.duration_ms
        }
        Recurrence::Daily => {
            let ms_in_day = 86_400_000u64;
            let time_of_day = now_ms % ms_in_day;
            let window_start = window.start_ms % ms_in_day;
            let window_end = window_start + window.duration_ms;
            time_of_day >= window_start && time_of_day < window_end
        }
        Recurrence::Weekly { days } => {
            let ms_in_day = 86_400_000u64;
            let ms_in_week = ms_in_day * 7;
            let time_in_week = now_ms % ms_in_week;
            let day_of_week = (time_in_week / ms_in_day) as u8;

            if !days.contains(&day_of_week) {
                return false;
            }

            let time_of_day = now_ms % ms_in_day;
            let window_start = window.start_ms % ms_in_day;
            time_of_day >= window_start && time_of_day < window_start + window.duration_ms
        }
        Recurrence::Orbital { period_ms } => {
            if *period_ms == 0 {
                return false;
            }
            let elapsed_in_period = (now_ms.saturating_sub(window.start_ms)) % period_ms;
            elapsed_in_period < window.duration_ms
        }
        Recurrence::Opportunistic => {
            // Opportunistic windows are always "potentially" active
            true
        }
    }
}

/// Next window opening time (ms from now). Returns None if opportunistic.
pub fn next_window(window: &ContactWindow, now_ms: u64) -> Option<u64> {
    if is_window_active(window, now_ms) {
        return Some(0); // Already active
    }

    match &window.recurrence {
        Recurrence::Once => {
            if now_ms < window.start_ms {
                Some(window.start_ms - now_ms)
            } else {
                None // Window has passed
            }
        }
        Recurrence::Daily => {
            let ms_in_day = 86_400_000u64;
            let time_of_day = now_ms % ms_in_day;
            let window_start = window.start_ms % ms_in_day;

            if time_of_day < window_start {
                Some(window_start - time_of_day)
            } else {
                Some(ms_in_day - time_of_day + window_start)
            }
        }
        Recurrence::Orbital { period_ms } => {
            if *period_ms == 0 {
                return None;
            }
            let elapsed_in_period = (now_ms.saturating_sub(window.start_ms)) % period_ms;
            Some(period_ms - elapsed_in_period)
        }
        Recurrence::Weekly { .. } => {
            // Simplified: return max 7 days
            Some(7 * 86_400_000)
        }
        Recurrence::Opportunistic => None,
    }
}

/// Routing decision for a bundle based on contact schedules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Send immediately through this peer/transport.
    SendNow { peer_id: String, transport: String },
    /// Queue and wait for next window.
    QueueUntil { peer_id: String, wait_ms: u64 },
    /// No known route to destination.
    NoRoute,
    /// Multiple paths available — use best.
    MultiPath(Vec<String>),
}

/// Select the best route for a given destination.
pub fn select_route(destination: &str, windows: &[ContactWindow], now_ms: u64) -> RoutingDecision {
    let mut active_routes = Vec::new();
    let mut best_queued: Option<(String, u64)> = None;

    for window in windows {
        if window.peer_id != destination {
            continue;
        }

        if is_window_active(window, now_ms) {
            active_routes.push(window.peer_id.clone());
        } else if let Some(wait) = next_window(window, now_ms) {
            match &best_queued {
                None => best_queued = Some((window.peer_id.clone(), wait)),
                Some((_, current_wait)) => {
                    if wait < *current_wait {
                        best_queued = Some((window.peer_id.clone(), wait));
                    }
                }
            }
        }
    }

    if active_routes.len() > 1 {
        RoutingDecision::MultiPath(active_routes)
    } else if let Some(route) = active_routes.into_iter().next() {
        // Find the transport for this route
        let transport = windows
            .iter()
            .find(|w| w.peer_id == route && is_window_active(w, now_ms))
            .map(|w| w.transport.clone())
            .unwrap_or_default();
        RoutingDecision::SendNow {
            peer_id: route,
            transport,
        }
    } else if let Some((peer_id, wait_ms)) = best_queued {
        RoutingDecision::QueueUntil { peer_id, wait_ms }
    } else {
        RoutingDecision::NoRoute
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_once_window_active() {
        let window = ContactWindow {
            peer_id: "sat-1".into(),
            transport: "radio".into(),
            start_ms: 1000,
            duration_ms: 5000,
            recurrence: Recurrence::Once,
            bandwidth_bps: 9600,
            confidence: 0.95,
        };

        assert!(!is_window_active(&window, 500));
        assert!(is_window_active(&window, 1000));
        assert!(is_window_active(&window, 3000));
        assert!(!is_window_active(&window, 6000));
    }

    #[test]
    fn test_orbital_window() {
        let window = ContactWindow {
            peer_id: "iss".into(),
            transport: "uhf".into(),
            start_ms: 0,
            duration_ms: 600_000, // 10 min pass
            recurrence: Recurrence::Orbital {
                period_ms: 5_400_000, // 90 min orbit
            },
            bandwidth_bps: 19200,
            confidence: 0.8,
        };

        assert!(is_window_active(&window, 300_000)); // During first pass
        assert!(!is_window_active(&window, 1_000_000)); // Between passes
        assert!(is_window_active(&window, 5_400_000)); // Start of second pass
    }

    #[test]
    fn test_opportunistic_always_active() {
        let window = ContactWindow {
            peer_id: "drone".into(),
            transport: "bluetooth".into(),
            start_ms: 0,
            duration_ms: 0,
            recurrence: Recurrence::Opportunistic,
            bandwidth_bps: 0,
            confidence: 0.1,
        };

        assert!(is_window_active(&window, 0));
        assert!(is_window_active(&window, u64::MAX - 1));
    }

    #[test]
    fn test_next_window_once() {
        let window = ContactWindow {
            peer_id: "sat".into(),
            transport: "radio".into(),
            start_ms: 10_000,
            duration_ms: 5000,
            recurrence: Recurrence::Once,
            bandwidth_bps: 0,
            confidence: 1.0,
        };

        assert_eq!(next_window(&window, 5000), Some(5000));
        assert_eq!(next_window(&window, 10_000), Some(0)); // Active
        assert_eq!(next_window(&window, 20_000), None); // Passed
    }

    #[test]
    fn test_select_route_active() {
        let windows = vec![ContactWindow {
            peer_id: "target".into(),
            transport: "websocket".into(),
            start_ms: 0,
            duration_ms: 100_000,
            recurrence: Recurrence::Once,
            bandwidth_bps: 1_000_000,
            confidence: 1.0,
        }];

        let decision = select_route("target", &windows, 5000);
        assert_eq!(
            decision,
            RoutingDecision::SendNow {
                peer_id: "target".into(),
                transport: "websocket".into()
            }
        );
    }

    #[test]
    fn test_select_route_queued() {
        let windows = vec![ContactWindow {
            peer_id: "target".into(),
            transport: "radio".into(),
            start_ms: 10_000,
            duration_ms: 5000,
            recurrence: Recurrence::Once,
            bandwidth_bps: 0,
            confidence: 1.0,
        }];

        let decision = select_route("target", &windows, 5000);
        match decision {
            RoutingDecision::QueueUntil { peer_id, wait_ms } => {
                assert_eq!(peer_id, "target");
                assert_eq!(wait_ms, 5000);
            }
            _ => panic!("expected QueueUntil"),
        }
    }

    #[test]
    fn test_select_route_no_route() {
        let decision = select_route("unknown", &[], 1000);
        assert_eq!(decision, RoutingDecision::NoRoute);
    }
}
