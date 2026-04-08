//! Model marketplace listing for the Xergon agent.
//!
//! Manages publishing, updating, and deprecating model listings
//! on the Xergon marketplace relay.
//!
//! API:
//! - POST   /api/marketplace/listings            -- create listing
//! - GET    /api/marketplace/listings            -- list own listings
//! - GET    /api/marketplace/listings/{id}       -- get listing
//! - PATCH  /api/marketplace/listings/{id}       -- update listing
//! - DELETE /api/marketplace/listings/{id}       -- remove listing
//! - POST   /api/marketplace/listings/{id}/publish    -- make public
//! - POST   /api/marketplace/listings/{id}/deprecate  -- deprecate listing

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Listing visibility level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ListingVisibility {
    Public,
    Unlisted,
    Private,
}

impl Default for ListingVisibility {
    fn default() -> Self {
        Self::Private
    }
}

/// Listing lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    Draft,
    Published,
    Deprecated,
    Archived,
}

/// Pricing model for a listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub prompt_per_1m: u64,
    pub completion_per_1m: u64,
    pub min_cost_nanoerg: u64,
}

impl Default for ModelPricing {
    fn default() -> Self {
        Self {
            prompt_per_1m: 1_000_000,   // 1 ERG per 1M prompt tokens
            completion_per_1m: 2_000_000, // 2 ERG per 1M completion tokens
            min_cost_nanoerg: 100_000,   // 0.0001 ERG minimum
        }
    }
}

/// A complete model listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelListing {
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub tags: Vec<String>,
    pub pricing: ModelPricing,
    pub benchmarks: HashMap<String, f64>,
    pub visibility: ListingVisibility,
    pub status: ListingStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub deprecated_at: Option<DateTime<Utc>>,
    pub downloads: u64,
}

/// Summary of a listing for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListingSummary {
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub status: ListingStatus,
    pub visibility: ListingVisibility,
    pub downloads: u64,
    pub created_at: DateTime<Utc>,
}

/// Request to create a new listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateListingRequest {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pricing: ModelPricing,
    #[serde(default)]
    pub benchmarks: HashMap<String, f64>,
    #[serde(default)]
    pub visibility: ListingVisibility,
}

/// Request to update an existing listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateListingRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub tags: Option<Vec<String>>,
    pub pricing: Option<ModelPricing>,
    pub benchmarks: Option<HashMap<String, f64>>,
    pub visibility: Option<ListingVisibility>,
}

// ---------------------------------------------------------------------------
// Marketplace Listing Manager
// ---------------------------------------------------------------------------

/// Manages model marketplace listings.
pub struct MarketplaceListingManager {
    /// Author/provider identity
    author: String,
    /// All listings: model_id -> listing
    listings: DashMap<String, ModelListing>,
    /// Counter for generating model IDs
    listing_counter: AtomicU64,
}

impl MarketplaceListingManager {
    /// Create a new marketplace listing manager.
    pub fn new(author: String) -> Self {
        Self {
            author,
            listings: DashMap::new(),
            listing_counter: AtomicU64::new(0),
        }
    }

    /// Create a new model listing.
    pub async fn create_listing(&self, request: CreateListingRequest) -> Result<ModelListing, String> {
        let model_id = format!(
            "model-{}",
            self.listing_counter.fetch_add(1, Ordering::Relaxed)
        );

        let now = Utc::now();

        let listing = ModelListing {
            model_id: model_id.clone(),
            name: request.name,
            description: request.description,
            author: self.author.clone(),
            version: request.version,
            tags: request.tags,
            pricing: request.pricing,
            benchmarks: request.benchmarks,
            visibility: request.visibility,
            status: ListingStatus::Draft,
            created_at: now,
            updated_at: now,
            published_at: None,
            deprecated_at: None,
            downloads: 0,
        };

        self.listings.insert(model_id.clone(), listing.clone());

        info!(
            model_id = %model_id,
            name = %listing.name,
            "Model listing created"
        );

        Ok(listing)
    }

    /// Get a specific listing by ID.
    pub fn get_listing(&self, model_id: &str) -> Option<ModelListing> {
        self.listings.get(model_id).map(|l| l.clone())
    }

