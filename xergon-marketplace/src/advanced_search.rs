use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SearchSortBy
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SearchSortBy {
    Relevance,
    Rating,
    Downloads,
    Price,
    CreatedAt,
    UpdatedAt,
    Name,
}

impl SearchSortBy {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "relevance" => Some(Self::Relevance),
            "rating" => Some(Self::Rating),
            "downloads" => Some(Self::Downloads),
            "price" => Some(Self::Price),
            "created_at" | "createdat" => Some(Self::CreatedAt),
            "updated_at" | "updatedat" => Some(Self::UpdatedAt),
            "name" => Some(Self::Name),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// SearchSortOrder
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SearchSortOrder {
    Asc,
    Desc,
}

impl SearchSortOrder {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Some(Self::Asc),
            "desc" | "descending" => Some(Self::Desc),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// SearchItemType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SearchItemType {
    Model,
    Provider,
    Template,
}

// ---------------------------------------------------------------------------
// FilterOperator
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
    Lt,
    Gt,
    Lte,
    Gte,
    In,
    Contains,
    Range,
}

impl FilterOperator {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "eq" => Some(Self::Eq),
            "lt" => Some(Self::Lt),
            "gt" => Some(Self::Gt),
            "lte" => Some(Self::Lte),
            "gte" => Some(Self::Gte),
            "in" => Some(Self::In),
            "contains" => Some(Self::Contains),
            "range" => Some(Self::Range),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// SearchFilter
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchFilter {
    pub filter_type: String,
    pub field: String,
    pub operator: FilterOperator,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// SearchQuery
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchQuery {
    pub query: String,
    pub filters: Vec<SearchFilter>,
    pub sort_by: Option<SearchSortBy>,
    pub sort_order: Option<SearchSortOrder>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            filters: Vec::new(),
            sort_by: Some(SearchSortBy::Relevance),
            sort_order: Some(SearchSortOrder::Desc),
            page: Some(1),
            page_size: Some(20),
        }
    }
}

// ---------------------------------------------------------------------------
// SearchResult
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResult {
    pub id: String,
    pub item_type: SearchItemType,
    pub title: String,
    pub description: String,
    pub score: f64,
    pub highlights: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// SearchResponse
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub query: String,
    pub took_ms: u64,
}

// ---------------------------------------------------------------------------
// SearchDocument
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchDocument {
    pub id: String,
    pub item_type: SearchItemType,
    pub title: String,
    pub description: String,
    pub fields: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub rating: f64,
    pub downloads: u64,
    pub price: Option<f64>,
    pub featured: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider_id: Option<String>,
    pub model_type: Option<String>,
    pub region: Option<String>,
    pub features: Vec<String>,
}

