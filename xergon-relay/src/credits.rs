//! Credits and Stripe payment module.
//!
//! Endpoints:
//!   GET  /v1/credits/balance     — get current USD credit balance
//!   GET  /v1/credits/transactions — recent transaction history
//!   POST /v1/credits/purchase    — create Stripe Checkout Session
//!   POST /v1/webhooks/stripe     — Stripe webhook handler (fulfills payment)
//!
//! Credit packs: $5, $10, $25 (configurable)
//! All pricing in USD only. ERG never shown.

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::auth::{authenticate_request, AppError};
use crate::proxy::AppState;

// ── Config ──

/// A purchasable credit pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPack {
    pub id: String,
    pub amount_usd: f64,
    pub display_price: String,
    pub bonus_credits_usd: f64, // extra credits as bonus
}

/// Get the default credit packs
pub fn default_credit_packs() -> Vec<CreditPack> {
    vec![
        CreditPack {
            id: "pack_5".into(),
            amount_usd: 5.0,
            display_price: "$5.00".into(),
            bonus_credits_usd: 0.0,
        },
        CreditPack {
            id: "pack_10".into(),
            amount_usd: 10.0,
            display_price: "$10.00".into(),
            bonus_credits_usd: 1.0, // $1 bonus
        },
        CreditPack {
            id: "pack_25".into(),
            amount_usd: 25.0,
            display_price: "$25.00".into(),
            bonus_credits_usd: 5.0, // $5 bonus
        },
    ]
}

// ── Request / Response types ──

#[derive(Debug, Deserialize)]
pub struct PurchaseRequest {
    pub pack_id: String,
}

#[derive(Debug, Serialize)]
pub struct PurchaseResponse {
    pub checkout_url: String,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub credits_usd: f64,
    pub currency: String,
}

#[derive(Debug, Serialize)]
pub struct TransactionsResponse {
    pub transactions: Vec<TransactionView>,
}

#[derive(Debug, Serialize)]
pub struct TransactionView {
    pub id: String,
    pub amount_usd: f64,
    pub balance_after: f64,
    pub kind: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct PacksResponse {
    pub packs: Vec<CreditPack>,
}

// ── Handlers ──

#[derive(Debug, Deserialize)]
pub struct AutoReplenishRequest {
    pub enabled: bool,
    pub pack_id: Option<String>,
    #[serde(default = "default_threshold")]
    pub threshold_usd: f64,
}

fn default_threshold() -> f64 { 1.0 }

#[derive(Debug, Serialize)]
pub struct AutoReplenishResponse {
    pub enabled: bool,
    pub pack_id: Option<String>,
    pub threshold_usd: f64,
}

/// PUT /v1/credits/auto-replenish — Update auto-replenish settings
pub async fn update_auto_replenish_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AutoReplenishRequest>,
) -> Result<Json<AutoReplenishResponse>, AppError> {
    let identity = authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db)?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".into()))?;

    if body.enabled && body.pack_id.is_none() {
        return Err(AppError::Validation("pack_id is required when auto-replenish is enabled".into()));
    }

    state
        .db
        .update_auto_replenish(
            &identity.sub,
            body.enabled,
            body.pack_id.as_deref(),
            body.threshold_usd,
        )
        .map_err(|e| AppError::Internal(format!("Failed to update auto-replenish: {}", e)))?;

    info!(
        user_id = %identity.sub,
        enabled = body.enabled,
        pack_id = ?body.pack_id,
        threshold = body.threshold_usd,
        "Auto-replenish settings updated"
    );

    Ok(Json(AutoReplenishResponse {
        enabled: body.enabled,
        pack_id: body.pack_id,
        threshold_usd: body.threshold_usd,
    }))
}

