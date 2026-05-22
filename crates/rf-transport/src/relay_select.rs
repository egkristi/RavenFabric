//! Region-aware relay endpoint selection.
//!
//! The [`RelaySelector`] combines three signals to choose the best relay for a
//! connecting agent:
//!
//! 1. **Geographic proximity** — via GeoIP, prefer relays in the same region as
//!    the client.
//! 2. **Measured latency** — if recent RTT probes are available, weight by
//!    round-trip time.
//! 3. **Affinity groups** — `multi_relay_affinity()` first tries same-continent
//!    relays, then adjacent continents, then falls back to any relay.
//!
//! When the `geoip` feature is disabled, the selector falls back to a simple
//! round-robin / latency-only approach with no geographic weighting.
//!
//! # Example
//! ```rust
//! use rf_transport::relay_select::{RelayEndpoint, RelaySelector};
//! use std::net::IpAddr;
//!
//! let relays = vec![
//!     RelayEndpoint::new("wss://eu-relay.example.com:9090").with_region_code("EU"),
//!     RelayEndpoint::new("wss://us-relay.example.com:9090").with_region_code("NA"),
//! ];
//! let selector = RelaySelector::new(relays);
//! let best = selector.round_robin();
//! ```

/// A relay endpoint with optional region and latency metadata.
#[derive(Clone, Debug)]
pub struct RelayEndpoint {
    /// WebSocket URL of the relay (e.g. `wss://relay.example.com:9090`).
    pub addr: String,

    /// ISO continent code for the relay's data-centre location
    /// (e.g. `"EU"`, `"NA"`, `"AS"`). Optional.
    pub continent: Option<String>,

    /// ISO 3166-1 alpha-2 country code of the relay. Optional.
    pub country_code: Option<String>,

    /// Latitude of the relay's data-centre. Used for Haversine calculations.
    pub latitude: Option<f64>,

    /// Longitude of the relay's data-centre.
    pub longitude: Option<f64>,

    /// Most recent measured round-trip time in milliseconds. `None` if not
    /// yet probed.
    pub rtt_ms: Option<u32>,

    /// Static weight in range (0.0, 1.0]; lower = preferred all else equal.
    /// Defaults to 1.0. Use to de-prefer expensive or distant relays.
    pub weight: f64,
}

impl RelayEndpoint {
    /// Create a relay endpoint with just an address; metadata can be added via
    /// builder-style setters.
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            continent: None,
            country_code: None,
            latitude: None,
            longitude: None,
            rtt_ms: None,
            weight: 1.0,
        }
    }

    /// Set the continent code for this relay.
    pub fn with_continent(mut self, continent: impl Into<String>) -> Self {
        self.continent = Some(continent.into());
        self
    }

    /// Set a region code (alias for continent for backward compat).
    pub fn with_region_code(mut self, code: impl Into<String>) -> Self {
        self.continent = Some(code.into());
        self
    }

    /// Set the country code.
    pub fn with_country(mut self, code: impl Into<String>) -> Self {
        self.country_code = Some(code.into());
        self
    }

    /// Set geographic coordinates.
    pub fn with_coords(mut self, latitude: f64, longitude: f64) -> Self {
        self.latitude = Some(latitude);
        self.longitude = Some(longitude);
        self
    }

    /// Set a measured RTT (milliseconds).
    pub fn with_rtt_ms(mut self, rtt_ms: u32) -> Self {
        self.rtt_ms = Some(rtt_ms);
        self
    }

    /// Set the static weight. Lower values = higher preference.
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight.max(0.001);
        self
    }

    /// Return true if this relay has geographic coordinates.
    pub fn has_coords(&self) -> bool {
        self.latitude.is_some() && self.longitude.is_some()
    }

    /// Haversine distance in km to a (lat, lon) point.
    /// Returns `f64::MAX` if this relay has no coordinates.
    pub fn distance_to(&self, lat: f64, lon: f64) -> f64 {
        match (self.latitude, self.longitude) {
            (Some(rlat), Some(rlon)) => haversine_km(rlat, rlon, lat, lon),
            _ => f64::MAX,
        }
    }
}

