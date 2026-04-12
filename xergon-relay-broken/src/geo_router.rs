//! Geo-Routing module.
//!
//! Estimates network latency between regions, tracks provider locations,
//! and provides geo-proximity-based provider sorting.

use dashmap::DashMap;
use serde::Serialize;
use tracing::debug;

// ---------------------------------------------------------------------------
// Provider Region
// ---------------------------------------------------------------------------

/// Geographic location of a provider.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderRegion {
    pub provider_pk: String,
    pub region: String,
    pub latitude: f64,
    pub longitude: f64,
    pub city: String,
    pub country: String,
}

impl ProviderRegion {
    /// Compute the Haversine distance in km between this and another location.
    pub fn distance_km_to(&self, other: &ProviderRegion) -> f64 {
        haversine_km(self.latitude, self.longitude, other.latitude, other.longitude)
    }
}

// ---------------------------------------------------------------------------
// Geo Latency (internal, mutable)
// ---------------------------------------------------------------------------

/// Internal latency data between two regions.
struct GeoLatencyEntry {
    /// Estimated latency (ms) based on geographic distance.
    estimated_ms: u64,
    /// Actual measured p50 latency (ms), 0 if no measurements.
    actual_p50_ms: u64,
    /// Number of actual latency samples.
    sample_count: u32,
}

/// Read-only latency info for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct GeoLatency {
    pub estimated_ms: u64,
    pub actual_p50_ms: u64,
    pub sample_count: u32,
}

// ---------------------------------------------------------------------------
// GeoRouter
// ---------------------------------------------------------------------------

/// Geo-routing engine for provider selection based on geographic proximity.
///
/// Maintains a registry of provider locations and a latency matrix between
/// regions. Uses Haversine distance for initial estimates and refines with
/// actual measurements over time.
pub struct GeoRouter {
    /// Provider public key -> region info
    provider_regions: DashMap<String, ProviderRegion>,
    /// (region1, region2) -> latency data (order-independent)
    latency_matrix: DashMap<(String, String), GeoLatencyEntry>,
    /// Default same-region latency estimate (ms)
    default_same_region_ms: u64,
    /// Default cross-region latency per 1000km (ms)
    default_cross_region_per_1000km_ms: u64,
}

impl GeoRouter {
    /// Create a new GeoRouter with default latency estimates.
    pub fn new() -> Self {
        Self {
            provider_regions: DashMap::new(),
            latency_matrix: DashMap::new(),
            default_same_region_ms: 20, // 20ms within same region
            default_cross_region_per_1000km_ms: 30, // 30ms per 1000km
        }
    }

    /// Register a provider's geographic location.
    pub fn register_provider_location(&self, provider: ProviderRegion) {
        let pk = provider.provider_pk.clone();
        let region = provider.region.clone();
        self.provider_regions
            .insert(provider.provider_pk.clone(), provider);
        debug!(provider = %pk, region = %region, "Registered provider location");
    }

    /// Estimate latency between two regions.
    ///
    /// Uses actual measurements if available (p50), otherwise falls back to
    /// Haversine distance-based estimation.
    pub fn estimate_latency(&self, from_region: &str, to_region: &str) -> u64 {
        if from_region == to_region {
            return self.default_same_region_ms;
        }

        let key = self.region_key(from_region, to_region);

        // Check for actual measurements
        if let Some(latency) = self.latency_matrix.get(&key) {
            if latency.actual_p50_ms > 0 {
                return latency.actual_p50_ms;
            }
            return latency.estimated_ms;
        }

        // Estimate from geographic coordinates
        self.estimate_from_coords(from_region, to_region)
    }

