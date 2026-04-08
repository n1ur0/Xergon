//! Model Lineage Graph — Model lineage and dependency tracking
//!
//! Tracks relationships between models, datasets, training runs,
//! deployments, and evaluations using a directed graph structure.
//!
//! Features:
//! - LineageNode: Model/Dataset/Training/Deployment/Evaluation nodes
//! - LineageEdge: DerivedFrom/TrainedOn/EvaluatedBy/DeployedFrom/ForkedFrom edges
//! - BFS/DFS traversal for ancestor/descendant queries
//! - Cycle detection during edge insertion
//! - Path finding between any two nodes
//! - Graph merge from external sources
//!
//! REST endpoints:
//! - POST /v1/lineage/nodes              — Create a node
//! - GET  /v1/lineage/nodes/{id}         — Get a node
//! - DELETE /v1/lineage/nodes/{id}       — Remove a node
//! - POST /v1/lineage/edges              — Create an edge
//! - GET  /v1/lineage/ancestors/{id}     — Get ancestors of a node
//! - GET  /v1/lineage/descendants/{id}   — Get descendants of a node
//! - GET  /v1/lineage/paths/{from}/{to}  — Find paths between nodes
//! - GET  /v1/lineage/graph/{id}         — Get full subgraph for a node

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Types of nodes in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Model,
    Dataset,
    Training,
    Deployment,
    Evaluation,
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeType::Model => write!(f, "model"),
            NodeType::Dataset => write!(f, "dataset"),
            NodeType::Training => write!(f, "training"),
            NodeType::Deployment => write!(f, "deployment"),
            NodeType::Evaluation => write!(f, "evaluation"),
        }
    }
}

/// Types of edges in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    DerivedFrom,
    TrainedOn,
    EvaluatedBy,
    DeployedFrom,
    ForkedFrom,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::DerivedFrom => write!(f, "derived_from"),
            EdgeType::TrainedOn => write!(f, "trained_on"),
            EdgeType::EvaluatedBy => write!(f, "evaluated_by"),
            EdgeType::DeployedFrom => write!(f, "deployed_from"),
            EdgeType::ForkedFrom => write!(f, "forked_from"),
        }
    }
}

// ---------------------------------------------------------------------------
// LineageNode
// ---------------------------------------------------------------------------

/// A node in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub node_id: String,
    pub node_type: NodeType,
    pub name: String,
    pub version: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub parent_ids: Vec<String>,
}

