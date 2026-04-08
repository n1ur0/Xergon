//! Governance Dashboard for the Xergon Network marketplace.
//!
//! Provides HTML page rendering and API handlers for browsing proposals,
//! voting, treasury visualization, and delegation management.
//!
//! Uses a dark theme consistent with other marketplace pages.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Proposal display for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardProposal {
    pub id: String,
    pub title: String,
    pub category: String,
    pub stage: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub quorum_met: bool,
    pub approval_met: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub description: String,
}

/// Treasury visualization data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryVisual {
    pub total_balance: u64,
    pub available: u64,
    pub locked: u64,
    pub spent_total: u64,
    pub recent_deposits: Vec<(u64, u64)>,
    pub recent_spends: Vec<(u64, u64)>,
}

impl Default for TreasuryVisual {
    fn default() -> Self {
        Self {
            total_balance: 0,
            available: 0,
            locked: 0,
            spent_total: 0,
            recent_deposits: Vec::new(),
            recent_spends: Vec::new(),
        }
    }
}

/// Delegation info for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationInfo {
    pub delegator: String,
    pub delegate: String,
    pub weight: u64,
    pub active: bool,
    pub created_at: u64,
}

/// Activity feed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityItem {
    pub id: String,
    pub activity_type: String,
    pub description: String,
    pub timestamp: u64,
    pub proposal_id: Option<String>,
}

/// Dashboard statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDashboardStats {
    pub total_proposals: u64,
    pub active_proposals: u64,
    pub passed_proposals: u64,
    pub failed_proposals: u64,
    pub participation_rate: f64,
    pub treasury_balance_erg: f64,
    pub total_delegations: u64,
}