impl SearchDocument {
    pub fn new(id: &str, item_type: SearchItemType, title: &str, description: &str) -> Self {
        let now = Utc::now();
        Self {
            id: id.to_string(),
            item_type,
            title: title.to_string(),
            description: description.to_string(),
            fields: HashMap::new(),
            tags: Vec::new(),
            rating: 0.0,
            downloads: 0,
            price: None,
            featured: false,
            created_at: now,
            updated_at: now,
            provider_id: None,
            model_type: None,
            region: None,
            features: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// InvertedIndexEntry
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct InvertedIndexEntry {
    document_ids: HashSet<String>,
    term_frequency: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// AdvancedSearchEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct AdvancedSearchEngine {
    documents: DashMap<String, SearchDocument>,
    inverted_index: DashMap<String, InvertedIndexEntry>,
    popular_searches: DashMap<String, u64>,
    document_count: AtomicU64,
}

impl Default for AdvancedSearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancedSearchEngine {
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
            inverted_index: DashMap::new(),
            popular_searches: DashMap::new(),
            document_count: AtomicU64::new(0),
        }
    }

    /// Add a document to the search index.
    pub fn add_document(&self, doc: SearchDocument) {
        let id = doc.id.clone();
        let tags = doc.tags.clone();
        let features = doc.features.clone();
        let title = doc.title.clone();
        let description = doc.description.clone();

        // Update inverted index
        self.index_text(&id, &title);
        self.index_text(&id, &description);
        for tag in &tags {
            self.index_term(&id, tag);
        }
        for feature in &features {
            self.index_term(&id, feature);
        }
        if let Some(ref model_type) = doc.model_type {
            self.index_term(&id, model_type);
        }
        if let Some(ref region) = doc.region {
            self.index_term(&id, region);
        }
        if let Some(ref provider_id) = doc.provider_id {
            self.index_term(&id, provider_id);
        }

        self.documents.insert(id, doc);
        self.document_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove a document from the search index.
    pub fn remove_document(&self, id: &str) -> bool {
        if let Some((_, doc)) = self.documents.remove(id) {
            self.remove_from_index(id, &doc.title);
            self.remove_from_index(id, &doc.description);
            for tag in &doc.tags {
                self.remove_term_from_index(id, tag);
            }
            for feature in &doc.features {
                self.remove_term_from_index(id, feature);
            }
            self.document_count.fetch_sub(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Rebuild the entire search index from current documents.
    pub fn reindex(&self) {
        self.inverted_index.clear();

        for entry in self.documents.iter() {
            let doc = entry.value();
            self.index_text(&doc.id, &doc.title);
            self.index_text(&doc.id, &doc.description);
            for tag in &doc.tags {
                self.index_term(&doc.id, tag);
            }
            for feature in &doc.features {
                self.index_term(&doc.id, feature);
            }
            if let Some(ref mt) = doc.model_type {
                self.index_term(&doc.id, mt);
            }
            if let Some(ref r) = doc.region {
                self.index_term(&doc.id, r);
            }
        }
    }

    /// Search the index with a query.
    pub fn search(&self, query: &SearchQuery) -> SearchResponse {
        let start = std::time::Instant::now();

        // Track popular search
        if !query.query.is_empty() {
            *self
                .popular_searches
                .entry(query.query.clone())
                .or_insert(0) += 1;
        }

        let query_terms: Vec<String> = query
            .query
            .to_lowercase()
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Score each document
        let mut scored: Vec<(String, f64)> = Vec::new();
        for entry in self.documents.iter() {
            let doc = entry.value();

            // Check filters first
            if !self.passes_filters(doc, &query.filters) {
                continue;
            }

            let score = self.score_document(doc, &query_terms, &query.query);
            scored.push((doc.id.clone(), score));
        }

        // Sort results
        let sort_by = query.sort_by.as_ref().unwrap_or(&SearchSortBy::Relevance);
        let sort_order = query.sort_order.as_ref().unwrap_or(&SearchSortOrder::Desc);

        self.sort_results(&mut scored, sort_by, sort_order);

        // Pagination
        let page = query.page.unwrap_or(1).max(1);
        let page_size = query.page_size.unwrap_or(20).max(1);
        let total = scored.len();
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
        let skip = ((page - 1) * page_size) as usize;
        let page_results: Vec<(String, f64)> = scored.into_iter().skip(skip).take(page_size as usize).collect();

        // Build search results with highlights
        let results: Vec<SearchResult> = page_results
            .into_iter()
            .map(|(id, score)| {
                let doc = self.documents.get(&id).map(|d| d.value().clone());
                match doc {
                    Some(d) => {
                        let highlights = self.extract_highlights(&d, &query_terms);
                        let mut metadata = HashMap::new();
                        metadata.insert("rating".to_string(), serde_json::json!(d.rating));
                        metadata.insert("downloads".to_string(), serde_json::json!(d.downloads));
                        if let Some(price) = d.price {
                            metadata.insert("price".to_string(), serde_json::json!(price));
                        }
                        if let Some(ref mt) = d.model_type {
                            metadata.insert("model_type".to_string(), serde_json::json!(mt));
                        }
                        SearchResult {
                            id: d.id,
                            item_type: d.item_type,
                            title: d.title,
                            description: d.description,
                            score,
                            highlights,
                            metadata,
                        }
                    }
                    None => SearchResult {
                        id,
                        item_type: SearchItemType::Model,
                        title: String::new(),
                        description: String::new(),
                        score,
                        highlights: Vec::new(),
                        metadata: HashMap::new(),
                    },
                }
            })
            .collect();

        let took_ms = start.elapsed().as_millis() as u64;

        SearchResponse {
            results,
            total,
            page,
            page_size,
            total_pages,
            query: query.query.clone(),
            took_ms,
        }
    }

    /// Get search suggestions based on a prefix.
    pub fn get_suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        let mut matches: Vec<(String, u64)> = Vec::new();

        for entry in self.inverted_index.iter() {
            let term = entry.key();
            if term.starts_with(&prefix_lower) {
                let count = entry.value().document_ids.len() as u64;
                matches.push((term.clone(), count));
            }
        }

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches.truncate(limit);
        matches.into_iter().map(|(term, _)| term).collect()
    }

    /// Get popular search terms.
    pub fn get_popular_searches(&self, limit: usize) -> Vec<(String, u64)> {
        let mut searches: Vec<(String, u64)> = self
            .popular_searches
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect();

        searches.sort_by(|a, b| b.1.cmp(&a.1));
        searches.truncate(limit);
        searches
    }

    /// Parse a filter string like "price:lt:100" into a SearchFilter.
    pub fn parse_filter(&self, filter_str: &str) -> Result<SearchFilter, String> {
        let parts: Vec<&str> = filter_str.splitn(3, ':').collect();
        if parts.len() < 3 {
            return Err(format!(
                "Invalid filter format '{}'. Expected 'field:operator:value'",
                filter_str
            ));
        }

        let field = parts[0].to_string();
        let operator = FilterOperator::from_str(parts[1])
            .ok_or_else(|| format!("Unknown operator '{}'", parts[1]))?;
        let value_str = parts[2].to_string();

        let value = match operator {
            FilterOperator::In => {
                let items: Vec<serde_json::Value> = value_str
                    .split(',')
                    .map(|s| serde_json::json!(s.trim()))
                    .collect();
                serde_json::json!(items)
            }
            FilterOperator::Eq
            | FilterOperator::Contains => serde_json::json!(value_str),
            FilterOperator::Lt
            | FilterOperator::Gt
            | FilterOperator::Lte
            | FilterOperator::Gte => {
                serde_json::json!(value_str.parse::<f64>().map_err(|_| {
                    format!("Cannot parse '{}' as number for field '{}'", value_str, field)
                })?)
            }
            FilterOperator::Range => {
                let range_parts: Vec<&str> = value_str.split("..").collect();
                if range_parts.len() != 2 {
                    return Err(format!(
                        "Range filter requires format 'min..max', got '{}'",
                        value_str
                    ));
                }
                let min: f64 = range_parts[0].parse().map_err(|_| {
                    format!("Cannot parse range min '{}'", range_parts[0])
                })?;
                let max: f64 = range_parts[1].parse().map_err(|_| {
                    format!("Cannot parse range max '{}'", range_parts[1])
                })?;
                serde_json::json!([min, max])
            }
        };

        Ok(SearchFilter {
            filter_type: "field".to_string(),
            field,
            operator,
            value,
        })
    }

    /// Get the filter schema (available fields and operators).
    pub fn get_filter_schema(&self) -> Vec<FilterSchemaEntry> {
        vec![
            FilterSchemaEntry {
                field: "price".to_string(),
                field_type: "number".to_string(),
                operators: vec![
                    "lt".to_string(),
                    "gt".to_string(),
                    "lte".to_string(),
                    "gte".to_string(),
                    "range".to_string(),
                ],
                description: "Filter by price".to_string(),
            },
            FilterSchemaEntry {
                field: "rating".to_string(),
                field_type: "number".to_string(),
                operators: vec![
                    "lt".to_string(),
                    "gt".to_string(),
                    "lte".to_string(),
                    "gte".to_string(),
                    "range".to_string(),
                ],
                description: "Filter by rating".to_string(),
            },
            FilterSchemaEntry {
                field: "model_type".to_string(),
                field_type: "string".to_string(),
                operators: vec![
                    "eq".to_string(),
                    "in".to_string(),
                    "contains".to_string(),
                ],
                description: "Filter by model type".to_string(),
            },
            FilterSchemaEntry {
                field: "provider".to_string(),
                field_type: "string".to_string(),
                operators: vec![
                    "eq".to_string(),
                    "in".to_string(),
                    "contains".to_string(),
                ],
                description: "Filter by provider".to_string(),
            },
            FilterSchemaEntry {
                field: "region".to_string(),
                field_type: "string".to_string(),
                operators: vec![
                    "eq".to_string(),
                    "in".to_string(),
                    "contains".to_string(),
                ],
                description: "Filter by region".to_string(),
            },
            FilterSchemaEntry {
                field: "features".to_string(),
                field_type: "string".to_string(),
                operators: vec!["contains".to_string(), "in".to_string()],
                description: "Filter by features".to_string(),
            },
            FilterSchemaEntry {
                field: "downloads".to_string(),
                field_type: "number".to_string(),
                operators: vec![
                    "lt".to_string(),
                    "gt".to_string(),
                    "lte".to_string(),
                    "gte".to_string(),
                    "range".to_string(),
                ],
                description: "Filter by download count".to_string(),
            },
        ]
    }

    /// Get search engine statistics.
    pub fn get_stats(&self) -> SearchStats {
        SearchStats {
            total_documents: self.document_count.load(Ordering::Relaxed),
            total_indexed_terms: self.inverted_index.len(),
            total_popular_searches: self.popular_searches.len(),
        }
    }

    // -- Internal methods --

    fn index_text(&self, doc_id: &str, text: &str) {
        let terms: Vec<String> = text
            .to_lowercase()
            .split_whitespace()
            .filter(|s| s.len() > 1)
            .map(|s| s.to_string())
            .collect();

        for term in terms {
            self.index_term(doc_id, &term);
        }
    }

    fn index_term(&self, doc_id: &str, term: &str) {
        let term_lower = term.to_lowercase();
        let mut entry = self
            .inverted_index
            .entry(term_lower)
            .or_insert_with(|| InvertedIndexEntry {
                document_ids: HashSet::new(),
                term_frequency: HashMap::new(),
            });
        entry.document_ids.insert(doc_id.to_string());
        *entry
            .term_frequency
            .entry(doc_id.to_string())
            .or_insert(0) += 1;
    }

    fn remove_from_index(&self, doc_id: &str, text: &str) {
        let terms: Vec<String> = text
            .to_lowercase()
            .split_whitespace()
            .filter(|s| s.len() > 1)
            .map(|s| s.to_string())
            .collect();

        for term in terms {
            self.remove_term_from_index(doc_id, &term);
        }
    }

    fn remove_term_from_index(&self, doc_id: &str, term: &str) {
        let term_lower = term.to_lowercase();
        if let Some(mut entry) = self.inverted_index.get_mut(&term_lower) {
            entry.document_ids.remove(doc_id);
            entry.term_frequency.remove(doc_id);
        }
    }

    fn score_document(&self, doc: &SearchDocument, query_terms: &[String], raw_query: &str) -> f64 {
        let mut score = 0.0;

        // TF-IDF style text relevance
        let total_docs = self.document_count.load(Ordering::Relaxed).max(1) as f64;

        for term in query_terms {
            if let Some(entry) = self.inverted_index.get(term) {
                // Term frequency in this document
                let tf = *entry
                    .term_frequency
                    .get(&doc.id)
                    .unwrap_or(&0) as f64;

                // Inverse document frequency
                let doc_freq = entry.document_ids.len() as f64;
                let idf = (total_docs / (doc_freq + 1.0)).ln() + 1.0;

                score += tf * idf;
            }
        }

        // Exact title match boost
        let title_lower = doc.title.to_lowercase();
        if title_lower.contains(raw_query) {
            score += 10.0;
        }
        for term in query_terms {
            if title_lower.contains(term) {
                score += 5.0;
            }
        }

        // Rating boost (normalize 0-5 to 0-2)
        score += (doc.rating / 5.0) * 2.0;

        // Recency boost (documents updated in last 7 days get a boost)
        let days_since_update = (Utc::now() - doc.updated_at).num_days();
        if days_since_update <= 7 {
            score += 1.5;
        } else if days_since_update <= 30 {
            score += 0.75;
        }

        // Featured boost
        if doc.featured {
            score += 3.0;
        }

        // Download popularity boost (logarithmic)
        if doc.downloads > 0 {
            score += (doc.downloads as f64).ln() * 0.5;
        }

        score
    }

    fn passes_filters(&self, doc: &SearchDocument, filters: &[SearchFilter]) -> bool {
        for filter in filters {
            match filter.field.as_str() {
                "price" => {
                    if let Some(price) = doc.price {
                        if !self.apply_numeric_filter(price, &filter.operator, &filter.value) {
                            return false;
                        }
                    } else if filter.operator != FilterOperator::Eq {
                        return false;
                    }
                }
                "rating" => {
                    if !self.apply_numeric_filter(doc.rating, &filter.operator, &filter.value) {
                        return false;
                    }
                }
                "model_type" => {
                    if !self.apply_string_filter(
                        doc.model_type.as_deref().unwrap_or(""),
                        &filter.operator,
                        &filter.value,
                    ) {
                        return false;
                    }
                }
                "provider" | "provider_id" => {
                    if !self.apply_string_filter(
                        doc.provider_id.as_deref().unwrap_or(""),
                        &filter.operator,
                        &filter.value,
                    ) {
                        return false;
                    }
                }
                "region" => {
                    if !self.apply_string_filter(
                        doc.region.as_deref().unwrap_or(""),
                        &filter.operator,
                        &filter.value,
                    ) {
                        return false;
                    }
                }
                "features" => {
                    let features_str = doc.features.join(",");
                    if !self.apply_string_filter(&features_str, &filter.operator, &filter.value) {
                        return false;
                    }
                }
                "downloads" => {
                    if !self.apply_numeric_filter(
                        doc.downloads as f64,
                        &filter.operator,
                        &filter.value,
                    ) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn apply_numeric_filter(
        &self,
        value: f64,
        operator: &FilterOperator,
        filter_val: &serde_json::Value,
    ) -> bool {
        let compare = match filter_val {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::Array(arr) => {
                // Range: [min, max]
                if arr.len() != 2 {
                    return true;
                }
                let min = arr[0].as_f64().unwrap_or(f64::NEG_INFINITY);
                let max = arr[1].as_f64().unwrap_or(f64::INFINITY);
                return value >= min && value <= max;
            }
            _ => return true,
        };

        match operator {
            FilterOperator::Eq => (value - compare).abs() < f64::EPSILON,
            FilterOperator::Lt => value < compare,
            FilterOperator::Gt => value > compare,
            FilterOperator::Lte => value <= compare,
            FilterOperator::Gte => value >= compare,
            _ => true,
        }
    }

    fn apply_string_filter(
        &self,
        value: &str,
        operator: &FilterOperator,
        filter_val: &serde_json::Value,
    ) -> bool {
        let value_lower = value.to_lowercase();
        match operator {
            FilterOperator::Eq => {
                if let Some(s) = filter_val.as_str() {
                    value_lower == s.to_lowercase()
                } else {
                    true
                }
            }
            FilterOperator::Contains => {
                if let Some(s) = filter_val.as_str() {
                    value_lower.contains(&s.to_lowercase())
                } else {
                    true
                }
            }
            FilterOperator::In => {
                if let Some(arr) = filter_val.as_array() {
                    arr.iter().any(|item| {
                        if let Some(s) = item.as_str() {
                            value_lower.contains(&s.to_lowercase())
                        } else {
                            false
                        }
                    })
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    fn sort_results(
        &self,
        results: &mut Vec<(String, f64)>,
        sort_by: &SearchSortBy,
        sort_order: &SearchSortOrder,
    ) {
        match sort_by {
            SearchSortBy::Relevance => {
                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            SearchSortBy::Rating => {
                results.sort_by(|a, b| {
                    let ra = self
                        .documents
                        .get(&a.0)
                        .map(|d| d.value().rating)
                        .unwrap_or(0.0);
                    let rb = self
                        .documents
                        .get(&b.0)
                        .map(|d| d.value().rating)
                        .unwrap_or(0.0);
                    rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SearchSortBy::Downloads => {
                results.sort_by(|a, b| {
                    let da = self
                        .documents
                        .get(&a.0)
                        .map(|d| d.value().downloads)
                        .unwrap_or(0);
                    let db = self
                        .documents
                        .get(&b.0)
                        .map(|d| d.value().downloads)
                        .unwrap_or(0);
                    db.cmp(&da)
                });
            }
            SearchSortBy::Price => {
                results.sort_by(|a, b| {
                    let pa = self
                        .documents
                        .get(&a.0)
                        .and_then(|d| d.value().price)
                        .unwrap_or(f64::INFINITY);
                    let pb = self
                        .documents
                        .get(&b.0)
                        .and_then(|d| d.value().price)
                        .unwrap_or(f64::INFINITY);
                    pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            SearchSortBy::CreatedAt => {
                results.sort_by(|a, b| {
                    let ta = self
                        .documents
                        .get(&a.0)
                        .map(|d| d.value().created_at)
                        .unwrap_or_else(Utc::now);
                    let tb = self
                        .documents
                        .get(&b.0)
                        .map(|d| d.value().created_at)
                        .unwrap_or_else(Utc::now);
                    tb.cmp(&ta)
                });
            }
            SearchSortBy::UpdatedAt => {
                results.sort_by(|a, b| {
                    let ta = self
                        .documents
                        .get(&a.0)
                        .map(|d| d.value().updated_at)
                        .unwrap_or_else(Utc::now);
                    let tb = self
                        .documents
                        .get(&b.0)
                        .map(|d| d.value().updated_at)
                        .unwrap_or_else(Utc::now);
                    tb.cmp(&ta)
                });
            }
            SearchSortBy::Name => {
                results.sort_by(|a, b| {
                    let na = self
                        .documents
                        .get(&a.0)
                        .map(|d| d.value().title.to_lowercase())
                        .unwrap_or_default();
                    let nb = self
                        .documents
                        .get(&b.0)
                        .map(|d| d.value().title.to_lowercase())
                        .unwrap_or_default();
                    na.cmp(&nb)
                });
            }
        }

        if *sort_order == SearchSortOrder::Asc {
            results.reverse();
        }
    }

    fn extract_highlights(&self, doc: &SearchDocument, query_terms: &[String]) -> Vec<String> {
        let mut highlights = Vec::new();
        let title_lower = doc.title.to_lowercase();

        for term in query_terms {
            if title_lower.contains(term) {
                highlights.push(format!("title contains '{}'", term));
            }
        }

        let desc_lower = doc.description.to_lowercase();
        for term in query_terms {
            if desc_lower.contains(term) && !highlights.iter().any(|h| h.contains(term)) {
                highlights.push(format!("description contains '{}'", term));
            }
        }

        if doc.featured {
            highlights.push("featured".to_string());
        }

        highlights.truncate(5);
        highlights
    }
}

// ---------------------------------------------------------------------------
// FilterSchemaEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FilterSchemaEntry {
    pub field: String,
    pub field_type: String,
    pub operators: Vec<String>,
    pub description: String,
}

// ---------------------------------------------------------------------------
// SearchStats
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchStats {
    pub total_documents: u64,
    pub total_indexed_terms: usize,
    pub total_popular_searches: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> AdvancedSearchEngine {
        AdvancedSearchEngine::new()
    }

    fn add_test_docs(engine: &AdvancedSearchEngine) {
        let mut doc1 = SearchDocument::new("m1", SearchItemType::Model, "LLaMA 3 8B", "A powerful 8 billion parameter language model for general tasks");
        doc1.rating = 4.8;
        doc1.downloads = 15000;
        doc1.price = Some(0.5);
        doc1.model_type = Some("llm".to_string());
        doc1.provider_id = Some("provider-a".to_string());
        doc1.tags = vec!["nlp".to_string(), "language".to_string()];
        doc1.features = vec!["streaming".to_string(), "function-calling".to_string()];
        doc1.region = Some("us-east".to_string());
        engine.add_document(doc1);

        let mut doc2 = SearchDocument::new("m2", SearchItemType::Model, "Stable Diffusion XL", "Image generation model with high quality outputs");
        doc2.rating = 4.5;
        doc2.downloads = 25000;
        doc2.price = Some(1.0);
        doc2.model_type = Some("image-gen".to_string());
        doc2.provider_id = Some("provider-b".to_string());
        doc2.tags = vec!["image".to_string(), "generation".to_string()];
        doc2.features = vec!["streaming".to_string()];
        doc2.region = Some("eu-west".to_string());
        engine.add_document(doc2);

        let mut doc3 = SearchDocument::new("m3", SearchItemType::Model, "Whisper Large V3", "Speech recognition and transcription model");
        doc3.rating = 4.2;
        doc3.downloads = 8000;
        doc3.price = Some(0.3);
        doc3.model_type = Some("audio".to_string());
        doc3.provider_id = Some("provider-a".to_string());
        doc3.tags = vec!["audio".to_string(), "transcription".to_string()];
        doc3.features = vec!["batch".to_string()];
        engine.add_document(doc3);

        let mut doc4 = SearchDocument::new("p1", SearchItemType::Provider, "Provider A Compute", "High performance GPU compute provider");
        doc4.rating = 4.7;
        doc4.downloads = 500;
        doc4.provider_id = Some("provider-a".to_string());
        engine.add_document(doc4);
    }

    // -- basic search --

    #[test]
    fn test_basic_search() {
        let engine = make_engine();
        add_test_docs(&engine);

        let query = SearchQuery {
            query: "language model".to_string(),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.total >= 1);
        assert!(response.results.len() >= 1);
        assert!(response.results[0].score > 0.0);
    }

    #[test]
    fn test_search_empty_query() {
        let engine = make_engine();
        add_test_docs(&engine);

        let query = SearchQuery {
            query: String::new(),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert_eq!(response.total, 4);
    }

    // -- filtered search --

    #[test]
    fn test_filtered_search_by_price() {
        let engine = make_engine();
        add_test_docs(&engine);

        let filter = SearchFilter {
            filter_type: "field".to_string(),
            field: "price".to_string(),
            operator: FilterOperator::Lt,
            value: serde_json::json!(0.6),
        };

        let query = SearchQuery {
            query: String::new(),
            filters: vec![filter],
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.total >= 2); // m1 (0.5) and m3 (0.3)
        for result in &response.results {
            let price = result.metadata.get("price").and_then(|v| v.as_f64()).unwrap_or(999.0);
            assert!(price < 0.6);
        }
    }

    #[test]
    fn test_filtered_search_by_rating() {
        let engine = make_engine();
        add_test_docs(&engine);

        let filter = SearchFilter {
            filter_type: "field".to_string(),
            field: "rating".to_string(),
            operator: FilterOperator::Gte,
            value: serde_json::json!(4.5),
        };

        let query = SearchQuery {
            query: String::new(),
            filters: vec![filter],
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.total >= 2); // m1 (4.8) and m2 (4.5)
    }

    #[test]
    fn test_filtered_search_by_model_type() {
        let engine = make_engine();
        add_test_docs(&engine);

        let filter = SearchFilter {
            filter_type: "field".to_string(),
            field: "model_type".to_string(),
            operator: FilterOperator::Eq,
            value: serde_json::json!("llm"),
        };

        let query = SearchQuery {
            query: String::new(),
            filters: vec![filter],
            ..Default::default()
        };
        let response = engine.search(&query);

        assert_eq!(response.total, 1);
        assert_eq!(response.results[0].id, "m1");
    }

    #[test]
    fn test_filtered_search_by_features() {
        let engine = make_engine();
        add_test_docs(&engine);

        let filter = SearchFilter {
            filter_type: "field".to_string(),
            field: "features".to_string(),
            operator: FilterOperator::Contains,
            value: serde_json::json!("streaming"),
        };

        let query = SearchQuery {
            query: String::new(),
            filters: vec![filter],
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.total >= 2); // m1 and m2 have streaming
    }

    // -- sorting --

    #[test]
    fn test_sort_by_rating() {
        let engine = make_engine();
        add_test_docs(&engine);

        let query = SearchQuery {
            query: String::new(),
            sort_by: Some(SearchSortBy::Rating),
            sort_order: Some(SearchSortOrder::Desc),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.results.len() >= 2);
        let first_rating = response.results[0].metadata.get("rating").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let second_rating = response.results[1].metadata.get("rating").and_then(|v| v.as_f64()).unwrap_or(0.0);
        assert!(first_rating >= second_rating);
    }

    #[test]
    fn test_sort_by_downloads() {
        let engine = make_engine();
        add_test_docs(&engine);

        let query = SearchQuery {
            query: String::new(),
            sort_by: Some(SearchSortBy::Downloads),
            sort_order: Some(SearchSortOrder::Desc),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert!(response.results.len() >= 2);
        let first_dl = response.results[0].metadata.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
        let second_dl = response.results[1].metadata.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(first_dl >= second_dl);
    }

    // -- pagination --

    #[test]
    fn test_pagination() {
        let engine = make_engine();
        add_test_docs(&engine);

        let query = SearchQuery {
            query: String::new(),
            page: Some(2),
            page_size: Some(2),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert_eq!(response.page, 2);
        assert_eq!(response.page_size, 2);
        assert!(response.results.len() <= 2);
        assert_eq!(response.total_pages, 2);
    }

    // -- suggestions --

    #[test]
    fn test_suggestions() {
        let engine = make_engine();
        add_test_docs(&engine);

        let suggestions = engine.get_suggestions("lang", 5);
        assert!(!suggestions.is_empty());
    }

    // -- popular searches --

    #[test]
    fn test_popular_searches() {
        let engine = make_engine();
        add_test_docs(&engine);

        // Perform some searches to build popularity
        let q1 = SearchQuery { query: "language model".to_string(), ..Default::default() };
        engine.search(&q1);
        engine.search(&q1);
        engine.search(&q1);

        let q2 = SearchQuery { query: "image".to_string(), ..Default::default() };
        engine.search(&q2);

        let popular = engine.get_popular_searches(5);
        assert!(!popular.is_empty());
        assert_eq!(popular[0].0, "language model");
        assert_eq!(popular[0].1, 3);
    }

    // -- filter parsing --

    #[test]
    fn test_parse_filter_price_lt() {
        let engine = make_engine();
        let filter = engine.parse_filter("price:lt:100").unwrap();
        assert_eq!(filter.field, "price");
        assert_eq!(filter.operator, FilterOperator::Lt);
        assert_eq!(filter.value, serde_json::json!(100.0));
    }

    #[test]
    fn test_parse_filter_rating_gte() {
        let engine = make_engine();
        let filter = engine.parse_filter("rating:gte:4.0").unwrap();
        assert_eq!(filter.field, "rating");
        assert_eq!(filter.operator, FilterOperator::Gte);
    }

    #[test]
    fn test_parse_filter_features_contains() {
        let engine = make_engine();
        let filter = engine.parse_filter("features:contains:streaming").unwrap();
        assert_eq!(filter.field, "features");
        assert_eq!(filter.operator, FilterOperator::Contains);
        assert_eq!(filter.value, serde_json::json!("streaming"));
    }

    #[test]
    fn test_parse_filter_invalid() {
        let engine = make_engine();
        let result = engine.parse_filter("badfilter");
        assert!(result.is_err());
    }

    // -- ranking --

    #[test]
    fn test_ranking_featured_boost() {
        let engine = make_engine();

        let mut doc1 = SearchDocument::new("f1", SearchItemType::Model, "Test Model", "A test");
        doc1.featured = true;
        engine.add_document(doc1);

        let mut doc2 = SearchDocument::new("f2", SearchItemType::Model, "Test Model", "A test");
        doc2.featured = false;
        engine.add_document(doc2);

        let query = SearchQuery {
            query: "Test Model".to_string(),
            ..Default::default()
        };
        let response = engine.search(&query);

        assert_eq!(response.results[0].id, "f1"); // Featured should rank first
    }

    // -- index management --

    #[test]
    fn test_remove_document() {
        let engine = make_engine();
        add_test_docs(&engine);

        assert!(engine.remove_document("m1"));
        assert!(!engine.remove_document("nonexistent"));

        let query = SearchQuery { query: String::new(), ..Default::default() };
        let response = engine.search(&query);
        assert_eq!(response.total, 3);
    }

    #[test]
    fn test_reindex() {
        let engine = make_engine();
        add_test_docs(&engine);

        engine.reindex();

        let stats = engine.get_stats();
        assert_eq!(stats.total_documents, 4);
        assert!(stats.total_indexed_terms > 0);
    }

    // -- stats --

    #[test]
    fn test_get_stats() {
        let engine = make_engine();
        add_test_docs(&engine);

        let stats = engine.get_stats();
        assert_eq!(stats.total_documents, 4);
        assert!(stats.total_indexed_terms > 0);
    }

    // -- filter schema --

    #[test]
    fn test_filter_schema() {
        let engine = make_engine();
        let schema = engine.get_filter_schema();

        assert!(!schema.is_empty());
        let fields: Vec<&str> = schema.iter().map(|s| s.field.as_str()).collect();
        assert!(fields.contains(&"price"));
        assert!(fields.contains(&"rating"));
        assert!(fields.contains(&"model_type"));
    }
}
