use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DisputeStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DisputeStatus {
    Opened,
    EvidenceCollection,
    Mediation,
    Voting,
    Resolved,
    Escalated,
    Expired,
}

impl DisputeStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Opened => "Opened",
            Self::EvidenceCollection => "EvidenceCollection",
            Self::Mediation => "Mediation",
            Self::Voting => "Voting",
            Self::Resolved => "Resolved",
            Self::Escalated => "Escalated",
            Self::Expired => "Expired",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Opened" => Some(Self::Opened),
            "EvidenceCollection" => Some(Self::EvidenceCollection),
            "Mediation" => Some(Self::Mediation),
            "Voting" => Some(Self::Voting),
            "Resolved" => Some(Self::Resolved),
            "Escalated" => Some(Self::Escalated),
            "Expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EvidenceType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EvidenceType {
    Text,
    Log,
    Screenshot,
    Contract,
}

impl EvidenceType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text => "Text",
            Self::Log => "Log",
            Self::Screenshot => "Screenshot",
            Self::Contract => "Contract",
        }
    }
}

// ---------------------------------------------------------------------------
// VoteDecision
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VoteDecision {
    ForComplainant,
    ForRespondent,
    Abstain,
}

impl VoteDecision {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ForComplainant => "ForComplainant",
            Self::ForRespondent => "ForRespondent",
            Self::Abstain => "Abstain",
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Evidence {
    pub evidence_id: String,
    pub dispute_id: String,
    pub submitter_id: String,
    pub content: String,
    pub attachment_hash: [u8; 32],
    pub timestamp: DateTime<Utc>,
    pub evidence_type: EvidenceType,
}

impl Evidence {
    pub fn new(
        dispute_id: &str,
        submitter_id: &str,
        content: &str,
        evidence_type: EvidenceType,
    ) -> Self {
        // BLAKE3-style hashing (we use a simple hash for portability since blake3
        // may not be available; the hash is 32 bytes).
        let hash = Self::hash_content(content);
        Self {
            evidence_id: uuid::Uuid::new_v4().to_string(),
            dispute_id: dispute_id.to_string(),
            submitter_id: submitter_id.to_string(),
            content: content.to_string(),
            attachment_hash: hash,
            timestamp: Utc::now(),
            evidence_type,
        }
    }

    /// Simple 32-byte hash of content (stand-in for BLAKE3).
    /// In production, use blake3::hash(content.as_bytes()).into_bytes().
    fn hash_content(content: &str) -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let h1 = hasher.finish();
        let mut hasher2 = DefaultHasher::new();
        h1.hash(&mut hasher2);
        let h2 = hasher2.finish();
        let bytes = h1.to_le_bytes();
        let bytes2 = h2.to_le_bytes();
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&bytes);
        out[8..16].copy_from_slice(&bytes2);
        out[16..24].copy_from_slice(&bytes);
        out[24..32].copy_from_slice(&bytes2);
        out
    }
}

// ---------------------------------------------------------------------------
// VoteRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VoteRecord {
    pub voter_id: String,
    pub decision: VoteDecision,
    pub weight: f64,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}

