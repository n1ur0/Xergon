use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ================================================================
// ProvenanceType
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProvenanceType {
    BaseModel,
    FineTuned,
    Merged,
    Quantized,
    Distilled,
    Pruned,
    Dataset,
    Evaluation,
    Deployment,
}

impl ProvenanceType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "base_model" | "base-model" | "basemodel" => Some(Self::BaseModel),
            "fine_tuned" | "fine-tuned" | "finetuned" => Some(Self::FineTuned),
            "merged" => Some(Self::Merged),
            "quantized" => Some(Self::Quantized),
            "distilled" => Some(Self::Distilled),
            "pruned" => Some(Self::Pruned),
            "dataset" => Some(Self::Dataset),
            "evaluation" => Some(Self::Evaluation),
            "deployment" => Some(Self::Deployment),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::BaseModel => "Base Model",
            Self::FineTuned => "Fine-Tuned",
            Self::Merged => "Merged",
            Self::Quantized => "Quantized",
            Self::Distilled => "Distilled",
            Self::Pruned => "Pruned",
            Self::Dataset => "Dataset",
            Self::Evaluation => "Evaluation",
            Self::Deployment => "Deployment",
        }
    }
}

// ================================================================
// TrustLevel
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrustLevel {
    Verified,
    Attested,
    Unsigned,
    Unknown,
}

impl TrustLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "verified" => Some(Self::Verified),
            "attested" => Some(Self::Attested),
            "unsigned" => Some(Self::Unsigned),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    pub fn color_label(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Attested => "ATTESTED",
            Self::Unsigned => "UNSIGNED",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn score_weight(&self) -> f64 {
        match self {
            Self::Verified => 1.0,
            Self::Attested => 0.75,
            Self::Unsigned => 0.25,
            Self::Unknown => 0.0,
        }
    }
}

// ================================================================
// ProvenanceNode
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvenanceNode {
    pub id: String,
    pub model_id: String,
    pub node_type: ProvenanceType,
    pub name: String,
    pub hash: String,
    pub created_by: String,
    pub created_at: i64,
    pub metadata: HashMap<String, String>,
    pub parent_ids: Vec<String>,
    pub children_ids: Vec<String>,
    pub attestation_id: Option<String>,
    pub trust_level: TrustLevel,
}

// ================================================================
// EdgeType
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EdgeType {
    DerivedFrom,
    TrainedOn,
    EvaluatedBy,
    MergedWith,
    QuantizedFrom,
    DistilledFrom,
}

impl EdgeType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "derived_from" | "derived-from" | "derivedfrom" => Some(Self::DerivedFrom),
            "trained_on" | "trained-on" | "trainedon" => Some(Self::TrainedOn),
            "evaluated_by" | "evaluated-by" | "evaluatedby" => Some(Self::EvaluatedBy),
            "merged_with" | "merged-with" | "mergedwith" => Some(Self::MergedWith),
            "quantized_from" | "quantized-from" | "quantizedfrom" => Some(Self::QuantizedFrom),
            "distilled_from" | "distilled-from" | "distilledfrom" => Some(Self::DistilledFrom),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::DerivedFrom => "Derived From",
            Self::TrainedOn => "Trained On",
            Self::EvaluatedBy => "Evaluated By",
            Self::MergedWith => "Merged With",
            Self::QuantizedFrom => "Quantized From",
            Self::DistilledFrom => "Distilled From",
        }
    }
}

// ================================================================
// ProvenanceEdge
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvenanceEdge {
    pub from_id: String,
    pub to_id: String,
    pub edge_type: EdgeType,
    pub description: String,
}

