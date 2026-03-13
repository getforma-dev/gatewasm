//! Impossible Travel Detection — geolocation, IP classification, and travel confidence assessment.
//!
//! This module provides:
//! - `IpType` / `classify_ip()` — categorise an IP as residential, VPN, cloud, etc.
//! - `geolocate_ip()` — stub geolocation (returns lat/lon for a small set of known IPs)
//! - `haversine_km()` — great-circle distance between two lat/lon pairs
//! - `TravelConfidence` / `assess_travel_confidence()` — tiered impossible-travel verdict
//!
//! In production the `classify_ip` and `geolocate_ip` stubs would be replaced by MaxMind
//! ASN + City database lookups injected at startup.

use chrono::{DateTime, Utc};

// ── IP Classification ───────────────────────────────────────────────────

/// Broad classification of an IP address source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpType {
    Residential,
    Corporate,
    Vpn,
    CloudProvider,
    MobileCarrier,
    Unknown,
}

impl IpType {
    /// Returns the string representation stored in the `login_ip_type` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            IpType::Residential => "residential",
            IpType::Corporate => "corporate",
            IpType::Vpn => "vpn",
            IpType::CloudProvider => "cloud",
            IpType::MobileCarrier => "mobile_carrier",
            IpType::Unknown => "unknown",
        }
    }

    /// Parse from a database string value.
    pub fn from_str_value(s: &str) -> Self {
        match s {
            "residential" => IpType::Residential,
            "corporate" => IpType::Corporate,
            "vpn" => IpType::Vpn,
            "cloud" => IpType::CloudProvider,
            "mobile_carrier" => IpType::MobileCarrier,
            _ => IpType::Unknown,
        }
    }
}

/// Classify an IP address by its likely source type.
///
/// **Stub implementation**: returns `Residential` by default.
/// In production this would use a MaxMind ASN database to identify VPN providers,
/// cloud ASNs (AWS, GCP, Azure), mobile carriers, etc.
pub fn classify_ip(_ip: &str) -> IpType {
    // Known cloud provider ranges (very simplified — real implementation uses ASN DB)
    // For now, everything is treated as residential.
    IpType::Residential
}

// ── Geolocation ─────────────────────────────────────────────────────────

/// Attempt to geolocate an IP address, returning `(latitude, longitude)`.
///
/// **Stub implementation**: returns `None` for all IPs.
/// A small lookup table is provided for unit/integration test use.
/// In production this would use a MaxMind City database.
pub fn geolocate_ip(ip: &str) -> Option<(f64, f64)> {
    // Small test-only lookup table
    match ip {
        // New York City
        "203.0.113.1" => Some((40.7128, -74.0060)),
        // London
        "203.0.113.2" => Some((51.5074, -0.1278)),
        // Tokyo
        "203.0.113.3" => Some((35.6762, 139.6503)),
        // Sydney
        "203.0.113.4" => Some((-33.8688, 151.2093)),
        // San Francisco
        "203.0.113.5" => Some((37.7749, -122.4194)),
        // Nearby NYC (for same-city tests)
        "203.0.113.6" => Some((40.7580, -73.9855)),
        _ => None,
    }
}

// ── Haversine Distance ──────────────────────────────────────────────────

/// Calculate the great-circle distance in kilometres between two lat/lon points
/// using the Haversine formula.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a = (d_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_KM * c
}

// ── Travel Confidence ───────────────────────────────────────────────────

/// Confidence level for an impossible-travel detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TravelConfidence {
    /// Both residential IPs, different continents, very short time window.
    Critical,
    /// Suspicious but mitigated (e.g. one IP is a VPN/proxy).
    Elevated,
    /// Low concern — one or both IPs are corporate / cloud / mobile carrier.
    Low,
    /// Same city or insufficient data — no action needed.
    Skip,
}

/// Assess the confidence level of a potential impossible-travel event.
///
/// # Arguments
/// * `lat1`, `lon1`, `time1`, `ip_type1` — previous login location + metadata
/// * `lat2`, `lon2`, `time2`, `ip_type2` — current login location + metadata
/// * `threshold_kmh` — maximum plausible travel speed (default 900 = commercial airline)
/// * `min_distance_km` — distances below this are considered same-city noise (default 100.0)
pub fn assess_travel_confidence(
    lat1: f64,
    lon1: f64,
    time1: DateTime<Utc>,
    ip_type1: &IpType,
    lat2: f64,
    lon2: f64,
    time2: DateTime<Utc>,
    ip_type2: &IpType,
    threshold_kmh: u32,
    min_distance_km: f64,
) -> TravelConfidence {
    let distance_km = haversine_km(lat1, lon1, lat2, lon2);

    // Same-city noise — skip
    if distance_km < min_distance_km {
        return TravelConfidence::Skip;
    }

    // If one or both IPs are cloud / VPN / mobile carrier, reduce confidence
    let is_noisy = |t: &IpType| {
        matches!(
            t,
            IpType::Vpn | IpType::CloudProvider | IpType::MobileCarrier
        )
    };
    if is_noisy(ip_type1) || is_noisy(ip_type2) {
        return TravelConfidence::Low;
    }

    // Calculate elapsed time in hours
    let elapsed_secs = (time2 - time1).num_seconds().abs();
    if elapsed_secs == 0 {
        // Simultaneous logins from different locations → very suspicious
        if distance_km > 5000.0
            && *ip_type1 == IpType::Residential
            && *ip_type2 == IpType::Residential
        {
            return TravelConfidence::Critical;
        }
        return TravelConfidence::Elevated;
    }

    let elapsed_hours = elapsed_secs as f64 / 3600.0;
    let speed_kmh = distance_km / elapsed_hours;

    // Speed within plausible travel range — skip
    if speed_kmh <= threshold_kmh as f64 {
        return TravelConfidence::Skip;
    }

    // Both residential, long distance, very short window → critical
    let elapsed_minutes = elapsed_secs as f64 / 60.0;
    if *ip_type1 == IpType::Residential
        && *ip_type2 == IpType::Residential
        && distance_km > 5000.0
        && elapsed_minutes < 30.0
    {
        return TravelConfidence::Critical;
    }

    // Everything else that exceeds the speed threshold → elevated
    TravelConfidence::Elevated
}

