//! Authentication module — signup, login, JWT session management.
//!
//! Endpoints:
//!   POST /v1/auth/signup   — create account (email + password)
//!   POST /v1/auth/login    — authenticate, get JWT
//!   POST /v1/auth/logout   — invalidate session (client-side, just clear token)
//!   GET  /v1/auth/me       — get current user from JWT
//!
//! JWT payload: { sub: user_id, email, tier, exp }
//! Token sent as Bearer token in Authorization header.

use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::proxy::AppState;

// ── JWT claims ──

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject — user ID
    pub sub: String,
    pub email: String,
    pub tier: String,
    /// Expiration (Unix timestamp)
    pub exp: usize,
    /// Issued at
    pub iat: usize,
}

/// JWT expiration: 7 days
const JWT_EXPIRATION_SECS: i64 = 7 * 24 * 60 * 60;

// ── Request / Response types ──

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: AuthUser,
}

#[derive(Debug, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub tier: String,
    pub credits_usd: f64,
    pub ergo_address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ResetPasswordResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    #[allow(dead_code)] // TODO: will be used in future profile update endpoint
    pub ergo_address: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ChangePasswordResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)] // TODO: planned for future error response formatting
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)] // TODO: planned for future error response formatting
pub struct ErrorBody {
    pub message: String,
    pub code: Option<String>,
}

// ── Handlers ──

/// POST /v1/auth/signup
pub async fn signup_handler(
    State(state): State<AppState>,
    Json(body): Json<SignupRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Validate input
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::Validation("Invalid email address".into()));
    }
    if body.password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 characters".into()));
    }

    // Check if user already exists
    if state.db.get_user_by_email(&email).map_err(|e| AppError::Internal(format!("DB error: {}", e)))?.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    // Hash password
    let password_hash = hash_password(&body.password)
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))?;

    // Create user
    let user_id = Uuid::new_v4().to_string();
    let user = state
        .db
        .create_user(&user_id, &email, body.name.as_deref(), &password_hash)
        .map_err(|e| AppError::Internal(format!("Failed to create user: {}", e)))?;

    // Issue JWT
    let token = issue_jwt(&user, &state.config.auth.jwt_secret)
        .map_err(|e| AppError::Internal(format!("JWT issuance failed: {}", e)))?;

    let credits = state
        .db
        .get_credit_balance(&user_id)
        .unwrap_or(0.0);

    info!(user_id = %user_id, email = %email, "User signed up");

    Ok(Json(AuthResponse {
        token,
        user: AuthUser {
            id: user.id,
            email: user.email,
            name: user.name,
            tier: user.tier,
            credits_usd: credits,
            ergo_address: None,
        },
    }))
}

/// POST /v1/auth/login
pub async fn login_handler(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = body.email.trim().to_lowercase();

    let user = state
        .db
        .get_user_by_email(&email)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".into()))?;

    // Verify password
    verify_password(&body.password, &user.password_hash)
        .map_err(|_| AppError::Unauthorized("Invalid email or password".into()))?;

    // Issue JWT
    let token = issue_jwt(&user, &state.config.auth.jwt_secret)
        .map_err(|e| AppError::Internal(format!("JWT issuance failed: {}", e)))?;

    let credits = state
        .db
        .get_credit_balance(&user.id)
        .unwrap_or(0.0);

    info!(user_id = %user.id, email = %email, "User logged in");

    Ok(Json(AuthResponse {
        token,
        user: AuthUser {
            id: user.id,
            email: user.email,
            name: user.name,
            tier: user.tier,
            credits_usd: credits,
            ergo_address: None,
        },
    }))
}

/// GET /v1/auth/me — Get current user from JWT
pub async fn me_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;
    let user = state
        .db
        .get_user_by_id(&claims.sub)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    let credits = state
        .db
        .get_credit_balance(&user.id)
        .unwrap_or(0.0);

    // Read ergo_address from DB (may be NULL for users who haven't linked a wallet)
    let ergo_address = state.db.get_user_ergo_address(&user.id).unwrap_or(None);

    Ok(Json(AuthUser {
        id: user.id,
        email: user.email,
        name: user.name,
        tier: user.tier,
        credits_usd: credits,
        ergo_address,
    }))
}