    /// Update an existing listing.
    pub fn update_listing(
        &self,
        model_id: &str,
        request: UpdateListingRequest,
    ) -> Result<ModelListing, String> {
        let mut listing = self.listings
            .get_mut(model_id)
            .ok_or_else(|| format!("Listing '{}' not found", model_id))?;

        if listing.status == ListingStatus::Archived {
            return Err("Cannot update an archived listing".into());
        }

        if let Some(name) = request.name {
            listing.name = name;
        }
        if let Some(description) = request.description {
            listing.description = description;
        }
        if let Some(version) = request.version {
            listing.version = version;
        }
        if let Some(tags) = request.tags {
            listing.tags = tags;
        }
        if let Some(pricing) = request.pricing {
            listing.pricing = pricing;
        }
        if let Some(benchmarks) = request.benchmarks {
            listing.benchmarks = benchmarks;
        }
        if let Some(visibility) = request.visibility {
            listing.visibility = visibility;
        }

        listing.updated_at = Utc::now();

        info!(
            model_id = %model_id,
            "Listing updated"
        );

        Ok(listing.clone())
    }

    /// Delete a listing.
    pub fn delete_listing(&self, model_id: &str) -> Result<(), String> {
        if self.listings.remove(model_id).is_none() {
            return Err(format!("Listing '{}' not found", model_id));
        }

        info!(model_id = %model_id, "Listing deleted");
        Ok(())
    }

    /// Publish a listing (make it public).
    pub fn publish_listing(&self, model_id: &str) -> Result<ModelListing, String> {
        let mut listing = self.listings
            .get_mut(model_id)
            .ok_or_else(|| format!("Listing '{}' not found", model_id))?;

        if listing.status == ListingStatus::Deprecated {
            return Err("Cannot publish a deprecated listing".into());
        }

        listing.status = ListingStatus::Published;
        listing.visibility = ListingVisibility::Public;
        listing.published_at = Some(Utc::now());
        listing.updated_at = Utc::now();

        info!(
            model_id = %model_id,
            "Listing published"
        );

        Ok(listing.clone())
    }

    /// Deprecate a listing.
    pub fn deprecate_listing(&self, model_id: &str) -> Result<ModelListing, String> {
        let mut listing = self.listings
            .get_mut(model_id)
            .ok_or_else(|| format!("Listing '{}' not found", model_id))?;

        if listing.status != ListingStatus::Published {
            return Err("Can only deprecate published listings".into());
        }

        listing.status = ListingStatus::Deprecated;
        listing.deprecated_at = Some(Utc::now());
        listing.updated_at = Utc::now();

        info!(
            model_id = %model_id,
            "Listing deprecated"
        );

        Ok(listing.clone())
    }

    /// Create a new version of an existing listing.
    pub fn create_version(&self, model_id: &str, new_version: String) -> Result<ModelListing, String> {
        let original = self.listings
            .get(model_id)
            .ok_or_else(|| format!("Listing '{}' not found", model_id))?
            .clone();

        let version_model_id = format!("{}-v{}", model_id, &new_version);
        let now = Utc::now();

        let new_listing = ModelListing {
            model_id: version_model_id.clone(),
            name: original.name.clone(),
            description: original.description.clone(),
            author: original.author.clone(),
            version: new_version,
            tags: original.tags.clone(),
            pricing: original.pricing.clone(),
            benchmarks: original.benchmarks.clone(),
            visibility: original.visibility.clone(),
            status: ListingStatus::Draft,
            created_at: now,
            updated_at: now,
            published_at: None,
            deprecated_at: None,
            downloads: 0,
        };

        self.listings.insert(version_model_id.clone(), new_listing.clone());

        info!(
            original_model_id = %model_id,
            new_model_id = %version_model_id,
            "New version created"
        );

        Ok(new_listing)
    }

    /// Auto-publish benchmarks for a listing.
    pub fn update_benchmarks(
        &self,
        model_id: &str,
        benchmarks: HashMap<String, f64>,
    ) -> Result<ModelListing, String> {
        let mut listing = self.listings
            .get_mut(model_id)
            .ok_or_else(|| format!("Listing '{}' not found", model_id))?;

        listing.benchmarks = benchmarks;
        listing.updated_at = Utc::now();

        debug!(
            model_id = %model_id,
            bench_count = listing.benchmarks.len(),
            "Benchmarks updated"
        );

        Ok(listing.clone())
    }

    /// List all listings (summaries).
    pub fn list_listings(&self) -> Vec<ListingSummary> {
        self.listings
            .iter()
            .map(|r| {
                let listing = r.value();
                ListingSummary {
                    model_id: listing.model_id.clone(),
                    name: listing.name.clone(),
                    version: listing.version.clone(),
                    status: listing.status.clone(),
                    visibility: listing.visibility.clone(),
                    downloads: listing.downloads,
                    created_at: listing.created_at,
                }
            })
            .collect()
    }

    /// Increment download count for a listing.
    pub fn increment_downloads(&self, model_id: &str) {
        if let Some(mut listing) = self.listings.get_mut(model_id) {
            listing.downloads += 1;
        }
    }
}