    /// Get providers sorted by geo-proximity to a given region.
    ///
    /// Providers in the same region are listed first, then sorted by
    /// estimated latency.
    pub fn get_nearby_providers(
        &self,
        region: &str,
        max_distance_km: Option<f64>,
    ) -> Vec<ProviderRegion> {
        let mut providers: Vec<_> = self
            .provider_regions
            .iter()
            .map(|r| r.value().clone())
            .collect();

        // Filter by max distance if specified
        if let Some(max_dist) = max_distance_km {
            let region_ref = self.get_region_center(region);
            providers.retain(|p| {
                if let Some(ref center) = region_ref {
                    haversine_km(center.0, center.1, p.latitude, p.longitude) <= max_dist
                } else {
                    true // no reference, can't filter
                }
            });
        }

        // Sort by estimated latency from the given region
        providers.sort_by(|a, b| {
            let lat_a = self.estimate_latency(region, &a.region);
            let lat_b = self.estimate_latency(region, &b.region);
            lat_a.cmp(&lat_b)
        });

        providers
    }

    /// Update actual latency measurement between two regions.
    ///
    /// Uses an exponential moving average for the p50 estimate.
    pub fn update_latency(&self, from_region: &str, to_region: &str, actual_ms: u64) {
        let key = self.region_key(from_region, to_region);

        let mut entry = self
            .latency_matrix
            .entry(key)
            .or_insert_with(|| GeoLatencyEntry {
                estimated_ms: self.estimate_from_coords(from_region, to_region),
                actual_p50_ms: 0,
                sample_count: 0,
            });

        entry.sample_count += 1;
        let count = entry.sample_count;

        // Exponential moving average
        let alpha = 2.0 / (count as f64 + 1.0);
        let current = entry.actual_p50_ms;
        entry.actual_p50_ms = if current == 0 {
            actual_ms
        } else {
            let updated = current as f64 * (1.0 - alpha) + actual_ms as f64 * alpha;
            updated.round() as u64
        };
    }

    /// Get the region for a specific provider.
    pub fn get_provider_region(&self, provider_pk: &str) -> Option<String> {
        self.provider_regions
            .get(provider_pk)
            .map(|r| r.value().region.clone())
    }