// ================================================================
// ProvenanceGraph
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvenanceGraph {
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

// ================================================================
// ProvenanceSummary
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProvenanceSummary {
    pub total_models: u64,
    pub verified_count: u64,
    pub attested_count: u64,
    pub unsigned_count: u64,
    pub avg_chain_depth: f64,
    pub most_common_type: ProvenanceType,
}

// ================================================================
// BadgeType
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BadgeType {
    OpenSource,
    AttestedProvider,
    HashVerified,
    CommunityReviewed,
    SecurityAudited,
}

impl BadgeType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open_source" | "open-source" | "opensource" => Some(Self::OpenSource),
            "attested_provider" | "attested-provider" | "attestedprovider" => Some(Self::AttestedProvider),
            "hash_verified" | "hash-verified" | "hashverified" => Some(Self::HashVerified),
            "community_reviewed" | "community-reviewed" | "communityreviewed" => Some(Self::CommunityReviewed),
            "security_audited" | "security-audited" | "securityaudited" => Some(Self::SecurityAudited),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::OpenSource => "Open Source",
            Self::AttestedProvider => "Attested Provider",
            Self::HashVerified => "Hash Verified",
            Self::CommunityReviewed => "Community Reviewed",
            Self::SecurityAudited => "Security Audited",
        }
    }
}

// ================================================================
// TrustBadge
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrustBadge {
    pub model_id: String,
    pub badge_type: BadgeType,
    pub level: TrustLevel,
    pub description: String,
    pub awarded_at: i64,
}

// ================================================================
// VerificationResult
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerificationResult {
    pub model_id: String,
    pub valid: bool,
    pub chain_depth: usize,
    pub issues: Vec<String>,
    pub verified_at: i64,
}

// ================================================================
// SearchQuery
// ================================================================

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub node_type: Option<String>,
    pub limit: Option<usize>,
}

// ================================================================
// AwardBadgeRequest
// ================================================================

#[derive(Deserialize)]
pub struct AwardBadgeRequest {
    pub badge_type: String,
    pub description: String,
}

// ================================================================
// ProvenanceDashboard
// ================================================================

pub struct ProvenanceDashboard {
    nodes: DashMap<String, ProvenanceNode>,
    edges: DashMap<String, ProvenanceEdge>,
    badges: DashMap<String, Vec<TrustBadge>>,
    /// model_id -> list of node_ids
    model_index: DashMap<String, Vec<String>>,
}

impl ProvenanceDashboard {
    pub fn new() -> Self {
        Self {
            nodes: DashMap::new(),
            edges: DashMap::new(),
            badges: DashMap::new(),
            model_index: DashMap::new(),
        }
    }

    // ── add_node ──────────────────────────────────────────────────

    pub fn add_node(&self, node: ProvenanceNode) -> ProvenanceNode {
        let id = node.id.clone();
        let model_id = node.model_id.clone();

        // Insert the node
        self.nodes.insert(id.clone(), node.clone());

        // Update model index
        self.model_index
            .entry(model_id.clone())
            .or_insert_with(Vec::new)
            .push(id.clone());

        // Link parents/children
        for parent_id in &node.parent_ids {
            if let Some(mut parent) = self.nodes.get_mut(parent_id) {
                if !parent.children_ids.contains(&id) {
                    parent.children_ids.push(id.clone());
                }
            }
        }
        for child_id in &node.children_ids {
            if let Some(mut child) = self.nodes.get_mut(child_id) {
                if !child.parent_ids.contains(&id) {
                    child.parent_ids.push(id.clone());
                }
            }
        }

        node
    }

    // ── add_edge ──────────────────────────────────────────────────

    pub fn add_edge(&self, from_id: String, to_id: String, edge_type: EdgeType, description: String) -> Option<ProvenanceEdge> {
        // Validate both endpoints exist
        if !self.nodes.contains_key(&from_id) || !self.nodes.contains_key(&to_id) {
            return None;
        }

        let edge = ProvenanceEdge {
            from_id: from_id.clone(),
            to_id: to_id.clone(),
            edge_type: edge_type.clone(),
            description,
        };

        let edge_key = format!("{}:{}", from_id, to_id);
        self.edges.insert(edge_key, edge.clone());

        // Update parent/child links
        if let Some(mut from_node) = self.nodes.get_mut(&from_id) {
            if !from_node.children_ids.contains(&to_id) {
                from_node.children_ids.push(to_id.clone());
            }
        }
        if let Some(mut to_node) = self.nodes.get_mut(&to_id) {
            if !to_node.parent_ids.contains(&from_id) {
                to_node.parent_ids.push(from_id);
            }
        }

        Some(edge)
    }

    // ── get_model_provenance ──────────────────────────────────────