impl VoteRecord {
    pub fn new(voter_id: &str, decision: VoteDecision, weight: f64, reason: &str) -> Self {
        Self {
            voter_id: voter_id.to_string(),
            decision,
            weight: weight.clamp(0.0, 10.0),
            reason: reason.to_string(),
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Dispute
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Dispute {
    pub dispute_id: String,
    pub transaction_id: String,
    pub complainant_id: String,
    pub respondent_id: String,
    pub reason: String,
    pub evidence: Vec<Evidence>,
    pub status: DisputeStatus,
    pub mediator_id: Option<String>,
    pub votes: HashMap<String, VoteRecord>,
    pub resolution: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
}

impl Dispute {
    pub fn new(
        transaction_id: &str,
        complainant_id: &str,
        respondent_id: &str,
        reason: &str,
        deadline: DateTime<Utc>,
    ) -> Self {
        Self {
            dispute_id: uuid::Uuid::new_v4().to_string(),
            transaction_id: transaction_id.to_string(),
            complainant_id: complainant_id.to_string(),
            respondent_id: respondent_id.to_string(),
            reason: reason.to_string(),
            evidence: Vec::new(),
            status: DisputeStatus::Opened,
            mediator_id: None,
            votes: HashMap::new(),
            resolution: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deadline,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.deadline
    }
}

// ---------------------------------------------------------------------------
// DisputeConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DisputeConfig {
    pub evidence_deadline_hours: i64,
    pub voting_duration_hours: i64,
    pub min_voters: u32,
    pub quorum_pct: u32,
    pub escalation_threshold: f64,
    pub max_evidence_count: usize,
}

impl Default for DisputeConfig {
    fn default() -> Self {
        Self {
            evidence_deadline_hours: 48,
            voting_duration_hours: 72,
            min_voters: 3,
            quorum_pct: 60,
            escalation_threshold: 0.8,
            max_evidence_count: 10,
        }
    }
}

// ---------------------------------------------------------------------------
// DisputeStats
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DisputeStats {
    pub total_disputes: usize,
    pub open_disputes: usize,
    pub resolved_disputes: usize,
    pub escalated_disputes: usize,
    pub expired_disputes: usize,
    pub avg_resolution_hours: f64,
}

// ---------------------------------------------------------------------------
// CreateDisputeRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateDisputeRequest {
    pub transaction_id: String,
    pub complainant_id: String,
    pub respondent_id: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// SubmitEvidenceRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmitEvidenceRequest {
    pub submitter_id: String,
    pub content: String,
    pub evidence_type: String,
}

// ---------------------------------------------------------------------------
// CastVoteRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CastVoteRequest {
    pub voter_id: String,
    pub decision: String,
    pub weight: Option<f64>,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// ResolveDisputeRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResolveDisputeRequest {
    pub resolution: String,
    pub in_favor_of: Option<String>,
}

// ---------------------------------------------------------------------------
// DisputeResolutionEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct DisputeResolutionEngine {
    disputes: DashMap<String, Dispute>,
    evidence: DashMap<String, Vec<Evidence>>,
    config: DashMap<String, DisputeConfig>,
    mediator_pool: DashMap<String, bool>,
}

impl DisputeResolutionEngine {
    pub fn new() -> Self {
        let config = DashMap::new();
        config.insert("default".to_string(), DisputeConfig::default());
        Self {
            disputes: DashMap::new(),
            evidence: DashMap::new(),
            config,
            mediator_pool: DashMap::new(),
        }
    }

    pub fn default() -> Self {
        Self::new()
    }