// ── Unit Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_haversine_known_distance() {
        // NYC (40.7128, -74.0060) → London (51.5074, -0.1278) ≈ 5570 km
        let dist = haversine_km(40.7128, -74.0060, 51.5074, -0.1278);
        assert!(
            (dist - 5570.0).abs() < 50.0,
            "NYC→London should be ~5570 km, got {}",
            dist
        );
    }

    #[test]
    fn test_haversine_same_point() {
        let dist = haversine_km(40.7128, -74.0060, 40.7128, -74.0060);
        assert!(
            dist.abs() < 0.001,
            "Same point should be 0 km, got {}",
            dist
        );
    }

    #[test]
    fn test_assess_travel_critical() {
        // Both residential, 10,000 km apart, 15 minutes → Critical
        let now = Utc::now();
        let prev = now - Duration::minutes(15);
        let confidence = assess_travel_confidence(
            40.7128, -74.0060, prev, &IpType::Residential, // NYC
            51.5074, -0.1278, now, &IpType::Residential, // London
            900,
            100.0,
        );
        assert_eq!(confidence, TravelConfidence::Critical);
    }

    #[test]
    fn test_assess_travel_skip_same_city() {
        // 50 km apart → Skip (below min_distance_km of 100)
        let now = Utc::now();
        let prev = now - Duration::minutes(5);
        // NYC → nearby NYC (about 5 km apart)
        let confidence = assess_travel_confidence(
            40.7128, -74.0060, prev, &IpType::Residential,
            40.7580, -73.9855, now, &IpType::Residential,
            900,
            100.0,
        );
        assert_eq!(confidence, TravelConfidence::Skip);
    }

    #[test]
    fn test_assess_travel_low_vpn() {
        // One VPN → Low
        let now = Utc::now();
        let prev = now - Duration::minutes(10);
        let confidence = assess_travel_confidence(
            40.7128, -74.0060, prev, &IpType::Residential, // NYC
            51.5074, -0.1278, now, &IpType::Vpn,           // London (VPN)
            900,
            100.0,
        );
        assert_eq!(confidence, TravelConfidence::Low);
    }

    #[test]
    fn test_assess_travel_elevated() {
        // Both residential, exceeds speed but not critical thresholds
        // e.g. 2000 km in 1 hour = 2000 km/h (exceeds 900 but distance < 5000)
        let now = Utc::now();
        let prev = now - Duration::hours(1);
        // NYC → roughly Chicago area (~1200 km)
        let confidence = assess_travel_confidence(
            40.7128, -74.0060, prev, &IpType::Residential,
            41.8781, -87.6298, now, &IpType::Residential,
            900,
            100.0,
        );
        assert_eq!(confidence, TravelConfidence::Elevated);
    }

    #[test]
    fn test_assess_travel_skip_slow_enough() {
        // Both residential, long distance, but enough time → Skip
        let now = Utc::now();
        let prev = now - Duration::hours(24);
        let confidence = assess_travel_confidence(
            40.7128, -74.0060, prev, &IpType::Residential, // NYC
            51.5074, -0.1278, now, &IpType::Residential,   // London
            900,
            100.0,
        );
        // 5570 km / 24 hours ≈ 232 km/h → below 900 threshold → Skip
        assert_eq!(confidence, TravelConfidence::Skip);
    }

    #[test]
    fn test_ip_type_roundtrip() {
        for ip_type in &[
            IpType::Residential,
            IpType::Corporate,
            IpType::Vpn,
            IpType::CloudProvider,
            IpType::MobileCarrier,
            IpType::Unknown,
        ] {
            let s = ip_type.as_str();
            let parsed = IpType::from_str_value(s);
            assert_eq!(&parsed, ip_type);
        }
    }

    #[test]
    fn test_geolocate_ip_known() {
        // NYC test IP
        let loc = geolocate_ip("203.0.113.1");
        assert!(loc.is_some());
        let (lat, lon) = loc.unwrap();
        assert!((lat - 40.7128).abs() < 0.01);
        assert!((lon - (-74.0060)).abs() < 0.01);
    }

    #[test]
    fn test_geolocate_ip_unknown() {
        assert!(geolocate_ip("192.168.1.1").is_none());
    }

    #[test]
    fn test_classify_ip_default() {
        // Stub always returns Residential
        assert_eq!(classify_ip("1.2.3.4"), IpType::Residential);
    }
}