    pub fn get_model_provenance(&self, model_id: &str) -> ProvenanceGraph {
        let mut node_ids = HashSet::new();
        let mut queue = VecDeque::new();

        // BFS to collect all connected nodes
        if let Some(ids) = self.model_index.get(model_id) {
            for id in ids.iter() {
                queue.push_back(id.clone());
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if node_ids.contains(&current_id) {
                continue;
            }
            node_ids.insert(current_id.clone());

            if let Some(node) = self.nodes.get(&current_id) {
                for parent_id in &node.parent_ids {
                    if !node_ids.contains(parent_id) {
                        queue.push_back(parent_id.clone());
                    }
                }
                for child_id in &node.children_ids {
                    if !node_ids.contains(child_id) {
                        queue.push_back(child_id.clone());
                    }
                }
            }
        }

        let nodes: Vec<ProvenanceNode> = node_ids
            .iter()
            .filter_map(|id| self.nodes.get(id).map(|n| n.clone()))
            .collect();

        let mut edges = Vec::new();
        for node in &nodes {
            for child_id in &node.children_ids {
                let edge_key = format!("{}:{}", node.id, child_id);
                if let Some(edge) = self.edges.get(&edge_key) {
                    edges.push(edge.clone());
                }
            }
        }

        ProvenanceGraph { nodes, edges }
    }

    // ── verify_model ──────────────────────────────────────────────

    pub fn verify_model(&self, model_id: &str) -> VerificationResult {
        let graph = self.get_model_provenance(model_id);
        let mut issues = Vec::new();
        let chain_depth = graph.nodes.len();

        if chain_depth == 0 {
            return VerificationResult {
                model_id: model_id.to_string(),
                valid: false,
                chain_depth: 0,
                issues: vec!["No provenance nodes found for this model".to_string()],
                verified_at: Utc::now().timestamp(),
            };
        }

        // Check for hash consistency
        let mut seen_hashes = HashSet::new();
        for node in &graph.nodes {
            if node.hash.is_empty() {
                issues.push(format!("Node {} has no hash", node.id));
            } else if seen_hashes.contains(&node.hash) {
                issues.push(format!("Duplicate hash detected: {} in node {}", &node.hash[..12.min(node.hash.len())], node.id));
            } else {
                seen_hashes.insert(node.hash.clone());
            }
        }

        // Check trust levels
        for node in &graph.nodes {
            if node.trust_level == TrustLevel::Unknown {
                issues.push(format!("Node {} ({}) has unknown trust level", node.id, node.name));
            }
            if node.trust_level == TrustLevel::Unsigned && node.node_type == ProvenanceType::BaseModel {
                issues.push(format!("Base model node {} ({}) is unsigned -- consider attestation", node.id, node.name));
            }
        }

        // Check for circular dependencies
        if self.has_cycle(model_id) {
            issues.push("Circular dependency detected in provenance chain".to_string());
        }

        // Check edge consistency
        for edge in &graph.edges {
            let from_exists = graph.nodes.iter().any(|n| n.id == edge.from_id);
            let to_exists = graph.nodes.iter().any(|n| n.id == edge.to_id);
            if !from_exists || !to_exists {
                issues.push(format!("Dangling edge: {} -> {}", edge.from_id, edge.to_id));
            }
        }

        VerificationResult {
            model_id: model_id.to_string(),
            valid: issues.is_empty(),
            chain_depth,
            issues,
            verified_at: Utc::now().timestamp(),
        }
    }

    // ── has_cycle (internal) ──────────────────────────────────────

    fn has_cycle(&self, model_id: &str) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut has_cycle = false;

        fn dfs(
            node_id: &str,
            nodes: &DashMap<String, ProvenanceNode>,
            visited: &mut HashSet<String>,
            rec_stack: &mut HashSet<String>,
            has_cycle: &mut bool,
        ) {
            visited.insert(node_id.to_string());
            rec_stack.insert(node_id.to_string());

            if let Some(node) = nodes.get(node_id) {
                for child_id in &node.children_ids {
                    if !visited.contains(child_id) {
                        dfs(child_id, nodes, visited, rec_stack, has_cycle);
                    } else if rec_stack.contains(child_id) {
                        *has_cycle = true;
                    }
                }
            }

            rec_stack.remove(node_id);
        }

        if let Some(ids) = self.model_index.get(model_id) {
            for id in ids.iter() {
                if !visited.contains(id.as_str()) {
                    dfs(id, &self.nodes, &mut visited, &mut rec_stack, &mut has_cycle);
                }
            }
        }

        has_cycle
    }

