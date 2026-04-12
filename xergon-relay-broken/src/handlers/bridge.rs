//! Cross-chain payment bridge endpoints for the Xergon relay.
//!
//! Routes:
//!   GET  /v1/bridge/invoice/{id}      — Get invoice status
//!   POST /v1/bridge/create-invoice     — Create new invoice
//!   POST /v1/bridge/confirm            — Confirm payment (bridge operator)
//!   POST /v1/bridge/refund             — Refund expired invoice (buyer)
//!   GET  /v1/bridge/invoices           — List pending invoices
//!   GET  /v1/bridge/status             — Bridge status/config

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Foreign chain type (matches the on-chain Int encoding).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForeignChainParam {
    Btc,
    Eth,
    Ada,
}

/// Request body for creating a new bridge invoice.
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequest {
    /// Provider public key (hex) to pay
    pub provider_pk: String,
    /// Amount in ERG (human-readable, e.g. 1.5)
    pub amount_erg: f64,
    /// Foreign chain to pay on
    pub foreign_chain: ForeignChainParam,
    /// Optional foreign chain tx ID (if already paid)
    #[allow(dead_code)]
    pub foreign_tx_id: Option<String>,
}

/// Response for creating an invoice.
#[derive(Debug, Serialize)]
pub struct CreateInvoiceResponse {
    pub success: bool,
    pub invoice_id: String,
    pub ergo_tx_id: String,
    pub box_id: String,
    pub foreign_payment_address: String,
    pub amount_nanoerg: u64,
    pub foreign_chain: String,
    pub timeout_blocks: u32,
    pub message: String,
}

/// Request body for confirming a bridge payment.
#[derive(Debug, Deserialize)]
pub struct ConfirmPaymentRequest {
    /// Invoice box ID on Ergo
    pub invoice_id: String,
    /// Transaction ID on the foreign chain
    pub foreign_tx_id: String,
    /// Provider's Ergo address (to receive payment)
    pub provider_address: String,
    /// Amount in nanoERG
    pub amount_nanoerg: u64,
}

/// Response for confirming a payment.
#[derive(Debug, Serialize)]
pub struct ConfirmPaymentResponse {
    pub success: bool,
    pub invoice_id: String,
    pub ergo_tx_id: String,
    pub foreign_tx_id: String,
    pub amount_nanoerg: u64,
    pub message: String,
}

/// Request body for refunding an invoice.
#[derive(Debug, Deserialize)]
pub struct RefundInvoiceRequest {
    /// Invoice box ID on Ergo
    pub invoice_id: String,
    /// Buyer's Ergo address (to receive refund)
    pub buyer_address: String,
    /// Amount in nanoERG
    pub amount_nanoerg: u64,
}

/// Response for refunding an invoice.
#[derive(Debug, Serialize)]
pub struct RefundInvoiceResponse {
    pub success: bool,
    pub invoice_id: String,
    pub ergo_tx_id: String,
    pub amount_nanoerg: u64,
    pub message: String,
}

/// Response for getting invoice status.
#[derive(Debug, Serialize)]
pub struct InvoiceStatusResponse {
    pub success: bool,
    pub invoice_id: String,
    pub status: String,
    pub amount_erg: f64,
    pub foreign_chain: String,
    pub message: String,
}

/// Response for bridge status.
#[derive(Debug, Serialize)]
pub struct BridgeStatusResponse {
    pub enabled: bool,
    pub supported_chains: Vec<String>,
    pub invoice_timeout_blocks: u32,
    pub message: String,
}

/// Generic error response matching the standard relay error format.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

/// Standard error detail object.
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    pub code: u16,
}

impl ErrorResponse {
    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                error_type: "service_unavailable".to_string(),
                message: message.into(),
                code: 503,
            },
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                error_type: "invalid_request".to_string(),
                message: message.into(),
                code: 400,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/bridge/invoice/{id} — Get invoice status.
