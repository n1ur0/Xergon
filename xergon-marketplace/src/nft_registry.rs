//! NFT Registry & Token Gallery
//!
//! EIP-4/EIP-34 compliant NFT browsing, collection explorer,
//! provenance tracking, and marketplace listing management.

use axum::{
    extract::{Path, Query, State},
    Json,
    Router,
    routing::{delete, get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ================================================================
// Types
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftMetadata {
    pub token_id: String,
    pub name: String,
    pub description: String,
    pub decimals: u8,
    pub asset_type: Option<String>,
    pub content_hash: Option<String>,
    pub url: Option<String>,
    pub image_url: Option<String>,
    pub collection_id: Option<String>,
    pub category: Option<String>,
    pub created_at: u64,
    pub created_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftCollection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: i32,
    pub logo_url: String,
    pub banner_url: String,
    pub category: String,
    pub socials: Vec<(String, String)>,
    pub minting_expiry: i64,
    pub total_nfts: u64,
    pub created_height: u32,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftListing {
    pub token_id: String,
    pub seller_address: String,
    pub price_nanoerg: u64,
    pub listed_at: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftProvenanceEntry {
    pub tx_id: String,
    pub event: String,
    pub from: String,
    pub to: String,
    pub height: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftDetail {
    pub metadata: NftMetadata,
    pub owner: String,
    pub price: Option<u64>,
    pub listing_status: Option<String>,
    pub collection: Option<NftCollection>,
    pub provenance: Vec<NftProvenanceEntry>,
    pub attributes: Vec<(String, String)>,
    pub view_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftStats {
    pub total_nfts: u64,
    pub total_collections: u64,
    pub total_listed: u64,
    pub total_volume_nanoerg: u64,
    pub categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrowseParams {
    pub collection_id: Option<String>,
    pub category: Option<String>,
    pub min_price: Option<u64>,
    pub max_price: Option<u64>,
    pub sort_by: Option<String>,
    pub search: Option<String>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

// ================================================================
// NFT Registry Service
// ================================================================

pub struct NftRegistryState {
    nfts: DashMap<String, NftMetadata>,
    collections: DashMap<String, NftCollection>,
    listings: DashMap<String, NftListing>,
    provenance: DashMap<String, Vec<NftProvenanceEntry>>,
    attributes: DashMap<String, Vec<(String, String)>>,
    owners: DashMap<String, String>,
    view_counts: DashMap<String, AtomicU64>,
    total_nfts: AtomicU64,
    total_collections: AtomicU64,
    total_listed: AtomicU64,
    total_volume: AtomicU64,
}

impl NftRegistryState {
    pub fn new() -> Self {
        Self {
            nfts: DashMap::new(),
            collections: DashMap::new(),
            listings: DashMap::new(),
            provenance: DashMap::new(),
            attributes: DashMap::new(),
            owners: DashMap::new(),
            view_counts: DashMap::new(),
            total_nfts: AtomicU64::new(0),
            total_collections: AtomicU64::new(0),
            total_listed: AtomicU64::new(0),
            total_volume: AtomicU64::new(0),
        }
    }

    pub fn register_nft(&self, metadata: NftMetadata, owner: String) -> Result<(), String> {
        let token_id = metadata.token_id.clone();
        self.nfts.insert(token_id.clone(), metadata);
        self.owners.insert(token_id.clone(), owner);
        self.total_nfts.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_nft(&self, token_id: &str) -> Option<NftMetadata> {
        self.nfts.get(token_id).map(|r| r.clone())
    }

    pub fn list_nfts(
        &self,
        collection_id: Option<&str>,
        category: Option<&str>,
        search: Option<&str>,
        sort_by: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Vec<NftMetadata> {
        let mut results: Vec<NftMetadata> = self.nfts.iter()
            .map(|r| r.value().clone())
            .filter(|n| {
                if let Some(cid) = collection_id {
                    if n.collection_id.as_deref() != Some(cid) { return false; }
                }
                if let Some(cat) = category {
                    if n.category.as_deref() != Some(cat) { return false; }
                }
                if let Some(q) = search {
                    let q_lower = q.to_lowercase();
                    if !n.name.to_lowercase().contains(&q_lower)
                        && !n.description.to_lowercase().contains(&q_lower)
                    { return false; }
                }
                true
            })
            .collect();

        match sort_by.as_deref() {
            Some("name") => results.sort_by(|a, b| a.name.cmp(&b.name)),
            Some("recent") => results.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            _ => results.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        }

        let offset = offset as usize;
        let limit = limit.min(100) as usize;
        results.into_iter().skip(offset).take(limit).collect()
    }

    pub fn get_trending(&self, limit: u32) -> Vec<NftMetadata> {
        let limit = limit.min(50) as usize;
        let mut with_views: Vec<(u64, NftMetadata)> = self.nfts.iter()
            .map(|r| {
                let vc = self.view_counts.get(r.key())
                    .map(|v| v.load(Ordering::Relaxed))
                    .unwrap_or(0);
                (vc, r.value().clone())
            })
            .collect();
        with_views.sort_by(|a, b| b.0.cmp(&a.0));
        with_views.into_iter().take(limit).map(|(_, n)| n).collect()
    }

    pub fn register_collection(&self, collection: NftCollection) -> Result<(), String> {
        let id = collection.id.clone();
        self.collections.insert(id, collection);
        self.total_collections.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_collection(&self, id: &str) -> Option<NftCollection> {
        self.collections.get(id).map(|r| r.clone())
    }

    pub fn list_collections(&self, offset: u32, limit: u32) -> Vec<NftCollection> {
        let offset = offset as usize;
        let limit = limit.min(100) as usize;
        self.collections.iter()
            .map(|r| r.value().clone())
            .skip(offset)
            .take(limit)
            .collect()
    }

    pub fn list_collection_nfts(&self, collection_id: &str) -> Vec<NftMetadata> {
        self.nfts.iter()
            .map(|r| r.value().clone())
            .filter(|n| n.collection_id.as_deref() == Some(collection_id))
            .collect()
    }

    pub fn create_listing(&self, token_id: &str, seller: &str, price: u64) -> Result<NftListing, String> {
        if self.nfts.get(token_id).is_none() {
            return Err("NFT not found".into());
        }
        if self.listings.get(token_id).is_some() {
            return Err("Already listed".into());
        }
        let listing = NftListing {
            token_id: token_id.into(),
            seller_address: seller.into(),
            price_nanoerg: price,
            listed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            status: "active".into(),
        };
        self.listings.insert(token_id.into(), listing.clone());
        self.total_listed.fetch_add(1, Ordering::Relaxed);
        Ok(listing)
    }

    pub fn cancel_listing(&self, token_id: &str) -> Result<(), String> {
        match self.listings.get_mut(token_id) {
            Some(mut l) => {
                l.status = "cancelled".into();
                self.total_listed.fetch_sub(1, Ordering::Relaxed);
                Ok(())
            }
            None => Err("No active listing".into()),
        }
    }

    pub fn buy_listing(&self, token_id: &str, buyer: &str) -> Result<NftProvenanceEntry, String> {
        let price = match self.listings.get(token_id) {
            Some(l) if l.status == "active" => l.price_nanoerg,
            _ => return Err("No active listing".into()),
        };

        self.total_volume.fetch_add(price, Ordering::Relaxed);
        self.total_listed.fetch_sub(1, Ordering::Relaxed);

        // Update listing status
        if let Some(mut l) = self.listings.get_mut(token_id) {
            l.status = "sold".into();
        }

        // Update owner
        if let Some(mut o) = self.owners.get_mut(token_id) {
            let _seller = o.clone();
            *o = buyer.into();
        }

        let entry = NftProvenanceEntry {
            tx_id: format!("tx_nft_{}", token_id[..8].to_string()),
            event: "Sold".into(),
            from: "listing".into(),
            to: buyer.into(),
            height: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.provenance.entry(token_id.into())
            .or_insert_with(Vec::new)
            .push(entry.clone());
        Ok(entry)
    }

    pub fn get_listing(&self, token_id: &str) -> Option<NftListing> {
        self.listings.get(token_id).map(|r| r.clone())
    }

    pub fn add_provenance(&self, token_id: &str, entry: NftProvenanceEntry) {
        self.provenance.entry(token_id.into())
            .or_insert_with(Vec::new)
            .push(entry);
    }

    pub fn get_provenance(&self, token_id: &str) -> Vec<NftProvenanceEntry> {
        self.provenance.get(token_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    pub fn set_attributes(&self, token_id: &str, attrs: Vec<(String, String)>) {
        self.attributes.insert(token_id.into(), attrs);
    }

    pub fn get_attributes(&self, token_id: &str) -> Vec<(String, String)> {
        self.attributes.get(token_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    pub fn record_view(&self, token_id: &str) {
        self.view_counts.entry(token_id.into())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn search_nfts(&self, query: &str, offset: u32, limit: u32) -> Vec<NftMetadata> {
        self.list_nfts(None, None, Some(query), Some("recent"), offset, limit)
    }

    pub fn get_stats(&self) -> NftStats {
        let categories: Vec<String> = self.collections.iter()
            .map(|r| r.value().category.clone())
            .filter(|c| !c.is_empty())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        NftStats {
            total_nfts: self.total_nfts.load(Ordering::Relaxed),
            total_collections: self.total_collections.load(Ordering::Relaxed),
            total_listed: self.total_listed.load(Ordering::Relaxed),
            total_volume_nanoerg: self.total_volume.load(Ordering::Relaxed),
            categories,
        }
    }

    /// Build a full NFT detail view
    pub fn get_nft_detail(&self, token_id: &str) -> Option<NftDetail> {
        let metadata = self.get_nft(token_id)?;
        let owner = self.owners.get(token_id).map(|r| r.clone()).unwrap_or_default();
        let listing = self.get_listing(token_id);
        let price = listing.as_ref().filter(|l| l.status == "active").map(|l| l.price_nanoerg);
        let listing_status = listing.as_ref().map(|l| l.status.clone());
        let collection = metadata.collection_id.as_ref()
            .and_then(|cid| self.get_collection(cid));
        let provenance = self.get_provenance(token_id);
        let attributes = self.get_attributes(token_id);
        let view_count = self.view_counts.get(token_id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);

        Some(NftDetail {
            metadata,
            owner,
            price,
            listing_status,
            collection,
            provenance,
            attributes,
            view_count,
        })
    }
}

// ================================================================
// REST Handlers
// ================================================================

async fn list_nfts_handler(
    Query(params): Query<BrowseParams>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    let nfts = state.list_nfts(
        params.collection_id.as_deref(),
        params.category.as_deref(),
        params.search.as_deref(),
        params.sort_by.as_deref(),
        params.offset.unwrap_or(0),
        params.limit.unwrap_or(25),
    );
    Json(serde_json::json!({ "nfts": nfts, "count": nfts.len() }))
}

async fn get_nft_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    state.record_view(&token_id);
    match state.get_nft_detail(&token_id) {
        Some(detail) => Json(serde_json::json!({ "ok": true, "nft": detail })),
        None => Json(serde_json::json!({ "ok": false, "error": "not found" })),
    }
}

async fn provenance_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "provenance": state.get_provenance(&token_id) }))
}

async fn attributes_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "attributes": state.get_attributes(&token_id) }))
}

async fn related_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    let collection_id = state.get_nft(&token_id)
        .and_then(|n| n.collection_id.clone());
    match collection_id {
        Some(cid) => {
            let related: Vec<_> = state.list_collection_nfts(&cid)
                .into_iter()
                .filter(|n| n.token_id != token_id)
                .take(10)
                .collect();
            Json(serde_json::json!({ "related": related }))
        }
        None => Json(serde_json::json!({ "related": [] })),
    }
}

#[derive(Debug, Deserialize)]
struct ListRequest {
    price: u64,
    seller: String,
}

#[derive(Debug, Deserialize)]
struct BuyRequest {
    buyer: String,
}

async fn list_nft_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
    Json(req): Json<ListRequest>,
) -> Json<serde_json::Value> {
    match state.create_listing(&token_id, &req.seller, req.price) {
        Ok(listing) => Json(serde_json::json!({ "ok": true, "listing": listing })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}

async fn cancel_listing_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    match state.cancel_listing(&token_id) {
        Ok(()) => Json(serde_json::json!({ "ok": true })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}

async fn buy_nft_handler(
    Path(token_id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
    Json(req): Json<BuyRequest>,
) -> Json<serde_json::Value> {
    match state.buy_listing(&token_id, &req.buyer) {
        Ok(entry) => Json(serde_json::json!({ "ok": true, "entry": entry })),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })),
    }
}

async fn trending_handler(
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "trending": state.get_trending(25) }))
}

async fn search_handler(
    Query(params): Query<BrowseParams>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    let results = state.search_nfts(
        params.search.as_deref().unwrap_or(""),
        params.offset.unwrap_or(0),
        params.limit.unwrap_or(25),
    );
    Json(serde_json::json!({ "results": results, "count": results.len() }))
}

async fn stats_handler(
    State(state): State<Arc<NftRegistryState>>,
) -> Json<NftStats> {
    Json(state.get_stats())
}

async fn list_collections_handler(
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    let cols = state.list_collections(0, 100);
    Json(serde_json::json!({ "collections": cols }))
}

async fn get_collection_handler(
    Path(id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    match state.get_collection(&id) {
        Some(col) => Json(serde_json::json!({ "ok": true, "collection": col })),
        None => Json(serde_json::json!({ "ok": false, "error": "not found" })),
    }
}

async fn collection_nfts_handler(
    Path(id): Path<String>,
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "nfts": state.list_collection_nfts(&id) }))
}

async fn categories_handler(
    State(state): State<Arc<NftRegistryState>>,
) -> Json<serde_json::Value> {
    let stats = state.get_stats();
    Json(serde_json::json!({ "categories": stats.categories }))
}

// ================================================================
// Router
// ================================================================

pub fn build_router() -> Router {
    let state = Arc::new(NftRegistryState::new());
    Router::new()
        .route("/v1/nfts", get(list_nfts_handler))
        .route("/v1/nfts/{token_id}", get(get_nft_handler))
        .route("/v1/nfts/{token_id}/provenance", get(provenance_handler))
        .route("/v1/nfts/{token_id}/attributes", get(attributes_handler))
        .route("/v1/nfts/{token_id}/related", get(related_handler))
        .route("/v1/nfts/{token_id}/list", post(list_nft_handler))
        .route("/v1/nfts/{token_id}/list", delete(cancel_listing_handler))
        .route("/v1/nfts/{token_id}/buy", post(buy_nft_handler))
        .route("/v1/nfts/trending", get(trending_handler))
        .route("/v1/nfts/search", get(search_handler))
        .route("/v1/nfts/stats", get(stats_handler))
        .route("/v1/nfts/collections", get(list_collections_handler))
        .route("/v1/nfts/collections/{id}", get(get_collection_handler))
        .route("/v1/nfts/collections/{id}/nfts", get(collection_nfts_handler))
        .route("/v1/nfts/categories", get(categories_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nft(token_id: &str, name: &str, collection_id: Option<&str>) -> NftMetadata {
        NftMetadata {
            token_id: token_id.into(),
            name: name.into(),
            description: format!("{} description", name),
            decimals: 0,
            asset_type: Some("image".into()),
            content_hash: Some("a".repeat(64)),
            url: Some(format!("https://example.com/{}.png", token_id)),
            image_url: None,
            collection_id: collection_id.map(|s| s.into()),
            category: Some("art".into()),
            created_at: 1000,
            created_height: 800000,
        }
    }

    #[test]
    fn test_register_and_get() {
        let state = NftRegistryState::new();
        let nft = make_nft(&"a".repeat(64), "TestNFT", None);
        state.register_nft(nft.clone(), "owner1".into()).unwrap();
        let retrieved = state.get_nft(&nft.token_id).unwrap();
        assert_eq!(retrieved.name, "TestNFT");
    }

    #[test]
    fn test_list_with_filters() {
        let state = NftRegistryState::new();
        state.register_nft(make_nft(&"a".repeat(64), "Alpha", Some("col1")), "o1".into()).unwrap();
        state.register_nft(make_nft(&"b".repeat(64), "Beta", Some("col2")), "o2".into()).unwrap();
        state.register_nft(make_nft(&"c".repeat(64), "Gamma", Some("col1")), "o3".into()).unwrap();

        let col1_nfts = state.list_nfts(Some("col1"), None, None, None, 0, 100);
        assert_eq!(col1_nfts.len(), 2);

        let search_nfts = state.list_nfts(None, None, Some("alpha"), None, 0, 100);
        assert_eq!(search_nfts.len(), 1);
        assert_eq!(search_nfts[0].name, "Alpha");
    }

    #[test]
    fn test_listing_lifecycle() {
        let state = NftRegistryState::new();
        let nft = make_nft(&"a".repeat(64), "Listable", None);
        state.register_nft(nft, "seller".into()).unwrap();

        // Create listing
        let listing = state.create_listing(&"a".repeat(64), "seller", 1_000_000_000).unwrap();
        assert_eq!(listing.status, "active");

        // Buy
        let entry = state.buy_listing(&"a".repeat(64), "buyer").unwrap();
        assert_eq!(entry.event, "Sold");

        // Listing should be sold
        let listing = state.get_listing(&"a".repeat(64)).unwrap();
        assert_eq!(listing.status, "sold");
    }

    #[test]
    fn test_provenance() {
        let state = NftRegistryState::new();
        let nft = make_nft(&"a".repeat(64), "Tracked", None);
        state.register_nft(nft, "owner".into()).unwrap();

        state.add_provenance(&"a".repeat(64), NftProvenanceEntry {
            tx_id: "tx1".into(), event: "Minted".into(),
            from: "".into(), to: "owner".into(), height: 800000, timestamp: 1000,
        });
        let prov = state.get_provenance(&"a".repeat(64));
        assert_eq!(prov.len(), 1);
    }

    #[test]
    fn test_trending() {
        let state = NftRegistryState::new();
        let id1 = "a".repeat(64);
        let id2 = "b".repeat(64);
        state.register_nft(make_nft(&id1, "Popular", None), "o1".into()).unwrap();
        state.register_nft(make_nft(&id2, "Unpopular", None), "o2".into()).unwrap();

        state.record_view(&id1);
        state.record_view(&id1);
        state.record_view(&id1);
        state.record_view(&id2);

        let trending = state.get_trending(10);
        assert_eq!(trending.len(), 2);
        assert_eq!(trending[0].name, "Popular"); // more views
    }

    #[test]
    fn test_stats() {
        let state = NftRegistryState::new();
        state.register_nft(make_nft(&"a".repeat(64), "A", None), "o1".into()).unwrap();
        state.register_nft(make_nft(&"b".repeat(64), "B", None), "o2".into()).unwrap();
        state.register_collection(NftCollection {
            id: "col1".into(), name: "Col1".into(), description: "".into(),
            version: 1, logo_url: "".into(), banner_url: "".into(),
            category: "art".into(), socials: vec![], minting_expiry: -1,
            total_nfts: 0, created_height: 800000, created_at: 1000,
        }).unwrap();

        let stats = state.get_stats();
        assert_eq!(stats.total_nfts, 2);
        assert_eq!(stats.total_collections, 1);
        assert!(stats.categories.contains(&"art".to_string()));
    }

    #[test]
    fn test_nft_detail() {
        let state = NftRegistryState::new();
        let id = &"a".repeat(64);
        let nft = make_nft(id, "Detailed", Some("col1"));
        state.register_nft(nft, "owner1".into()).unwrap();
        state.register_collection(NftCollection {
            id: "col1".into(), name: "My Collection".into(), description: "".into(),
            version: 1, logo_url: "".into(), banner_url: "".into(),
            category: "art".into(), socials: vec![], minting_expiry: -1,
            total_nfts: 0, created_height: 800000, created_at: 1000,
        }).unwrap();
        state.set_attributes(id, vec![("color".into(), "blue".into())]);
        state.record_view(id);

        let detail = state.get_nft_detail(id).unwrap();
        assert_eq!(detail.metadata.name, "Detailed");
        assert_eq!(detail.owner, "owner1");
        assert!(detail.collection.is_some());
        assert_eq!(detail.attributes.len(), 1);
        assert_eq!(detail.view_count, 1);
    }
}
