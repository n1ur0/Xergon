use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SearchFacetType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SearchFacetType {
    Range,
    Checkbox,
    Radio,
}

impl SearchFacetType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Range => "Range",
            Self::Checkbox => "Checkbox",
            Self::Radio => "Radio",
        }
    }
}

// ---------------------------------------------------------------------------
// SearchFacet
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchFacet {
    pub field: String,
    pub label: String,
    pub facet_type: SearchFacetType,
    pub options: Vec<serde_json::Value>,
    pub selected: Vec<serde_json::Value>,
}

impl SearchFacet {
    pub fn new(field: &str, label: &str, facet_type: SearchFacetType) -> Self {
        Self {
            field: field.to_string(),
            label: label.to_string(),
            facet_type,
            options: Vec::new(),
            selected: Vec::new(),
        }
    }

    pub fn with_options(mut self, options: Vec<serde_json::Value>) -> Self {
        self.options = options;
        self
    }

    pub fn select(&mut self, values: Vec<serde_json::Value>) {
        self.selected = values;
    }

    pub fn is_active(&self) -> bool {
        !self.selected.is_empty()
    }

    pub fn clear(&mut self) {
        self.selected.clear();
    }
}

// ---------------------------------------------------------------------------
// TypeaheadType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TypeaheadType {
    Model,
    Provider,
    Category,
    Tag,
}

impl TypeaheadType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Model => "Model",
            Self::Provider => "Provider",
            Self::Category => "Category",
            Self::Tag => "Tag",
        }
    }
}

