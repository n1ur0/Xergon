use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde::Deserialize;

use crate::{
    advanced_search::{AdvancedSearchEngine, SearchQuery, SearchResponse},
    billing_invoicing::{
        ApplyCreditRequest, BillingEngine, CreateInvoiceRequest, InvoiceStatus, PayInvoiceRequest,
    },
    dispute_resolution::{
        CastVoteRequest, CreateDisputeRequest, DisputeListQuery,
        DisputeStatus, ResolveDisputeRequest, SubmitEvidenceRequest,
        DisputeResolutionEngine,
    },
    earnings_dashboard_v2::{ChartPeriod, EarningsDashboardV2, RequestWithdrawalRequest},
    ergopay_qr::{ErgoPayQrManager, CreateErgoPayQrBody, ErgoPayQrRequest},
    escrow_contracts::{
        CreateEscrowRequest, EscrowListQuery, EscrowStatus,
        FundEscrowRequest, ReleaseFundsRequest, TimeoutCheckBody,
        EscrowManager,
    },
    protocol_health::HealthChecker,
    model_comparison_matrix::{
        CompareRequest, ComparisonMatrix, RecommendRequest,
        UpdateWeightsRequest,
    },
    notifications_v2::{NotificationInbox, SendNotificationRequest},
    og_image_generator::{OgImageConfig, OgImageGenerator},
    provider_portfolio::{
        ProviderPortfolio, ProviderStats, SearchProvidersQuery, UpdateStatsRequest,
    },
    provider_sla_dashboard::SlaDashboard,
    provider_verification_v2::{ProviderVerificationV2, ReviewDocumentRequest, SubmitVerificationRequest},
    reputation_v2::ReputationEngine,
    review_moderation::{ModerationQueue, ModerationRule, ModerationStatus},
    search_v2::{SearchEngineV2, SearchV2Query},
    usage_analytics_pipeline::{AnalyticsBucket, IngestRequest, UsagePipeline},
    treasury_governance::TreasuryGovernanceManager,
    provider_chain_verify::{ProviderChainVerifyState, chain_verify_routes},
};

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AppState {
    pub og_image_generator: Arc<OgImageGenerator>,
    pub advanced_search: Arc<AdvancedSearchEngine>,
    pub provider_sla_dashboard: Arc<SlaDashboard>,
    pub notification_inbox: Arc<NotificationInbox>,
    pub comparison_matrix: Arc<ComparisonMatrix>,
    pub provider_portfolio: Arc<ProviderPortfolio>,
    pub billing_engine: Arc<BillingEngine>,
    pub earnings_dashboard: Arc<EarningsDashboardV2>,
    pub usage_pipeline: Arc<UsagePipeline>,
    pub search_v2: Arc<SearchEngineV2>,
    pub moderation_queue: Arc<ModerationQueue>,
    pub provider_verification: Arc<ProviderVerificationV2>,
    pub reputation_engine: Arc<ReputationEngine>,
    pub dispute_engine: Arc<DisputeResolutionEngine>,
    pub escrow_manager: Arc<EscrowManager>,
    pub health_checker: Arc<HealthChecker>,
    pub ergopay_qr_manager: Arc<ErgoPayQrManager>,
    pub treasury_governance: Arc<TreasuryGovernanceManager>,
}