/// POST /v1/auth/forgot-password
pub async fn forgot_password_handler(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Json<ForgotPasswordResponse>, AppError> {
    let email = body.email.trim().to_lowercase();

    // Always return the same response to prevent email enumeration
    if let Ok(Some(user)) = state.db.get_user_by_email(&email) {
        match state.db.create_password_reset_token(&user.id) {
            Ok(_token) => {
                info!(
                    user_id = %user.id,
                    email = %email,
                    "Password reset token generated"
                );
            }
            Err(e) => {
                warn!(error = %e, "Failed to create password reset token");
            }
        }
    }

    Ok(Json(ForgotPasswordResponse {
        message: "If the email exists, a reset link was generated".into(),
    }))
}

/// POST /v1/auth/reset-password
pub async fn reset_password_handler(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<ResetPasswordResponse>, AppError> {
    if body.new_password.len() < 8 {
        return Err(AppError::Validation("Password must be at least 8 characters".into()));
    }

    let user_id = state
        .db
        .consume_password_reset_token(&body.token)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Validation("Invalid or expired reset token".into()))?;

    let new_hash = hash_password(&body.new_password)
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))?;

    state
        .db
        .update_user_password(&user_id, &new_hash)
        .map_err(|e| AppError::Internal(format!("Failed to update password: {}", e)))?;

    info!(user_id = %user_id, "Password reset successfully");

    Ok(Json(ResetPasswordResponse {
        message: "Password updated".into(),
    }))
}

/// PUT /v1/auth/profile — Update user profile (name/email)
pub async fn update_profile_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<AuthUser>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    // Validate email if changing
    if let Some(ref email) = body.email {
        let email = email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AppError::Validation("Invalid email address".into()));
        }
        // Check email uniqueness (excluding current user)
        if let Ok(Some(existing)) = state.db.get_user_by_email(&email) {
            if existing.id != claims.sub {
                return Err(AppError::Conflict("Email already in use by another account".into()));
            }
        }
    }

    let name = body.name.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let email_owned = body.email.as_ref().map(|s| s.trim().to_lowercase());
    let email = email_owned.as_deref();

    let updated = state
        .db
        .update_user_profile(&claims.sub, name, email)
        .map_err(|e| AppError::Internal(format!("Failed to update profile: {}", e)))?;

    let credits = state
        .db
        .get_credit_balance(&updated.id)
        .unwrap_or(0.0);

    info!(user_id = %claims.sub, "Profile updated");

    let ergo_address = state.db.get_user_ergo_address(&updated.id).unwrap_or(None);

    Ok(Json(AuthUser {
        id: updated.id,
        email: updated.email,
        name: updated.name,
        tier: updated.tier,
        credits_usd: credits,
        ergo_address,
    }))
}

/// PUT /v1/auth/password — Change user password
pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ChangePasswordResponse>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    if body.new_password.len() < 8 {
        return Err(AppError::Validation("New password must be at least 8 characters".into()));
    }

    let user = state
        .db
        .get_user_by_id(&claims.sub)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Unauthorized("User not found".into()))?;

    // Verify current password
    verify_password(&body.current_password, &user.password_hash)
        .map_err(|_| AppError::Unauthorized("Current password is incorrect".into()))?;

    let new_hash = hash_password(&body.new_password)
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))?;

    state
        .db
        .update_user_password(&user.id, &new_hash)
        .map_err(|e| AppError::Internal(format!("Failed to update password: {}", e)))?;

    info!(user_id = %claims.sub, "Password changed");

    Ok(Json(ChangePasswordResponse {
        message: "Password updated".into(),
    }))
}

// ── Wallet ──

#[derive(Debug, Deserialize)]
pub struct UpdateWalletRequest {
    pub ergo_address: Option<String>,
}

/// PUT /v1/auth/wallet — Link or unlink an Ergo wallet address
pub async fn update_wallet_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateWalletRequest>,
) -> Result<Json<AuthUser>, AppError> {
    let claims = extract_claims(&headers, &state.config.auth.jwt_secret)?;

    // If ergo_address is Some(""), treat as unlink (set to NULL)
    let ergo_addr = body.ergo_address.as_deref().filter(|s| !s.is_empty());

    state
        .db
        .update_wallet_address(&claims.sub, ergo_addr)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update wallet address");
            AppError::Internal("Failed to update wallet address".into())
        })?;

    // Fetch the updated user for the response
    let user = state
        .db
        .get_user_by_id(&claims.sub)
        .map_err(|_| AppError::Internal("Internal server error".into()))?
        .ok_or_else(|| AppError::Internal("User not found".into()))?;

    let credits = state.db.get_credit_balance(&user.id).unwrap_or(0.0);

    Ok(Json(AuthUser {
        id: user.id,
        email: user.email,
        name: user.name,
        tier: user.tier,
        credits_usd: credits,
        ergo_address: ergo_addr.map(String::from),
    }))
}

// ── Helpers ──

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<()> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| anyhow::anyhow!("Invalid password hash format: {}", e))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|e| anyhow::anyhow!("Password verification failed: {}", e))?;
    Ok(())
}