    // ── award_badge ───────────────────────────────────────────────

    pub fn award_badge(&self, model_id: &str, badge_type: BadgeType, description: String) -> Option<TrustBadge> {
        // Verify model exists
        if !self.model_index.contains_key(model_id) {
            return None;
        }

        let badge = TrustBadge {
            model_id: model_id.to_string(),
            badge_type,
            level: TrustLevel::Verified,
            description,
            awarded_at: Utc::now().timestamp(),
        };

        self.badges
            .entry(model_id.to_string())
            .or_insert_with(Vec::new)
            .push(badge.clone());

        Some(badge)
    }

    // ── get_badges ────────────────────────────────────────────────

    pub fn get_badges(&self, model_id: &str) -> Vec<TrustBadge> {
        self.badges
            .get(model_id)
            .map(|b| b.clone())
            .unwrap_or_default()
    }

    // ── get_summary ───────────────────────────────────────────────

    pub fn get_summary(&self) -> ProvenanceSummary {
        let mut total = 0u64;
        let mut verified = 0u64;
        let mut attested = 0u64;
        let mut unsigned = 0u64;
        let mut type_counts: HashMap<ProvenanceType, u64> = HashMap::new();
        let mut chain_depths: Vec<usize> = Vec::new();

        for entry in self.model_index.iter() {
            total += 1;
            let node_ids = entry.value();

            // Compute chain depth for this model
            chain_depths.push(node_ids.len());

            for node_id in node_ids {
                if let Some(node) = self.nodes.get(node_id) {
                    match node.trust_level {
                        TrustLevel::Verified => verified += 1,
                        TrustLevel::Attested => attested += 1,
                        TrustLevel::Unsigned => unsigned += 1,
                        TrustLevel::Unknown => {}
                    }
                    *type_counts.entry(node.node_type.clone()).or_insert(0) += 1;
                }
            }
        }

        let most_common_type = type_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| t)
            .unwrap_or(ProvenanceType::BaseModel);

        let avg_chain_depth = if chain_depths.is_empty() {
            0.0
        } else {
            chain_depths.iter().sum::<usize>() as f64 / chain_depths.len() as f64
        };