    /// Get all provider regions.
    pub fn get_all_provider_regions(&self) -> Vec<ProviderRegion> {
        self.provider_regions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get the full latency matrix for display.
    pub fn get_latency_matrix(&self) -> Vec<(String, String, GeoLatency)> {
        self.latency_matrix
            .iter()
            .map(|r| {
                let ((r1, r2), entry) = (r.key().clone(), r.value());
                (
                    r1,
                    r2,
                    GeoLatency {
                        estimated_ms: entry.estimated_ms,
                        actual_p50_ms: entry.actual_p50_ms,
                        sample_count: entry.sample_count,
                    },
                )
            })
            .collect()
    }

    /// Check if geo-routing is available (at least one provider has a region).
    pub fn is_available(&self) -> bool {
        !self.provider_regions.is_empty()
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn region_key(&self, r1: &str, r2: &str) -> (String, String) {
        // Canonical ordering: alphabetically smaller first
        if r1 < r2 {
            (r1.to_string(), r2.to_string())
        } else {
            (r2.to_string(), r1.to_string())
        }
    }

    fn estimate_from_coords(&self, from_region: &str, to_region: &str) -> u64 {
        let from_center = self.get_region_center(from_region);
        let to_center = self.get_region_center(to_region);

        match (from_center, to_center) {
            (Some((lat1, lon1)), Some((lat2, lon2))) => {
                let dist_km = haversine_km(lat1, lon1, lat2, lon2);
                let base_latency = (dist_km / 1000.0 * self.default_cross_region_per_1000km_ms as f64) as u64;
                base_latency.max(self.default_same_region_ms)
            }
            _ => {
                // No coordinate data — use a conservative default
                150 // 150ms default for unknown regions
            }
        }
    }

    fn get_region_center(&self, region: &str) -> Option<(f64, f64)> {
        let mut lat_sum = 0.0;
        let mut lon_sum = 0.0;
        let mut count = 0;

        for entry in self.provider_regions.iter() {
            if entry.value().region == region {
                lat_sum += entry.value().latitude;
                lon_sum += entry.value().longitude;
                count += 1;
            }
        }

        if count == 0 {
            None
        } else {
            Some((lat_sum / count as f64, lon_sum / count as f64))
        }
    }
}

impl Default for GeoRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Haversine Distance
// ---------------------------------------------------------------------------

/// Compute the Haversine distance between two points in kilometers.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let a = (dlat / 2.0).sin() * (dlat / 2.0).sin()
        + lat1.to_radians().cos()
            * lat2.to_radians().cos()
            * (dlon / 2.0).sin()
            * (dlon / 2.0).sin();

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nyc() -> ProviderRegion {
        ProviderRegion {
            provider_pk: "nyc-provider".into(),
            region: "us-east".into(),
            latitude: 40.7128,
            longitude: -74.0060,
            city: "New York".into(),
            country: "US".into(),
        }
    }

    fn make_london() -> ProviderRegion {
        ProviderRegion {
            provider_pk: "london-provider".into(),
            region: "eu-west".into(),
            latitude: 51.5074,
            longitude: -0.1278,
            city: "London".into(),
            country: "UK".into(),
        }
    }

    fn make_tokyo() -> ProviderRegion {
        ProviderRegion {
            provider_pk: "tokyo-provider".into(),
            region: "ap-northeast".into(),
            latitude: 35.6762,
            longitude: 139.6503,
            city: "Tokyo".into(),
            country: "JP".into(),
        }
    }

    #[test]
    fn test_geo_routing_latency_estimation() {
        let router = GeoRouter::new();
        router.register_provider_location(make_nyc());
        router.register_provider_location(make_london());

        // Same region
        let same = router.estimate_latency("us-east", "us-east");
        assert_eq!(same, 20); // default same region

        // Different regions — should estimate from distance
        let diff = router.estimate_latency("us-east", "eu-west");
        assert!(diff > 20, "cross-region should be higher: {}", diff);
        assert!(diff < 500, "should be reasonable: {}", diff);
    }

    #[test]
    fn test_geo_routing_nearby_providers() {
        let router = GeoRouter::new();
        router.register_provider_location(make_nyc());
        router.register_provider_location(make_london());
        router.register_provider_location(make_tokyo());

        let nearby = router.get_nearby_providers("us-east", None);
        assert_eq!(nearby.len(), 3);
        // NYC should be first (same region)
        assert_eq!(nearby[0].provider_pk, "nyc-provider");
    }

    #[test]
    fn test_geo_routing_nearby_with_max_distance() {
        let router = GeoRouter::new();
        router.register_provider_location(make_nyc());
        router.register_provider_location(make_london());
        router.register_provider_location(make_tokyo());

        // NYC to London is ~5570km, NYC to Tokyo is ~10850km
        let nearby = router.get_nearby_providers("us-east", Some(6000.0));
        // Should include NYC and London but not Tokyo
        assert!(nearby.len() <= 2, "max_distance should filter Tokyo");
        assert!(nearby.iter().any(|p| p.provider_pk == "nyc-provider"));
    }

    #[test]
    fn test_geo_routing_actual_latency_update() {
        let router = GeoRouter::new();
        router.register_provider_location(make_nyc());
        router.register_provider_location(make_london());

        // Before any updates, use estimated
        let before = router.estimate_latency("us-east", "eu-west");
        assert!(before > 0);

        // Update with actual measurements
        router.update_latency("us-east", "eu-west", 80);
        router.update_latency("us-east", "eu-west", 90);
        router.update_latency("us-east", "eu-west", 85);

        let after = router.estimate_latency("us-east", "eu-west");
        // Should use actual p50 now
        assert!(after > 0);
        // The matrix should have an entry
        let matrix = router.get_latency_matrix();
        assert!(!matrix.is_empty());
    }

    #[test]
    fn test_haversine_distance() {
        // NYC to London is approximately 5570 km
        let dist = haversine_km(40.7128, -74.0060, 51.5074, -0.1278);
        assert!((dist - 5570.0).abs() < 200.0, "NYC-London distance should be ~5570km, got {}", dist);
    }

    #[test]
    fn test_geo_router_not_available_when_empty() {
        let router = GeoRouter::new();
        assert!(!router.is_available());
    }

    #[test]
    fn test_geo_router_available_with_providers() {
        let router = GeoRouter::new();
        router.register_provider_location(make_nyc());
        assert!(router.is_available());
    }
}