/// GET /v1/credits/auto-replenish — Get current auto-replenish settings
pub async fn get_auto_replenish_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AutoReplenishResponse>, AppError> {
    let identity = authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db)?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".into()))?;
    let user = state
        .db
        .get_user_by_id(&identity.sub)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    Ok(Json(AutoReplenishResponse {
        enabled: user.auto_replenish,
        pack_id: user.replenish_pack_id,
        threshold_usd: user.replenish_threshold_usd,
    }))
}

/// GET /v1/credits/balance — Get current credit balance
pub async fn get_balance_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BalanceResponse>, AppError> {
    let identity = authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db)?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".into()))?;
    let balance = state
        .db
        .get_credit_balance(&identity.sub)
        .map_err(|e| AppError::Internal(format!("Failed to get balance: {}", e)))?;

    Ok(Json(BalanceResponse {
        credits_usd: balance,
        currency: "USD".into(),
    }))
}

/// GET /v1/credits/transactions — Recent transaction history
pub async fn get_transactions_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TransactionsResponse>, AppError> {
    let identity = authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db)?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".into()))?;
    let txs = state
        .db
        .get_transactions(&identity.sub, 50)
        .map_err(|e| AppError::Internal(format!("Failed to get transactions: {}", e)))?;

    let views = txs
        .into_iter()
        .map(|tx| TransactionView {
            id: tx.id,
            amount_usd: tx.amount_usd,
            balance_after: tx.balance_after,
            kind: tx.kind,
            description: tx.description,
            created_at: tx.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(TransactionsResponse {
        transactions: views,
    }))
}

/// GET /v1/credits/packs — List available credit packs
pub async fn get_packs_handler() -> Json<PacksResponse> {
    Json(PacksResponse {
        packs: default_credit_packs(),
    })
}

/// POST /v1/credits/purchase — Create a Stripe Checkout Session
pub async fn purchase_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PurchaseRequest>,
) -> Result<Json<PurchaseResponse>, AppError> {
    let identity = authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db)?
        .ok_or_else(|| AppError::Unauthorized("Authentication required".into()))?;

    // Find the pack
    let packs = default_credit_packs();
    let pack = packs
        .iter()
        .find(|p| p.id == body.pack_id)
        .ok_or_else(|| AppError::Validation(format!("Invalid pack_id: {}", body.pack_id)))?;

    let total_usd = pack.amount_usd + pack.bonus_credits_usd;

    // If Stripe secret key is configured, create a real Checkout Session
    if !state.config.stripe.secret_key.is_empty() {
        return create_stripe_checkout(&state, &identity, pack, total_usd, &state.config.stripe.secret_key).await;
    }

    // No Stripe configured — for dev/demo: add credits directly
    info!(
        user_id = %identity.sub,
        pack_id = %body.pack_id,
        amount = total_usd,
        "Stripe not configured, adding credits directly (dev mode)"
    );

    let tx_id = Uuid::new_v4().to_string();
    state
        .db
        .add_credits(
            &tx_id,
            &identity.sub,
            total_usd,
            "purchase",
            &format!("{} credit pack (dev mode)", pack.display_price),
            None,
        )
        .map_err(|e| AppError::Internal(format!("Failed to add credits: {}", e)))?;

    Ok(Json(PurchaseResponse {
        checkout_url: format!("/settings?credits_added={}", total_usd),
        session_id: format!("dev_{}", tx_id),
    }))
}