        ProvenanceSummary {
            total_models: total,
            verified_count: verified,
            attested_count: attested,
            unsigned_count: unsigned,
            avg_chain_depth,
            most_common_type,
        }
    }

    // ── search_models ─────────────────────────────────────────────

    pub fn search_models(&self, query: &str, node_type: Option<&str>, limit: usize) -> Vec<ProvenanceNode> {
        let query_lower = query.to_lowercase();
        let parsed_type = node_type.and_then(ProvenanceType::from_str);
        let mut results: Vec<ProvenanceNode> = Vec::new();

        for entry in self.nodes.iter() {
            let node = entry.value();

            // Filter by type
            if let Some(ref pt) = parsed_type {
                if node.node_type != *pt {
                    continue;
                }
            }

            // Match query against name, model_id, hash, or created_by
            if node.name.to_lowercase().contains(&query_lower)
                || node.model_id.to_lowercase().contains(&query_lower)
                || node.hash.to_lowercase().contains(&query_lower)
                || node.created_by.to_lowercase().contains(&query_lower)
            {
                results.push(node.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }

        results
    }

    // ── get_lineage ───────────────────────────────────────────────

    pub fn get_lineage(&self, model_id: &str, depth: usize) -> ProvenanceGraph {
        let mut node_ids = HashSet::new();
        let mut edges = Vec::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        // Start from model nodes
        if let Some(ids) = self.model_index.get(model_id) {
            for id in ids.iter() {
                queue.push_back((id.clone(), 0));
            }
        }

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if node_ids.contains(&current_id) || current_depth > depth {
                continue;
            }
            node_ids.insert(current_id.clone());

            if let Some(node) = self.nodes.get(&current_id) {
                // Walk up to parents (lineage = ancestors)
                for parent_id in &node.parent_ids {
                    let edge_key = format!("{}:{}", parent_id, current_id);
                    if let Some(edge) = self.edges.get(&edge_key) {
                        edges.push(edge.clone());
                    }
                    if !node_ids.contains(parent_id) {
                        queue.push_back((parent_id.clone(), current_depth + 1));
                    }
                }
            }
        }

        let nodes: Vec<ProvenanceNode> = node_ids
            .iter()
            .filter_map(|id| self.nodes.get(id).map(|n| n.clone()))
            .collect();

        ProvenanceGraph { nodes, edges }
    }

    // ── get_derivatives ───────────────────────────────────────────

    pub fn get_derivatives(&self, model_id: &str) -> ProvenanceGraph {
        let mut node_ids = HashSet::new();
        let mut edges = Vec::new();
        let mut queue: VecDeque<String> = VecDeque::new();

        if let Some(ids) = self.model_index.get(model_id) {
            for id in ids.iter() {
                queue.push_back(id.clone());
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if node_ids.contains(&current_id) {
                continue;
            }
            node_ids.insert(current_id.clone());

            if let Some(node) = self.nodes.get(&current_id) {
                for child_id in &node.children_ids {
                    let edge_key = format!("{}:{}", current_id, child_id);
                    if let Some(edge) = self.edges.get(&edge_key) {
                        edges.push(edge.clone());
                    }
                    if !node_ids.contains(child_id) {
                        queue.push_back(child_id.clone());
                    }
                }
            }
        }

        let nodes: Vec<ProvenanceNode> = node_ids
            .iter()
            .filter_map(|id| self.nodes.get(id).map(|n| n.clone()))
            .collect();

        ProvenanceGraph { nodes, edges }
    }
}

// ================================================================
// Request / Response types
// ================================================================

#[derive(Deserialize)]
pub struct AddNodeRequest {
    pub id: Option<String>,
    pub model_id: String,
    pub node_type: String,
    pub name: String,
    pub hash: String,
    pub created_by: String,
    pub metadata: Option<HashMap<String, String>>,
    pub parent_ids: Option<Vec<String>>,
    pub trust_level: Option<String>,
}

#[derive(Deserialize)]
pub struct AddEdgeRequest {
    pub from_id: String,
    pub to_id: String,
    pub edge_type: String,
    pub description: Option<String>,
}

// ================================================================
// REST Handlers
// ================================================================

async fn add_node_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Json(req): Json<AddNodeRequest>,
) -> Json<ProvenanceNode> {
    let node_type = ProvenanceType::from_str(&req.node_type).unwrap_or(ProvenanceType::BaseModel);
    let trust_level = req
        .trust_level
        .as_deref()
        .and_then(TrustLevel::from_str)
        .unwrap_or(TrustLevel::Unknown);

    let id = req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let node = ProvenanceNode {
        id: id.clone(),
        model_id: req.model_id,
        node_type,
        name: req.name,
        hash: req.hash,
        created_by: req.created_by,
        created_at: Utc::now().timestamp(),
        metadata: req.metadata.unwrap_or_default(),
        parent_ids: req.parent_ids.unwrap_or_default(),
        children_ids: Vec::new(),
        attestation_id: None,
        trust_level,
    };

    let added = dashboard.add_node(node);
    Json(added)
}

async fn add_edge_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Json(req): Json<AddEdgeRequest>,
) -> Json<serde_json::Value> {
    let edge_type = EdgeType::from_str(&req.edge_type);

    match edge_type {
        Some(et) => {
            let description = req.description.unwrap_or_default();
            match dashboard.add_edge(req.from_id, req.to_id, et, description) {
                Some(edge) => Json(serde_json::to_value(edge).unwrap_or_default()),
                None => Json(serde_json::json!({
                    "error": "Edge could not be created -- one or both nodes not found"
                })),
            }
        }
        None => Json(serde_json::json!({
            "error": format!("Invalid edge type: {}", req.edge_type)
        })),
    }
}

async fn get_model_provenance_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Path(model_id): Path<String>,
) -> Json<ProvenanceGraph> {
    Json(dashboard.get_model_provenance(&model_id))
}

async fn verify_model_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Path(model_id): Path<String>,
) -> Json<VerificationResult> {
    Json(dashboard.verify_model(&model_id))
}