    pub fn get_config(&self) -> DisputeConfig {
        self.config
            .get("default")
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Create a new dispute.
    pub fn create_dispute(&self, req: &CreateDisputeRequest) -> Result<Dispute, String> {
        let config = self.get_config();
        let deadline =
            Utc::now() + Duration::hours(config.evidence_deadline_hours + config.voting_duration_hours);

        let dispute = Dispute::new(
            &req.transaction_id,
            &req.complainant_id,
            &req.respondent_id,
            &req.reason,
            deadline,
        );

        let id = dispute.dispute_id.clone();
        self.disputes.insert(id.clone(), dispute);
        self.evidence.insert(id.clone(), Vec::new());

        self.disputes
            .get(&id)
            .map(|d| d.clone())
            .ok_or_else(|| "Failed to create dispute".to_string())
    }

    /// Submit evidence for a dispute.
    pub fn submit_evidence(
        &self,
        dispute_id: &str,
        req: &SubmitEvidenceRequest,
    ) -> Result<Evidence, String> {
        let config = self.get_config();

        let evidence_type = match req.evidence_type.as_str() {
            "Text" => EvidenceType::Text,
            "Log" => EvidenceType::Log,
            "Screenshot" => EvidenceType::Screenshot,
            "Contract" => EvidenceType::Contract,
            _ => EvidenceType::Text,
        };

        if let Some(mut dispute) = self.disputes.get_mut(dispute_id) {
            if dispute.is_expired() {
                dispute.status = DisputeStatus::Expired;
                return Err("Dispute has expired".to_string());
            }
            if dispute.status != DisputeStatus::Opened
                && dispute.status != DisputeStatus::EvidenceCollection
            {
                return Err(format!(
                    "Cannot submit evidence in {} status",
                    dispute.status.as_str()
                ));
            }
            if dispute.evidence.len() >= config.max_evidence_count {
                return Err(format!(
                    "Maximum evidence count ({}) reached",
                    config.max_evidence_count
                ));
            }

            // Transition to EvidenceCollection if still Opened
            if dispute.status == DisputeStatus::Opened {
                dispute.status = DisputeStatus::EvidenceCollection;
            }

            let evidence = Evidence::new(dispute_id, &req.submitter_id, &req.content, evidence_type.clone());
            let evidence_clone = evidence.clone();
            dispute.evidence.push(evidence);
            dispute.updated_at = Utc::now();

            if let Some(mut ev_list) = self.evidence.get_mut(dispute_id) {
                ev_list.push(evidence_clone);
            }

            Ok(dispute
                .evidence
                .last()
                .cloned()
                .unwrap_or_else(|| Evidence::new(dispute_id, &req.submitter_id, "", evidence_type)))
        } else {
            Err("Dispute not found".to_string())
        }
    }

    /// Assign a mediator to a dispute.
    pub fn assign_mediator(&self, dispute_id: &str, mediator_id: &str) -> Result<Dispute, String> {
        // Register mediator in pool
        self.mediator_pool.insert(mediator_id.to_string(), true);

        if let Some(mut dispute) = self.disputes.get_mut(dispute_id) {
            if dispute.is_expired() {
                dispute.status = DisputeStatus::Expired;
                return Err("Dispute has expired".to_string());
            }
            dispute.mediator_id = Some(mediator_id.to_string());
            dispute.status = DisputeStatus::Mediation;
            dispute.updated_at = Utc::now();
            Ok(dispute.clone())
        } else {
            Err("Dispute not found".to_string())
        }
    }

    /// Cast a vote on a dispute.
    pub fn cast_vote(&self, dispute_id: &str, req: &CastVoteRequest) -> Result<Dispute, String> {
        let decision = match req.decision.as_str() {
            "ForComplainant" => VoteDecision::ForComplainant,
            "ForRespondent" => VoteDecision::ForRespondent,
            _ => VoteDecision::Abstain,
        };

        let weight = req.weight.unwrap_or(1.0);
        let reason = req.reason.as_deref().unwrap_or("No reason provided");

        if let Some(mut dispute) = self.disputes.get_mut(dispute_id) {
            if dispute.is_expired() {
                dispute.status = DisputeStatus::Expired;
                return Err("Dispute has expired".to_string());
            }
            if dispute.status != DisputeStatus::Voting && dispute.status != DisputeStatus::Mediation {
                return Err(format!("Cannot vote in {} status", dispute.status.as_str()));
            }

            // Transition to Voting if still in Mediation
            if dispute.status == DisputeStatus::Mediation {
                dispute.status = DisputeStatus::Voting;
            }

            let vote = VoteRecord::new(&req.voter_id, decision, weight, reason);
            dispute.votes.insert(req.voter_id.clone(), vote);
            dispute.updated_at = Utc::now();
            Ok(dispute.clone())
        } else {
            Err("Dispute not found".to_string())
        }
    }

    /// Resolve a dispute.
    pub fn resolve(&self, dispute_id: &str, req: &ResolveDisputeRequest) -> Result<Dispute, String> {
        let config = self.get_config();

        if let Some(mut dispute) = self.disputes.get_mut(dispute_id) {
            if dispute.status == DisputeStatus::Resolved {
                return Err("Dispute already resolved".to_string());
            }

            // Check quorum
            let total_votes = dispute.votes.len() as u32;
            if total_votes < config.min_voters {
                return Err(format!(
                    "Minimum {} voters required, got {}",
                    config.min_voters, total_votes
                ));
            }

            // Tally votes
            let mut for_complainant_weight = 0.0f64;
            let mut for_respondent_weight = 0.0f64;
            for vote in dispute.votes.values() {
                match vote.decision {
                    VoteDecision::ForComplainant => for_complainant_weight += vote.weight,
                    VoteDecision::ForRespondent => for_respondent_weight += vote.weight,
                    VoteDecision::Abstain => {}
                }
            }

            let total_weight = for_complainant_weight + for_respondent_weight;
            if total_weight > 0.0 {
                let complainant_ratio = for_complainant_weight / total_weight;
                // Check for escalation
                if complainant_ratio > config.escalation_threshold
                    || (1.0 - complainant_ratio) > config.escalation_threshold
                {
                    dispute.status = DisputeStatus::Escalated;
                    dispute.updated_at = Utc::now();
                    dispute.resolution = Some(format!("Escalated: complainant_ratio={:.2}", complainant_ratio));
                    return Err("Dispute escalated due to high disagreement".to_string());
                }
            }

            dispute.status = DisputeStatus::Resolved;
            dispute.resolution = Some(req.resolution.clone());
            dispute.updated_at = Utc::now();

            Ok(dispute.clone())
        } else {
            Err("Dispute not found".to_string())
        }
    }

    /// Escalate a dispute.
    pub fn escalate(&self, dispute_id: &str) -> Result<Dispute, String> {
        if let Some(mut dispute) = self.disputes.get_mut(dispute_id) {
            dispute.status = DisputeStatus::Escalated;
            dispute.updated_at = Utc::now();
            dispute.resolution = Some("Escalated to higher authority".to_string());
            Ok(dispute.clone())
        } else {
            Err("Dispute not found".to_string())
        }
    }

    /// Get a dispute by ID.
    pub fn get_dispute(&self, dispute_id: &str) -> Option<Dispute> {
        self.disputes.get(dispute_id).map(|d| {
            let mut d = d.clone();
            // Check for deadline expiration
            if d.is_expired() && d.status != DisputeStatus::Resolved && d.status != DisputeStatus::Expired {
                d.status = DisputeStatus::Expired;
            }
            d
        })
    }

    /// List disputes with optional status filter.
    pub fn list_disputes(&self, status: Option<&DisputeStatus>, limit: usize, offset: usize) -> Vec<Dispute> {
        let mut all: Vec<Dispute> = self
            .disputes
            .iter()
            .map(|d| d.value().clone())
            .filter(|d| {
                status.is_none() || status == Some(&d.status)
            })
            .collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.into_iter().skip(offset).take(limit).collect()
    }

    /// Get dispute statistics.
    pub fn get_stats(&self) -> DisputeStats {
        let mut stats = DisputeStats::default();
        let mut total_resolution_hours = 0.0f64;
        let mut resolved_count = 0usize;

        for entry in self.disputes.iter() {
            let d = entry.value();
            stats.total_disputes += 1;
            match d.status {
                DisputeStatus::Opened
                | DisputeStatus::EvidenceCollection
                | DisputeStatus::Mediation
                | DisputeStatus::Voting => stats.open_disputes += 1,
                DisputeStatus::Resolved => {
                    stats.resolved_disputes += 1;
                    let hours = (d.updated_at - d.created_at).num_hours() as f64;
                    total_resolution_hours += hours;
                    resolved_count += 1;
                }
                DisputeStatus::Escalated => stats.escalated_disputes += 1,
                DisputeStatus::Expired => stats.expired_disputes += 1,
            }
        }

        stats.avg_resolution_hours = if resolved_count > 0 {
            total_resolution_hours / resolved_count as f64
        } else {
            0.0
        };

        stats
    }

    /// Check for expired disputes and update their status.
    pub fn check_expired(&self) -> usize {
        let mut count = 0;
        for mut entry in self.disputes.iter_mut() {
            if entry.is_expired()
                && entry.status != DisputeStatus::Resolved
                && entry.status != DisputeStatus::Expired
            {
                entry.status = DisputeStatus::Expired;
                entry.updated_at = Utc::now();
                count += 1;
            }
        }
        count
    }
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

pub async fn create_dispute_handler(
    State(state): State<super::proxy::AppState>,
    Json(req): Json<CreateDisputeRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.create_dispute(&req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn get_dispute_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.get_dispute(&id) {
        Some(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

pub async fn submit_evidence_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitEvidenceRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.submit_evidence(&id, &req) {
        Ok(evidence) => Json(serde_json::to_value(evidence).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn cast_vote_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
    Json(req): Json<CastVoteRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.cast_vote(&id, &req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn resolve_dispute_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
    Json(req): Json<ResolveDisputeRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.resolve(&id, &req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
pub struct DisputeListQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn list_disputes_handler(
    State(state): State<super::proxy::AppState>,
    Query(params): Query<DisputeListQuery>,
) -> Json<serde_json::Value> {
    let status = params.status.as_deref().and_then(DisputeStatus::from_str);
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let disputes = state.dispute_engine.list_disputes(status.as_ref(), limit, offset);
    Json(serde_json::json!({
        "disputes": disputes,
        "limit": limit,
        "offset": offset,
    }))
}

pub async fn dispute_stats_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let stats = state.dispute_engine.get_stats();
    Json(serde_json::to_value(stats).unwrap_or_default())
}

pub async fn dispute_config_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let config = state.dispute_engine.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> DisputeResolutionEngine {
        DisputeResolutionEngine::new()
    }

    fn make_create_req() -> CreateDisputeRequest {
        CreateDisputeRequest {
            transaction_id: "tx-123".to_string(),
            complainant_id: "user-1".to_string(),
            respondent_id: "user-2".to_string(),
            reason: "Service not delivered".to_string(),
        }
    }

    #[test]
    fn test_create_dispute() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Opened);
        assert_eq!(dispute.complainant_id, "user-1");
        assert_eq!(dispute.respondent_id, "user-2");
        assert!(!dispute.dispute_id.is_empty());
    }

    #[test]
    fn test_submit_evidence() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        let req = SubmitEvidenceRequest {
            submitter_id: "user-1".to_string(),
            content: "Proof of non-delivery".to_string(),
            evidence_type: "Text".to_string(),
        };
        let evidence = engine.submit_evidence(&dispute.dispute_id, &req).unwrap();
        assert_eq!(evidence.submitter_id, "user-1");
        assert_eq!(evidence.content, "Proof of non-delivery");
        assert_ne!(evidence.attachment_hash, [0u8; 32]);

        // Check status transitioned
        let updated = engine.get_dispute(&dispute.dispute_id).unwrap();
        assert_eq!(updated.status, DisputeStatus::EvidenceCollection);
    }

    #[test]
    fn test_assign_mediator() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        let result = engine.assign_mediator(&dispute.dispute_id, "mediator-1");
        assert!(result.is_ok());
        let updated = engine.get_dispute(&dispute.dispute_id).unwrap();
        assert_eq!(updated.mediator_id, Some("mediator-1".to_string()));
        assert_eq!(updated.status, DisputeStatus::Mediation);
    }

    #[test]
    fn test_cast_vote() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        engine.assign_mediator(&dispute.dispute_id, "mediator-1").unwrap();

        let vote_req = CastVoteRequest {
            voter_id: "voter-1".to_string(),
            decision: "ForComplainant".to_string(),
            weight: Some(2.0),
            reason: Some("Evidence is clear".to_string()),
        };
        let result = engine.cast_vote(&dispute.dispute_id, &vote_req);
        assert!(result.is_ok());

        let updated = engine.get_dispute(&dispute.dispute_id).unwrap();
        assert_eq!(updated.status, DisputeStatus::Voting);
        assert!(updated.votes.contains_key("voter-1"));
    }

    #[test]
    fn test_resolve_dispute() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        engine.assign_mediator(&dispute.dispute_id, "mediator-1").unwrap();

        // Add minimum votes
        for i in 0..3 {
            engine.cast_vote(&dispute.dispute_id, &CastVoteRequest {
                voter_id: format!("voter-{}", i),
                decision: "ForComplainant".to_string(),
                weight: Some(1.0),
                reason: None,
            }).unwrap();
        }

        let resolve_req = ResolveDisputeRequest {
            resolution: "Refund to complainant".to_string(),
            in_favor_of: Some("user-1".to_string()),
        };
        let result = engine.resolve(&dispute.dispute_id, &resolve_req);
        assert!(result.is_ok());
        let resolved = engine.get_dispute(&dispute.dispute_id).unwrap();
        assert_eq!(resolved.status, DisputeStatus::Resolved);
        assert_eq!(resolved.resolution, Some("Refund to complainant".to_string()));
    }

    #[test]
    fn test_resolve_insufficient_voters() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        // Only 2 votes, min is 3
        engine.cast_vote(&dispute.dispute_id, &CastVoteRequest {
            voter_id: "v1".to_string(),
            decision: "ForComplainant".to_string(),
            weight: None,
            reason: None,
        }).unwrap();
        engine.cast_vote(&dispute.dispute_id, &CastVoteRequest {
            voter_id: "v2".to_string(),
            decision: "ForRespondent".to_string(),
            weight: None,
            reason: None,
        }).unwrap();

        let resolve_req = ResolveDisputeRequest {
            resolution: "test".to_string(),
            in_favor_of: None,
        };
        let result = engine.resolve(&dispute.dispute_id, &resolve_req);
        assert!(result.is_err());
    }

    #[test]
    fn test_escalate() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        let result = engine.escalate(&dispute.dispute_id);
        assert!(result.is_ok());
        let updated = engine.get_dispute(&dispute.dispute_id).unwrap();
        assert_eq!(updated.status, DisputeStatus::Escalated);
    }

