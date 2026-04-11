//! GPU Bazar endpoints — browse, rent, and manage GPU listings
//!
//! Endpoints:
//!   GET  /v1/gpu/listings           — Browse available GPU listings
//!   GET  /v1/gpu/listings/:id       — Get specific listing details
//!   POST /v1/gpu/rent               — Proxy rental request to agent
//!   GET  /v1/gpu/rentals/:renter_pk — Get active rentals for a renter
//!   GET  /v1/gpu/pricing            — Get average/market pricing for GPU types

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ── Request/Response types ────────────────────────────────────────────

/// Query parameters for GET /v1/gpu/listings
#[derive(Debug, Deserialize, Default)]
pub struct ListGpuListingsQuery {
    pub region: Option<String>,
    pub min_vram: Option<u32>,
    pub max_price_per_hour: Option<u64>,
    pub gpu_type: Option<String>,
}

/// A single GPU listing returned by the listing endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct GpuListingResponse {
    pub box_id: String,
    pub listing_id: String,
    pub provider_pk: String,
    pub gpu_type: String,
    pub gpu_specs_json: String,
    pub price_per_hour_nanoerg: u64,
    pub price_per_hour_erg: f64,
    pub region: String,
}

/// Response for GET /v1/gpu/listings
#[derive(Debug, Serialize)]
pub struct GpuListingsResponse {
    pub listings: Vec<GpuListingResponse>,
    pub total: usize,
}

/// Response for GET /v1/gpu/listings/:id
#[derive(Debug, Serialize)]
pub struct GpuListingDetailResponse {
    pub box_id: String,
    pub listing_id: String,
    pub provider_pk: String,
    pub gpu_type: String,
    pub gpu_specs_json: String,
    pub price_per_hour_nanoerg: u64,
    pub price_per_hour_erg: f64,
    pub region: String,
    pub value_nanoerg: u64,
}

/// Request body for POST /v1/gpu/rent
#[derive(Debug, Deserialize, Serialize)]
pub struct RentGpuRequest {
    pub listing_id: String,
    pub hours: u32,
    pub renter_public_key: String,
}

/// Response for POST /v1/gpu/rent
#[derive(Debug, Serialize, Deserialize)]
pub struct RentGpuResponse {
    pub success: bool,
    pub rental_tx_id: Option<String>,
    pub deadline_height: Option<u64>,
    pub rental_box_id: Option<String>,
    pub error: Option<String>,
}

/// A single GPU rental returned by the rentals endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct GpuRentalResponse {
    pub box_id: String,
    pub rental_id: String,
    pub renter_pk: String,
    pub listing_box_id: String,
    pub provider_pk: String,
    pub deadline_height: u64,
    pub hours_rented: u64,
}

/// Response for GET /v1/gpu/rentals/:renter_pk
#[derive(Debug, Serialize)]
pub struct GpuRentalsResponse {
    pub rentals: Vec<GpuRentalResponse>,
    pub total: usize,
}

/// Pricing info for a single GPU type.
#[derive(Debug, Clone, Serialize)]
pub struct GpuPricingEntry {
    pub gpu_type: String,
    pub avg_price_per_hour_nanoerg: u64,
    pub avg_price_per_hour_erg: f64,
    pub min_price_per_hour_nanoerg: u64,
    pub max_price_per_hour_nanoerg: u64,
    pub listing_count: usize,
}

/// Response for GET /v1/gpu/pricing
#[derive(Debug, Serialize)]
pub struct GpuPricingResponse {
    pub pricing: Vec<GpuPricingEntry>,
}

/// Request body for POST /v1/gpu/rate
#[derive(Debug, Deserialize, Serialize)]
pub struct RateGpuRequest {
    pub rental_id: String,
    pub rated_public_key: String,
    pub rating: u8,
    pub role: String,
    pub comment: Option<String>,
}

/// Response for POST /v1/gpu/rate
#[derive(Debug, Serialize, Deserialize)]
pub struct RateGpuResponse {
    pub success: bool,
    pub tx_id: Option<String>,
    pub error: Option<String>,
}

/// Star breakdown for reputation.
#[derive(Debug, Clone, Serialize)]
pub struct StarBreakdown {
    pub one_star: usize,
    pub two_star: usize,
    pub three_star: usize,
    pub four_star: usize,
    pub five_star: usize,
}