// ---------------------------------------------------------------------------
// build_router
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    let og_routes = Router::new()
        .route("/v1/og/metadata/:page_type/:id", get(og_metadata_handler))
        .route("/v1/og/image/:page_type/:id", get(og_image_handler))
        .route("/v1/og/templates", get(og_templates_handler))
        .route("/v1/og/cache/invalidate/:key", post(og_cache_invalidate_handler))
        .route("/v1/og/cache/list", get(og_cache_list_handler));

    let search_routes = Router::new()
        .route("/v1/search", post(search_handler))
        .route("/v1/search/suggestions", get(search_suggestions_handler))
        .route("/v1/search/popular", get(search_popular_handler))
        .route("/v1/search/filters/schema", get(search_filters_schema_handler))
        .route("/v1/search/index", post(search_index_handler))
        .route("/v1/search/stats", get(search_stats_handler));

    let sla_routes = Router::new()
        .route("/v1/sla/provider/:id", get(sla_provider_handler))
        .route("/v1/sla/level/:id", get(sla_level_handler))
        .route("/v1/sla/dashboard", get(sla_dashboard_handler))
        .route("/v1/sla/violations/:id", get(sla_violations_handler))
        .route("/v1/sla/credits/:id", get(sla_credits_handler))
        .route("/v1/sla/trends/:id", get(sla_trends_handler))
        .route("/v1/sla/config/:id", get(sla_config_handler));

    let notification_routes = Router::new()
        .route("/v1/notifications", get(notifications_list_handler))
        .route("/v1/notifications", post(notification_send_handler))
        .route(
            "/v1/notifications/:id/read",
            put(notification_mark_read_handler),
        )
        .route(
            "/v1/notifications/read-all",
            put(notification_mark_all_read_handler),
        )
        .route(
            "/v1/notifications/unread-count",
            get(notification_unread_count_handler),
        )
        .route(
            "/v1/notifications/preferences",
            get(notification_get_prefs_handler),
        )
        .route(
            "/v1/notifications/preferences",
            put(notification_set_prefs_handler),
        )
        .route(
            "/v1/notifications/:id",
            delete(notification_delete_handler),
        );

    let compare_routes = Router::new()
        .route("/v1/compare", post(compare_handler))
        .route("/v1/compare/models", get(compare_models_handler))
        .route(
            "/v1/compare/result/:comparison_id",
            get(compare_result_handler),
        )
        .route("/v1/compare/dimensions", get(compare_dimensions_handler))
        .route("/v1/compare/weights", put(compare_update_weights_handler))
        .route(
            "/v1/compare/recommend",
            post(compare_recommend_handler),
        );

    let portfolio_routes = Router::new()
        .route(
            "/v1/portfolio/:provider_id",
            get(portfolio_get_handler),
        )
        .route(
            "/v1/portfolio/:provider_id/:section",
            get(portfolio_section_handler),
        )
        .route(
            "/v1/portfolio/:provider_id/stats",
            put(portfolio_update_stats_handler),
        )
        .route("/v1/portfolio/search", get(portfolio_search_handler))
        .route("/v1/portfolio/featured", get(portfolio_featured_handler))
        .route(
            "/v1/portfolio/:provider_id/models",
            get(portfolio_models_handler),
        );

    let billing_routes = Router::new()
        .route("/v1/billing/invoices", post(billing_create_invoice_handler))
        .route("/v1/billing/invoices", get(billing_list_invoices_handler))
        .route(
            "/v1/billing/invoices/:id",
            get(billing_get_invoice_handler),
        )
        .route(
            "/v1/billing/invoices/:id/pay",
            put(billing_pay_invoice_handler),
        )
        .route(
            "/v1/billing/invoices/:id/credit",
            put(billing_apply_credit_handler),
        )
        .route(
            "/v1/billing/statements/:provider_id",
            get(billing_statement_handler),
        )
        .route("/v1/billing/outstanding", get(billing_outstanding_handler))
        .route("/v1/billing/config", get(billing_config_handler));

    let earnings_routes = Router::new()
        .route(
            "/v1/earnings/:provider_id",
            get(earnings_summary_handler),
        )
        .route(
            "/v1/earnings/:provider_id/chart",
            get(earnings_chart_handler),
        )
        .route(
            "/v1/earnings/:provider_id/models",
            get(earnings_top_models_handler),
        )
        .route("/v1/earnings/withdraw", post(earnings_withdraw_handler))
        .route(
            "/v1/earnings/withdrawals/:provider_id",
            get(earnings_withdrawals_handler),
        )
        .route("/v1/earnings/config", get(earnings_config_handler));

    let usage_routes = Router::new()
        .route("/v1/usage/ingest", post(usage_ingest_handler))
        .route("/v1/usage/aggregated", get(usage_aggregated_handler))
        .route("/v1/usage/trends", get(usage_trends_handler))
        .route(
            "/v1/usage/report/:period",
            get(usage_report_handler),
        )
        .route(
            "/v1/usage/rankings/models",
            get(usage_model_rankings_handler),
        )
        .route(
            "/v1/usage/rankings/users",
            get(usage_user_rankings_handler),
        )
        .route("/v1/usage/config", get(usage_config_handler));

    let search_v2_routes = Router::new()
        .route("/v1/search/v2", post(search_v2_handler))
        .route("/v1/search/v2/typeahead", get(search_v2_typeahead_handler))
        .route("/v1/search/v2/facets", get(search_v2_facets_handler))
        .route("/v1/search/v2/suggestions", get(search_v2_suggestions_handler))
        .route("/v1/search/v2/popular", get(search_v2_popular_handler));

    let moderation_routes = Router::new()
        .route("/v1/moderation/queue", get(moderation_queue_handler))
        .route("/v1/moderation/:id/approve", put(moderation_approve_handler))
        .route("/v1/moderation/:id/reject", put(moderation_reject_handler))
        .route("/v1/moderation/:id/flag", post(moderation_flag_handler))
        .route("/v1/moderation/flags", get(moderation_flags_handler))
        .route("/v1/moderation/rules", post(moderation_add_rule_handler))
        .route("/v1/moderation/stats", get(moderation_stats_handler));

    let verification_routes = Router::new()
        .route("/v1/verification/submit", post(verification_submit_handler))
        .route(
            "/v1/verification/:provider_id",
            get(verification_get_handler),
        )
        .route(
            "/v1/verification/:provider_id/review",
            put(verification_review_handler),
        )
        .route("/v1/verification/pending", get(verification_pending_handler))
        .route("/v1/verification/levels", get(verification_levels_handler))
        .route("/v1/verification/stats", get(verification_stats_handler));

    let reputation_routes = Router::new()
        .route("/v1/reputation/:provider_id", get(reputation_get_handler))
        .route("/v1/reputation/:provider_id/tier", get(reputation_tier_handler))
        .route("/v1/reputation/leaderboard", get(reputation_leaderboard_handler))
        .route("/v1/reputation/recalculate", post(reputation_recalculate_handler))
        .route("/v1/reputation/history/:provider_id", get(reputation_history_handler))
        .route("/v1/reputation/config", get(reputation_config_handler));

    let dispute_routes = Router::new()
        .route("/v1/disputes", post(dispute_create_handler))
        .route("/v1/disputes/:id", get(dispute_get_handler))
        .route("/v1/disputes/:id/evidence", post(dispute_evidence_handler))
        .route("/v1/disputes/:id/vote", post(dispute_vote_handler))
        .route("/v1/disputes/:id/resolve", post(dispute_resolve_handler))
        .route("/v1/disputes", get(dispute_list_handler))
        .route("/v1/disputes/stats", get(dispute_stats_handler))
        .route("/v1/disputes/config", get(dispute_config_handler));

    let escrow_routes = Router::new()
        .route("/v1/escrow/create", post(escrow_create_handler))
        .route("/v1/escrow/:id/fund", post(escrow_fund_handler))
        .route("/v1/escrow/:id/release", post(escrow_release_handler))
        .route("/v1/escrow/:id/refund", post(escrow_refund_handler))
        .route("/v1/escrow/:id/dispute", post(escrow_dispute_handler))
        .route("/v1/escrow/:id", get(escrow_get_handler))
        .route("/v1/escrow", get(escrow_list_handler))
        .route("/v1/escrow/:id/history", get(escrow_history_handler))
        .route("/v1/escrow/balance", get(escrow_balance_handler))
        .route("/v1/escrow/timeout-check", post(escrow_timeout_check_handler))
        .route("/v1/escrow/config", get(escrow_config_handler));

    let health_routes = Router::new()
        .route("/v1/health", get(health_overall_handler))
        .route("/v1/health/components", get(health_components_handler))
        .route("/v1/health/history", get(health_history_handler));

    let ergopay_routes = Router::new()
        .route("/v1/ergopay/qr", post(ergopay_create_qr_handler))
        .route("/v1/ergopay/qr/:id", get(ergopay_get_qr_handler))
        .route("/v1/ergopay/status/:id", get(ergopay_status_handler));

    let treasury_routes = Router::new()
        .route("/v1/treasury/snapshot", get(treasury_snapshot_handler))
        .route("/v1/treasury/proposals", get(treasury_proposals_handler))
        .route("/v1/treasury/proposals/:id", get(treasury_proposal_get_handler))
        .route("/v1/treasury/operations", get(treasury_operations_handler))
        .route("/v1/treasury/quorum/:id", get(treasury_quorum_handler))
        .route("/v1/treasury/summary", get(treasury_summary_handler));

    let chain_verify_state = Arc::new(ProviderChainVerifyState::new());
    let chain_verify_routes = chain_verify_routes()
        .with_state(chain_verify_state);

    Router::new()
        .merge(og_routes)
        .merge(search_routes)
        .merge(sla_routes)
        .merge(notification_routes)
        .merge(compare_routes)
        .merge(portfolio_routes)
        .merge(billing_routes)
        .merge(earnings_routes)
        .merge(usage_routes)
        .merge(search_v2_routes)
        .merge(moderation_routes)
        .merge(verification_routes)
        .merge(reputation_routes)
        .merge(dispute_routes)
        .merge(escrow_routes)
        .merge(health_routes)
        .merge(ergopay_routes)
        .merge(treasury_routes)
        .merge(chain_verify_routes)
        .with_state(state)
}