/// Selects the best relay for an agent connection.
pub struct RelaySelector {
    relays: Vec<RelayEndpoint>,
}

impl RelaySelector {
    /// Create a selector from a list of relay endpoints.
    pub fn new(relays: Vec<RelayEndpoint>) -> Self {
        Self { relays }
    }

    /// Return all relays (empty slice if none configured).
    pub fn all(&self) -> &[RelayEndpoint] {
        &self.relays
    }

    /// Return a relay using simple round-robin (no weighting). Returns `None`
    /// if no relays are configured.
    pub fn round_robin(&self) -> Option<&RelayEndpoint> {
        self.relays.first()
    }

    /// Return the relay with the lowest measured RTT.
    ///
    /// Relays without an RTT measurement are ranked last. If multiple relays
    /// share the lowest RTT, the first encountered is returned.
    pub fn lowest_rtt(&self) -> Option<&RelayEndpoint> {
        if self.relays.is_empty() {
            return None;
        }
        self.relays
            .iter()
            .min_by(|a, b| {
                let rtt_a = a.rtt_ms.unwrap_or(u32::MAX);
                let rtt_b = b.rtt_ms.unwrap_or(u32::MAX);
                rtt_a.cmp(&rtt_b)
            })
    }

    /// Return the geographically nearest relay to the given coordinates.
    ///
    /// Relays without coordinate data are ranked last. Returns `None` if the
    /// relay list is empty.
    pub fn nearest_to_coords(&self, lat: f64, lon: f64) -> Option<&RelayEndpoint> {
        if self.relays.is_empty() {
            return None;
        }
        self.relays
            .iter()
            .min_by(|a, b| {
                let da = a.distance_to(lat, lon);
                let db = b.distance_to(lat, lon);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Select the best relay using a latency-weighted geo-distance score.
    ///
    /// Score formula: `score = 0.7 × rtt_ms + 0.3 × (geo_distance_km / 100)`
    ///
    /// Lower score = better. RTT is given higher weight (0.7) than geographic
    /// distance (0.3) since actual latency is a better predictor of performance
    /// than raw distance.
    ///
    /// Relays without RTT data are penalised by adding 500 ms to the RTT term.
    /// Relays without coordinates are penalised by adding 10,000 km to the
    /// distance term.
    pub fn latency_weighted(&self, client_lat: f64, client_lon: f64) -> Option<&RelayEndpoint> {
        if self.relays.is_empty() {
            return None;
        }
        self.relays
            .iter()
            .min_by(|a, b| {
                let score_a = latency_geo_score(a, client_lat, client_lon);
                let score_b = latency_geo_score(b, client_lat, client_lon);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Return relays ordered by continental affinity relative to `client_continent`.
    ///
    /// Priority:
    /// 1. Same continent as the client.
    /// 2. Adjacent continents (EU↔NA, NA↔SA, AS↔OC, etc.).
    /// 3. All remaining relays.
    ///
    /// Within each tier, relays are sorted by measured RTT (ascending).
    /// Returns all relays (never empty if the relay list is non-empty).
    pub fn multi_relay_affinity(&self, client_continent: &str) -> Vec<&RelayEndpoint> {
        let mut same = Vec::new();
        let mut adjacent = Vec::new();
        let mut other = Vec::new();

        for relay in &self.relays {
            match relay.continent.as_deref() {
                Some(c) if c == client_continent => same.push(relay),
                Some(c) if continents_adjacent(client_continent, c) => adjacent.push(relay),
                _ => other.push(relay),
            }
        }

        let sort_by_rtt = |a: &&RelayEndpoint, b: &&RelayEndpoint| {
            a.rtt_ms
                .unwrap_or(u32::MAX)
                .cmp(&b.rtt_ms.unwrap_or(u32::MAX))
        };

        same.sort_by(sort_by_rtt);
        adjacent.sort_by(sort_by_rtt);
        other.sort_by(sort_by_rtt);

        same.into_iter()
            .chain(adjacent)
            .chain(other)
            .collect()
    }

    /// Unified "best relay" selection.
    ///
    /// When geographic coordinates are available, uses
    /// [`latency_weighted`][Self::latency_weighted]. Falls back to
    /// [`lowest_rtt`][Self::lowest_rtt] when no coordinates are supplied, and
    /// finally [`round_robin`][Self::round_robin] if no RTT data exists either.
    pub fn best(
        &self,
        client_lat: Option<f64>,
        client_lon: Option<f64>,
    ) -> Option<&RelayEndpoint> {
        match (client_lat, client_lon) {
            (Some(lat), Some(lon)) => self.latency_weighted(lat, lon),
            _ => self.lowest_rtt().or_else(|| self.round_robin()),
        }
    }

    /// Lookup the best relay by client IP address using a MaxMind GeoIP2 database.
    /// Falls back to [`best`][Self::best] without coordinates if the
    /// IP cannot be resolved.
    ///
    /// Only available when `feature = "geoip"` is enabled.
    #[cfg(feature = "geoip")]
    pub fn best_for_ip(&self, client_ip: IpAddr, db_path: &std::path::Path) -> Option<&RelayEndpoint> {
        if let Ok(db) = maxminddb::Reader::<Vec<u8>>::open_readfile(db_path) {
            if let Ok(city) = db.lookup::<maxminddb::geoip2::City>(client_ip) {
                let lat = city.location.as_ref().and_then(|l| l.latitude);
                let lon = city.location.as_ref().and_then(|l| l.longitude);
                if let (Some(lat), Some(lon)) = (lat, lon) {
                    return self.latency_weighted(lat, lon);
                }
            }
        }
        self.best(None, None)
    }
}

// ── Scoring helpers ───────────────────────────────────────────────────────────

fn latency_geo_score(relay: &RelayEndpoint, client_lat: f64, client_lon: f64) -> f64 {
    const RTT_WEIGHT: f64 = 0.7;
    const GEO_WEIGHT: f64 = 0.3;
    const RTT_PENALTY_MS: f64 = 500.0;
    const GEO_PENALTY_KM: f64 = 10_000.0;

    let rtt = relay.rtt_ms.map(|r| r as f64).unwrap_or(RTT_PENALTY_MS);
    let geo = match (relay.latitude, relay.longitude) {
        (Some(lat), Some(lon)) => haversine_km(lat, lon, client_lat, client_lon),
        _ => GEO_PENALTY_KM,
    };

    (RTT_WEIGHT * rtt + GEO_WEIGHT * (geo / 100.0)) * relay.weight
}

/// Haversine great-circle distance in kilometres.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371.0;
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Return true if two continent codes are geographically adjacent or
/// functionally close (for affinity grouping).
fn continents_adjacent(a: &str, b: &str) -> bool {
    const ADJACENCY: &[(&str, &str)] = &[
        ("EU", "NA"),
        ("EU", "AF"),
        ("EU", "AS"),
        ("NA", "SA"),
        ("NA", "EU"),
        ("SA", "NA"),
        ("AS", "EU"),
        ("AS", "OC"),
        ("AF", "EU"),
        ("AF", "AS"),
        ("OC", "AS"),
    ];
    ADJACENCY
        .iter()
        .any(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn eu_relay() -> RelayEndpoint {
        RelayEndpoint::new("wss://eu.example.com:9090")
            .with_continent("EU")
            .with_coords(52.52, 13.405) // Berlin
            .with_rtt_ms(20)
    }

    fn us_relay() -> RelayEndpoint {
        RelayEndpoint::new("wss://us.example.com:9090")
            .with_continent("NA")
            .with_coords(37.77, -122.42) // San Francisco
            .with_rtt_ms(150)
    }

    fn ap_relay() -> RelayEndpoint {
        RelayEndpoint::new("wss://ap.example.com:9090")
            .with_continent("AS")
            .with_coords(35.69, 139.69) // Tokyo
            .with_rtt_ms(300)
    }

    fn selector() -> RelaySelector {
        RelaySelector::new(vec![eu_relay(), us_relay(), ap_relay()])
    }

    #[test]
    fn test_round_robin_returns_first() {
        let s = selector();
        assert_eq!(s.round_robin().unwrap().addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_lowest_rtt_selects_eu() {
        let s = selector();
        assert_eq!(s.lowest_rtt().unwrap().addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_nearest_to_london() {
        let s = selector();
        // London (51.51, -0.13) should be nearest to Berlin
        let best = s.nearest_to_coords(51.51, -0.13).unwrap();
        assert_eq!(best.addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_nearest_to_tokyo() {
        let s = selector();
        // Tokyo coords should select the AP relay
        let best = s.nearest_to_coords(35.69, 139.69).unwrap();
        assert_eq!(best.addr, "wss://ap.example.com:9090");
    }

    #[test]
    fn test_latency_weighted_prefers_low_rtt_and_nearby() {
        let s = selector();
        // From Paris (48.86, 2.35) — EU relay has low rtt AND nearby
        let best = s.latency_weighted(48.86, 2.35).unwrap();
        assert_eq!(best.addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_latency_weighted_geo_penalty_on_no_coords() {
        let no_coords = RelayEndpoint::new("wss://unknown.example.com:9090")
            .with_rtt_ms(1); // very low RTT but no coords

        let eu = eu_relay(); // RTT=20 with coords

        let s = RelaySelector::new(vec![no_coords, eu]);
        // From Paris, EU relay with RTT=20 and nearby coords should win over
        // 1ms relay with GEO_PENALTY (10,000 km geo penalty → score ≈ 30).
        let best = s.latency_weighted(48.86, 2.35).unwrap();
        // no_coords score ≈ 0.7*1 + 0.3*(10000/100) = 0.7 + 30 = 30.7
        // eu score ≈ 0.7*20 + 0.3*(~500/100) ≈ 14 + 1.5 = 15.5 → eu wins
        assert_eq!(best.addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_multi_relay_affinity_same_continent_first() {
        let s = selector();
        let ordered = s.multi_relay_affinity("EU");
        // EU relay first, then adjacent (NA, AS are both adjacent to EU), then rest
        assert_eq!(ordered[0].addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_multi_relay_affinity_all_relays_returned() {
        let s = selector();
        let ordered = s.multi_relay_affinity("SA");
        assert_eq!(ordered.len(), 3);
    }

    #[test]
    fn test_best_with_coords() {
        let s = selector();
        let best = s.best(Some(51.51), Some(-0.13)).unwrap();
        assert_eq!(best.addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_best_without_coords_falls_back_to_rtt() {
        let s = selector();
        let best = s.best(None, None).unwrap();
        // lowest_rtt fallback → EU relay (rtt=20)
        assert_eq!(best.addr, "wss://eu.example.com:9090");
    }

    #[test]
    fn test_empty_selector() {
        let s = RelaySelector::new(vec![]);
        assert!(s.round_robin().is_none());
        assert!(s.lowest_rtt().is_none());
        assert!(s.nearest_to_coords(0.0, 0.0).is_none());
        assert!(s.best(None, None).is_none());
        assert_eq!(s.multi_relay_affinity("EU").len(), 0);
    }

    #[test]
    fn test_continents_adjacent() {
        assert!(continents_adjacent("EU", "NA"));
        assert!(continents_adjacent("NA", "EU")); // symmetric
        assert!(continents_adjacent("AS", "OC"));
        assert!(!continents_adjacent("NA", "OC"));
    }

    #[test]
    fn test_haversine_accuracy() {
        // Berlin to Paris: ~878 km
        let dist = haversine_km(52.52, 13.405, 48.86, 2.35);
        assert!((dist - 878.0).abs() < 20.0, "Berlin→Paris distance: {dist}");
    }

    #[test]
    fn test_weight_adjusts_score() {
        let cheap = RelayEndpoint::new("wss://cheap.example.com:9090")
            .with_continent("EU")
            .with_coords(52.52, 13.405) // Berlin
            .with_rtt_ms(20)
            .with_weight(1.0);

        let preferred = RelayEndpoint::new("wss://preferred.example.com:9090")
            .with_continent("EU")
            .with_coords(52.52, 13.405) // same coords
            .with_rtt_ms(20)
            .with_weight(0.5); // halved weight → lower score

        let s = RelaySelector::new(vec![cheap, preferred]);
        let best = s.latency_weighted(52.52, 13.405).unwrap();
        assert_eq!(best.addr, "wss://preferred.example.com:9090");
    }
}