/// Vote intent from the UI.
#[derive(Debug, Deserialize)]
pub struct VoteIntent {
    pub proposal_id: String,
    pub vote: String,
    pub voter_address: String,
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ProposalQuery {
    pub stage: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct DelegationQuery {
    pub address: Option<String>,
}

// ---------------------------------------------------------------------------
// Governance Dashboard
// ---------------------------------------------------------------------------

/// Governance dashboard managing proposals, treasury, and activity feed.
pub struct GovernanceDashboard {
    proposals: DashMap<String, DashboardProposal>,
    activity_feed: Mutex<VecDeque<ActivityItem>>,
    activity_counter: AtomicU64,
    max_activity: usize,
    total_proposals: AtomicU64,
    treasury: Arc<RwLock<TreasuryVisual>>,
    delegations: DashMap<String, DelegationInfo>,
}

impl GovernanceDashboard {
    /// Create a new governance dashboard.
    pub fn new() -> Self {
        Self {
            proposals: DashMap::new(),
            activity_feed: Mutex::new(VecDeque::with_capacity(200)),
            activity_counter: AtomicU64::new(0),
            max_activity: 200,
            total_proposals: AtomicU64::new(0),
            treasury: Arc::new(RwLock::new(TreasuryVisual::default())),
            delegations: DashMap::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Proposal management
    // -----------------------------------------------------------------------

    /// Add a proposal to the dashboard.
    pub fn add_proposal(&self, proposal: DashboardProposal) {
        self.total_proposals.fetch_add(1, Ordering::Relaxed);
        self.proposals.insert(proposal.id.clone(), proposal);

        self.record_activity(ActivityItem {
            id: format!("act_{}", self.activity_counter.fetch_add(1, Ordering::Relaxed)),
            activity_type: "proposal_created".to_string(),
            description: "New proposal created".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            proposal_id: None,
        });
    }

    /// Get proposals with optional stage filter.
    pub fn get_proposals(&self, stage_filter: Option<&str>, limit: usize) -> Vec<DashboardProposal> {
        let mut results: Vec<DashboardProposal> = self
            .proposals
            .iter()
            .filter(|e| {
                stage_filter
                    .map(|s| e.value().stage == s)
                    .unwrap_or(true)
            })
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        results
    }

    /// Get a single proposal by ID.
    pub fn get_proposal(&self, id: &str) -> Option<DashboardProposal> {
        self.proposals.get(id).map(|r| r.value().clone())
    }

    // -----------------------------------------------------------------------
    // Activity feed
    // -----------------------------------------------------------------------

    /// Record an activity event.
    pub fn record_activity(&self, item: ActivityItem) {
        let mut feed = self.activity_feed.lock().unwrap_or_else(|e| e.into_inner());
        if feed.len() >= self.max_activity {
            feed.pop_front();
        }
        feed.push_back(item);
    }

    /// Get recent activity items.
    pub fn get_activity_feed(&self, limit: usize) -> Vec<ActivityItem> {
        let feed = self.activity_feed.lock().unwrap_or_else(|e| e.into_inner());
        feed.iter().rev().take(limit).cloned().collect()
    }

    // -----------------------------------------------------------------------
    // Voting
    // -----------------------------------------------------------------------

    /// Submit a vote on a proposal.
    pub fn submit_vote(&self, intent: &VoteIntent) -> Result<String, String> {
        let mut proposal = self
            .proposals
            .get_mut(&intent.proposal_id)
            .ok_or("Proposal not found")?;

        if proposal.stage == "executed" || proposal.stage == "closed" || proposal.stage == "expired" {
            return Err("Proposal is finalized".to_string());
        }

        match intent.vote.as_str() {
            "for" => proposal.votes_for += 1,
            "against" => proposal.votes_against += 1,
            _ => return Err("Invalid vote direction".to_string()),
        }

        proposal.quorum_met = (proposal.votes_for + proposal.votes_against) >= 10;
        let total = proposal.votes_for + proposal.votes_against;
        proposal.approval_met = total > 0 && (proposal.votes_for * 100 / total) >= 60;

        self.record_activity(ActivityItem {
            id: format!("act_{}", self.activity_counter.fetch_add(1, Ordering::Relaxed)),
            activity_type: "vote_cast".to_string(),
            description: format!(
                "{} voted {} on proposal {}",
                &intent.voter_address, &intent.vote, &intent.proposal_id
            ),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            proposal_id: Some(intent.proposal_id.clone()),
        });

        info!(
            proposal_id = %intent.proposal_id,
            vote = %intent.vote,
            "Vote recorded in dashboard"
        );
        Ok("Vote recorded".to_string())
    }

    // -----------------------------------------------------------------------
    // Treasury
    // -----------------------------------------------------------------------

    /// Update treasury visualization data.
    pub async fn update_treasury(&self, visual: TreasuryVisual) {
        *self.treasury.write().await = visual;
    }

    /// Get treasury visualization data.
    pub async fn get_treasury(&self) -> TreasuryVisual {
        self.treasury.read().await.clone()
    }

    // -----------------------------------------------------------------------
    // Delegations
    // -----------------------------------------------------------------------

    /// Add a delegation.
    pub fn add_delegation(&self, info: DelegationInfo) {
        let key = format!("{}:{}", info.delegator, info.delegate);
        self.delegations.insert(key, info);
    }

    /// Get delegations for an address.
    pub fn get_delegations(&self, address: &str) -> Vec<DelegationInfo> {
        self.delegations
            .iter()
            .filter(|e| {
                e.value().delegator == address || e.value().delegate == address
            })
            .map(|e| e.value().clone())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    /// Get dashboard statistics.
    pub async fn get_stats(&self) -> GovernanceDashboardStats {
        let total = self.total_proposals.load(Ordering::Relaxed);
        let active = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "created" || e.value().stage == "voting")
            .count() as u64;
        let passed = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "executed")
            .count() as u64;
        let failed = self
            .proposals
            .iter()
            .filter(|e| e.value().stage == "closed" || e.value().stage == "expired")
            .count() as u64;

        let treasury = self.treasury.read().await;
        let balance_erg = treasury.total_balance as f64 / 1_000_000_000.0;

        let total_delegations = self
            .delegations
            .iter()
            .filter(|e| e.value().active)
            .count() as u64;

        let participation = if total > 0 {
            (active as f64 / total as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        GovernanceDashboardStats {
            total_proposals: total,
            active_proposals: active,
            passed_proposals: passed,
            failed_proposals: failed,
            participation_rate: participation,
            treasury_balance_erg: balance_erg,
            total_delegations,
        }
    }
}

impl Default for GovernanceDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTML Page
// ---------------------------------------------------------------------------

/// Generate the governance dashboard HTML page.
pub fn dashboard_page(stats: &GovernanceDashboardStats, proposals: &[DashboardProposal]) -> String {
    let mut proposal_cards = String::new();
    for p in proposals.iter().take(20) {
        let stage_color = match p.stage.as_str() {
            "executed" => "#10b981",
            "voting" => "#3b82f6",
            "created" => "#f59e0b",
            "closed" => "#ef4444",
            "expired" => "#6b7280",
            _ => "#6b7280",
        };
        proposal_cards.push_str(&format!(r#"
        <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;margin-bottom:12px;">
          <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
            <span style="font-weight:600;color:#e5e5e5;font-size:14px;">{title}</span>
            <span style="background:{color};color:#000;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600;text-transform:uppercase;">{stage}</span>
          </div>
          <div style="color:#737373;font-size:12px;margin-bottom:8px;">{category} &middot; {votes_for} for / {votes_against} against</div>
          <div style="display:flex;gap:16px;font-size:12px;">
            <span style="color:{q_color};">Quorum: {quorum}</span>
            <span style="color:{a_color};">Approval: {approval}</span>
          </div>
        </div>"#,
            title = html_escape(&p.title),
            stage = html_escape(&p.stage),
            color = stage_color,
            category = html_escape(&p.category),
            votes_for = p.votes_for,
            votes_against = p.votes_against,
            quorum = if p.quorum_met { "MET" } else { "NOT MET" },
            q_color = if p.quorum_met { "#10b981" } else { "#ef4444" },
            approval = if p.approval_met { "MET" } else { "NOT MET" },
            a_color = if p.approval_met { "#10b981" } else { "#ef4444" },
        ));
    }

    if proposal_cards.is_empty() {
        proposal_cards = r#"<div style="text-align:center;color:#737373;padding:40px;">No proposals yet</div>"#.to_string();
    }

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Xergon Governance</title>
</head>
<body style="background:#0a0a0a;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;margin:0;padding:20px;">
<div style="max-width:1200px;margin:0 auto;">
  <h1 style="font-size:24px;font-weight:700;margin-bottom:4px;">Xergon Governance</h1>
  <p style="color:#737373;font-size:13px;margin-bottom:24px;">On-chain proposal management and treasury oversight</p>

  <!-- Stats Bar -->
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:24px;">
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Total Proposals</div>
      <div style="font-size:24px;font-weight:700;color:#10b981;">{total}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Active</div>
      <div style="font-size:24px;font-weight:700;color:#3b82f6;">{active}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Passed</div>
      <div style="font-size:24px;font-weight:700;color:#10b981;">{passed}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Failed</div>
      <div style="font-size:24px;font-weight:700;color:#ef4444;">{failed}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Treasury</div>
      <div style="font-size:24px;font-weight:700;color:#f59e0b;">{treasury} ERG</div>
    </div>
  </div>

  <!-- Main Grid -->
  <div style="display:grid;grid-template-columns:2fr 1fr;gap:20px;">
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">Proposals</h2>
      {proposal_cards}
    </div>
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">Activity Feed</h2>
      <div id="activity-feed" style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;min-height:200px;">
        <div style="color:#737373;font-size:13px;">Loading activity...</div>
      </div>
    </div>
  </div>
</div>
</body>
</html>"#,
        total = stats.total_proposals,
        active = stats.active_proposals,
        passed = stats.passed_proposals,
        failed = stats.failed_proposals,
        treasury = format!("{:.2}", stats.treasury_balance_erg),
        proposal_cards = proposal_cards,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// API Handlers
// ---------------------------------------------------------------------------

/// Shared state type for handlers.
pub type DashboardState = Arc<GovernanceDashboard>;

/// Handler: GET /api/gov/dashboard/stats
pub async fn get_dashboard_stats_handler(
    State(dashboard): State<DashboardState>,
) -> Json<GovernanceDashboardStats> {
    let stats = dashboard.get_stats().await;
    Json(stats)
}

/// Handler: GET /api/gov/dashboard/proposals?stage=&limit=
pub async fn list_proposals_handler(
    State(dashboard): State<DashboardState>,
    Query(query): Query<ProposalQuery>,
) -> Json<Vec<DashboardProposal>> {
    let stage = query.stage.as_deref();
    let limit = query.limit.unwrap_or(50);
    let proposals = dashboard.get_proposals(stage, limit);
    Json(proposals)
}

/// Handler: GET /api/gov/dashboard/proposals/:id
pub async fn get_proposal_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<DashboardProposal>>) {
    let proposal = dashboard.get_proposal(&id);
    let status = if proposal.is_some() { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (status, Json(proposal))
}

/// Handler: GET /api/gov/dashboard/treasury
pub async fn get_treasury_handler(
    State(dashboard): State<DashboardState>,
) -> Json<TreasuryVisual> {
    let treasury = dashboard.get_treasury().await;
    Json(treasury)
}

/// Handler: GET /api/gov/dashboard/activity?limit=
pub async fn get_activity_handler(
    State(dashboard): State<DashboardState>,
    Query(query): Query<ActivityQuery>,
) -> Json<Vec<ActivityItem>> {
    let limit = query.limit.unwrap_or(50);
    let feed = dashboard.get_activity_feed(limit);
    Json(feed)
}

/// Handler: POST /api/gov/dashboard/vote
pub async fn submit_vote_handler(
    State(dashboard): State<DashboardState>,
    Json(intent): Json<VoteIntent>,
) -> (StatusCode, Json<serde_json::Value>) {
    match dashboard.submit_vote(&intent) {
        Ok(msg) => (StatusCode::OK, Json(serde_json::json!({"status": "ok", "message": msg}))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: GET /api/gov/dashboard/delegations?address=
pub async fn get_delegations_handler(
    State(dashboard): State<DashboardState>,
    Query(query): Query<DelegationQuery>,
) -> Json<Vec<DelegationInfo>> {
    let address = query.address.as_deref().unwrap_or("");
    let delegations = dashboard.get_delegations(address);
    Json(delegations)
}

/// Handler: GET /governance (HTML page)
pub async fn dashboard_page_handler(
    State(dashboard): State<DashboardState>,
) -> Html<String> {
    let stats = dashboard.get_stats().await;
    let proposals = dashboard.get_proposals(None, 20);
    let html = dashboard_page(&stats, &proposals);
    Html(html)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn governance_dashboard_routes() -> Router<DashboardState> {
    Router::new()
        .route("/api/gov/dashboard/stats", axum::routing::get(get_dashboard_stats_handler))
        .route("/api/gov/dashboard/proposals", axum::routing::get(list_proposals_handler))
        .route("/api/gov/dashboard/proposals/:id", axum::routing::get(get_proposal_handler))
        .route("/api/gov/dashboard/treasury", axum::routing::get(get_treasury_handler))
        .route("/api/gov/dashboard/activity", axum::routing::get(get_activity_handler))
        .route("/api/gov/dashboard/vote", axum::routing::post(submit_vote_handler))
        .route("/api/gov/dashboard/delegations", axum::routing::get(get_delegations_handler))
        .route("/governance", axum::routing::get(dashboard_page_handler))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dashboard() -> GovernanceDashboard {
        GovernanceDashboard::new()
    }

    fn sample_proposal(id: &str, stage: &str) -> DashboardProposal {
        DashboardProposal {
            id: id.to_string(),
            title: format!("Proposal {}", id),
            category: "protocol_param".to_string(),
            stage: stage.to_string(),
            votes_for: 5,
            votes_against: 2,
            quorum_met: stage == "voting",
            approval_met: stage == "executed",
            created_at: 1000,
            expires_at: 11000,
            description: "Test proposal".to_string(),
        }
    }

    #[test]
    fn test_dashboard_creation() {
        let dash = make_dashboard();
        assert_eq!(dash.total_proposals.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_add_and_get_proposals() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "created"));
        dash.add_proposal(sample_proposal("p2", "voting"));
        let all = dash.get_proposals(None, 100);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_filter_proposals_by_stage() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "created"));
        dash.add_proposal(sample_proposal("p2", "voting"));
        dash.add_proposal(sample_proposal("p3", "executed"));
        let voting = dash.get_proposals(Some("voting"), 100);
        assert_eq!(voting.len(), 1);
        assert_eq!(voting[0].id, "p2");
    }

    #[test]
    fn test_get_proposal_by_id() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "created"));
        let p = dash.get_proposal("p1");
        assert!(p.is_some());
        assert_eq!(p.unwrap().title, "Proposal p1");
        let missing = dash.get_proposal("p999");
        assert!(missing.is_none());
    }

    #[test]
    fn test_activity_feed_recording() {
        let dash = make_dashboard();
        dash.record_activity(ActivityItem {
            id: "a1".into(),
            activity_type: "proposal_created".into(),
            description: "Test".into(),
            timestamp: 1000,
            proposal_id: Some("p1".into()),
        });
        dash.record_activity(ActivityItem {
            id: "a2".into(),
            activity_type: "vote_cast".into(),
            description: "Vote".into(),
            timestamp: 2000,
            proposal_id: None,
        });
        let feed = dash.get_activity_feed(10);
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].id, "a2");
    }

    #[test]
    fn test_activity_feed_limit() {
        let dash = make_dashboard();
        for i in 0..250 {
            dash.record_activity(ActivityItem {
                id: format!("a{}", i),
                activity_type: "test".into(),
                description: "Test".into(),
                timestamp: i as u64,
                proposal_id: None,
            });
        }
        let feed = dash.get_activity_feed(100);
        assert_eq!(feed.len(), 100);
    }

    #[test]
    fn test_treasury_update_and_get() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dash = make_dashboard();
        rt.block_on(async {
            dash.update_treasury(TreasuryVisual {
                total_balance: 1_000_000_000,
                available: 800_000_000,
                locked: 200_000_000,
                spent_total: 50_000_000,
                recent_deposits: vec![(1000, 100_000_000)],
                recent_spends: vec![(2000, 50_000_000)],
            }).await;
            let t = dash.get_treasury().await;
            assert_eq!(t.total_balance, 1_000_000_000);
            assert_eq!(t.available, 800_000_000);
        });
    }

    #[test]
    fn test_delegation_add_and_query() {
        let dash = make_dashboard();
        dash.add_delegation(DelegationInfo {
            delegator: "alice".into(),
            delegate: "bob".into(),
            weight: 500,
            active: true,
            created_at: 1000,
        });
        dash.add_delegation(DelegationInfo {
            delegator: "carol".into(),
            delegate: "bob".into(),
            weight: 300,
            active: true,
            created_at: 2000,
        });
        let bob_delegations = dash.get_delegations("bob");
        assert_eq!(bob_delegations.len(), 2);
        let alice_delegations = dash.get_delegations("alice");
        assert_eq!(alice_delegations.len(), 1);
    }

    #[test]
    fn test_vote_submission() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "voting"));
        let intent = VoteIntent {
            proposal_id: "p1".into(),
            vote: "for".into(),
            voter_address: "alice".into(),
        };
        let result = dash.submit_vote(&intent);
        assert!(result.is_ok());
        let p = dash.get_proposal("p1").unwrap();
        assert_eq!(p.votes_for, 6);
    }

    #[test]
    fn test_vote_on_finalized_proposal() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "executed"));
        let intent = VoteIntent {
            proposal_id: "p1".into(),
            vote: "for".into(),
            voter_address: "alice".into(),
        };
        let result = dash.submit_vote(&intent);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_vote() {
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "voting"));
        let intent = VoteIntent {
            proposal_id: "p1".into(),
            vote: "abstain".into(),
            voter_address: "alice".into(),
        };
        let result = dash.submit_vote(&intent);
        assert!(result.is_err());
    }

    #[test]
    fn test_stats_computation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dash = make_dashboard();
        dash.add_proposal(sample_proposal("p1", "created"));
        dash.add_proposal(sample_proposal("p2", "voting"));
        dash.add_proposal(sample_proposal("p3", "executed"));
        rt.block_on(async {
            dash.update_treasury(TreasuryVisual {
                total_balance: 5_000_000_000,
                ..Default::default()
            }).await;
            let stats = dash.get_stats().await;
            assert_eq!(stats.total_proposals, 3);
            assert_eq!(stats.active_proposals, 2);
            assert_eq!(stats.passed_proposals, 1);
            assert!((stats.treasury_balance_erg - 5.0).abs() < 0.01);
        });
    }

    #[test]
    fn test_concurrent_access() {
        let dash = Arc::new(make_dashboard());
        let mut handles = vec![];
        for i in 0..5 {
            let d = dash.clone();
            handles.push(std::thread::spawn(move || {
                d.add_proposal(sample_proposal(&format!("p{}", i), "created"));
                d.record_activity(ActivityItem {
                    id: format!("a{}", i),
                    activity_type: "test".into(),
                    description: "Test".into(),
                    timestamp: i as u64,
                    proposal_id: None,
                });
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let proposals = dash.get_proposals(None, 100);
        assert_eq!(proposals.len(), 5);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let p = sample_proposal("p1", "voting");
        let json = serde_json::to_string(&p).unwrap();
        let decoded: DashboardProposal = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "p1");
    }

    #[test]
    fn test_stats_serialization() {
        let stats = GovernanceDashboardStats {
            total_proposals: 10,
            active_proposals: 3,
            passed_proposals: 5,
            failed_proposals: 2,
            participation_rate: 30.0,
            treasury_balance_erg: 100.5,
            total_delegations: 4,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: GovernanceDashboardStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_proposals, 10);
    }

    #[test]
    fn test_html_page_generation() {
        let stats = GovernanceDashboardStats {
            total_proposals: 2,
            active_proposals: 1,
            passed_proposals: 1,
            failed_proposals: 0,
            participation_rate: 50.0,
            treasury_balance_erg: 100.0,
            total_delegations: 3,
        };
        let proposals = vec![sample_proposal("p1", "voting"), sample_proposal("p2", "executed")];
        let html = dashboard_page(&stats, &proposals);
        assert!(html.contains("Xergon Governance"));
        assert!(html.contains("Proposal p1"));
        assert!(html.contains("100 ERG"));
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_html_page_empty() {
        let stats = GovernanceDashboardStats {
            total_proposals: 0,
            active_proposals: 0,
            passed_proposals: 0,
            failed_proposals: 0,
            participation_rate: 0.0,
            treasury_balance_erg: 0.0,
            total_delegations: 0,
        };
        let html = dashboard_page(&stats, &[]);
        assert!(html.contains("No proposals yet"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
        );
    }
}