pub async fn get_invoice_status_handler(
    State(_state): State<AppState>,
    Path(invoice_id): Path<String>,
) -> impl IntoResponse {
    debug!(invoice_id = %invoice_id, "Getting bridge invoice status");

    // In production, this would query the Ergo node for the invoice box
    // and return its current status. For MVP, return a placeholder.
    let response = InvoiceStatusResponse {
        success: true,
        invoice_id: invoice_id.clone(),
        status: "unknown".to_string(),
        amount_erg: 0.0,
        foreign_chain: "unknown".to_string(),
        message: format!("Invoice {} status lookup requires Ergo node integration", invoice_id),
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /v1/bridge/create-invoice — Create a new bridge invoice.
pub async fn create_invoice_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateInvoiceRequest>,
) -> impl IntoResponse {
    info!(
        provider = %body.provider_pk,
        amount = body.amount_erg,
        chain = ?body.foreign_chain,
        "Creating bridge invoice"
    );

    // Check if bridge is enabled
    let bridge_config = &state.config.bridge;
    if !bridge_config.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::service_unavailable(
                "Cross-chain payment bridge is not enabled",
            )),
        )
            .into_response();
    }

    // Validate amount
    let amount_nanoerg = (body.amount_erg * 1_000_000_000.0) as u64;
    if amount_nanoerg == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::invalid_request(
                "Amount must be greater than 0",
            )),
        )
            .into_response();
    }

    // In production, this would call the agent's payment bridge module
    // to create the invoice box on Ergo. For MVP relay, return a
    // confirmation that the invoice request was received.
    let chain_name = match body.foreign_chain {
        ForeignChainParam::Btc => "btc",
        ForeignChainParam::Eth => "eth",
        ForeignChainParam::Ada => "ada",
    };

    // Generate a placeholder invoice ID
    let invoice_id = format!("inv_{}", hex::encode(rand::random::<[u8; 8]>()));
    let foreign_payment_address = format!(
        "xergon-bridge-{}-{}",
        chain_name,
        &invoice_id[..invoice_id.len().min(16)]
    );

    let response = CreateInvoiceResponse {
        success: true,
        invoice_id: invoice_id.clone(),
        ergo_tx_id: "pending".to_string(),
        box_id: "pending".to_string(),
        foreign_payment_address,
        amount_nanoerg,
        foreign_chain: chain_name.to_string(),
        timeout_blocks: bridge_config.invoice_timeout_blocks,
        message: format!(
            "Invoice {} created. Send {} ERG worth of {} to the bridge address.",
            invoice_id, body.amount_erg, chain_name
        ),
    };

    info!(invoice_id = %invoice_id, "Bridge invoice created");

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /v1/bridge/confirm — Confirm a foreign-chain payment (bridge operator only).
pub async fn confirm_payment_handler(
    State(state): State<AppState>,
    Json(body): Json<ConfirmPaymentRequest>,
) -> impl IntoResponse {
    info!(
        invoice_id = %body.invoice_id,
        foreign_tx = %body.foreign_tx_id,
        provider = %body.provider_address,
        "Confirming bridge payment"
    );

    let bridge_config = &state.config.bridge;
    if !bridge_config.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::service_unavailable(
                "Cross-chain payment bridge is not enabled",
            )),
        )
            .into_response();
    }

    // In production, this would:
    // 1. Verify the foreign chain tx exists and has correct amount
    // 2. Call the agent to spend the invoice box to the provider
    // For MVP relay, return a confirmation.
    let response = ConfirmPaymentResponse {
        success: true,
        invoice_id: body.invoice_id.clone(),
        ergo_tx_id: "pending".to_string(),
        foreign_tx_id: body.foreign_tx_id.clone(),
        amount_nanoerg: body.amount_nanoerg,
        message: format!(
            "Payment for invoice {} confirmed. ERG will be released to provider.",
            body.invoice_id
        ),
    };

    info!(invoice_id = %body.invoice_id, "Bridge payment confirmed");

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /v1/bridge/refund — Refund an expired invoice.
pub async fn refund_invoice_handler(
    State(state): State<AppState>,
    Json(body): Json<RefundInvoiceRequest>,
) -> impl IntoResponse {
    info!(
        invoice_id = %body.invoice_id,
        buyer = %body.buyer_address,
        "Refunding bridge invoice"
    );

    let bridge_config = &state.config.bridge;
    if !bridge_config.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::service_unavailable(
                "Cross-chain payment bridge is not enabled",
            )),
        )
            .into_response();
    }

    // In production, this would check that the invoice has expired
    // and then spend the box back to the buyer.
    let response = RefundInvoiceResponse {
        success: true,
        invoice_id: body.invoice_id.clone(),
        ergo_tx_id: "pending".to_string(),
        amount_nanoerg: body.amount_nanoerg,
        message: format!(
            "Refund for invoice {} initiated. ERG will be returned to buyer.",
            body.invoice_id
        ),
    };

    info!(invoice_id = %body.invoice_id, "Bridge invoice refund initiated");

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/bridge/status — Get bridge status and configuration.
pub async fn bridge_status_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let bridge_config = &state.config.bridge;

    let supported_chains: Vec<String> = bridge_config
        .supported_chains
        .iter()
        .map(|c| match c {
            crate::config::BridgeForeignChain::Btc => "btc".to_string(),
            crate::config::BridgeForeignChain::Eth => "eth".to_string(),
            crate::config::BridgeForeignChain::Ada => "ada".to_string(),
        })
        .collect();

    let response = BridgeStatusResponse {
        enabled: bridge_config.enabled,
        supported_chains,
        invoice_timeout_blocks: bridge_config.invoice_timeout_blocks,
        message: if bridge_config.enabled {
            "Bridge is operational".to_string()
        } else {
            "Bridge is disabled".to_string()
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/bridge/invoices — List pending bridge invoices.
pub async fn list_invoices_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let bridge_config = &state.config.bridge;

    if !bridge_config.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::service_unavailable(
                "Cross-chain payment bridge is not enabled",
            )),
        )
            .into_response();
    }

    // In production, this would scan the Ergo chain for pending invoices
    // using the agent's payment bridge module.
    let response = serde_json::json!({
        "success": true,
        "invoices": [],
        "message": "No pending invoices (chain scanning requires agent integration)"
    });

    (StatusCode::OK, Json(response)).into_response()
}