fn issue_jwt(user: &crate::db::User, secret: &str) -> Result<String> {
    let now = Utc::now();
    let claims = JwtClaims {
        sub: user.id.clone(),
        email: user.email.clone(),
        tier: user.tier.clone(),
        iat: now.timestamp() as usize,
        exp: (now.timestamp() + JWT_EXPIRATION_SECS) as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .context("Failed to issue JWT")
}

/// Extract and validate JWT claims from request headers.
/// Returns Ok(claims) if valid token found, Err if missing/invalid.
pub fn extract_claims(
    headers: &HeaderMap,
    secret: &str,
) -> Result<JwtClaims, AppError> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Invalid Authorization format. Use: Bearer <token>".into()))?;

    let token_data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| {
        warn!(error = %e, "JWT validation failed");
        AppError::Unauthorized("Invalid or expired token".into())
    })?;

    Ok(token_data.claims)
}

// ── Unified auth identity (works with JWT or API key) ──

/// Resolved auth identity — produced by either JWT or API key validation.
#[derive(Debug, Clone)]
pub struct AuthIdentity {
    /// User ID
    pub sub: String,
    /// User email
    pub email: String,
    /// User tier ("free" | "pro")
    pub tier: String,
}

/// Try to authenticate a request via JWT first, then API key.
/// Returns Ok(Some(identity)) if authenticated, Ok(None) if no auth provided.
/// Returns Err if auth was provided but is invalid.
pub fn authenticate_request(
    headers: &HeaderMap,
    jwt_secret: &str,
    db: &crate::db::Db,
) -> Result<Option<AuthIdentity>, AppError> {
    // Extract the raw bearer token (if present)
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let token = match bearer {
        Some(t) => t,
        None => return Ok(None), // No auth provided — anonymous
    };

    // 1. Try JWT first
    if let Ok(identity) = try_jwt_auth(token, jwt_secret) {
        return Ok(Some(identity));
    } // JWT failed — fall through to API key

    // 2. Try API key
    if let Ok(identity) = try_api_key_auth(token, db) {
        return Ok(Some(identity));
    } // API key also failed

    // Both failed — return unauthorized
    Err(AppError::Unauthorized(
        "Invalid or expired token".into(),
    ))
}

fn try_jwt_auth(token: &str, secret: &str) -> Result<AuthIdentity, AppError> {
    let token_data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::Unauthorized("Invalid JWT".into()))?;

    Ok(AuthIdentity {
        sub: token_data.claims.sub,
        email: token_data.claims.email,
        tier: token_data.claims.tier,
    })
}

fn try_api_key_auth(token: &str, db: &crate::db::Db) -> Result<AuthIdentity, AppError> {
    use crate::handlers::api_keys::hash_api_key;


    let key_hash = hash_api_key(token);

    let key_row = db
        .find_active_api_key(&key_hash)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

    // Touch last_used_at (fire-and-forget)
    let key_id = key_row.id.clone();
    if let Err(e) = db.touch_api_key_last_used(&key_id) {
        tracing::warn!(error = %e, "Failed to update API key last_used_at");
    }

    // Look up the user to get email and tier
    let user = db
        .get_user_by_id(&key_row.user_id)
        .map_err(|e| AppError::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| AppError::Internal("API key references non-existent user".into()))?;

    Ok(AuthIdentity {
        sub: user.id,
        email: user.email,
        tier: user.tier,
    })
}

// ── Error types ──

#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    Validation(String),
    Conflict(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::Unauthorized(msg) => (axum::http::StatusCode::UNAUTHORIZED, Some("unauthorized"), msg.clone()),
            AppError::Validation(msg) => (axum::http::StatusCode::BAD_REQUEST, Some("validation_error"), msg.clone()),
            AppError::Conflict(msg) => (axum::http::StatusCode::CONFLICT, Some("conflict"), msg.clone()),
            AppError::Internal(msg) => {
                tracing::error!(msg, "Internal error");
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Some("internal_error"), "Internal server error".into())
            }
        };

        let body = serde_json::json!({
            "error": {
                "message": message,
                "code": code,
            }
        });

        (
            status,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            serde_json::to_string(&body).unwrap(),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "correct-horse-battery-staple";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).is_ok());
    }

    #[test]
    fn test_verify_wrong_password_fails() {
        let hash = hash_password("correct-horse-battery-staple").unwrap();
        assert!(verify_password("wrong-password", &hash).is_err());
    }

    #[test]
    fn test_verify_different_passwords_produce_different_hashes() {
        let h1 = hash_password("password-aaa").unwrap();
        let h2 = hash_password("password-bbb").unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_password_minimum_length() {
        // 8 characters should work
        let hash = hash_password("12345678").unwrap();
        assert!(verify_password("12345678", &hash).is_ok());
    }

    #[test]
    fn test_verify_empty_password_fails() {
        let hash = hash_password("nonempty").unwrap();
        assert!(verify_password("", &hash).is_err());
    }
}
