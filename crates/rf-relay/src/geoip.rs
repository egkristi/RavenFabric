//! GeoIP database integration for relay region selection.
//!
//! Requires the `geoip` feature and a MaxMind GeoLite2 or GeoIP2 database file
//! (e.g. `GeoLite2-City.mmdb`). The database is loaded once at startup and kept
//! in memory for low-latency lookups.
//!
//! # Usage
//! ```no_run
//! # use rf_relay::geoip::{GeoIpDb, Region};
//! # use std::net::IpAddr;
//! # use std::path::Path;
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = GeoIpDb::open(Path::new("/etc/ravenfabric/GeoLite2-City.mmdb"))?;
//! let ip: IpAddr = "1.2.3.4".parse()?;
//! if let Some(region) = db.lookup(ip) {
//!     println!("Visitor from {}, {}", region.city, region.country_code);
//! }
//! # Ok(())
//! # }
//! ```

use std::net::IpAddr;
use std::path::Path;

use thiserror::Error;

/// Error type for GeoIP operations.
#[derive(Error, Debug)]
pub enum GeoIpError {
    #[error("failed to open GeoIP database: {0}")]
    Open(String),

    #[error("lookup failed: {0}")]
    Lookup(String),
}

/// Geographic region information for an IP address.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// ISO 3166-1 alpha-2 country code (e.g. `"US"`, `"DE"`, `"JP"`).
    pub country_code: String,
    /// Human-readable country name (e.g. `"United States"`).
    pub country_name: String,
    /// ISO 3166-2 region / state code (e.g. `"CA"`, `"NY"`). May be empty.
    pub subdivision_code: String,
    /// City name if available. May be empty.
    pub city: String,
    /// Latitude in decimal degrees (-90.0..=90.0).
    pub latitude: f64,
    /// Longitude in decimal degrees (-180.0..=180.0).
    pub longitude: f64,
    /// Continent code (e.g. `"NA"`, `"EU"`, `"AS"`).
    pub continent: String,
}

impl Region {
    /// Returns `true` if both latitude and longitude are non-zero (i.e. the
    /// record has usable coordinates for distance calculations).
    pub fn has_coords(&self) -> bool {
        self.latitude != 0.0 || self.longitude != 0.0
    }
}

/// Loaded MaxMind GeoIP2/GeoLite2 database.
pub struct GeoIpDb {
    #[cfg(feature = "geoip")]
    inner: maxminddb::Reader<Vec<u8>>,
}

impl GeoIpDb {
    /// Open a MaxMind `.mmdb` database file from disk.
    ///
    /// The entire database is loaded into memory so subsequent lookups are fast.
    pub fn open(path: &Path) -> Result<Self, GeoIpError> {
        #[cfg(feature = "geoip")]
        {
            let reader = maxminddb::Reader::open_readfile(path)
                .map_err(|e| GeoIpError::Open(e.to_string()))?;
            Ok(Self { inner: reader })
        }
        #[cfg(not(feature = "geoip"))]
        {
            let _ = path;
            Err(GeoIpError::Open(
                "rf-relay compiled without the 'geoip' feature".into(),
            ))
        }
    }