impl LineageNode {
    /// Create a new lineage node.
    pub fn new(node_id: &str, node_type: NodeType, name: &str, version: &str) -> Self {
        LineageNode {
            node_id: node_id.to_string(),
            node_type,
            name: name.to_string(),
            version: version.to_string(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            parent_ids: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// LineageEdge
// ---------------------------------------------------------------------------

/// A directed edge in the lineage graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub edge_id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: EdgeType,
    pub attributes: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl LineageEdge {
    /// Create a new lineage edge.
    pub fn new(
        edge_id: &str,
        source_id: &str,
        target_id: &str,
        edge_type: EdgeType,
    ) -> Self {
        LineageEdge {
            edge_id: edge_id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            edge_type,
            attributes: HashMap::new(),
            created_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// LineageGraph — main manager
// ---------------------------------------------------------------------------

/// The model lineage graph manager.
///
/// Stores nodes and edges in DashMaps for concurrent access.
/// Supports BFS/DFS traversal, cycle detection, and path finding.
pub struct LineageGraph {
    /// Nodes indexed by node_id.
    nodes: DashMap<String, LineageNode>,
    /// Edges indexed by edge_id.
    edges: DashMap<String, LineageEdge>,
    /// Outgoing edges index: source_id -> Vec<edge_id>
    outgoing: DashMap<String, Vec<String>>,
    /// Incoming edges index: target_id -> Vec<edge_id>
    incoming: DashMap<String, Vec<String>>,
    /// Cache for traversal results (node_id -> set of ancestor IDs)
    ancestors_cache: DashMap<String, HashSet<String>>,
    /// Cache for traversal results (node_id -> set of descendant IDs)
    descendants_cache: DashMap<String, HashSet<String>>,
}

impl LineageGraph {
    /// Create a new empty lineage graph.
    pub fn new() -> Self {
        LineageGraph {
            nodes: DashMap::new(),
            edges: DashMap::new(),
            outgoing: DashMap::new(),
            incoming: DashMap::new(),
            ancestors_cache: DashMap::new(),
            descendants_cache: DashMap::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&self, node: LineageNode) -> Result<(), String> {
        if self.nodes.contains_key(&node.node_id) {
            return Err(format!("Node '{}' already exists", node.node_id));
        }
        self.nodes.insert(node.node_id.clone(), node);
        self.invalidate_caches_for(&String::new()); // broad invalidation
        Ok(())
    }

    /// Add an edge to the graph. Checks for cycle before insertion.
    pub fn add_edge(&self, edge: LineageEdge) -> Result<(), String> {
        if !self.nodes.contains_key(&edge.source_id) {
            return Err(format!("Source node '{}' not found", edge.source_id));
        }
        if !self.nodes.contains_key(&edge.target_id) {
            return Err(format!("Target node '{}' not found", edge.target_id));
        }
        if self.would_create_cycle(&edge.source_id, &edge.target_id) {
            return Err(format!(
                "Edge from '{}' to '{}' would create a cycle",
                edge.source_id, edge.target_id
            ));
        }

        let source_id = edge.source_id.clone();
        let target_id = edge.target_id.clone();
        self.outgoing
            .entry(edge.source_id.clone())
            .or_default()
            .push(edge.edge_id.clone());
        self.incoming
            .entry(edge.target_id.clone())
            .or_default()
            .push(edge.edge_id.clone());
        self.edges.insert(edge.edge_id.clone(), edge);
        self.invalidate_caches_for(&source_id);
        self.invalidate_caches_for(&target_id);
        Ok(())
    }

    /// Remove a node and all its connected edges.
    pub fn remove_node(&self, node_id: &str) -> bool {
        let removed = self.nodes.remove(node_id).is_some();
        if !removed {
            return false;
        }

        // Remove all edges connected to this node
        let edge_ids_to_remove: Vec<String> = self
            .edges
            .iter()
            .filter(|e| e.value().source_id == node_id || e.value().target_id == node_id)
            .map(|e| e.key().clone())
            .collect();

        for edge_id in &edge_ids_to_remove {
            if let Some((_, edge)) = self.edges.remove(edge_id) {
                // Clean up outgoing index
                if let Some(mut out) = self.outgoing.get_mut(&edge.source_id) {
                    out.retain(|id| id != edge_id);
                    if out.is_empty() {
                        drop(out);
                        self.outgoing.remove(&edge.source_id);
                    }
                }
                // Clean up incoming index
                if let Some(mut inc) = self.incoming.get_mut(&edge.target_id) {
                    inc.retain(|id| id != edge_id);
                    if inc.is_empty() {
                        drop(inc);
                        self.incoming.remove(&edge.target_id);
                    }
                }
            }
        }

        self.outgoing.remove(node_id);
        self.incoming.remove(node_id);
        self.invalidate_caches_for(node_id);
        true
    }

    /// Get a node by ID.
    pub fn get_node(&self, node_id: &str) -> Option<LineageNode> {
        self.nodes.get(node_id).map(|n| n.clone())
    }

    /// Get an edge by ID.
    pub fn get_edge(&self, edge_id: &str) -> Option<LineageEdge> {
        self.edges.get(edge_id).map(|e| e.clone())
    }

    /// Get all ancestors of a node (BFS traversal following incoming edges).
    pub fn get_ancestors(&self, node_id: &str) -> Vec<LineageNode> {
        if let Some(cached) = self.ancestors_cache.get(node_id) {
            return cached
                .iter()
                .filter_map(|id| self.nodes.get(id).map(|n| n.clone()))
                .collect();
        }

        let ancestors = self.bfs_traverse(node_id, TraversalDirection::Up);
        let ancestor_ids: HashSet<String> = ancestors.iter().map(|n| n.node_id.clone()).collect();
        self.ancestors_cache
            .insert(node_id.to_string(), ancestor_ids);
        ancestors
    }

    /// Get all descendants of a node (BFS traversal following outgoing edges).
    pub fn get_descendants(&self, node_id: &str) -> Vec<LineageNode> {
        if let Some(cached) = self.descendants_cache.get(node_id) {
            return cached
                .iter()
                .filter_map(|id| self.nodes.get(id).map(|n| n.clone()))
                .collect();
        }

        let descendants = self.bfs_traverse(node_id, TraversalDirection::Down);
        let desc_ids: HashSet<String> = descendants
            .iter()
            .map(|n| n.node_id.clone())
            .collect();
        self.descendants_cache
            .insert(node_id.to_string(), desc_ids);
        descendants
    }

    /// Get the full lineage (ancestors + node + descendants) for a node.
    pub fn get_lineage(&self, node_id: &str) -> LineageResult {
        let node = self.get_node(node_id);
        let ancestors = self.get_ancestors(node_id);
        let descendants = self.get_descendants(node_id);
        let edges: Vec<LineageEdge> = self
            .edges
            .iter()
            .filter(|e| {
                let val = e.value();
                val.source_id == node_id
                    || val.target_id == node_id
                    || ancestors.iter().any(|a| a.node_id == val.source_id)
                    || descendants.iter().any(|d| d.node_id == val.target_id)
            })
            .map(|e| e.value().clone())
            .collect();
        LineageResult {
            node,
            ancestors,
            descendants,
            edges,
        }
    }

    /// Find all paths between two nodes using DFS.
    pub fn find_paths(&self, from_id: &str, to_id: &str) -> Vec<Vec<String>> {
        if !self.nodes.contains_key(from_id) || !self.nodes.contains_key(to_id) {
            return Vec::new();
        }
        if from_id == to_id {
            return vec![vec![from_id.to_string()]];
        }

        let mut all_paths = Vec::new();
        let mut visited = HashSet::new();
        visited.insert(from_id.to_string());
        let mut current_path = vec![from_id.to_string()];
        self.dfs_paths(from_id, to_id, &mut visited, &mut current_path, &mut all_paths);

        all_paths
    }

    /// Get all root nodes (nodes with no incoming edges).
    pub fn get_root_nodes(&self) -> Vec<LineageNode> {
        self.nodes
            .iter()
            .filter(|entry| {
                let has_incoming = self
                    .incoming
                    .get(entry.key())
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                !has_incoming
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Merge another graph into this one. Skips nodes/edges that already exist.
    pub fn merge_graph(&self, other: &LineageGraph) -> MergeResult {
        let mut nodes_added = 0usize;
        let mut nodes_skipped = 0usize;
        let mut edges_added = 0usize;
        let mut edges_skipped = 0usize;
        let mut errors = Vec::new();

        for entry in other.nodes.iter() {
            match self.add_node(entry.value().clone()) {
                Ok(()) => nodes_added += 1,
                Err(e) => {
                    nodes_skipped += 1;
                    errors.push(e);
                }
            }
        }

        for entry in other.edges.iter() {
            match self.add_edge(entry.value().clone()) {
                Ok(()) => edges_added += 1,
                Err(e) => {
                    edges_skipped += 1;
                    errors.push(e);
                }
            }
        }

        MergeResult {
            nodes_added,
            nodes_skipped,
            edges_added,
            edges_skipped,
            errors,
        }
    }

    /// Get the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// List all node IDs.
    pub fn list_node_ids(&self) -> Vec<String> {
        self.nodes.iter().map(|e| e.key().clone()).collect()
    }

    /// Get nodes by type.
    pub fn get_nodes_by_type(&self, node_type: &NodeType) -> Vec<LineageNode> {
        self.nodes
            .iter()
            .filter(|e| &e.value().node_type == node_type)
            .map(|e| e.value().clone())
            .collect()
    }

    // ---- Private helpers ----

    /// BFS traversal in the given direction.
    fn bfs_traverse(&self, start_id: &str, direction: TraversalDirection) -> Vec<LineageNode> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        queue.push_back(start_id.to_string());
        visited.insert(start_id.to_string());

        while let Some(current_id) = queue.pop_front() {
            let neighbor_ids: Vec<String> = match direction {
                TraversalDirection::Up => {
                    // Follow incoming edges (who points to us)
                    self.incoming
                        .get(&current_id)
                        .map(|inc| {
                            inc.iter()
                                .filter_map(|eid| self.edges.get(eid))
                                .map(|e| e.value().source_id.clone())
                                .collect()
                        })
                        .unwrap_or_default()
                }
                TraversalDirection::Down => {
                    // Follow outgoing edges (who we point to)
                    self.outgoing
                        .get(&current_id)
                        .map(|out| {
                            out.iter()
                                .filter_map(|eid| self.edges.get(eid))
                                .map(|e| e.value().target_id.clone())
                                .collect()
                        })
                        .unwrap_or_default()
                }
            };

            for neighbor_id in neighbor_ids {
                if visited.insert(neighbor_id.clone()) {
                    queue.push_back(neighbor_id.clone());
                    if let Some(node) = self.nodes.get(&neighbor_id) {
                        result.push(node.clone());
                    }
                }
            }
        }

        result
    }

    /// DFS path finding.
    fn dfs_paths(
        &self,
        current: &str,
        target: &str,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<String>,
        all_paths: &mut Vec<Vec<String>>,
    ) {
        if current == target {
            all_paths.push(current_path.clone());
            return;
        }

        let neighbor_ids: Vec<String> = self
            .outgoing
            .get(current)
            .map(|out| {
                out.iter()
                    .filter_map(|eid| self.edges.get(eid))
                    .map(|e| e.value().target_id.clone())
                    .collect()
            })
            .unwrap_or_default();

        for neighbor_id in neighbor_ids {
            if visited.insert(neighbor_id.clone()) {
                current_path.push(neighbor_id.clone());
                self.dfs_paths(&neighbor_id, target, visited, current_path, all_paths);
                current_path.pop();
                visited.remove(&neighbor_id);
            }
        }
    }

    /// Check if adding an edge from source to target would create a cycle.
    /// Uses BFS from target following outgoing edges to see if we can reach source.
    fn would_create_cycle(&self, source_id: &str, target_id: &str) -> bool {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(target_id.to_string());
        visited.insert(target_id.to_string());

        while let Some(current) = queue.pop_front() {
            if current == source_id {
                return true;
            }

            let next_ids: Vec<String> = self
                .outgoing
                .get(&current)
                .map(|out| {
                    out.iter()
                        .filter_map(|eid| self.edges.get(eid))
                        .map(|e| e.value().target_id.clone())
                        .collect()
                })
                .unwrap_or_default();

            for next in next_ids {
                if visited.insert(next.clone()) {
                    queue.push_back(next);
                }
            }
        }

        false
    }

    /// Invalidate traversal caches.
    fn invalidate_caches_for(&self, _node_id: &str) {
        // Simple broad invalidation for correctness
        self.ancestors_cache.clear();
        self.descendants_cache.clear();
    }
}

impl Default for LineageGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction for graph traversal.
enum TraversalDirection {
    Up,   // follow incoming edges (ancestors)
    Down, // follow outgoing edges (descendants)
}

/// Result of a lineage query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageResult {
    pub node: Option<LineageNode>,
    pub ancestors: Vec<LineageNode>,
    pub descendants: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
}

/// Result of a graph merge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub nodes_added: usize,
    pub nodes_skipped: usize,
    pub edges_added: usize,
    pub edges_skipped: usize,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// REST request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    pub node_id: String,
    pub node_type: String,
    pub name: String,
    pub version: String,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEdgeRequest {
    pub edge_id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub attributes: Option<HashMap<String, serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /v1/lineage/nodes
async fn create_node_handler(
    State(graph): State<Arc<LineageGraph>>,
    Json(body): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let node_type = parse_node_type(&body.node_type)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    let mut node = LineageNode::new(&body.node_id, node_type, &body.name, &body.version);
    if let Some(meta) = body.metadata {
        node.metadata = meta;
    }

    graph
        .add_node(node.clone())
        .map_err(|e| (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))))?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "node": node }))))
}

/// GET /v1/lineage/nodes/{id}
async fn get_node_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let node = graph.get_node(&node_id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "node": node })))
}

