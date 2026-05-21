//! Sliding-window per-IP rate limiter (same pattern as rf-relay).

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Per-IP sliding-window HTTP request rate limiter.
pub struct RateLimiter {
    window: Duration,
    max_requests: u32,
    /// Map of IP → list of request timestamps within the current window.
    buckets: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    /// Create a new limiter allowing `max_requests` per `window_secs` seconds.
    pub fn new(window_secs: u64, max_requests: u32) -> Self {
        Self {
            window: Duration::from_secs(window_secs),
            max_requests,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if this request is allowed, `false` if it should be
    /// rate-limited.
    pub fn check_and_record(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let timestamps = buckets.entry(ip).or_default();

        // Evict entries outside the window.
        timestamps.retain(|t| now.duration_since(*t) < self.window);

        if timestamps.len() as u32 >= self.max_requests {
            return false;
        }
        timestamps.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn allows_up_to_limit() {
        let limiter = RateLimiter::new(60, 3);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(limiter.check_and_record(ip));
        assert!(limiter.check_and_record(ip));
        assert!(limiter.check_and_record(ip));
        assert!(!limiter.check_and_record(ip)); // 4th request denied
    }

    #[test]
    fn different_ips_have_separate_buckets() {
        let limiter = RateLimiter::new(60, 1);
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        assert!(limiter.check_and_record(ip1));
        assert!(!limiter.check_and_record(ip1));
        assert!(limiter.check_and_record(ip2)); // separate bucket
    }
}