/// Response for GET /v1/gpu/reputation/:public_key
#[derive(Debug, Serialize)]
pub struct GpuReputationResponse {
    pub public_key: String,
    pub total_ratings: usize,
    pub average_rating: f64,
    pub stars: StarBreakdown,
    pub provider_reputation: Option<f64>,
    pub provider_rating_count: usize,
    pub renter_reputation: Option<f64>,
    pub renter_rating_count: usize,
}

/// Generic error response matching the standard relay error format.
#[derive(Debug, Serialize)]
pub struct GpuErrorResponse {
    pub error: GpuErrorDetail,
}

/// Standard error detail object.
#[derive(Debug, Serialize)]
pub struct GpuErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    pub code: u16,
}

impl GpuErrorResponse {
    #[allow(dead_code)]
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            error: GpuErrorDetail {
                error_type: "not_found".to_string(),
                message: message.into(),
                code: 404,
            },
        }
    }

    #[allow(dead_code)]
    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            error: GpuErrorDetail {
                error_type: "service_unavailable".to_string(),
                message: message.into(),
                code: 503,
            },
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            error: GpuErrorDetail {
                error_type: "bad_gateway".to_string(),
                message: message.into(),
                code: 502,
            },
        }
    }
}

// ── Handler implementations ───────────────────────────────────────────

/// GET /v1/gpu/listings — Browse available GPU listings
pub async fn list_gpu_listings_handler(
    State(state): State<AppState>,
    Query(params): Query<ListGpuListingsQuery>,
) -> impl IntoResponse {
    info!("Listing GPU listings (chain cache)");

    let listings = match &state.chain_cache {
        Some(cache) => {
            if let Some(listings) = cache.get_gpu_listings() {
                listings
            } else if cache.is_populated() {
                debug!("GPU listing cache is stale, returning stale data");
                cache.get_gpu_listings_or_empty()
            } else {
                warn!("GPU listing cache never populated, triggering lazy scan");
                trigger_lazy_gpu_scan(&state);
                cache.get_gpu_listings_or_empty()
            }
        }
        None => {
            debug!("Chain scanning disabled, no GPU listings");
            Vec::new()
        }
    };

    let mut response_listings: Vec<GpuListingResponse> = listings
        .into_iter()
        .map(|l| GpuListingResponse {
            box_id: l.box_id,
            listing_id: l.listing_id,
            provider_pk: l.provider_pk,
            gpu_type: l.gpu_type,
            gpu_specs_json: l.gpu_specs_json,
            price_per_hour_nanoerg: l.price_per_hour_nanoerg,
            price_per_hour_erg: nanoerg_to_erg(l.price_per_hour_nanoerg),
            region: l.region,
        })
        .collect();

    // Apply filters
    if let Some(ref region) = params.region {
        let region_lower = region.to_lowercase();
        response_listings.retain(|l| l.region.to_lowercase().contains(&region_lower));
    }
    if let Some(min_vram) = params.min_vram {
        response_listings.retain(|l| {
            // Try to parse vram from the gpu_specs_json
            extract_vram_from_specs(&l.gpu_specs_json)
                .map(|vram| vram >= min_vram)
                .unwrap_or(false)
        });
    }
    if let Some(max_price) = params.max_price_per_hour {
        response_listings.retain(|l| l.price_per_hour_nanoerg <= max_price);
    }
    if let Some(ref gpu_type) = params.gpu_type {
        let gpu_type_lower = gpu_type.to_lowercase();
        response_listings.retain(|l| l.gpu_type.to_lowercase().contains(&gpu_type_lower));
    }

    let total = response_listings.len();
    Json(GpuListingsResponse {
        listings: response_listings,
        total,
    })
}

