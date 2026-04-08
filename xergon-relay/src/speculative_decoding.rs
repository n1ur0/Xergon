use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Completed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftModel {
    pub name: String,
    pub draft_tokens: u32,
    pub temperature: f64,
    pub max_batch_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetModel {
    pub name: String,
}

/// Fixed-point acceptance rate stored as u64 (rate * 10_000).
/// E.g. 0.8472 → 8472.
#[derive(Debug)]
pub struct DraftPair {
    pub draft: DraftModel,
    pub target: TargetModel,
    acceptance_rate_fp: AtomicU64, // fixed-point: value * 10_000
}

impl DraftPair {
    pub fn new(draft: DraftModel, target: TargetModel, acceptance_rate: f64) -> Self {
        let fp = (acceptance_rate.clamp(0.0, 1.0) * 10_000.0).round() as u64;
        Self {
            draft,
            target,
            acceptance_rate_fp: AtomicU64::new(fp),
        }
    }

    pub fn acceptance_rate(&self) -> f64 {
        self.acceptance_rate_fp.load(Ordering::Relaxed) as f64 / 10_000.0
    }

    pub fn set_acceptance_rate(&self, rate: f64) {
        let fp = (rate.clamp(0.0, 1.0) * 10_000.0).round() as u64;
        self.acceptance_rate_fp.store(fp, Ordering::Relaxed);
    }
}

impl Clone for DraftPair {
    fn clone(&self) -> Self {
        Self {
            draft: self.draft.clone(),
            target: self.target.clone(),
            acceptance_rate_fp: AtomicU64::new(self.acceptance_rate_fp.load(Ordering::Relaxed)),
        }
    }
}

impl Serialize for DraftPair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Ser<'a> {
            draft: &'a DraftModel,
            target: &'a TargetModel,
            acceptance_rate: f64,
        }
        Ser {
            draft: &self.draft,
            target: &self.target,
            acceptance_rate: self.acceptance_rate(),
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativeSession {
    pub id: String,
    pub status: SessionStatus,
    pub draft_model: String,
    pub target_model: String,
    pub prompt: String,
    pub draft_tokens_generated: u32,
    pub accepted_tokens: u32,
    pub rejected_tokens: u32,
    pub tokens_saved: u32,
    pub total_latency_ms: u64,
    pub speedup_factor: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculationResult {
    pub session_id: String,
    pub acceptance_rate: f64,
    pub tokens_saved: u32,
    pub speedup_factor: f64,
    pub draft_tokens: u32,
    pub accepted_tokens: u32,
    pub rejected_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodingMetricsSnapshot {
    pub total_sessions: u64,
    pub active_sessions: u64,
    pub avg_acceptance_rate: f64,
    pub total_tokens_saved: u64,
    pub avg_speedup: f64,
    pub total_draft_pairs: u64,
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

pub struct SpeculativeDecodingCoordinator {
    sessions: DashMap<String, SpeculativeSession>,
    draft_pairs: DashMap<String, DraftPair>,
    /// Monotonically increasing total sessions ever created.
    total_sessions: AtomicU64,
    /// Cumulative tokens saved across all completed sessions.
    total_tokens_saved: AtomicU64,
    /// Cumulative sum of speedup factors (for computing average).
    total_speedup_sum: AtomicU64, // fixed-point * 10_000
    /// Cumulative count of sessions that contributed to speedup sum.
    speedup_count: AtomicU64,
}

impl SpeculativeDecodingCoordinator {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            draft_pairs: DashMap::new(),
            total_sessions: AtomicU64::new(0),
            total_tokens_saved: AtomicU64::new(0),
            total_speedup_sum: AtomicU64::new(0),
            speedup_count: AtomicU64::new(0),
        }
    }

    // ---- Draft-pair management ----

    /// Register a new draft-target pair. Returns an error if the draft model
    /// name is already registered.
    pub fn register_draft_pair(&self, pair: DraftPair) -> Result<DraftPair, String> {
        let name = pair.draft.name.clone();
        if self.draft_pairs.contains_key(&name) {
            return Err(format!("draft model '{}' already registered", name));
        }
        self.draft_pairs.insert(name.clone(), pair);
        Ok(self.draft_pairs.get(&name).unwrap().clone())
    }

    /// Remove a draft pair by draft-model name. Returns true if it existed.
    pub fn unregister_draft_pair(&self, draft_model: &str) -> bool {
        self.draft_pairs.remove(draft_model).is_some()
    }

    /// List all registered draft pairs.
    pub fn list_draft_pairs(&self) -> Vec<DraftPair> {
        self.draft_pairs.iter().map(|r| r.value().clone()).collect()
    }

    // ---- Session management ----

    /// Create a new speculative decoding session. The draft-target pair must
    /// already be registered.
    pub fn create_session(
        &self,
        prompt: String,
        draft_model: &str,
        target_model: &str,
    ) -> Result<SpeculativeSession, String> {
        // Validate the pair exists
        let pair = self
            .draft_pairs
            .get(draft_model)
            .ok_or_else(|| format!("draft model '{}' not registered", draft_model))?;

        if pair.target.name != target_model {
            return Err(format!(
                "target model mismatch: draft '{}' is paired with target '{}', not '{}'",
                draft_model, pair.target.name, target_model
            ));
        }

        let now = Utc::now();
        let session = SpeculativeSession {
            id: Uuid::new_v4().to_string(),
            status: SessionStatus::Active,
            draft_model: draft_model.to_string(),
            target_model: target_model.to_string(),
            prompt,
            draft_tokens_generated: 0,
            accepted_tokens: 0,
            rejected_tokens: 0,
            tokens_saved: 0,
            total_latency_ms: 0,
            speedup_factor: 1.0,
            created_at: now,
            updated_at: now,
        };

        self.sessions.insert(session.id.clone(), session.clone());
        self.total_sessions.fetch_add(1, Ordering::Relaxed);

        Ok(session)
    }

    /// Retrieve a session by id.
    pub fn get_session(&self, id: &str) -> Option<SpeculativeSession> {
        self.sessions.get(id).map(|r| r.value().clone())
    }

    /// Cancel an active session. Returns true if it was actually cancelled.
    pub fn cancel_session(&self, id: &str) -> bool {
        if let Some(mut session) = self.sessions.get_mut(id) {
            if session.status == SessionStatus::Active {
                session.status = SessionStatus::Cancelled;
                session.updated_at = Utc::now();
                return true;
            }
        }
        false
    }

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<SpeculativeSession> {
        self.sessions.iter().map(|r| r.value().clone()).collect()
    }

    // ---- Speculation ----

    /// Run a single speculation round on an active session.
    ///
    /// **Mock implementation:**
    /// 1. Read the draft pair's `draft_tokens` and `acceptance_rate`.
    /// 2. Simulate acceptance: each draft token is independently accepted with
    ///    probability equal to the pair's acceptance rate (deterministic for
    ///    testing – we use a simple hash of session_id + round counter so
    ///    results are reproducible given the same inputs).
    /// 3. Speedup is calculated as:
    ///    ```
    ///    verify_overhead = 0.1   // 10 % overhead per accepted token
    ///    speedup = 1.0 / (1.0 - accept_rate + accept_rate * verify_overhead)
    ///    ```
    ///    This is capped at a minimum of 1.0 (no slowdown).
    pub fn speculate(&self, session_id: &str) -> Result<SpeculationResult, String> {
        let (session, pair) = {
            let sess = self
                .sessions
                .get(session_id)
                .ok_or_else(|| format!("session '{}' not found", session_id))?;

            if sess.status != SessionStatus::Active {
                return Err(format!(
                    "session '{}' is not active (status: {:?})",
                    session_id, sess.status
                ));
            }

            let draft_name = sess.draft_model.clone();
            let pair = self
                .draft_pairs
                .get(&draft_name)
                .ok_or_else(|| format!("draft model '{}' not found", draft_name))?;

            (sess.value().clone(), pair.clone())
        };

        let draft_count = pair.draft.draft_tokens;
        let accept_rate = pair.acceptance_rate();

        // Deterministic pseudo-random acceptance using a simple hash.
        // For each of the draft_count tokens, decide accepted or rejected.
        let mut accepted: u32 = 0;
        let mut rejected: u32 = 0;
        let seed = session_id.len() as u64 + session.draft_tokens_generated as u64;
        for i in 0..draft_count {
            // Simple LCG to produce a deterministic value in [0, 1).
            let rand = ((seed.wrapping_mul(6364136223846793005)
                .wrapping_add(1)
                .wrapping_add(i as u64 * 1442695040888963407))
                % 10_000) as f64
                / 10_000.0;
            if rand < accept_rate {
                accepted += 1;
            } else {
                rejected += 1;
            }
        }

        // Tokens saved = accepted (those we didn't need to re-generate from scratch).
        // In a real system each accepted draft token avoids a full forward pass.
        let tokens_saved = accepted;

        // Speedup model:
        //   Without speculation: every token costs 1 unit of work.
        //   With speculation: for each accepted token we pay verify_overhead;
        //     for each rejected token we pay 1 (regenerate).
        //   Cost per speculative token:
        //     expected_cost = accept_rate * verify_overhead + (1 - accept_rate) * 1
        //   Speedup = 1 / expected_cost
        const VERIFY_OVERHEAD: f64 = 0.1;
        let expected_cost = accept_rate * VERIFY_OVERHEAD + (1.0 - accept_rate) * 1.0;
        let speedup = if expected_cost > 0.0 {
            1.0 / expected_cost
        } else {
            1.0
        };

        // Mock latency: 10 ms base + 2 ms per draft token.
        let latency_ms = 10 + draft_count as u64 * 2;

        // Update session.
        if let Some(mut sess) = self.sessions.get_mut(session_id) {
            sess.draft_tokens_generated += draft_count;
            sess.accepted_tokens += accepted;
            sess.rejected_tokens += rejected;
            sess.tokens_saved += tokens_saved;
            sess.total_latency_ms += latency_ms;
            sess.speedup_factor = speedup;
            sess.updated_at = Utc::now();

            // Mark completed if we've done a few rounds (mock heuristic).
            if sess.draft_tokens_generated >= 100 {
                sess.status = SessionStatus::Completed;
                self.total_tokens_saved
                    .fetch_add(sess.tokens_saved as u64, Ordering::Relaxed);
                let speedup_fp = (speedup * 10_000.0).round() as u64;
                self.total_speedup_sum
                    .fetch_add(speedup_fp, Ordering::Relaxed);
                self.speedup_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        Ok(SpeculationResult {
            session_id: session_id.to_string(),
            acceptance_rate: accept_rate,
            tokens_saved,
            speedup_factor: speedup,
            draft_tokens: draft_count,
            accepted_tokens: accepted,
            rejected_tokens: rejected,
        })
    }

    /// Convenience: get the latest speculation result for a session.
    pub fn get_speculation_result(&self, session_id: &str) -> Result<SpeculationResult, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("session '{}' not found", session_id))?;

        if session.draft_tokens_generated == 0 {
            return Err(format!("no speculation runs for session '{}'", session_id));
        }

        let pair = self
            .draft_pairs
            .get(&session.draft_model)
            .ok_or_else(|| format!("draft model '{}' not found", session.draft_model))?;

        let accept_rate = pair.acceptance_rate();

        const VERIFY_OVERHEAD: f64 = 0.1;
        let expected_cost = accept_rate * VERIFY_OVERHEAD + (1.0 - accept_rate) * 1.0;
        let _speedup = if expected_cost > 0.0 {
            1.0 / expected_cost
        } else {
            1.0
        };

        Ok(SpeculationResult {
            session_id: session_id.to_string(),
            acceptance_rate: accept_rate,
            tokens_saved: session.tokens_saved,
            speedup_factor: session.speedup_factor,
            draft_tokens: session.draft_tokens_generated,
            accepted_tokens: session.accepted_tokens,
            rejected_tokens: session.rejected_tokens,
        })
    }

    // ---- Metrics ----

    /// Set the acceptance rate on an existing draft pair.
    pub fn set_acceptance_rate(&self, draft_model: &str, rate: f64) -> bool {
        if let Some(pair) = self.draft_pairs.get(draft_model) {
            pair.set_acceptance_rate(rate);
            true
        } else {
            false
        }
    }

    /// Snapshot of aggregate metrics.
    pub fn get_metrics(&self) -> DecodingMetricsSnapshot {
        let total_sessions = self.total_sessions.load(Ordering::Relaxed);
        let total_tokens_saved = self.total_tokens_saved.load(Ordering::Relaxed);
        let total_pairs = self.draft_pairs.len() as u64;

        // Count active sessions.
        let active_sessions = self
            .sessions
            .iter()
            .filter(|r| r.value().status == SessionStatus::Active)
            .count() as u64;

        // Average acceptance rate across all pairs.
        let avg_acceptance_rate = if total_pairs > 0 {
            let sum: f64 = self
                .draft_pairs
                .iter()
                .map(|r| r.value().acceptance_rate())
                .sum();
            sum / total_pairs as f64
        } else {
            0.0
        };

        // Average speedup across completed sessions.
        let avg_speedup = {
            let count = self.speedup_count.load(Ordering::Relaxed);
            if count > 0 {
                let sum_fp = self.total_speedup_sum.load(Ordering::Relaxed);
                (sum_fp as f64 / 10_000.0) / count as f64
            } else {
                1.0
            }
        };

        DecodingMetricsSnapshot {
            total_sessions,
            active_sessions,
            avg_acceptance_rate,
            total_tokens_saved,
            avg_speedup,
            total_draft_pairs: total_pairs,
        }
    }
}

// ---------------------------------------------------------------------------
// REST request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterPairRequest {
    pub draft: DraftModel,
    pub target: TargetModel,
    pub acceptance_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct RegisterPairResponse {
    pub success: bool,
    pub pair: DraftPairSer,
    pub error: Option<String>,
}

/// Serializable version of DraftPair (acceptance_rate as plain f64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftPairSer {
    pub draft: DraftModel,
    pub target: TargetModel,
    pub acceptance_rate: f64,
}

impl From<&DraftPair> for DraftPairSer {
    fn from(p: &DraftPair) -> Self {
        Self {
            draft: p.draft.clone(),
            target: p.target.clone(),
            acceptance_rate: p.acceptance_rate(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UnregisterPairResponse {
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct ListPairsResponse {
    pub pairs: Vec<DraftPairSer>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub prompt: String,
    pub draft_model: String,
    pub target_model: String,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub success: bool,
    pub session: Option<SpeculativeSession>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CancelSessionResponse {
    pub success: bool,
}

#[derive(Debug, Serialize)]
pub struct GetSessionResponse {
    pub session: Option<SpeculativeSession>,
}

#[derive(Debug, Serialize)]
pub struct SpeculateResponse {
    pub success: bool,
    pub result: Option<SpeculationResult>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    pub sessions: Vec<SpeculativeSession>,
}

#[derive(Debug, Deserialize)]
pub struct SetAcceptanceRateRequest {
    pub acceptance_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct SetAcceptanceRateResponse {
    pub success: bool,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn register_pair(
    State(state): State<AppState>,
    Json(req): Json<RegisterPairRequest>,
) -> (StatusCode, Json<RegisterPairResponse>) {
    let pair = DraftPair::new(req.draft, req.target, req.acceptance_rate);
    let name = pair.draft.name.clone();
    match state.speculative_coordinator.register_draft_pair(pair) {
        Ok(registered) => (
            StatusCode::CREATED,
            Json(RegisterPairResponse {
                success: true,
                pair: DraftPairSer::from(&registered),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(RegisterPairResponse {
                success: false,
                pair: DraftPairSer {
                    draft: DraftModel {
                        name: name.clone(),
                        draft_tokens: 0,
                        temperature: 0.0,
                        max_batch_size: 0,
                    },
                    target: TargetModel { name: String::new() },
                    acceptance_rate: 0.0,
                },
                error: Some(e),
            }),
        ),
    }
}

async fn unregister_pair(
    State(state): State<AppState>,
    Path(draft_model): Path<String>,
) -> (StatusCode, Json<UnregisterPairResponse>) {
    let removed = state.speculative_coordinator.unregister_draft_pair(&draft_model);
    let status = if removed { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (
        status,
        Json(UnregisterPairResponse { success: removed }),
    )
}

async fn list_pairs(
    State(state): State<AppState>,
) -> (StatusCode, Json<ListPairsResponse>) {
    let pairs: Vec<DraftPairSer> = state
        .speculative_coordinator
        .list_draft_pairs()
        .iter()
        .map(DraftPairSer::from)
        .collect();
    (StatusCode::OK, Json(ListPairsResponse { pairs }))
}

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<CreateSessionResponse>) {
    match state
        .speculative_coordinator
        .create_session(req.prompt, &req.draft_model, &req.target_model)
    {
        Ok(session) => (
            StatusCode::CREATED,
            Json(CreateSessionResponse {
                success: true,
                session: Some(session),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(CreateSessionResponse {
                success: false,
                session: None,
                error: Some(e),
            }),
        ),
    }
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<GetSessionResponse>) {
    let session = state.speculative_coordinator.get_session(&id);
    let status = if session.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (
        status,
        Json(GetSessionResponse { session }),
    )
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<CancelSessionResponse>) {
    let cancelled = state.speculative_coordinator.cancel_session(&id);
    let status = if cancelled {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (
        status,
        Json(CancelSessionResponse { success: cancelled }),
    )
}

async fn speculate(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<SpeculateResponse>) {
    match state.speculative_coordinator.speculate(&id) {
        Ok(result) => (
            StatusCode::OK,
            Json(SpeculateResponse {
                success: true,
                result: Some(result),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(SpeculateResponse {
                success: false,
                result: None,
                error: Some(e),
            }),
        ),
    }
}

async fn get_speculation_result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<SpeculateResponse>) {
    match state.speculative_coordinator.get_speculation_result(&id) {
        Ok(result) => (
            StatusCode::OK,
            Json(SpeculateResponse {
                success: true,
                result: Some(result),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(SpeculateResponse {
                success: false,
                result: None,
                error: Some(e),
            }),
        ),
    }
}

async fn list_sessions(
    State(state): State<AppState>,
) -> (StatusCode, Json<ListSessionsResponse>) {
    let sessions = state.speculative_coordinator.list_sessions();
    (StatusCode::OK, Json(ListSessionsResponse { sessions }))
}

async fn get_metrics(
    State(state): State<AppState>,
) -> (StatusCode, Json<DecodingMetricsSnapshot>) {
    let metrics = state.speculative_coordinator.get_metrics();
    (StatusCode::OK, Json(metrics))
}

async fn set_acceptance_rate(
    State(state): State<AppState>,
    Path(draft_model): Path<String>,
    Json(req): Json<SetAcceptanceRateRequest>,
) -> (StatusCode, Json<SetAcceptanceRateResponse>) {
    let updated = state
        .speculative_coordinator
        .set_acceptance_rate(&draft_model, req.acceptance_rate);
    let status = if updated {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (
        status,
        Json(SetAcceptanceRateResponse { success: updated }),
    )
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/speculative/pairs", post(register_pair))
        .route(
            "/api/speculative/pairs/:draft_model",
            delete(unregister_pair),
        )
        .route("/api/speculative/pairs", get(list_pairs))
        .route("/api/speculative/sessions", post(create_session))
        .route(
            "/api/speculative/sessions/:id",
            get(get_session).delete(cancel_session),
        )
        .route(
            "/api/speculative/sessions/:id/speculate",
            post(speculate),
        )
        .route(
            "/api/speculative/sessions/:id/result",
            get(get_speculation_result),
        )
        .route("/api/speculative/sessions", get(list_sessions))
        .route("/api/speculative/metrics", get(get_metrics))
        .route(
            "/api/speculative/pairs/:draft_model/acceptance-rate",
            put(set_acceptance_rate),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_coordinator() -> SpeculativeDecodingCoordinator {
        SpeculativeDecodingCoordinator::new()
    }

    fn make_draft_pair(name: &str, target: &str, rate: f64, tokens: u32) -> DraftPair {
        DraftPair::new(
            DraftModel {
                name: name.to_string(),
                draft_tokens: tokens,
                temperature: 0.8,
                max_batch_size: 32,
            },
            TargetModel {
                name: target.to_string(),
            },
            rate,
        )
    }

    fn register_default_pair(coord: &SpeculativeDecodingCoordinator) {
        let pair = make_draft_pair("draft-smol", "target-big", 0.8, 8);
        coord.register_draft_pair(pair).unwrap();
    }

    // ---- DraftPair unit tests ----

    #[test]
    fn draft_pair_acceptance_rate_round_trip() {
        let pair = make_draft_pair("d", "t", 0.7534, 4);
        assert!((pair.acceptance_rate() - 0.7534).abs() < 0.0001);

        pair.set_acceptance_rate(0.92);
        assert!((pair.acceptance_rate() - 0.92).abs() < 0.0001);
    }

    #[test]
    fn draft_pair_clamps_rate() {
        let pair = make_draft_pair("d", "t", 1.5, 4);
        assert!((pair.acceptance_rate() - 1.0).abs() < 0.0001);

        pair.set_acceptance_rate(-0.3);
        assert!((pair.acceptance_rate() - 0.0).abs() < 0.0001);
    }

    // ---- Pair management tests ----

    #[test]
    fn register_and_list_pairs() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let pairs = coord.list_draft_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].draft.name, "draft-smol");
    }

    #[test]
    fn register_duplicate_pair_fails() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let pair = make_draft_pair("draft-smol", "target-big", 0.5, 4);
        let result = coord.register_draft_pair(pair);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn unregister_pair() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        assert!(coord.unregister_draft_pair("draft-smol"));
        assert!(!coord.unregister_draft_pair("draft-smol")); // already gone
        assert_eq!(coord.list_draft_pairs().len(), 0);
    }

    #[test]
    fn unregister_nonexistent_pair() {
        let coord = make_coordinator();
        assert!(!coord.unregister_draft_pair("nope"));
    }

    #[test]
    fn set_acceptance_rate() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        assert!(coord.set_acceptance_rate("draft-smol", 0.65));
        let pairs = coord.list_draft_pairs();
        assert!((pairs[0].acceptance_rate() - 0.65).abs() < 0.0001);
    }

    #[test]
    fn set_acceptance_rate_nonexistent() {
        let coord = make_coordinator();
        assert!(!coord.set_acceptance_rate("ghost", 0.5));
    }

    // ---- Session lifecycle tests ----

    #[test]
    fn create_session_success() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("hello world".into(), "draft-smol", "target-big")
            .unwrap();
        assert_eq!(session.status, SessionStatus::Active);
        assert_eq!(session.draft_model, "draft-smol");
        assert_eq!(session.target_model, "target-big");
        assert_eq!(session.prompt, "hello world");
        assert_eq!(session.draft_tokens_generated, 0);
        assert_eq!(session.accepted_tokens, 0);
    }

    #[test]
    fn create_session_missing_draft_pair() {
        let coord = make_coordinator();
        let result = coord.create_session("prompt".into(), "nope", "also-nope");
        assert!(result.is_err());
    }

    #[test]
    fn create_session_target_mismatch() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let result = coord.create_session("prompt".into(), "draft-smol", "wrong-target");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("target model mismatch"));
    }

    #[test]
    fn get_session() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("hi".into(), "draft-smol", "target-big")
            .unwrap();
        let fetched = coord.get_session(&session.id).unwrap();
        assert_eq!(fetched.id, session.id);
    }

    #[test]
    fn get_nonexistent_session() {
        let coord = make_coordinator();
        assert!(coord.get_session("nope").is_none());
    }

    #[test]
    fn cancel_session() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("hi".into(), "draft-smol", "target-big")
            .unwrap();
        assert!(coord.cancel_session(&session.id));
        let fetched = coord.get_session(&session.id).unwrap();
        assert_eq!(fetched.status, SessionStatus::Cancelled);
    }

    #[test]
    fn cancel_already_cancelled_session() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("hi".into(), "draft-smol", "target-big")
            .unwrap();
        assert!(coord.cancel_session(&session.id));
        assert!(!coord.cancel_session(&session.id)); // second call
    }

    #[test]
    fn cancel_nonexistent_session() {
        let coord = make_coordinator();
        assert!(!coord.cancel_session("nope"));
    }

    #[test]
    fn list_sessions() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        coord.create_session("a".into(), "draft-smol", "target-big").unwrap();
        coord.create_session("b".into(), "draft-smol", "target-big").unwrap();
        assert_eq!(coord.list_sessions().len(), 2);
    }

    // ---- Speculation tests ----

    #[test]
    fn speculate_updates_session() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();

        let result = coord.speculate(&session.id).unwrap();
        assert_eq!(result.draft_tokens, 8); // draft_tokens from pair config
        assert_eq!(
            result.accepted_tokens + result.rejected_tokens,
            result.draft_tokens
        );

        let updated = coord.get_session(&session.id).unwrap();
        assert_eq!(updated.draft_tokens_generated, 8);
        assert!(updated.total_latency_ms > 0);
    }

    #[test]
    fn speculate_nonexistent_session() {
        let coord = make_coordinator();
        let result = coord.speculate("nope");
        assert!(result.is_err());
    }

    #[test]
    fn speculate_cancelled_session() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();
        coord.cancel_session(&session.id);
        let result = coord.speculate(&session.id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not active"));
    }

    #[test]
    fn speculate_completed_session() {
        let coord = make_coordinator();
        // Use a large draft_tokens so one round exceeds 100 total.
        let pair = make_draft_pair("big-draft", "big-target", 0.9, 120);
        coord.register_draft_pair(pair).unwrap();
        let session = coord
            .create_session("test".into(), "big-draft", "big-target")
            .unwrap();

        let result = coord.speculate(&session.id).unwrap();
        let updated = coord.get_session(&session.id).unwrap();
        assert_eq!(updated.status, SessionStatus::Completed);

        // Second speculation should fail because session is completed.
        let err = coord.speculate(&session.id);
        assert!(err.is_err());
    }

    #[test]
    fn speculation_speedup_formula() {
        // Verify the speedup formula: 1 / (1 - a + a * v)
        let accept_rate = 0.8;
        let verify_overhead = 0.1;
        let expected_cost = (1.0 - accept_rate) * 1.0 + accept_rate * verify_overhead;
        let speedup = 1.0 / expected_cost;
        // (1-0.8)*1 + 0.8*0.1 = 0.2 + 0.08 = 0.28
        assert!((expected_cost - 0.28_f64).abs() < 1e-9);
        assert!((speedup - (1.0 / 0.28)).abs() < 1e-9);
        // speedup ≈ 3.571
        assert!(speedup > 3.5 && speedup < 3.6);
    }

    #[test]
    fn speculation_tokens_saved_equals_accepted() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();

        coord.speculate(&session.id).unwrap();
        let updated = coord.get_session(&session.id).unwrap();
        assert_eq!(updated.tokens_saved, updated.accepted_tokens);
    }

    #[test]
    fn speculation_accumulates_across_rounds() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();

        // Run multiple rounds (8 tokens each, need 13 to hit 100+).
        for _ in 0..13 {
            let result = coord.speculate(&session.id);
            if result.is_err() {
                break; // session completed
            }
        }

        let updated = coord.get_session(&session.id).unwrap();
        assert!(updated.draft_tokens_generated >= 100);
        assert!(updated.accepted_tokens + updated.rejected_tokens >= 100);
    }

    // ---- get_speculation_result tests ----

    #[test]
    fn get_result_before_speculation() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();
        let result = coord.get_speculation_result(&session.id);
        assert!(result.is_err());
    }

    #[test]
    fn get_result_after_speculation() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let session = coord
            .create_session("test".into(), "draft-smol", "target-big")
            .unwrap();

        coord.speculate(&session.id).unwrap();
        let result = coord.get_speculation_result(&session.id).unwrap();
        assert_eq!(result.session_id, session.id);
        assert!(result.tokens_saved > 0 || result.accepted_tokens == 0);
    }

    // ---- Metrics tests ----

    #[test]
    fn initial_metrics() {
        let coord = make_coordinator();
        let m = coord.get_metrics();
        assert_eq!(m.total_sessions, 0);
        assert_eq!(m.active_sessions, 0);
        assert_eq!(m.avg_acceptance_rate, 0.0);
        assert_eq!(m.total_tokens_saved, 0);
        assert_eq!(m.avg_speedup, 1.0);
        assert_eq!(m.total_draft_pairs, 0);
    }

    #[test]
    fn metrics_after_registration() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        let m = coord.get_metrics();
        assert_eq!(m.total_draft_pairs, 1);
        assert!((m.avg_acceptance_rate - 0.8).abs() < 0.0001);
    }

    #[test]
    fn metrics_after_session_creation() {
        let coord = make_coordinator();
        register_default_pair(&coord);
        coord.create_session("hi".into(), "draft-smol", "target-big").unwrap();
        let m = coord.get_metrics();
        assert_eq!(m.total_sessions, 1);
        assert_eq!(m.active_sessions, 1);
    }

    #[test]
    fn metrics_after_session_completion() {
        let coord = make_coordinator();
        let pair = make_draft_pair("d", "t", 0.9, 120);
        coord.register_draft_pair(pair).unwrap();
        let session = coord.create_session("hi".into(), "d", "t").unwrap();
        coord.speculate(&session.id).unwrap(); // completes immediately
        let m = coord.get_metrics();
        assert_eq!(m.active_sessions, 0);
        assert!(m.total_tokens_saved > 0);
        assert!(m.avg_speedup > 1.0);
    }

    #[test]
    fn metrics_avg_acceptance_multiple_pairs() {
        let coord = make_coordinator();
        coord.register_draft_pair(make_draft_pair("d1", "t1", 0.6, 4)).unwrap();
        coord.register_draft_pair(make_draft_pair("d2", "t2", 0.8, 4)).unwrap();
        let m = coord.get_metrics();
        assert!((m.avg_acceptance_rate - 0.7).abs() < 0.0001);
    }
}