    /// Look up the geographic region for `ip`.
    ///
    /// Returns `None` if the IP is not found in the database (e.g. private
    /// ranges, localhost) or if the record lacks location data.
    pub fn lookup(&self, ip: IpAddr) -> Option<Region> {
        #[cfg(feature = "geoip")]
        {
            let record: maxminddb::geoip2::City = self.inner.lookup(ip).ok()?;

            let country_code = record
                .country
                .as_ref()
                .and_then(|c| c.iso_code)
                .unwrap_or("")
                .to_owned();

            let country_name = record
                .country
                .as_ref()
                .and_then(|c| c.names.as_ref())
                .and_then(|n| n.get("en").copied())
                .unwrap_or("")
                .to_owned();

            let subdivision_code = record
                .subdivisions
                .as_ref()
                .and_then(|s| s.first())
                .and_then(|s| s.iso_code)
                .unwrap_or("")
                .to_owned();

            let city = record
                .city
                .as_ref()
                .and_then(|c| c.names.as_ref())
                .and_then(|n| n.get("en").copied())
                .unwrap_or("")
                .to_owned();

            let (latitude, longitude) = record
                .location
                .as_ref()
                .map(|l| {
                    (
                        l.latitude.unwrap_or(0.0),
                        l.longitude.unwrap_or(0.0),
                    )
                })
                .unwrap_or((0.0, 0.0));

            let continent = record
                .continent
                .as_ref()
                .and_then(|c| c.code)
                .unwrap_or("")
                .to_owned();

            // Skip records with no useful geographic data.
            if country_code.is_empty() && latitude == 0.0 && longitude == 0.0 {
                return None;
            }

            Some(Region {
                country_code,
                country_name,
                subdivision_code,
                city,
                latitude,
                longitude,
                continent,
            })
        }
        #[cfg(not(feature = "geoip"))]
        {
            let _ = ip;
            None
        }
    }

    /// Haversine great-circle distance between two regions in kilometres.
    ///
    /// Returns `f64::MAX` if either region lacks valid coordinates.
    pub fn distance_km(a: &Region, b: &Region) -> f64 {
        if !a.has_coords() || !b.has_coords() {
            return f64::MAX;
        }
        haversine_km(a.latitude, a.longitude, b.latitude, b.longitude)
    }
}

/// Haversine formula for great-circle distance.
///
/// Returns kilometres between the two (lat, lon) pairs.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6_371.0;

    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();

    let a = (dphi / 2.0).sin().powi(2)
        + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_london_newyork() {
        // London (51.5074 N, 0.1278 W) to New York (40.7128 N, 74.0060 W)
        // Expected: approximately 5,570 km
        let dist = haversine_km(51.5074, -0.1278, 40.7128, -74.0060);
        assert!((dist - 5570.0).abs() < 100.0, "unexpected distance: {dist}");
    }

    #[test]
    fn test_haversine_same_point() {
        let dist = haversine_km(48.8566, 2.3522, 48.8566, 2.3522);
        assert!(dist < 0.001, "same point should have ~0 distance, got {dist}");
    }

    #[test]
    fn test_haversine_antipodal() {
        // Antipodal points should be ~20,015 km (half Earth circumference)
        let dist = haversine_km(0.0, 0.0, 0.0, 180.0);
        assert!((dist - 20_015.0).abs() < 10.0, "antipodal distance: {dist}");
    }

    #[test]
    fn test_region_has_coords() {
        let r = Region {
            country_code: "US".into(),
            country_name: "United States".into(),
            subdivision_code: "CA".into(),
            city: "San Francisco".into(),
            latitude: 37.77,
            longitude: -122.42,
            continent: "NA".into(),
        };
        assert!(r.has_coords());
    }

    #[test]
    fn test_region_no_coords() {
        let r = Region {
            country_code: "XX".into(),
            country_name: "Unknown".into(),
            subdivision_code: "".into(),
            city: "".into(),
            latitude: 0.0,
            longitude: 0.0,
            continent: "".into(),
        };
        assert!(!r.has_coords());
    }

    #[test]
    fn test_distance_no_coords_returns_max() {
        let a = Region {
            country_code: "US".into(),
            country_name: "".into(),
            subdivision_code: "".into(),
            city: "".into(),
            latitude: 0.0,
            longitude: 0.0,
            continent: "NA".into(),
        };
        let b = a.clone();
        assert_eq!(GeoIpDb::distance_km(&a, &b), f64::MAX);
    }

    #[test]
    fn test_open_nonexistent_db() {
        let result = GeoIpDb::open(Path::new("/nonexistent/GeoLite2-City.mmdb"));
        assert!(result.is_err());
    }
}