/// GET /v1/gpu/listings/:id — Get specific listing details
pub async fn get_gpu_listing_handler(
    State(state): State<AppState>,
    Path(listing_id): Path<String>,
) -> Response {
    info!(listing_id = %listing_id, "Getting GPU listing details");

    let listings = match &state.chain_cache {
        Some(cache) => cache.get_gpu_listings_or_empty(),
        None => Vec::new(),
    };

    match listings.into_iter().find(|l| l.listing_id == listing_id) {
        Some(listing) => {
            let body = GpuListingDetailResponse {
                box_id: listing.box_id,
                listing_id: listing.listing_id,
                provider_pk: listing.provider_pk,
                gpu_type: listing.gpu_type,
                gpu_specs_json: listing.gpu_specs_json,
                price_per_hour_nanoerg: listing.price_per_hour_nanoerg,
                price_per_hour_erg: nanoerg_to_erg(listing.price_per_hour_nanoerg),
                region: listing.region,
                value_nanoerg: listing.value_nanoerg,
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(GpuErrorResponse::not_found(
                format!("GPU listing '{}' not found", listing_id),
            )),
        )
            .into_response(),
    }
}

/// POST /v1/gpu/rent — Proxy rental request to agent
///
/// Forwards to agent's /api/gpu/rent endpoint.
/// Returns: { rental_tx_id, deadline_height, rental_box_id }
pub async fn rent_gpu_handler(
    State(state): State<AppState>,
    Json(body): Json<RentGpuRequest>,
) -> impl IntoResponse {
    info!(
        listing_id = %body.listing_id,
        hours = body.hours,
        "Proxying GPU rental request to agent"
    );

    let agent_url = state.config.chain.agent_gpu_endpoint.trim();
    if agent_url.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RentGpuResponse {
                success: false,
                rental_tx_id: None,
                deadline_height: None,
                rental_box_id: None,
                error: Some("GPU rental agent endpoint not configured".into()),
            }),
        );
    }

    let url = format!("{}/api/gpu/rent", agent_url.trim_end_matches('/'));

    match state.http_client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<RentGpuResponse>().await {
                Ok(rental_resp) => (StatusCode::OK, Json(rental_resp)),
                Err(e) => {
                    warn!(error = %e, "Failed to parse agent rental response");
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(RentGpuResponse {
                            success: false,
                            rental_tx_id: None,
                            deadline_height: None,
                            rental_box_id: None,
                            error: Some(format!("Failed to parse agent response: {}", e)),
                        }),
                    )
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(status = %status, "Agent returned error for GPU rental");
            (
                StatusCode::BAD_GATEWAY,
                Json(RentGpuResponse {
                    success: false,
                    rental_tx_id: None,
                    deadline_height: None,
                    rental_box_id: None,
                    error: Some(format!("Agent returned status: {}", status)),
                }),
            )
        }
        Err(e) => {
            warn!(error = %e, "Failed to reach agent for GPU rental");
            (
                StatusCode::BAD_GATEWAY,
                Json(RentGpuResponse {
                    success: false,
                    rental_tx_id: None,
                    deadline_height: None,
                    rental_box_id: None,
                    error: Some(format!("Failed to reach agent: {}", e)),
                }),
            )
        }
    }
}

/// GET /v1/gpu/rentals/:renter_pk — Get active rentals for a renter
///
/// Scans chain for rental boxes where R5 matches renter_pk.
pub async fn get_gpu_rentals_handler(
    State(state): State<AppState>,
    Path(renter_pk): Path<String>,
) -> impl IntoResponse {
    info!(renter_pk = %renter_pk, "Getting GPU rentals for renter");

    let rentals = match &state.chain_cache {
        Some(cache) => cache.get_gpu_rentals_or_empty(),
        None => Vec::new(),
    };

    let renter_rentals: Vec<GpuRentalResponse> = rentals
        .into_iter()
        .filter(|r| r.renter_pk == renter_pk)
        .map(|r| GpuRentalResponse {
            box_id: r.box_id,
            rental_id: r.rental_id,
            renter_pk: r.renter_pk,
            listing_box_id: r.listing_box_id,
            provider_pk: r.provider_pk,
            deadline_height: r.deadline_height,
            hours_rented: r.hours_rented,
        })
        .collect();

    let total = renter_rentals.len();
    Json(GpuRentalsResponse {
        rentals: renter_rentals,
        total,
    })
}

/// GET /v1/gpu/pricing — Get average/market pricing for GPU types
///
/// Aggregated from listing boxes: avg/min/max price per GPU type.
pub async fn get_gpu_pricing_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("Getting GPU pricing aggregation");

    let listings = match &state.chain_cache {
        Some(cache) => cache.get_gpu_listings_or_empty(),
        None => Vec::new(),
    };

    // Group by GPU type
    let mut by_type: std::collections::HashMap<String, Vec<u64>> =
        std::collections::HashMap::new();

    for listing in &listings {
        by_type
            .entry(listing.gpu_type.clone())
            .or_default()
            .push(listing.price_per_hour_nanoerg);
    }

    let mut pricing: Vec<GpuPricingEntry> = by_type
        .into_iter()
        .map(|(gpu_type, prices)| {
            let count = prices.len();
            let sum: u64 = prices.iter().sum();
            let avg = sum / count as u64;
            let min = *prices.iter().min().unwrap_or(&0);
            let max = *prices.iter().max().unwrap_or(&0);

            GpuPricingEntry {
                gpu_type,
                avg_price_per_hour_nanoerg: avg,
                avg_price_per_hour_erg: nanoerg_to_erg(avg),
                min_price_per_hour_nanoerg: min,
                max_price_per_hour_nanoerg: max,
                listing_count: count,
            }
        })
        .collect();

    // Sort by listing count descending (most popular first)
    pricing.sort_by(|a, b| b.listing_count.cmp(&a.listing_count));

    Json(GpuPricingResponse { pricing })
}