async fn get_badges_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Path(model_id): Path<String>,
) -> Json<Vec<TrustBadge>> {
    Json(dashboard.get_badges(&model_id))
}

async fn award_badge_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Path(model_id): Path<String>,
    Json(req): Json<AwardBadgeRequest>,
) -> Json<serde_json::Value> {
    let badge_type = BadgeType::from_str(&req.badge_type);

    match badge_type {
        Some(bt) => {
            match dashboard.award_badge(&model_id, bt, req.description) {
                Some(badge) => Json(serde_json::to_value(badge).unwrap_or_default()),
                None => Json(serde_json::json!({
                    "error": "Badge could not be awarded -- model not found"
                })),
            }
        }
        None => Json(serde_json::json!({
            "error": format!("Invalid badge type: {}", req.badge_type)
        })),
    }
}

async fn get_summary_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
) -> Json<ProvenanceSummary> {
    Json(dashboard.get_summary())
}

async fn search_handler(
    State(dashboard): State<Arc<ProvenanceDashboard>>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<ProvenanceNode>> {
    let query = params.q.unwrap_or_default();
    let limit = params.limit.unwrap_or(20);
    Json(dashboard.search_models(&query, params.node_type.as_deref(), limit))
}

// ================================================================
// Router
// ================================================================

pub fn router() -> Router<Arc<ProvenanceDashboard>> {
    Router::new()
        .route("/v1/provenance/node", post(add_node_handler))
        .route("/v1/provenance/edge", post(add_edge_handler))
        .route("/v1/provenance/model/:model_id", get(get_model_provenance_handler))
        .route("/v1/provenance/model/:model_id/verify", post(verify_model_handler))
        .route(
            "/v1/provenance/model/:model_id/badges",
            get(get_badges_handler),
        )
        .route(
            "/v1/provenance/model/:model_id/badges",
            post(award_badge_handler),
        )
        .route("/v1/provenance/summary", get(get_summary_handler))
        .route("/v1/provenance/search", get(search_handler))
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dashboard() -> Arc<ProvenanceDashboard> {
        Arc::new(ProvenanceDashboard::new())
    }

    fn make_node(id: &str, model_id: &str, name: &str, node_type: ProvenanceType, trust_level: TrustLevel, hash: &str) -> ProvenanceNode {
        ProvenanceNode {
            id: id.to_string(),
            model_id: model_id.to_string(),
            node_type,
            name: name.to_string(),
            hash: hash.to_string(),
            created_by: "test-user".to_string(),
            created_at: Utc::now().timestamp(),
            metadata: HashMap::new(),
            parent_ids: Vec::new(),
            children_ids: Vec::new(),
            attestation_id: None,
            trust_level,
        }
    }

    // ── test_add_node ────────────────────────────────────────────

    #[test]
    fn test_add_node() {
        let dash = make_dashboard();
        let node = make_node("n1", "model-a", "LLaMA 7B", ProvenanceType::BaseModel, TrustLevel::Verified, "hash001");

        dash.add_node(node.clone());

        assert!(dash.nodes.contains_key("n1"));
        assert!(dash.model_index.contains_key("model-a"));
        let ids = dash.model_index.get("model-a").unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&"n1".to_string()));
    }

    // ── test_add_edge ────────────────────────────────────────────

    #[test]
    fn test_add_edge() {
        let dash = make_dashboard();
        let parent = make_node("p1", "model-a", "Parent", ProvenanceType::BaseModel, TrustLevel::Verified, "h_parent");
        let child = make_node("c1", "model-b", "Child", ProvenanceType::FineTuned, TrustLevel::Attested, "h_child");

        dash.add_node(parent);
        dash.add_node(child);

        let edge = dash.add_edge("p1".to_string(), "c1".to_string(), EdgeType::DerivedFrom, "fine-tuned from".to_string());
        assert!(edge.is_some());

        // Verify parent/child links
        let p = dash.nodes.get("p1").unwrap();
        assert!(p.children_ids.contains(&"c1".to_string()));

        let c = dash.nodes.get("c1").unwrap();
        assert!(c.parent_ids.contains(&"p1".to_string()));
    }

    // ── test_provenance_graph ────────────────────────────────────

    #[test]
    fn test_provenance_graph() {
        let dash = make_dashboard();

        dash.add_node(make_node("base", "model-a", "Base", ProvenanceType::BaseModel, TrustLevel::Verified, "h_base"));
        dash.add_node(make_node("ft", "model-b", "FineTuned", ProvenanceType::FineTuned, TrustLevel::Attested, "h_ft"));
        dash.add_edge("base".to_string(), "ft".to_string(), EdgeType::DerivedFrom, "fine-tuned".to_string());

        let graph = dash.get_model_provenance("model-b");
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }

    // ── test_verify_model_chain ──────────────────────────────────

    #[test]
    fn test_verify_model_chain() {
        let dash = make_dashboard();

        dash.add_node(make_node("b1", "model-a", "Base", ProvenanceType::BaseModel, TrustLevel::Verified, "h_b1"));
        dash.add_node(make_node("f1", "model-a", "FineTuned", ProvenanceType::FineTuned, TrustLevel::Verified, "h_f1"));
        dash.add_edge("b1".to_string(), "f1".to_string(), EdgeType::DerivedFrom, "ft".to_string());

        let result = dash.verify_model("model-a");
        assert!(result.valid);
        assert_eq!(result.chain_depth, 2);
        assert!(result.issues.is_empty());
    }

    // ── test_award_badge ─────────────────────────────────────────

    #[test]
    fn test_award_badge() {
        let dash = make_dashboard();
        dash.add_node(make_node("n1", "model-a", "Test", ProvenanceType::BaseModel, TrustLevel::Verified, "h1"));

        let badge = dash.award_badge("model-a", BadgeType::HashVerified, "SHA-256 hash verified".to_string());
        assert!(badge.is_some());

        let badges = dash.get_badges("model-a");
        assert_eq!(badges.len(), 1);
        assert_eq!(badges[0].badge_type, BadgeType::HashVerified);
    }

    // ── test_get_badges ──────────────────────────────────────────

    #[test]
    fn test_get_badges() {
        let dash = make_dashboard();

        // No badges for non-existent model
        let badges = dash.get_badges("nonexistent");
        assert!(badges.is_empty());

        dash.add_node(make_node("n1", "model-x", "Test", ProvenanceType::BaseModel, TrustLevel::Verified, "hx"));
        dash.award_badge("model-x", BadgeType::OpenSource, "MIT license".to_string());
        dash.award_badge("model-x", BadgeType::CommunityReviewed, "10 reviews".to_string());

        let badges = dash.get_badges("model-x");
        assert_eq!(badges.len(), 2);
    }

    // ── test_summary ─────────────────────────────────────────────

    #[test]
    fn test_summary() {
        let dash = make_dashboard();

        dash.add_node(make_node("n1", "m1", "Base1", ProvenanceType::BaseModel, TrustLevel::Verified, "h1"));
        dash.add_node(make_node("n2", "m2", "Base2", ProvenanceType::BaseModel, TrustLevel::Attested, "h2"));
        dash.add_node(make_node("n3", "m3", "Base3", ProvenanceType::FineTuned, TrustLevel::Unsigned, "h3"));

        let summary = dash.get_summary();
        assert_eq!(summary.total_models, 3);
        assert_eq!(summary.verified_count, 1);
        assert_eq!(summary.attested_count, 1);
        assert_eq!(summary.unsigned_count, 1);
        assert!((summary.avg_chain_depth - 1.0).abs() < 0.01);
    }

    // ── test_search ──────────────────────────────────────────────

    #[test]
    fn test_search() {
        let dash = make_dashboard();

        dash.add_node(make_node("n1", "m1", "LLaMA 7B", ProvenanceType::BaseModel, TrustLevel::Verified, "abc123"));
        dash.add_node(make_node("n2", "m2", "Mistral 7B", ProvenanceType::BaseModel, TrustLevel::Attested, "def456"));
        dash.add_node(make_node("n3", "m3", "LLaMA 70B", ProvenanceType::FineTuned, TrustLevel::Verified, "ghi789"));

        let results = dash.search_models("llama", None, 10);
        assert_eq!(results.len(), 2);

        let results = dash.search_models("mistral", None, 10);
        assert_eq!(results.len(), 1);

        let results = dash.search_models("xyz", None, 10);
        assert!(results.is_empty());
    }

    // ── test_lineage_depth ───────────────────────────────────────

    #[test]
    fn test_lineage_depth() {
        let dash = make_dashboard();

        dash.add_node(make_node("base", "m1", "Base", ProvenanceType::BaseModel, TrustLevel::Verified, "hb"));
        dash.add_node(make_node("mid", "m1", "Mid", ProvenanceType::FineTuned, TrustLevel::Attested, "hm"));
        dash.add_node(make_node("top", "m1", "Top", ProvenanceType::Quantized, TrustLevel::Verified, "ht"));

        dash.add_edge("base".to_string(), "mid".to_string(), EdgeType::DerivedFrom, "ft".to_string());
        dash.add_edge("mid".to_string(), "top".to_string(), EdgeType::QuantizedFrom, "quant".to_string());

        // Depth 0 = just the model's own nodes
        let lineage0 = dash.get_lineage("m1", 0);
        // Should include the root nodes of the model
        assert!(!lineage0.nodes.is_empty());

        // Depth 1 = one level up from base
        let lineage1 = dash.get_lineage("m1", 1);
        assert!(lineage1.nodes.len() >= 2);

        // Depth 10 = full chain
        let lineage_full = dash.get_lineage("m1", 10);
        assert_eq!(lineage_full.nodes.len(), 3);
    }

    // ── test_derivatives ─────────────────────────────────────────

    #[test]
    fn test_derivatives() {
        let dash = make_dashboard();

        dash.add_node(make_node("base", "m-base", "Base", ProvenanceType::BaseModel, TrustLevel::Verified, "hb"));
        dash.add_node(make_node("ft1", "m-ft1", "FT1", ProvenanceType::FineTuned, TrustLevel::Attested, "hf1"));
        dash.add_node(make_node("q1", "m-q1", "Q1", ProvenanceType::Quantized, TrustLevel::Verified, "hq1"));

        dash.add_edge("base".to_string(), "ft1".to_string(), EdgeType::DerivedFrom, "ft".to_string());
        dash.add_edge("ft1".to_string(), "q1".to_string(), EdgeType::QuantizedFrom, "quant".to_string());

        let derivs = dash.get_derivatives("m-base");
        assert_eq!(derivs.nodes.len(), 3);
        assert_eq!(derivs.edges.len(), 2);

        // Derivatives of ft1 should include q1
        let derivs2 = dash.get_derivatives("m-ft1");
        assert_eq!(derivs2.nodes.len(), 2);
    }

    // ── test_circular_detection ──────────────────────────────────

    #[test]
    fn test_circular_detection() {
        let dash = make_dashboard();

        dash.add_node(make_node("a", "m1", "NodeA", ProvenanceType::BaseModel, TrustLevel::Verified, "ha"));
        dash.add_node(make_node("b", "m1", "NodeB", ProvenanceType::FineTuned, TrustLevel::Attested, "hb"));
        dash.add_node(make_node("c", "m1", "NodeC", ProvenanceType::Merged, TrustLevel::Verified, "hc"));

        dash.add_edge("a".to_string(), "b".to_string(), EdgeType::DerivedFrom, "a->b".to_string());
        dash.add_edge("b".to_string(), "c".to_string(), EdgeType::DerivedFrom, "b->c".to_string());
        // Create a cycle: c -> a
        dash.add_edge("c".to_string(), "a".to_string(), EdgeType::MergedWith, "c->a".to_string());

        let result = dash.verify_model("m1");
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.contains("Circular")));
    }

    // ── test_concurrent_operations ───────────────────────────────

    #[test]
    fn test_concurrent_operations() {
        use std::thread;

        let dash = make_dashboard();
        let dash_clone = dash.clone();

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let d = dash_clone.clone();
                thread::spawn(move || {
                    let id = format!("node-{}", i);
                    let model_id = format!("model-{}", i);
                    d.add_node(make_node(&id, &model_id, &format!("Node {}", i), ProvenanceType::BaseModel, TrustLevel::Verified, &format!("hash{}", i)));
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(dash.nodes.len(), 10);
        assert_eq!(dash.model_index.len(), 10);
    }
}