// ---------------------------------------------------------------------------
// OG Image Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct OgMetadataParams {
    page_type: String,
    id: String,
}

async fn og_metadata_handler(
    State(state): State<AppState>,
    Path(params): Path<OgMetadataParams>,
) -> Json<serde_json::Value> {
    let config = OgImageConfig::default();
    let metadata = state
        .og_image_generator
        .generate_metadata(&params.page_type, &params.id, &config);
    Json(serde_json::to_value(metadata).unwrap_or_default())
}

async fn og_image_handler(
    State(state): State<AppState>,
    Path(params): Path<OgMetadataParams>,
) -> Json<serde_json::Value> {
    let cached = state
        .og_image_generator
        .get_cached(&params.page_type, &params.id);

    match cached {
        Some(c) => Json(serde_json::json!({
            "svg": c.svg_markup,
            "cached": true,
        })),
        None => {
            let config = OgImageConfig::default();
            let svg = state.og_image_generator.generate_svg(&config);
            Json(serde_json::json!({
                "svg": svg,
                "cached": false,
            }))
        }
    }
}

async fn og_templates_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let templates = state.og_image_generator.list_templates();
    Json(serde_json::to_value(templates).unwrap_or_default())
}

#[derive(Deserialize)]
struct CacheInvalidateParams {
    key: String,
}

async fn og_cache_invalidate_handler(
    State(state): State<AppState>,
    Path(params): Path<CacheInvalidateParams>,
) -> Json<serde_json::Value> {
    // The key is expected to be "page_type:id"
    let parts: Vec<&str> = params.key.splitn(2, ':').collect();
    let invalidated = if parts.len() == 2 {
        state
            .og_image_generator
            .invalidate_cache(parts[0], parts[1])
    } else {
        false
    };
    Json(serde_json::json!({ "invalidated": invalidated }))
}

async fn og_cache_list_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cached = state.og_image_generator.list_cached();
    let stats = state.og_image_generator.cache_stats();
    Json(serde_json::json!({
        "items": cached,
        "stats": stats,
    }))
}

// ---------------------------------------------------------------------------
// Search Handlers
// ---------------------------------------------------------------------------

async fn search_handler(
    State(state): State<AppState>,
    Json(query): Json<SearchQuery>,
) -> Json<SearchResponse> {
    let response = state.advanced_search.search(&query);
    Json(response)
}

#[derive(Deserialize)]
struct SuggestionParams {
    prefix: Option<String>,
    limit: Option<usize>,
}

async fn search_suggestions_handler(
    State(state): State<AppState>,
    Query(params): Query<SuggestionParams>,
) -> Json<serde_json::Value> {
    let prefix = params.prefix.as_deref().unwrap_or("");
    let limit = params.limit.unwrap_or(10);
    let suggestions = state.advanced_search.get_suggestions(prefix, limit);
    Json(serde_json::json!({ "suggestions": suggestions }))
}

async fn search_popular_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let popular = state.advanced_search.get_popular_searches(20);
    Json(serde_json::json!({ "popular": popular }))
}

async fn search_filters_schema_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let schema = state.advanced_search.get_filter_schema();
    Json(serde_json::json!({ "filters": schema }))
}

#[derive(Deserialize)]
struct IndexAction {
    _action: Option<String>,
}

async fn search_index_handler(
    State(state): State<AppState>,
    Json(_action): Json<IndexAction>,
) -> Json<serde_json::Value> {
    state.advanced_search.reindex();
    let stats = state.advanced_search.get_stats();
    Json(serde_json::json!({
        "status": "reindexed",
        "stats": stats,
    }))
}