/// POST /v1/gpu/rate — Proxy rating submission to agent
///
/// Forwards to agent's /api/gpu/rate endpoint.
/// Returns: { success, tx_id }
pub async fn rate_gpu_handler(
    State(state): State<AppState>,
    Json(body): Json<RateGpuRequest>,
) -> impl IntoResponse {
    info!(
        rental_id = %body.rental_id,
        rated_pk = %body.rated_public_key,
        rating = body.rating,
        role = %body.role,
        "Proxying GPU rating request to agent"
    );

    let agent_url = state.config.chain.agent_gpu_endpoint.trim();
    if agent_url.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RateGpuResponse {
                success: false,
                tx_id: None,
                error: Some("GPU rental agent endpoint not configured".into()),
            }),
        );
    }

    // Validate rating range
    if !(1..=5).contains(&body.rating) {
        return (
            StatusCode::BAD_REQUEST,
            Json(RateGpuResponse {
                success: false,
                tx_id: None,
                error: Some("Rating must be between 1 and 5".into()),
            }),
        );
    }

    if body.role != "provider" && body.role != "renter" {
        return (
            StatusCode::BAD_REQUEST,
            Json(RateGpuResponse {
                success: false,
                tx_id: None,
                error: Some("Role must be 'provider' or 'renter'".into()),
            }),
        );
    }

    let url = format!("{}/api/gpu/rate", agent_url.trim_end_matches('/'));

    let agent_body = serde_json::json!({
        "rental_box_id": body.rental_id,
        "rated_pk": body.rated_public_key,
        "role": body.role,
        "rating": body.rating,
        "comment": body.comment,
    });

    match state.http_client.post(&url).json(&agent_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<serde_json::Value>().await {
                Ok(resp_body) => {
                    let tx_id = resp_body.get("tx_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    (StatusCode::OK, Json(RateGpuResponse {
                        success: true,
                        tx_id,
                        error: None,
                    }))
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse agent rating response");
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(RateGpuResponse {
                            success: false,
                            tx_id: None,
                            error: Some(format!("Failed to parse agent response: {}", e)),
                        }),
                    )
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(status = %status, "Agent returned error for GPU rating");
            (
                StatusCode::BAD_GATEWAY,
                Json(RateGpuResponse {
                    success: false,
                    tx_id: None,
                    error: Some(format!("Agent returned status: {}", status)),
                }),
            )
        }
        Err(e) => {
            warn!(error = %e, "Failed to reach agent for GPU rating");
            (
                StatusCode::BAD_GATEWAY,
                Json(RateGpuResponse {
                    success: false,
                    tx_id: None,
                    error: Some(format!("Failed to reach agent: {}", e)),
                }),
            )
        }
    }
}