/// Create a real Stripe Checkout Session
async fn create_stripe_checkout(
    state: &AppState,
    identity: &crate::auth::AuthIdentity,
    pack: &CreditPack,
    total_usd: f64,
    stripe_key: &str,
) -> Result<Json<PurchaseResponse>, AppError> {
    let session_id = Uuid::new_v4().to_string();
    // Store metadata so we can match the webhook back to user + pack
    let metadata = serde_json::json!({
        "user_id": identity.sub,
        "pack_id": pack.id,
        "amount_usd": pack.amount_usd,
        "bonus_usd": pack.bonus_credits_usd,
        "session_internal_id": session_id,
    });

    let body = serde_json::json!({
        "payment_method_types": ["card"],
        "mode": "payment",
        "line_items": [{
            "price_data": {
                "currency": "usd",
                "product_data": {
                    "name": format!("Xergon Credits — {}", pack.display_price),
                    "description": format!("${:.2} in credits", total_usd),
                },
                "unit_amount": (pack.amount_usd * 100.0) as i64, // Stripe uses cents
            },
            "quantity": 1,
        }],
        "success_url": format!("{}/settings?checkout=success", state.config.stripe.success_url_base),
        "cancel_url": format!("{}/pricing?checkout=cancelled", state.config.stripe.success_url_base),
        "metadata": metadata,
        // Pre-fill email if available
        "customer_email": identity.email,
    });

    let resp = state
        .http_client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(stripe_key, None::<&str>)
        .form(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Stripe API error: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        warn!(status = %status, body = %err_body, "Stripe checkout creation failed");
        return Err(AppError::Internal(format!("Failed to create checkout session: {}", status)));
    }

    let session: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Stripe response: {}", e)))?;

    let checkout_url = session["url"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let stripe_session_id = session["id"]
        .as_str()
        .unwrap_or("")
        .to_string();

    info!(
        user_id = %identity.sub,
        stripe_session = %stripe_session_id,
        checkout_url = %checkout_url,
        "Stripe Checkout Session created"
    );

    Ok(Json(PurchaseResponse {
        checkout_url,
        session_id: stripe_session_id,
    }))
}

/// POST /v1/webhooks/stripe — Handle Stripe webhook events
///
/// Expected events:
///   checkout.session.completed — fulfill credit purchase
///
/// Signature verification uses HMAC-SHA256 per Stripe's spec:
///   signed_payload = timestamp + "." + raw_body
///   signature = HMAC-SHA256(webhook_secret, signed_payload)
///   header format: t=timestamp,v1=signature
pub async fn stripe_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Verify Stripe webhook signature if secret is configured
    if !state.config.stripe.webhook_secret.is_empty() {
        let sig_header = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if sig_header.is_empty() {
            warn!("Stripe webhook received without signature header");
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing stripe-signature header"})),
            );
        }

        match verify_stripe_signature(sig_header, &body, &state.config.stripe.webhook_secret) {
            Ok(()) => {
                info!("Stripe webhook signature verified");
            }
            Err(e) => {
                warn!(error = %e, "Stripe webhook signature verification failed");
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Invalid signature"})),
                );
            }
        }

        // Check timestamp to prevent replay attacks (tolerate 5 minute skew)
        if let Err(e) = verify_stripe_timestamp(sig_header) {
            warn!(error = %e, "Stripe webhook timestamp verification failed");
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Timestamp outside tolerance"})),
            );
        }
    }

    // Parse the event
    let event: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to parse Stripe webhook body");
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid JSON"})),
            );
        }
    };

    let event_type = event["type"].as_str().unwrap_or("unknown");
    let event_id = event["id"].as_str().unwrap_or("");
    info!(event_type, event_id, "Stripe webhook received");

    // Idempotency check — reject already-processed events
    if !event_id.is_empty() {
        match state.db.is_event_processed(event_id) {
            Ok(true) => {
                info!(event_id, "Stripe webhook already processed, skipping");
                return (
                    axum::http::StatusCode::OK,
                    Json(serde_json::json!({"received": true, "duplicate": true})),
                );
            }
            Ok(false) => {} // proceed
            Err(e) => {
                warn!(error = %e, "Failed to check event idempotency, processing anyway");
            }
        }
    }

    match event_type {
        "checkout.session.completed" => {
            if let Err(e) = handle_checkout_completed(&state, &event) {
                warn!(error = %e, "Failed to process checkout.session.completed");
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Processing failed"})),
                );
            }
            // Mark event as processed
            if !event_id.is_empty() {
                if let Err(e) = state.db.mark_event_processed(event_id, event_type) {
                    warn!(error = %e, event_id, "Failed to mark webhook event as processed");
                }
            }
            (axum::http::StatusCode::OK, Json(serde_json::json!({"received": true})))
        }
        _ => {
            // Acknowledge other events (still mark as processed)
            if !event_id.is_empty() {
                if let Err(e) = state.db.mark_event_processed(event_id, event_type) {
                    warn!(error = %e, event_id, "Failed to mark webhook event as processed");
                }
            }
            (axum::http::StatusCode::OK, Json(serde_json::json!({"received": true})))
        }
    }
}