    #[test]
    fn test_list_disputes() {
        let engine = make_engine();
        engine.create_dispute(&make_create_req()).unwrap();
        engine.create_dispute(&CreateDisputeRequest {
            transaction_id: "tx-456".to_string(),
            complainant_id: "user-3".to_string(),
            respondent_id: "user-4".to_string(),
            reason: "Bad quality".to_string(),
        }).unwrap();

        let all = engine.list_disputes(None, 10, 0);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_stats() {
        let engine = make_engine();
        engine.create_dispute(&make_create_req()).unwrap();
        let stats = engine.get_stats();
        assert_eq!(stats.total_disputes, 1);
        assert_eq!(stats.open_disputes, 1);
    }

    #[test]
    fn test_dispute_config() {
        let config = DisputeConfig::default();
        assert_eq!(config.evidence_deadline_hours, 48);
        assert_eq!(config.voting_duration_hours, 72);
        assert_eq!(config.min_voters, 3);
        assert_eq!(config.quorum_pct, 60);
        assert_eq!(config.max_evidence_count, 10);
    }

    #[test]
    fn test_max_evidence_count() {
        let engine = make_engine();
        let dispute = engine.create_dispute(&make_create_req()).unwrap();
        let config = engine.get_config();

        for i in 0..config.max_evidence_count {
            engine.submit_evidence(&dispute.dispute_id, &SubmitEvidenceRequest {
                submitter_id: "user-1".to_string(),
                content: format!("Evidence {}", i),
                evidence_type: "Text".to_string(),
            }).unwrap();
        }

        // One more should fail
        let result = engine.submit_evidence(&dispute.dispute_id, &SubmitEvidenceRequest {
            submitter_id: "user-1".to_string(),
            content: "Too much".to_string(),
            evidence_type: "Text".to_string(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_evidence_hash_uniqueness() {
        let e1 = Evidence::new("d1", "u1", "content A", EvidenceType::Text);
        let e2 = Evidence::new("d1", "u1", "content B", EvidenceType::Text);
        assert_ne!(e1.attachment_hash, e2.attachment_hash);
    }

    #[test]
    fn test_dispute_status_from_str() {
        assert_eq!(DisputeStatus::from_str("Opened"), Some(DisputeStatus::Opened));
        assert_eq!(DisputeStatus::from_str("Resolved"), Some(DisputeStatus::Resolved));
        assert_eq!(DisputeStatus::from_str("Invalid"), None);
    }

    #[test]
    fn test_vote_record() {
        let vr = VoteRecord::new("v1", VoteDecision::ForComplainant, 5.0, "Good evidence");
        assert_eq!(vr.weight, 5.0);
        assert_eq!(vr.decision, VoteDecision::ForComplainant);
    }
}