/// GET /v1/gpu/reputation/{public_key} — Get aggregated reputation for a PK
///
/// Proxies to agent's /api/gpu/reputation/{pk} endpoint.
pub async fn get_gpu_reputation_handler(
    State(state): State<AppState>,
    Path(public_key): Path<String>,
) -> impl IntoResponse {
    info!(public_key = %public_key, "Getting GPU reputation");

    let agent_url = state.config.chain.agent_gpu_endpoint.trim();
    if agent_url.is_empty() {
        // Return empty reputation if agent not configured
        return (
            StatusCode::OK,
            Json(GpuReputationResponse {
                public_key,
                total_ratings: 0,
                average_rating: 0.0,
                stars: StarBreakdown {
                    one_star: 0,
                    two_star: 0,
                    three_star: 0,
                    four_star: 0,
                    five_star: 0,
                },
                provider_reputation: None,
                provider_rating_count: 0,
                renter_reputation: None,
                renter_rating_count: 0,
            }),
        )
            .into_response();
    }

    let url = format!(
        "{}/api/gpu/reputation/{}",
        agent_url.trim_end_matches('/'),
        public_key
    );

    match state.http_client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<serde_json::Value>().await {
                Ok(resp_body) => {
                    let rep = resp_body.get("reputation");
                    let total = rep.and_then(|r| r.get("total_ratings"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let avg = rep.and_then(|r| r.get("average_rating"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let one = rep.and_then(|r| r.get("one_star"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let two = rep.and_then(|r| r.get("two_star"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let three = rep.and_then(|r| r.get("three_star"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let four = rep.and_then(|r| r.get("four_star"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let five = rep.and_then(|r| r.get("five_star"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let prov_rep = rep.and_then(|r| r.get("provider_reputation"))
                        .and_then(|v| v.as_f64());
                    let prov_count = rep.and_then(|r| r.get("provider_rating_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                    let rent_rep = rep.and_then(|r| r.get("renter_reputation"))
                        .and_then(|v| v.as_f64());
                    let rent_count = rep.and_then(|r| r.get("renter_rating_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;

                    (StatusCode::OK, Json(GpuReputationResponse {
                        public_key,
                        total_ratings: total,
                        average_rating: avg,
                        stars: StarBreakdown {
                            one_star: one,
                            two_star: two,
                            three_star: three,
                            four_star: four,
                            five_star: five,
                        },
                        provider_reputation: prov_rep,
                        provider_rating_count: prov_count,
                        renter_reputation: rent_rep,
                        renter_rating_count: rent_count,
                    }))
                        .into_response()
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse agent reputation response");
                    (
                        StatusCode::BAD_GATEWAY,
                        Json(GpuErrorResponse::bad_gateway(
                            format!("Failed to parse agent response: {}", e),
                        )),
                    )
                        .into_response()
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            warn!(status = %status, "Agent returned error for GPU reputation");
            (
                StatusCode::BAD_GATEWAY,
                Json(GpuErrorResponse::bad_gateway(
                    format!("Agent returned status: {}", status),
                )),
            )
                .into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to reach agent for GPU reputation");
            (
                StatusCode::BAD_GATEWAY,
                Json(GpuErrorResponse::bad_gateway(
                    format!("Failed to reach agent: {}", e),
                )),
            )
                .into_response()
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Convert nanoERG to ERG (floating point).
fn nanoerg_to_erg(nanoerg: u64) -> f64 {
    nanoerg as f64 / 1_000_000_000.0
}

/// Try to extract VRAM (in GB) from the GPU specs JSON string.
fn extract_vram_from_specs(specs_json: &str) -> Option<u32> {
    if specs_json.is_empty() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(specs_json)
        .ok()
        .and_then(|v| v.get("vram_gb")?.as_u64().map(|v| v as u32))
}

/// Trigger a background lazy scan for GPU listings if the cache is stale.
fn trigger_lazy_gpu_scan(state: &AppState) {
    if let (Some(scanner), Some(cache)) = (&state.chain_scanner, &state.chain_cache) {
        let scanner = scanner.clone();
        let cache = cache.clone();
        tokio::spawn(async move {
            debug!("Lazy GPU listing scan triggered");
            let listings = scanner.scan_gpu_listings().await;
            cache.update_gpu_listings(listings.clone());
            info!(count = listings.len(), "Lazy GPU listing scan complete");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nanoerg_to_erg() {
        assert!((nanoerg_to_erg(1_000_000_000) - 1.0).abs() < f64::EPSILON);
        assert!((nanoerg_to_erg(500_000_000) - 0.5).abs() < f64::EPSILON);
        assert!((nanoerg_to_erg(0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_vram_from_specs() {
        let specs = r#"{"vram_gb": 24, "cuda_cores": 16384}"#;
        assert_eq!(extract_vram_from_specs(specs), Some(24));

        let specs_empty = r#"{"cuda_cores": 16384}"#;
        assert_eq!(extract_vram_from_specs(specs_empty), None);

        assert_eq!(extract_vram_from_specs(""), None);
    }

    #[test]
    fn test_listings_query_defaults() {
        let q = ListGpuListingsQuery::default();
        assert!(q.region.is_none());
        assert!(q.min_vram.is_none());
        assert!(q.max_price_per_hour.is_none());
        assert!(q.gpu_type.is_none());
    }
}