/// Process a completed Stripe Checkout Session
fn handle_checkout_completed(state: &AppState, event: &serde_json::Value) -> anyhow::Result<()> {
    let session = &event["data"]["object"];
    let metadata = &session["metadata"];

    let user_id = metadata["user_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing user_id in checkout metadata"))?;

    let amount_usd = metadata["amount_usd"]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("Missing amount_usd in metadata"))?;

    let bonus_usd = metadata["bonus_usd"].as_f64().unwrap_or(0.0);
    let total = amount_usd + bonus_usd;
    let stripe_payment_id = session["payment_intent"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let tx_id = Uuid::new_v4().to_string();
    state.db.add_credits(
        &tx_id,
        user_id,
        total,
        "purchase",
        &format!("${:.2} credit pack", amount_usd),
        if stripe_payment_id.is_empty() { None } else { Some(&stripe_payment_id) },
    )?;

    info!(
        user_id,
        amount = amount_usd,
        bonus = bonus_usd,
        total,
        payment_id = %stripe_payment_id,
        "Credits fulfilled from Stripe payment"
    );

    Ok(())
}

// ── Stripe webhook signature verification ──

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verify the Stripe webhook signature.
///
/// Stripe sends: stripe-signature: t=timestamp,v1=signature
/// We compute: HMAC-SHA256(secret, timestamp + "." + body)
/// and compare with the v1 signature.
fn verify_stripe_signature(
    sig_header: &str,
    body: &str,
    secret: &str,
) -> anyhow::Result<()> {
    // Parse the signature header
    let mut timestamp: &str = "";
    let mut v1_signature: &str = "";

    for part in sig_header.split(',') {
        let part = part.trim();
        if let Some(ts) = part.strip_prefix("t=") {
            timestamp = ts;
        } else if let Some(sig) = part.strip_prefix("v1=") {
            v1_signature = sig;
        }
    }

    if timestamp.is_empty() || v1_signature.is_empty() {
        anyhow::bail!("Invalid stripe-signature header: missing t= or v1=");
    }

    // Construct the signed payload
    let signed_payload = format!("{}.{}", timestamp, body);

    // Compute HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC key error: {}", e))?;
    mac.update(signed_payload.as_bytes());

    // Get the expected signature as hex
    let expected_bytes = mac.finalize().into_bytes();
    let expected_hex = hex::encode(expected_bytes);

    // Constant-time comparison of hex strings
    if subtle::ConstantTimeEq::ct_eq(expected_hex.as_bytes(), v1_signature.as_bytes()).into() {
        Ok(())
    } else {
        anyhow::bail!("Signature mismatch")
    }
}

/// Verify the Stripe webhook timestamp is within tolerance (5 minutes).
/// Prevents replay attacks.
fn verify_stripe_timestamp(sig_header: &str) -> anyhow::Result<()> {
    let mut timestamp: &str = "";
    for part in sig_header.split(',') {
        let part = part.trim();
        if let Some(ts) = part.strip_prefix("t=") {
            timestamp = ts;
        }
    }

    let ts: i64 = timestamp
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid timestamp in signature header"))?;

    let now = chrono::Utc::now().timestamp();
    let skew = (now - ts).abs();

    const MAX_SKEW_SECS: i64 = 300; // 5 minutes
    if skew > MAX_SKEW_SECS {
        anyhow::bail!("Timestamp skew too large: {} seconds (max {})", skew, MAX_SKEW_SECS);
    }

    Ok(())
}