// ---------------------------------------------------------------------------
// TypeaheadResult
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TypeaheadResult {
    pub text: String,
    pub result_type: TypeaheadType,
    pub score: f64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl TypeaheadResult {
    pub fn new(text: &str, result_type: TypeaheadType, score: f64) -> Self {
        Self {
            text: text.to_string(),
            result_type,
            score,
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

// ---------------------------------------------------------------------------
// SearchSuggestion
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchSuggestion {
    pub query: String,
    pub frequency: u64,
    pub category: String,
    pub last_used: DateTime<Utc>,
}

impl SearchSuggestion {
    pub fn new(query: &str, category: &str) -> Self {
        Self {
            query: query.to_string(),
            frequency: 1,
            category: category.to_string(),
            last_used: Utc::now(),
        }
    }

    pub fn record_use(&mut self) {
        self.frequency += 1;
        self.last_used = Utc::now();
    }
}

// ---------------------------------------------------------------------------
// SearchV2Config
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchV2Config {
    pub max_results: usize,
    pub typeahead_limit: usize,
    pub min_query_length: usize,
    pub enable_fuzzy: bool,
    pub fuzzy_threshold: f64,
}

impl Default for SearchV2Config {
    fn default() -> Self {
        Self {
            max_results: 50,
            typeahead_limit: 10,
            min_query_length: 2,
            enable_fuzzy: true,
            fuzzy_threshold: 0.7,
        }
    }
}

// ---------------------------------------------------------------------------
// SearchDocument
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchDocument {
    pub id: String,
    pub title: String,
    pub description: String,
    pub doc_type: String,
    pub tags: Vec<String>,
    pub category: String,
    pub fields: HashMap<String, serde_json::Value>,
    pub score: f64,
    pub created_at: DateTime<Utc>,
}

impl SearchDocument {
    pub fn new(id: &str, title: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            doc_type: String::new(),
            tags: Vec::new(),
            category: String::new(),
            fields: HashMap::new(),
            score: 0.0,
            created_at: Utc::now(),
        }
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.title.to_lowercase().contains(&q)
            || self.description.to_lowercase().contains(&q)
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
            || self.category.to_lowercase().contains(&q)
    }

    pub fn relevance_score(&self, query: &str) -> f64 {
        let q = query.to_lowercase();
        let mut score = 0.0;
        let words: Vec<&str> = q.split_whitespace().collect();

        for word in &words {
            if self.title.to_lowercase().contains(word) {
                score += 3.0;
            }
            if self.description.to_lowercase().contains(word) {
                score += 1.0;
            }
            if self.tags.iter().any(|t| t.to_lowercase().contains(word)) {
                score += 2.0;
            }
            if self.category.to_lowercase().contains(word) {
                score += 2.0;
            }
        }

        score
    }

    pub fn fuzzy_matches(&self, query: &str, threshold: f64) -> f64 {
        let q = query.to_lowercase();
        let title_lower = self.title.to_lowercase();
        let desc_lower = self.description.to_lowercase();

        let title_sim = levenshtein_similarity(&q, &title_lower);
        let desc_sim = levenshtein_similarity(&q, &desc_lower);

        let best = title_sim.max(desc_sim);
        if best >= threshold {
            best
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// SearchV2Query (request)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchV2Query {
    pub query: String,
    pub facets: HashMap<String, Vec<serde_json::Value>>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub doc_types: Option<Vec<String>>,
}

impl Default for SearchV2Query {
    fn default() -> Self {
        Self {
            query: String::new(),
            facets: HashMap::new(),
            sort_by: None,
            sort_order: None,
            page: Some(0),
            page_size: Some(20),
            doc_types: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SearchV2Response
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchV2Response {
    pub results: Vec<SearchDocument>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub facets: Vec<SearchFacet>,
    pub query: String,
}

// ---------------------------------------------------------------------------
// SearchEngineV2
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SearchEngineV2 {
    documents: DashMap<String, SearchDocument>,
    facets: DashMap<String, SearchFacet>,
    suggestions: DashMap<String, SearchSuggestion>,
    config: SearchV2Config,
}

impl SearchEngineV2 {
    pub fn new(config: SearchV2Config) -> Self {
        Self {
            documents: DashMap::new(),
            facets: DashMap::new(),
            suggestions: DashMap::new(),
            config,
        }
    }

    pub fn default() -> Self {
        Self::new(SearchV2Config::default())
    }

    // ---- Document management ----

    pub fn add_document(&self, doc: SearchDocument) -> String {
        let id = doc.id.clone();
        self.documents.insert(id.clone(), doc);
        id
    }

    pub fn remove_document(&self, id: &str) -> bool {
        self.documents.remove(id).is_some()
    }

    pub fn get_document(&self, id: &str) -> Option<SearchDocument> {
        self.documents.get(id).map(|d| d.clone())
    }

    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    // ---- Search ----

    pub fn search(&self, query: &SearchV2Query) -> SearchV2Response {
        if query.query.len() < self.config.min_query_length {
            return SearchV2Response {
                results: Vec::new(),
                total: 0,
                page: query.page.unwrap_or(0),
                page_size: query.page_size.unwrap_or(20),
                facets: self.get_facets(),
                query: query.query.clone(),
            };
        }

        let mut results: Vec<SearchDocument> = self
            .documents
            .iter()
            .filter_map(|entry| {
                let doc = entry.value();
                if !self.matches_facets(doc, &query.facets) {
                    return None;
                }
                if let Some(ref types) = query.doc_types {
                    if !types.is_empty() && !types.contains(&doc.doc_type) {
                        return None;
                    }
                }

                let mut scored = doc.clone();
                scored.score = if self.config.enable_fuzzy {
                    let exact = scored.relevance_score(&query.query);
                    if exact > 0.0 {
                        exact
                    } else {
                        scored.fuzzy_matches(&query.query, self.config.fuzzy_threshold)
                    }
                } else {
                    scored.relevance_score(&query.query)
                };

                if scored.score > 0.0 {
                    Some(scored)
                } else {
                    None
                }
            })
            .collect();

        // Sort
        let sort_desc = query.sort_order.as_deref() != Some("asc");
        match query.sort_by.as_deref() {
            Some("score" | "relevance") => {
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            }
            Some("created_at") => {
                if sort_desc {
                    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                } else {
                    results.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                }
            }
            Some("title") => {
                if sort_desc {
                    results.sort_by(|a, b| b.title.cmp(&a.title));
                } else {
                    results.sort_by(|a, b| a.title.cmp(&b.title));
                }
            }
            _ => {
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            }
        }

        let total = results.len();
        let page = query.page.unwrap_or(0);
        let page_size = query.page_size.unwrap_or(20).min(self.config.max_results);
        let start = page * page_size;
        let end = (start + page_size).min(results.len());
        let paginated = if start < results.len() {
            results[start..end].to_vec()
        } else {
            Vec::new()
        };

        SearchV2Response {
            results: paginated,
            total,
            page,
            page_size,
            facets: self.get_facets(),
            query: query.query.clone(),
        }
    }

    fn matches_facets(
        &self,
        doc: &SearchDocument,
        selected_facets: &HashMap<String, Vec<serde_json::Value>>,
    ) -> bool {
        if selected_facets.is_empty() {
            return true;
        }

        for (field, values) in selected_facets {
            if values.is_empty() {
                continue;
            }
            let field_val = doc.fields.get(field);
            match field_val {
                Some(val) => {
                    if !values.iter().any(|v| v == val) {
                        return false;
                    }
                }
                None => {
                    if !values.is_empty() {
                        return false;
                    }
                }
            }
        }
        true
    }

    // ---- Typeahead ----

    pub fn typeahead(&self, prefix: &str) -> Vec<TypeaheadResult> {
        if prefix.len() < self.config.min_query_length {
            return Vec::new();
        }

        let lower = prefix.to_lowercase();
        let mut results: Vec<TypeaheadResult> = self
            .documents
            .iter()
            .filter_map(|entry| {
                let doc = entry.value();
                if doc.title.to_lowercase().starts_with(&lower) {
                    let score = 1.0 + (doc.score * 0.1);
                    let result_type = match doc.doc_type.as_str() {
                        "model" => TypeaheadType::Model,
                        "provider" => TypeaheadType::Provider,
                        _ => TypeaheadType::Model,
                    };
                    let mut r = TypeaheadResult::new(&doc.title, result_type, score);
                    r.metadata.insert("id".to_string(), serde_json::json!(doc.id));
                    Some(r)
                } else if doc.description.to_lowercase().starts_with(&lower) {
                    let result_type = match doc.doc_type.as_str() {
                        "model" => TypeaheadType::Model,
                        "provider" => TypeaheadType::Provider,
                        _ => TypeaheadType::Model,
                    };
                    let mut r = TypeaheadResult::new(&doc.title, result_type, 0.5);
                    r.metadata.insert("id".to_string(), serde_json::json!(doc.id));
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        // Add category matches
        let categories: Vec<String> = self
            .documents
            .iter()
            .filter_map(|e| {
                let d = e.value();
                if d.category.to_lowercase().starts_with(&lower) {
                    Some(d.category.clone())
                } else {
                    None
                }
            })
            .collect();

        for cat in categories {
            let mut r = TypeaheadResult::new(&cat, TypeaheadType::Category, 0.7);
            r.metadata.insert("type".to_string(), serde_json::json!("category"));
            results.push(r);
        }

        // Add tag matches
        let tags: Vec<String> = self
            .documents
            .iter()
            .flat_map(|e| {
                e.value()
                    .tags
                    .iter()
                    .filter(|t| t.to_lowercase().starts_with(&lower))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();

        for tag in tags {
            let mut r = TypeaheadResult::new(&tag, TypeaheadType::Tag, 0.6);
            r.metadata.insert("type".to_string(), serde_json::json!("tag"));
            results.push(r);
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(self.config.typeahead_limit);
        results
    }

    // ---- Autocomplete (suggestions-based) ----

    pub fn autocomplete(&self, prefix: &str) -> Vec<String> {
        let typeahead_results = self.typeahead(prefix);
        let mut texts: Vec<String> = typeahead_results.iter().map(|r| r.text.clone()).collect();

        // Add suggestion matches
        let lower = prefix.to_lowercase();
        let suggestion_matches: Vec<String> = self
            .suggestions
            .iter()
            .filter(|e| e.value().query.to_lowercase().starts_with(&lower))
            .map(|e| e.value().query.clone())
            .collect();

        for s in suggestion_matches {
            if !texts.contains(&s) {
                texts.push(s);
            }
        }

        texts.truncate(self.config.typeahead_limit);
        texts
    }

    // ---- Facets ----

    pub fn add_facet(&self, facet: SearchFacet) {
        self.facets.insert(facet.field.clone(), facet);
    }

    pub fn get_facets(&self) -> Vec<SearchFacet> {
        self.facets.iter().map(|e| e.value().clone()).collect()
    }

    pub fn remove_facet(&self, field: &str) -> bool {
        self.facets.remove(field).is_some()
    }

    pub fn update_facet_selection(&self, field: &str, selected: Vec<serde_json::Value>) {
        if let Some(mut f) = self.facets.get_mut(field) {
            f.select(selected);
        }
    }

    pub fn clear_facet_selection(&self, field: &str) {
        if let Some(mut f) = self.facets.get_mut(field) {
            f.clear();
        }
    }

    // ---- Suggestions ----

    pub fn record_suggestion(&self, query: &str, category: &str) {
        let key = query.to_lowercase();
        if let Some(mut sug) = self.suggestions.get_mut(&key) {
            sug.record_use();
        } else {
            self.suggestions
                .insert(key, SearchSuggestion::new(query, category));
        }
    }

    pub fn get_suggestions(&self, prefix: &str, limit: usize) -> Vec<SearchSuggestion> {
        let lower = prefix.to_lowercase();
        let mut sugs: Vec<SearchSuggestion> = self
            .suggestions
            .iter()
            .filter(|e| e.value().query.to_lowercase().contains(&lower))
            .map(|e| e.value().clone())
            .collect();

        sugs.sort_by(|a, b| {
            b.frequency
                .cmp(&a.frequency)
                .then_with(|| b.last_used.cmp(&a.last_used))
        });
        sugs.truncate(limit);
        sugs
    }

    pub fn get_popular(&self, limit: usize) -> Vec<SearchSuggestion> {
        let mut sugs: Vec<SearchSuggestion> = self
            .suggestions
            .iter()
            .map(|e| e.value().clone())
            .collect();

        sugs.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        sugs.truncate(limit);
        sugs
    }

    pub fn get_recent(&self, limit: usize) -> Vec<SearchSuggestion> {
        let mut sugs: Vec<SearchSuggestion> = self
            .suggestions
            .iter()
            .map(|e| e.value().clone())
            .collect();

        sugs.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        sugs.truncate(limit);
        sugs
    }

    // ---- Config ----

    pub fn get_config(&self) -> &SearchV2Config {
        &self.config
    }

    pub fn update_config(&self, config: SearchV2Config) {
        // We can't replace the config field directly since it's not in a DashMap.
        // For simplicity we expose a reference; callers should create a new engine.
        // This is a no-op placeholder that logs a warning in real impl.
        let _ = config;
    }

    // ---- Stats ----

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "documents": self.documents.len(),
            "facets": self.facets.len(),
            "suggestions": self.suggestions.len(),
            "config": serde_json::to_value(&self.config).unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Fuzzy matching (Levenshtein distance based similarity)
// ---------------------------------------------------------------------------

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, a_char) in a.chars().enumerate() {
        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let max_len = a.chars().count().max(b.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein_distance(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> SearchEngineV2 {
        SearchEngineV2::default()
    }

    fn add_sample_docs(engine: &SearchEngineV2) {
        let mut d1 = SearchDocument::new("model-1", "GPT-4 Turbo", "Fast GPT-4 model");
        d1.doc_type = "model".to_string();
        d1.category = "language".to_string();
        d1.tags = vec!["llm".to_string(), "openai".to_string()];
        d1.fields.insert("price".to_string(), serde_json::json!(0.03));

        let mut d2 = SearchDocument::new("model-2", "Claude 3 Opus", "Anthropic Claude model");
        d2.doc_type = "model".to_string();
        d2.category = "language".to_string();
        d2.tags = vec!["llm".to_string(), "anthropic".to_string()];
        d2.fields.insert("price".to_string(), serde_json::json!(0.015));

        let mut d3 = SearchDocument::new("prov-1", "AI Provider Inc", "Cloud AI provider");
        d3.doc_type = "provider".to_string();
        d3.category = "compute".to_string();
        d3.tags = vec!["cloud".to_string(), "gpu".to_string()];

        engine.add_document(d1);
        engine.add_document(d2);
        engine.add_document(d3);
    }

    #[test]
    fn test_search_basic() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let query = SearchV2Query {
            query: "GPT".to_string(),
            ..Default::default()
        };
        let resp = engine.search(&query);
        assert_eq!(resp.total, 1);
        assert_eq!(resp.results[0].id, "model-1");
    }

    #[test]
    fn test_search_no_results() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let query = SearchV2Query {
            query: "xyznonexistent".to_string(),
            ..Default::default()
        };
        let resp = engine.search(&query);
        assert_eq!(resp.total, 0);
        assert!(resp.results.is_empty());
    }

    #[test]
    fn test_search_below_min_length() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let query = SearchV2Query {
            query: "G".to_string(),
            ..Default::default()
        };
        let resp = engine.search(&query);
        assert_eq!(resp.total, 0);
    }

    #[test]
    fn test_search_pagination() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let query = SearchV2Query {
            query: "model".to_string(),
            page: Some(0),
            page_size: Some(1),
            ..Default::default()
        };
        let resp = engine.search(&query);
        assert!(resp.results.len() <= 1);
    }

    #[test]
    fn test_typeahead() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let results = engine.typeahead("GP");
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.text == "GPT-4 Turbo"));
    }

    #[test]
    fn test_typeahead_empty() {
        let engine = make_engine();
        add_sample_docs(&engine);
        let results = engine.typeahead("");
        assert!(results.is_empty());
    }

    #[test]
    fn test_facets_add_get() {
        let engine = make_engine();
        let facet = SearchFacet::new("price", "Price", SearchFacetType::Range)
            .with_options(vec![serde_json::json!(0), serde_json::json!(100)]);
        engine.add_facet(facet);
        let facets = engine.get_facets();
        assert_eq!(facets.len(), 1);
        assert_eq!(facets[0].field, "price");
    }

    #[test]
    fn test_suggestions_record_and_get() {
        let engine = make_engine();
        engine.record_suggestion("gpt models", "language");
        engine.record_suggestion("gpt models", "language");
        engine.record_suggestion("gpt models", "language");

        let sugs = engine.get_suggestions("gpt", 10);
        assert_eq!(sugs.len(), 1);
        assert_eq!(sugs[0].frequency, 3);
    }

    #[test]
    fn test_get_popular() {
        let engine = make_engine();
        engine.record_suggestion("llm", "language");
        engine.record_suggestion("llm", "language");
        engine.record_suggestion("image gen", "vision");

        let popular = engine.get_popular(10);
        assert_eq!(popular.len(), 2);
        assert_eq!(popular[0].query, "llm");
    }

    #[test]
    fn test_get_recent() {
        let engine = make_engine();
        engine.record_suggestion("old query", "test");
        // Small delay is implicit via test ordering
        engine.record_suggestion("new query", "test");

        let recent = engine.get_recent(10);
        assert_eq!(recent[0].query, "new query");
    }

    #[test]
    fn test_autocomplete() {
        let engine = make_engine();
        add_sample_docs(&engine);
        engine.record_suggestion("gpt models", "language");
        let results = engine.autocomplete("gp");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_fuzzy_search() {
        let engine = SearchEngineV2::new(SearchV2Config {
            enable_fuzzy: true,
            fuzzy_threshold: 0.5,
            ..Default::default()
        });
        let mut doc = SearchDocument::new("fuzzy-1", "Llama 3.1", "Meta Llama model");
        doc.doc_type = "model".to_string();
        doc.tags = vec!["llm".to_string()];
        engine.add_document(doc);

        let query = SearchV2Query {
            query: "Lama".to_string(),
            ..Default::default()
        };
        let resp = engine.search(&query);
        // Fuzzy should find approximate match
        assert!(resp.total >= 1);
    }
}