/// DELETE /v1/lineage/nodes/{id}
async fn delete_node_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let removed = graph.remove_node(&node_id);
    if removed {
        Ok(Json(serde_json::json!({ "removed": true })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// POST /v1/lineage/edges
async fn create_edge_handler(
    State(graph): State<Arc<LineageGraph>>,
    Json(body): Json<CreateEdgeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let edge_type = parse_edge_type(&body.edge_type)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))))?;

    let mut edge =
        LineageEdge::new(&body.edge_id, &body.source_id, &body.target_id, edge_type);
    if let Some(attrs) = body.attributes {
        edge.attributes = attrs;
    }

    graph
        .add_edge(edge.clone())
        .map_err(|e| (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))))?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "edge": edge }))))
}

/// GET /v1/lineage/ancestors/{id}
async fn get_ancestors_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !graph.get_node(&node_id).is_some() {
        return Err(StatusCode::NOT_FOUND);
    }
    let ancestors = graph.get_ancestors(&node_id);
    Ok(Json(serde_json::json!({
        "node_id": node_id,
        "ancestors": ancestors,
        "count": ancestors.len(),
    })))
}

/// GET /v1/lineage/descendants/{id}
async fn get_descendants_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !graph.get_node(&node_id).is_some() {
        return Err(StatusCode::NOT_FOUND);
    }
    let descendants = graph.get_descendants(&node_id);
    Ok(Json(serde_json::json!({
        "node_id": node_id,
        "descendants": descendants,
        "count": descendants.len(),
    })))
}