async fn search_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.advanced_search.get_stats();
    Json(serde_json::to_value(stats).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// SLA Handlers
// ---------------------------------------------------------------------------

async fn sla_provider_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.provider_sla_dashboard.get_report(&id, 720) {
        Ok(report) => Json(serde_json::to_value(report).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn sla_level_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.provider_sla_dashboard.calculate_sla(&id) {
        Ok(level) => Json(serde_json::json!({
            "provider_id": id,
            "level": level.as_str(),
        })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn sla_dashboard_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let summary = state.provider_sla_dashboard.get_dashboard();
    Json(serde_json::to_value(summary).unwrap_or_default())
}

async fn sla_violations_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.provider_sla_dashboard.get_violations(&id) {
        Ok(violations) => Json(serde_json::json!({ "violations": violations })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct SlaCreditsParams {
    id: String,
    hours: Option<u64>,
}

async fn sla_credits_handler(
    State(state): State<AppState>,
    Path(params): Path<SlaCreditsParams>,
) -> Json<serde_json::Value> {
    let hours = params.hours.unwrap_or(720);
    match state.provider_sla_dashboard.calculate_credits(&params.id, hours) {
        Ok(credits) => Json(serde_json::json!({
            "provider_id": params.id,
            "credits": credits,
            "period_hours": hours,
        })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct SlaTrendsParams {
    id: String,
    limit: Option<usize>,
}

async fn sla_trends_handler(
    State(state): State<AppState>,
    Path(params): Path<SlaTrendsParams>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(50);
    match state.provider_sla_dashboard.get_trends(&params.id, limit) {
        Ok(trends) => Json(serde_json::json!({ "trends": trends })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn sla_config_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.provider_sla_dashboard.get_config(&id) {
        Ok(config) => Json(serde_json::to_value(config).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ---------------------------------------------------------------------------
// Notification Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct NotificationListQuery {
    user_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
    include_read: Option<bool>,
}

async fn notifications_list_handler(
    State(state): State<AppState>,
    Query(params): Query<NotificationListQuery>,
) -> Json<serde_json::Value> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20);
    let include_read = params.include_read.unwrap_or(true);
    let items = state
        .notification_inbox
        .get_inbox(&params.user_id, offset, limit, include_read);
    Json(serde_json::json!({
        "items": items,
        "offset": offset,
        "limit": limit,
    }))
}

async fn notification_send_handler(
    State(state): State<AppState>,
    Json(req): Json<SendNotificationRequest>,
) -> Json<serde_json::Value> {
    let notification = crate::notifications_v2::Notification {
        id: String::new(),
        user_id: req.user_id,
        title: req.title,
        body: req.body,
        channel: req.channel,
        priority: req.priority,
        read: false,
        created_at: chrono::Utc::now(),
        expires_at: req.expires_at,
        metadata: req.metadata,
        action_url: req.action_url,
    };
    match state.notification_inbox.send(notification) {
        Ok(id) => Json(serde_json::json!({ "notification_id": id })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct MarkReadParams {
    id: String,
    user_id: String,
}

async fn notification_mark_read_handler(
    State(state): State<AppState>,
    Query(params): Query<MarkReadParams>,
) -> Json<serde_json::Value> {
    let marked = state
        .notification_inbox
        .mark_read(&params.user_id, &params.id);
    Json(serde_json::json!({ "marked": marked }))
}

#[derive(Deserialize)]
struct MarkAllReadParams {
    user_id: String,
}

async fn notification_mark_all_read_handler(
    State(state): State<AppState>,
    Query(params): Query<MarkAllReadParams>,
) -> Json<serde_json::Value> {
    let count = state.notification_inbox.mark_all_read(&params.user_id);
    Json(serde_json::json!({ "marked_count": count }))
}

#[derive(Deserialize)]
struct UnreadCountParams {
    user_id: String,
}

async fn notification_unread_count_handler(
    State(state): State<AppState>,
    Query(params): Query<UnreadCountParams>,
) -> Json<serde_json::Value> {
    let count = state.notification_inbox.get_unread_count(&params.user_id);
    Json(serde_json::json!({ "unread_count": count }))
}

#[derive(Deserialize)]
struct GetPrefsParams {
    user_id: String,
}

async fn notification_get_prefs_handler(
    State(state): State<AppState>,
    Query(params): Query<GetPrefsParams>,
) -> Json<serde_json::Value> {
    let prefs = state.notification_inbox.get_preferences(&params.user_id);
    Json(serde_json::to_value(prefs).unwrap_or_default())
}

async fn notification_set_prefs_handler(
    _state: State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Use PUT with user_id in preferences object"
    }))
}

#[derive(Deserialize)]
struct DeleteNotificationParams {
    id: String,
    user_id: String,
}

async fn notification_delete_handler(
    State(state): State<AppState>,
    Query(params): Query<DeleteNotificationParams>,
) -> Json<serde_json::Value> {
    let deleted = state
        .notification_inbox
        .delete(&params.user_id, &params.id);
    Json(serde_json::json!({ "deleted": deleted }))
}

// ---------------------------------------------------------------------------
// Comparison Handlers
// ---------------------------------------------------------------------------

async fn compare_handler(
    State(state): State<AppState>,
    Json(req): Json<CompareRequest>,
) -> Json<serde_json::Value> {
    let ids: Vec<&str> = req.model_ids.iter().map(|s| s.as_str()).collect();
    match state.comparison_matrix.compare(&ids) {
        Ok(results) => {
            let comparison_id = results
                .first()
                .map(|r| r.comparison_id.clone())
                .unwrap_or_default();
            Json(serde_json::json!({
                "comparison_id": comparison_id,
                "results": results,
            }))
        }
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn compare_models_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let models = state.comparison_matrix.get_matrix();
    Json(serde_json::json!({ "models": models }))
}

#[derive(Deserialize)]
struct CompareResultParams {
    comparison_id: String,
}

async fn compare_result_handler(
    State(state): State<AppState>,
    Path(params): Path<CompareResultParams>,
) -> Json<serde_json::Value> {
    match state.comparison_matrix.get_result(&params.comparison_id) {
        Some(results) => Json(serde_json::json!({ "results": results })),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

async fn compare_dimensions_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let dims = state.comparison_matrix.get_dimensions();
    let config = state.comparison_matrix.get_public_config();
    Json(serde_json::json!({
        "dimensions": dims.iter().map(|d| d.as_str()).collect::<Vec<_>>(),
        "weights": config.weights,
        "normalization": config.normalization,
    }))
}

async fn compare_update_weights_handler(
    State(state): State<AppState>,
    Json(req): Json<UpdateWeightsRequest>,
) -> Json<serde_json::Value> {
    state.comparison_matrix.update_weights(req.weights);
    let config = state.comparison_matrix.get_public_config();
    Json(serde_json::json!({
        "status": "updated",
        "weights": config.weights,
    }))
}

async fn compare_recommend_handler(
    State(state): State<AppState>,
    Json(req): Json<RecommendRequest>,
) -> Json<serde_json::Value> {
    let ids: Vec<&str> = req.model_ids.iter().map(|s| s.as_str()).collect();
    match state.comparison_matrix.get_recommendation(&ids, &req.use_case) {
        Ok(results) => {
            let comparison_id = results
                .first()
                .map(|r| r.comparison_id.clone())
                .unwrap_or_default();
            Json(serde_json::json!({
                "comparison_id": comparison_id,
                "use_case": req.use_case,
                "results": results,
            }))
        }
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ---------------------------------------------------------------------------
// Portfolio Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PortfolioParams {
    provider_id: String,
}

async fn portfolio_get_handler(
    State(state): State<AppState>,
    Path(params): Path<PortfolioParams>,
) -> Json<serde_json::Value> {
    match state.provider_portfolio.get_portfolio(&params.provider_id) {
        Some(portfolio) => Json(portfolio),
        None => Json(serde_json::json!({ "error": "provider_not_found" })),
    }
}

#[derive(Deserialize)]
struct PortfolioSectionParams {
    provider_id: String,
    section: String,
}

async fn portfolio_section_handler(
    State(state): State<AppState>,
    Path(params): Path<PortfolioSectionParams>,
) -> Json<serde_json::Value> {
    match state
        .provider_portfolio
        .get_section(&params.provider_id, &params.section)
    {
        Ok(section) => Json(section),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn portfolio_update_stats_handler(
    State(state): State<AppState>,
    Path(params): Path<PortfolioParams>,
    Json(req): Json<UpdateStatsRequest>,
) -> Json<serde_json::Value> {
    let existing = state
        .provider_portfolio
        .get_portfolio(&params.provider_id);
    let base = existing
        .and_then(|v| serde_json::from_value::<ProviderStats>(v["stats"].clone()).ok())
        .unwrap_or_default();

    let stats = ProviderStats {
        total_models: req.total_models.unwrap_or(base.total_models),
        total_inferences: req.total_inferences.unwrap_or(base.total_inferences),
        avg_rating: req.avg_rating.unwrap_or(base.avg_rating),
        uptime_pct: req.uptime_pct.unwrap_or(base.uptime_pct),
        total_earnings: req.total_earnings.unwrap_or(base.total_earnings),
        active_since: req.active_since.unwrap_or(base.active_since),
    };

    state.provider_portfolio.update_stats(&params.provider_id, stats);
    Json(serde_json::json!({ "status": "updated" }))
}

async fn portfolio_search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchProvidersQuery>,
) -> Json<serde_json::Value> {
    let query = params.q.as_deref().unwrap_or("");
    let limit = params.limit.unwrap_or(20);
    let results = state.provider_portfolio.search_providers(query, limit);
    Json(serde_json::json!({ "providers": results }))
}

async fn portfolio_featured_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let featured = state.provider_portfolio.get_featured();
    Json(serde_json::json!({ "featured": featured }))
}

async fn portfolio_models_handler(
    State(state): State<AppState>,
    Path(params): Path<PortfolioParams>,
) -> Json<serde_json::Value> {
    let models = state.provider_portfolio.get_models(&params.provider_id);
    Json(serde_json::json!({ "models": models }))
}

// ---------------------------------------------------------------------------
// Billing Handlers
// ---------------------------------------------------------------------------

async fn billing_create_invoice_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateInvoiceRequest>,
) -> Json<serde_json::Value> {
    match state.billing_engine.create_invoice(&req) {
        Ok(invoice) => Json(serde_json::to_value(invoice).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct BillingListQuery {
    provider_id: Option<String>,
    user_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn billing_list_invoices_handler(
    State(state): State<AppState>,
    Query(params): Query<BillingListQuery>,
) -> Json<serde_json::Value> {
    let status = params
        .status
        .as_deref()
        .and_then(|s| match s {
            "Draft" => Some(InvoiceStatus::Draft),
            "Pending" => Some(InvoiceStatus::Pending),
            "Paid" => Some(InvoiceStatus::Paid),
            "Overdue" => Some(InvoiceStatus::Overdue),
            "Cancelled" => Some(InvoiceStatus::Cancelled),
            "Refunded" => Some(InvoiceStatus::Refunded),
            _ => None,
        });
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let invoices = state.billing_engine.list_invoices(
        params.provider_id.as_deref(),
        params.user_id.as_deref(),
        status.as_ref(),
        limit,
        offset,
    );
    Json(serde_json::json!({
        "invoices": invoices,
        "limit": limit,
        "offset": offset,
    }))
}

async fn billing_get_invoice_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.billing_engine.get_invoice(&id) {
        Some(invoice) => Json(serde_json::to_value(invoice).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

async fn billing_pay_invoice_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<PayInvoiceRequest>,
) -> Json<serde_json::Value> {
    match state.billing_engine.mark_paid(&id, req.payment_method.as_deref()) {
        Ok(invoice) => Json(serde_json::to_value(invoice).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn billing_apply_credit_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ApplyCreditRequest>,
) -> Json<serde_json::Value> {
    let reason = req.reason.as_deref().unwrap_or("Credit applied");
    match state.billing_engine.apply_credit(&id, req.amount, reason) {
        Ok(invoice) => Json(serde_json::to_value(invoice).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn billing_statement_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(30);
    match state
        .billing_engine
        .generate_statement(&provider_id, start, now)
    {
        Ok(stmt) => Json(serde_json::to_value(stmt).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn billing_outstanding_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let summary = state.billing_engine.get_outstanding();
    Json(serde_json::to_value(summary).unwrap_or_default())
}

async fn billing_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.billing_engine.get_config() {
        Ok(config) => Json(serde_json::to_value(config).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ---------------------------------------------------------------------------
// Earnings Handlers
// ---------------------------------------------------------------------------

async fn earnings_summary_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.earnings_dashboard.get_summary(&provider_id) {
        Ok(summary) => Json(serde_json::to_value(summary).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct EarningsChartQuery {
    period: Option<String>,
    limit: Option<usize>,
}

async fn earnings_chart_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Query(params): Query<EarningsChartQuery>,
) -> Json<serde_json::Value> {
    let period = match params.period.as_deref() {
        Some("weekly" | "Weekly") => ChartPeriod::Weekly,
        Some("monthly" | "Monthly") => ChartPeriod::Monthly,
        _ => ChartPeriod::Daily,
    };
    let limit = params.limit.unwrap_or(30);
    match state.earnings_dashboard.get_chart(&provider_id, &period, limit) {
        Ok(chart) => Json(serde_json::to_value(chart).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct EarningsModelsQuery {
    limit: Option<usize>,
}

async fn earnings_top_models_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Query(params): Query<EarningsModelsQuery>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(10);
    let models = state.earnings_dashboard.get_top_models(&provider_id, limit);
    Json(serde_json::json!({ "models": models }))
}

async fn earnings_withdraw_handler(
    State(state): State<AppState>,
    Json(req): Json<RequestWithdrawalRequest>,
) -> Json<serde_json::Value> {
    match state.earnings_dashboard.request_withdrawal(&req) {
        Ok(withdrawal) => Json(serde_json::to_value(withdrawal).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn earnings_withdrawals_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    let history = state.earnings_dashboard.get_withdrawal_history(&provider_id, 50);
    Json(serde_json::json!({ "withdrawals": history }))
}

async fn earnings_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.earnings_dashboard.get_config() {
        Ok(config) => Json(serde_json::to_value(config).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ---------------------------------------------------------------------------
// Usage Analytics Handlers
// ---------------------------------------------------------------------------

async fn usage_ingest_handler(
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> Json<serde_json::Value> {
    match state.usage_pipeline.ingest_event(&req) {
        Ok(event) => Json(serde_json::json!({
            "status": "ingested",
            "event_id": event.event_id,
        })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn usage_aggregated_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let daily = state.usage_pipeline.get_aggregated(&AnalyticsBucket::Daily);
    Json(serde_json::json!({
        "aggregated": daily,
    }))
}

async fn usage_trends_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let trends = state.usage_pipeline.get_trends(&AnalyticsBucket::Daily, 30);
    Json(serde_json::json!({ "trends": trends }))
}

async fn usage_report_handler(
    State(state): State<AppState>,
    Path(period): Path<String>,
) -> Json<serde_json::Value> {
    let bucket = AnalyticsBucket::from_str(&period).unwrap_or(AnalyticsBucket::Daily);
    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(30);
    let report = state.usage_pipeline.get_usage_report(&bucket, start, now);
    Json(serde_json::to_value(report).unwrap_or_default())
}

async fn usage_model_rankings_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let rankings = state.usage_pipeline.get_model_rankings(20);
    Json(serde_json::json!({ "rankings": rankings }))
}

async fn usage_user_rankings_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let rankings = state.usage_pipeline.get_user_rankings(20);
    Json(serde_json::json!({ "rankings": rankings }))
}

async fn usage_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match state.usage_pipeline.get_config() {
        Ok(config) => Json(serde_json::to_value(config).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

// ---------------------------------------------------------------------------
// Search V2 Handlers
// ---------------------------------------------------------------------------

async fn search_v2_handler(
    State(state): State<AppState>,
    Json(query): Json<SearchV2Query>,
) -> Json<serde_json::Value> {
    let response = state.search_v2.search(&query);
    state.search_v2.record_suggestion(&query.query, "search");
    Json(serde_json::to_value(response).unwrap_or_default())
}

#[derive(Deserialize)]
struct TypeaheadParams {
    q: Option<String>,
}

async fn search_v2_typeahead_handler(
    State(state): State<AppState>,
    Query(params): Query<TypeaheadParams>,
) -> Json<serde_json::Value> {
    let prefix = params.q.as_deref().unwrap_or("");
    let results = state.search_v2.typeahead(prefix);
    Json(serde_json::json!({ "results": results }))
}

async fn search_v2_facets_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let facets = state.search_v2.get_facets();
    Json(serde_json::json!({ "facets": facets }))
}

#[derive(Deserialize)]
struct SearchV2SuggestionParams {
    prefix: Option<String>,
    limit: Option<usize>,
}

async fn search_v2_suggestions_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchV2SuggestionParams>,
) -> Json<serde_json::Value> {
    let prefix = params.prefix.as_deref().unwrap_or("");
    let limit = params.limit.unwrap_or(10);
    let suggestions = state.search_v2.get_suggestions(prefix, limit);
    Json(serde_json::json!({ "suggestions": suggestions }))
}

async fn search_v2_popular_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let popular = state.search_v2.get_popular(20);
    Json(serde_json::json!({ "popular": popular }))
}

// ---------------------------------------------------------------------------
// Moderation Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ModerationQueueParams {
    status: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn moderation_queue_handler(
    State(state): State<AppState>,
    Query(params): Query<ModerationQueueParams>,
) -> Json<serde_json::Value> {
    let status = params.status.as_deref().and_then(ModerationStatus::from_str);
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let queue = state.moderation_queue.get_queue(status.as_ref(), limit, offset);
    Json(serde_json::json!({ "queue": queue }))
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ModerationActionParams {
    id: String,
    moderator_id: Option<String>,
    reason: Option<String>,
}

async fn moderation_approve_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ModerationActionParams>,
) -> Json<serde_json::Value> {
    let moderator_id = params.moderator_id.as_deref().unwrap_or("anonymous");
    let reason = params.reason.as_deref().unwrap_or("Approved by moderator");
    let ok = state.moderation_queue.approve(&id, moderator_id, reason);
    Json(serde_json::json!({ "approved": ok }))
}

async fn moderation_reject_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<ModerationActionParams>,
) -> Json<serde_json::Value> {
    let moderator_id = params.moderator_id.as_deref().unwrap_or("anonymous");
    let reason = params.reason.as_deref().unwrap_or("Rejected by moderator");
    let ok = state.moderation_queue.reject(&id, moderator_id, reason);
    Json(serde_json::json!({ "rejected": ok }))
}

#[derive(Deserialize)]
struct FlagReviewBody {
    reason: String,
    reporter_id: String,
}

async fn moderation_flag_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<FlagReviewBody>,
) -> Json<serde_json::Value> {
    match state.moderation_queue.flag(&id, &body.reason, &body.reporter_id) {
        Some(flag_id) => Json(serde_json::json!({ "flag_id": flag_id })),
        None => Json(serde_json::json!({ "error": "review_not_found" })),
    }
}

#[derive(Deserialize)]
struct ModerationFlagsParams {
    review_id: Option<String>,
    resolved: Option<bool>,
}

async fn moderation_flags_handler(
    State(state): State<AppState>,
    Query(params): Query<ModerationFlagsParams>,
) -> Json<serde_json::Value> {
    let flags = state.moderation_queue.get_flags(params.review_id.as_deref(), params.resolved);
    Json(serde_json::json!({ "flags": flags }))
}

async fn moderation_add_rule_handler(
    State(state): State<AppState>,
    Json(rule): Json<ModerationRule>,
) -> Json<serde_json::Value> {
    let id = state.moderation_queue.add_rule(rule);
    Json(serde_json::json!({ "rule_id": id }))
}

async fn moderation_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.moderation_queue.get_stats();
    Json(stats)
}

// ---------------------------------------------------------------------------
// Verification Handlers
// ---------------------------------------------------------------------------

async fn verification_submit_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitVerificationRequest>,
) -> Json<serde_json::Value> {
    match state.provider_verification.submit_verification(&req) {
        Ok(provider_id) => Json(serde_json::json!({
            "status": "submitted",
            "provider_id": provider_id,
        })),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn verification_get_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.provider_verification.get_verification(&provider_id) {
        Some(record) => Json(serde_json::to_value(record).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

#[derive(Deserialize)]
struct VerificationReviewBody {
    doc_id: String,
    approved: bool,
    reviewer_id: String,
    notes: Option<String>,
}

async fn verification_review_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(body): Json<VerificationReviewBody>,
) -> Json<serde_json::Value> {
    let review_req = ReviewDocumentRequest {
        doc_id: body.doc_id,
        approved: body.approved,
        reviewer_id: body.reviewer_id,
        notes: body.notes,
    };
    match state.provider_verification.review_document(&review_req) {
        Ok(_) => {
            // If all docs approved, auto-approve verification
            let record = state.provider_verification.get_verification(&provider_id);
            if let Some(rec) = record {
                let all_approved = rec.document_ids.iter().all(|did| {
                    state
                        .provider_verification
                        .get_document(did)
                        .map(|d| d.status.as_str() == "Approved")
                        .unwrap_or(true)
                });
                if all_approved {
                    let _ = state.provider_verification.approve_verification(
                        &provider_id,
                        &review_req.reviewer_id,
                        "All documents approved",
                    );
                }
            }
            Json(serde_json::json!({ "status": "reviewed" }))
        }
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct VerificationPendingParams {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn verification_pending_handler(
    State(state): State<AppState>,
    Query(params): Query<VerificationPendingParams>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let pending = state.provider_verification.get_pending(limit, offset);
    Json(serde_json::json!({ "pending": pending }))
}

async fn verification_levels_handler(
    State(_state): State<AppState>,
) -> Json<serde_json::Value> {
    let levels = ProviderVerificationV2::get_all_levels();
    Json(serde_json::json!({ "levels": levels }))
}

async fn verification_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.provider_verification.get_stats();
    Json(stats)
}

// ---------------------------------------------------------------------------
// Reputation Handlers
// ---------------------------------------------------------------------------

async fn reputation_get_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.reputation_engine.get_score(&provider_id) {
        Some(score) => Json(serde_json::to_value(score).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "provider_not_found" })),
    }
}

async fn reputation_tier_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.reputation_engine.get_tier(&provider_id) {
        Some(tier) => Json(serde_json::json!({
            "provider_id": provider_id,
            "tier": tier.as_str(),
            "level": tier.level(),
        })),
        None => Json(serde_json::json!({ "error": "provider_not_found" })),
    }
}

async fn reputation_leaderboard_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let leaderboard = state.reputation_engine.get_leaderboard(50);
    let entries: Vec<serde_json::Value> = leaderboard
        .into_iter()
        .enumerate()
        .map(|(i, (pid, score, tier))| {
            serde_json::json!({
                "rank": i + 1,
                "provider_id": pid,
                "score": score,
                "tier": tier.as_str(),
            })
        })
        .collect();
    Json(serde_json::json!({ "leaderboard": entries }))
}

async fn reputation_recalculate_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let count = state.reputation_engine.recalculate();
    Json(serde_json::json!({
        "status": "recalculated",
        "providers_updated": count,
    }))
}

async fn reputation_history_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    let history = state.reputation_engine.get_history(&provider_id, 50);
    Json(serde_json::json!({
        "provider_id": provider_id,
        "history": history,
    }))
}

async fn reputation_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.reputation_engine.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Dispute Handlers
// ---------------------------------------------------------------------------

async fn dispute_create_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateDisputeRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.create_dispute(&req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn dispute_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.get_dispute(&id) {
        Some(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

async fn dispute_evidence_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitEvidenceRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.submit_evidence(&id, &req) {
        Ok(evidence) => Json(serde_json::to_value(evidence).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn dispute_vote_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CastVoteRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.cast_vote(&id, &req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn dispute_resolve_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ResolveDisputeRequest>,
) -> Json<serde_json::Value> {
    match state.dispute_engine.resolve(&id, &req) {
        Ok(dispute) => Json(serde_json::to_value(dispute).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn dispute_list_handler(
    State(state): State<AppState>,
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

async fn dispute_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.dispute_engine.get_stats();
    Json(serde_json::to_value(stats).unwrap_or_default())
}

async fn dispute_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.dispute_engine.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Escrow Handlers
// ---------------------------------------------------------------------------

async fn escrow_create_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateEscrowRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.create_escrow(&req) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn escrow_fund_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<FundEscrowRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.fund_escrow(&id, &req) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn escrow_release_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ReleaseFundsRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.release_funds(&id, &req) {
        Ok(release) => Json(serde_json::to_value(release).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn escrow_refund_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.refund(&id, None) {
        Ok(release) => Json(serde_json::to_value(release).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn escrow_dispute_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.dispute(&id) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn escrow_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.get_escrow(&id) {
        Some(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

async fn escrow_list_handler(
    State(state): State<AppState>,
    Query(params): Query<EscrowListQuery>,
) -> Json<serde_json::Value> {
    let status = params.status.as_deref().and_then(EscrowStatus::from_str);
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let escrows = state.escrow_manager.list_escrows(status.as_ref(), limit, offset);
    Json(serde_json::json!({
        "escrows": escrows,
        "limit": limit,
        "offset": offset,
    }))
}

async fn escrow_history_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let history = state.escrow_manager.get_history(&id);
    Json(serde_json::json!({
        "escrow_id": id,
        "history": history,
    }))
}

async fn escrow_balance_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let balance = state.escrow_manager.get_balance();
    Json(serde_json::to_value(balance).unwrap_or_default())
}

async fn escrow_timeout_check_handler(
    State(state): State<AppState>,
    Json(body): Json<TimeoutCheckBody>,
) -> Json<serde_json::Value> {
    let count = state.escrow_manager.timeout_check(body.current_height);
    Json(serde_json::json!({
        "status": "checked",
        "expired_count": count,
        "current_height": body.current_height,
    }))
}

async fn escrow_config_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let config = state.escrow_manager.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Health Handlers
// ---------------------------------------------------------------------------

async fn health_overall_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let status = state.health_checker.run_health_check();
    Json(serde_json::to_value(status).unwrap_or_default())
}

async fn health_components_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let components = state.health_checker.get_components();
    Json(serde_json::json!({
        "components": components,
        "total": components.len(),
    }))
}

async fn health_history_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let history = state.health_checker.get_history();
    Json(serde_json::json!({
        "history": history,
        "count": history.len(),
        "period_hours": 24,
    }))
}

// ---------------------------------------------------------------------------
// ErgoPay QR Handlers
// ---------------------------------------------------------------------------

async fn ergopay_create_qr_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateErgoPayQrBody>,
) -> Json<serde_json::Value> {
    let req = ErgoPayQrRequest::new(body.amount_nanoerg, &body.recipient, body.description.as_deref());
    match state.ergopay_qr_manager.create_request(req) {
        Ok(response) => Json(serde_json::to_value(response).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

async fn ergopay_get_qr_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.ergopay_qr_manager.get_request(&id) {
        Some(response) => Json(serde_json::to_value(response).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found_or_expired" })),
    }
}

async fn ergopay_status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.ergopay_qr_manager.get_status(&id) {
        Some(status) => Json(serde_json::to_value(status).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

// ---------------------------------------------------------------------------
// Treasury Governance Handlers
// ---------------------------------------------------------------------------

async fn treasury_snapshot_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let snapshot = state.treasury_governance.get_snapshot();
    Json(serde_json::to_value(snapshot).unwrap_or_default())
}

async fn treasury_proposals_handler(
    State(state): State<AppState>,
    Query(params): Query<TreasuryListQuery>,
) -> Json<serde_json::Value> {
    let proposals = state.treasury_governance.list_proposals(
        params.stage.as_deref(),
        params.limit.unwrap_or(50).min(200) as usize,
    );
    Json(serde_json::json!({
        "proposals": proposals,
        "count": proposals.len(),
    }))
}

async fn treasury_proposal_get_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.treasury_governance.get_proposal(&id) {
        Some(proposal) => Json(serde_json::to_value(proposal).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "proposal_not_found" })),
    }
}

async fn treasury_operations_handler(
    State(state): State<AppState>,
    Query(params): Query<TreasuryOpsQuery>,
) -> Json<serde_json::Value> {
    let ops = state.treasury_governance.get_treasury_ops(
        params.status.as_deref(),
        params.limit.unwrap_or(50).min(200) as usize,
    );
    Json(serde_json::json!({
        "operations": ops,
        "count": ops.len(),
    }))
}

async fn treasury_quorum_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.treasury_governance.get_proposal(&id) {
        Some(proposal) => {
            let qs = state.treasury_governance.compute_quorum_status(&proposal);
            Json(serde_json::to_value(qs).unwrap_or_default())
        }
        None => Json(serde_json::json!({ "error": "proposal_not_found" })),
    }
}

async fn treasury_summary_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let summary = state.treasury_governance.proposal_summary();
    let active = state.treasury_governance.active_proposal_count();
    let snapshot = state.treasury_governance.get_snapshot();
    Json(serde_json::json!({
        "by_stage": summary,
        "active_proposals": active,
        "total_proposals": snapshot.total_proposals,
        "available_balance_erg": TreasuryGovernanceManager::format_erg(snapshot.available_balance),
        "total_deposits_erg": TreasuryGovernanceManager::format_erg(snapshot.total_deposits_nanoerg),
        "total_spent_erg": TreasuryGovernanceManager::format_erg(snapshot.total_spent_nanoerg),
    }))
}

#[derive(Deserialize)]
struct TreasuryListQuery {
    stage: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct TreasuryOpsQuery {
    status: Option<String>,
    limit: Option<u32>,
}