/// GET /v1/lineage/paths/{from}/{to}
async fn get_paths_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path((from_id, to_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let paths = graph.find_paths(&from_id, &to_id);
    Ok(Json(serde_json::json!({
        "from": from_id,
        "to": to_id,
        "paths": paths,
        "count": paths.len(),
    })))
}

/// GET /v1/lineage/graph/{id}
async fn get_graph_handler(
    State(graph): State<Arc<LineageGraph>>,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let lineage = graph.get_lineage(&node_id);
    if lineage.node.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(serde_json::json!(lineage)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_node_type(s: &str) -> Result<NodeType, String> {
    match s.to_lowercase().as_str() {
        "model" => Ok(NodeType::Model),
        "dataset" => Ok(NodeType::Dataset),
        "training" => Ok(NodeType::Training),
        "deployment" => Ok(NodeType::Deployment),
        "evaluation" => Ok(NodeType::Evaluation),
        _ => Err(format!("Unknown node type: '{}'", s)),
    }
}

fn parse_edge_type(s: &str) -> Result<EdgeType, String> {
    match s.to_lowercase().as_str() {
        "derived_from" => Ok(EdgeType::DerivedFrom),
        "trained_on" => Ok(EdgeType::TrainedOn),
        "evaluated_by" => Ok(EdgeType::EvaluatedBy),
        "deployed_from" => Ok(EdgeType::DeployedFrom),
        "forked_from" => Ok(EdgeType::ForkedFrom),
        _ => Err(format!("Unknown edge type: '{}'", s)),
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the model lineage graph router.
pub fn build_lineage_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/lineage/nodes", post(create_node_handler).get(get_node_handler))
        .route("/v1/lineage/nodes/{id}", get(get_node_handler).delete(delete_node_handler))
        .route("/v1/lineage/edges", post(create_edge_handler))
        .route("/v1/lineage/ancestors/{id}", get(get_ancestors_handler))
        .route("/v1/lineage/descendants/{id}", get(get_descendants_handler))
        .route("/v1/lineage/paths/{from}/{to}", get(get_paths_handler))
        .route("/v1/lineage/graph/{id}", get(get_graph_handler))
        .with_state(state.lineage_graph.clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> Arc<LineageGraph> {
        Arc::new(LineageGraph::new())
    }

    #[test]
    fn test_add_and_get_node() {
        let g = make_graph();
        let node = LineageNode::new("m1", NodeType::Model, "Llama 3", "v1.0");
        g.add_node(node.clone()).unwrap();
        let found = g.get_node("m1").unwrap();
        assert_eq!(found.name, "Llama 3");
        assert_eq!(found.node_type, NodeType::Model);
    }

    #[test]
    fn test_add_duplicate_node_fails() {
        let g = make_graph();
        let node = LineageNode::new("m1", NodeType::Model, "Llama 3", "v1.0");
        g.add_node(node).unwrap();
        let dup = LineageNode::new("m1", NodeType::Model, "Llama 3", "v2.0");
        assert!(g.add_node(dup).is_err());
    }

    #[test]
    fn test_add_and_get_edge() {
        let g = make_graph();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "Data", "v1")).unwrap();
        g.add_node(LineageNode::new("m1", NodeType::Model, "Model", "v1")).unwrap();
        let edge = LineageEdge::new("e1", "d1", "m1", EdgeType::TrainedOn);
        g.add_edge(edge).unwrap();
        let found = g.get_edge("e1").unwrap();
        assert_eq!(found.source_id, "d1");
        assert_eq!(found.target_id, "m1");
    }

    #[test]
    fn test_cycle_detection() {
        let g = make_graph();
        g.add_node(LineageNode::new("a", NodeType::Model, "A", "v1")).unwrap();
        g.add_node(LineageNode::new("b", NodeType::Model, "B", "v1")).unwrap();
        g.add_node(LineageNode::new("c", NodeType::Model, "C", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "a", "b", EdgeType::DerivedFrom))
            .unwrap();
        g.add_edge(LineageEdge::new("e2", "b", "c", EdgeType::DerivedFrom))
            .unwrap();
        // Adding c -> a would create a cycle
        let result = g.add_edge(LineageEdge::new("e3", "c", "a", EdgeType::DerivedFrom));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_ancestors() {
        let g = make_graph();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "Data", "v1")).unwrap();
        g.add_node(LineageNode::new("m1", NodeType::Model, "Model", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "d1", "m1", EdgeType::TrainedOn))
            .unwrap();
        let ancestors = g.get_ancestors("m1");
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].node_id, "d1");
    }

    #[test]
    fn test_get_descendants() {
        let g = make_graph();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "Data", "v1")).unwrap();
        g.add_node(LineageNode::new("m1", NodeType::Model, "Model", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "d1", "m1", EdgeType::TrainedOn))
            .unwrap();
        let descendants = g.get_descendants("d1");
        assert_eq!(descendants.len(), 1);
        assert_eq!(descendants[0].node_id, "m1");
    }

    #[test]
    fn test_find_paths() {
        let g = make_graph();
        g.add_node(LineageNode::new("a", NodeType::Model, "A", "v1")).unwrap();
        g.add_node(LineageNode::new("b", NodeType::Model, "B", "v1")).unwrap();
        g.add_node(LineageNode::new("c", NodeType::Model, "C", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "a", "b", EdgeType::DerivedFrom))
            .unwrap();
        g.add_edge(LineageEdge::new("e2", "b", "c", EdgeType::DerivedFrom))
            .unwrap();
        let paths = g.find_paths("a", "c");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], vec!["a", "b", "c"]);
    }

    #[test]
    fn test_find_paths_no_path() {
        let g = make_graph();
        g.add_node(LineageNode::new("a", NodeType::Model, "A", "v1")).unwrap();
        g.add_node(LineageNode::new("b", NodeType::Model, "B", "v1")).unwrap();
        let paths = g.find_paths("a", "b");
        assert!(paths.is_empty());
    }

    #[test]
    fn test_get_root_nodes() {
        let g = make_graph();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "Data", "v1")).unwrap();
        g.add_node(LineageNode::new("m1", NodeType::Model, "Model", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "d1", "m1", EdgeType::TrainedOn))
            .unwrap();
        let roots = g.get_root_nodes();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].node_id, "d1");
    }

    #[test]
    fn test_remove_node_cleans_edges() {
        let g = make_graph();
        g.add_node(LineageNode::new("a", NodeType::Model, "A", "v1")).unwrap();
        g.add_node(LineageNode::new("b", NodeType::Model, "B", "v1")).unwrap();
        g.add_edge(LineageEdge::new("e1", "a", "b", EdgeType::DerivedFrom))
            .unwrap();
        g.remove_node("a");
        assert!(g.get_node("a").is_none());
        assert!(g.get_edge("e1").is_none());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_merge_graph() {
        let g1 = make_graph();
        let g2 = LineageGraph::new();
        g2.add_node(LineageNode::new("x", NodeType::Model, "X", "v1"))
            .unwrap();
        g2.add_node(LineageNode::new("y", NodeType::Model, "Y", "v1"))
            .unwrap();
        g2.add_edge(LineageEdge::new("ex", "x", "y", EdgeType::DerivedFrom))
            .unwrap();

        let result = g1.merge_graph(&g2);
        assert_eq!(result.nodes_added, 2);
        assert_eq!(result.edges_added, 1);
        assert_eq!(g1.node_count(), 2);
    }

    #[test]
    fn test_get_nodes_by_type() {
        let g = make_graph();
        g.add_node(LineageNode::new("m1", NodeType::Model, "M1", "v1")).unwrap();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "D1", "v1")).unwrap();
        g.add_node(LineageNode::new("m2", NodeType::Model, "M2", "v1")).unwrap();
        let models = g.get_nodes_by_type(&NodeType::Model);
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_get_lineage() {
        let g = make_graph();
        g.add_node(LineageNode::new("d1", NodeType::Dataset, "Data", "v1")).unwrap();
        g.add_node(LineageNode::new("m1", NodeType::Model, "Model", "v1")).unwrap();
        g.add_node(LineageNode::new("dep1", NodeType::Deployment, "Deploy", "v1"))
            .unwrap();
        g.add_edge(LineageEdge::new("e1", "d1", "m1", EdgeType::TrainedOn))
            .unwrap();
        g.add_edge(LineageEdge::new("e2", "m1", "dep1", EdgeType::DeployedFrom))
            .unwrap();

        let lineage = g.get_lineage("m1");
        assert!(lineage.node.is_some());
        assert_eq!(lineage.ancestors.len(), 1);
        assert_eq!(lineage.descendants.len(), 1);
        assert_eq!(lineage.edges.len(), 2);
    }

    #[test]
    fn test_edge_to_nonexistent_node_fails() {
        let g = make_graph();
        let edge = LineageEdge::new("e1", "nonexistent", "also_missing", EdgeType::DerivedFrom);
        assert!(g.add_edge(edge).is_err());
    }
}
