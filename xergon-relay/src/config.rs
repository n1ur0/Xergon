//! Configuration for the Xergon relay server

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct RelayConfig {
    pub relay: RelaySettings,
    pub providers: ProviderSettings,
    pub credits: CreditsSettings,
    #[serde(default)]
    pub auth: AuthSettings,
    #[serde(default)]
    pub stripe: StripeSettings,
    #[serde(default)]
    pub database: DatabaseSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelaySettings {
    /// Address to bind (e.g. "0.0.0.0:8080")
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// CORS origins (comma-separated, or "*" for all)
    #[serde(default = "default_cors_origins")]
    pub cors_origins: String,

    /// Health poll interval in seconds
    #[serde(default = "default_health_poll")]
    pub health_poll_interval_secs: u64,

    /// Provider request timeout in seconds
    #[serde(default = "default_provider_timeout")]
    pub provider_timeout_secs: u64,

    /// Max fallback attempts
    #[serde(default = "default_max_fallback")]
    pub max_fallback_attempts: usize,

    /// Anonymous rate limit per day per IP
    #[serde(default = "default_anon_rate_limit")]
    pub anonymous_rate_limit_per_day: u32,

    /// Max tokens per request for anonymous users
    #[serde(default = "default_anon_max_tokens")]
    pub anonymous_max_tokens_per_request: u32,

    /// Interval in seconds for reporting aggregated usage to provider agents
    #[serde(default = "default_usage_report_interval")]
    pub usage_report_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSettings {
    /// Static list of known xergon-agent endpoints (legacy, still supported)
    #[serde(default = "default_known_endpoints")]
    pub known_endpoints: Vec<String>,
    /// Shared secret token for provider registration.
    /// Agents must send this as X-Provider-Token header.
    /// Generate with: openssl rand -hex 32
    #[serde(default = "default_registration_token")]
    pub registration_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditsSettings {
    /// Credits for new users
    #[serde(default)]
    #[allow(dead_code)] // TODO: will be used when new user signup flow is completed
    pub new_user_credits_usd: f64,

    /// Cost per 1000 tokens in USD
    #[serde(default = "default_cost_per_1k")]
    pub cost_per_1k_tokens: f64,
}

fn default_listen_addr() -> String { "0.0.0.0:8080".into() }
fn default_cors_origins() -> String { "*".into() }
fn default_health_poll() -> u64 { 30 }
fn default_provider_timeout() -> u64 { 30 }
fn default_max_fallback() -> usize { 3 }
fn default_anon_rate_limit() -> u32 { 10 }
fn default_anon_max_tokens() -> u32 { 500 }
fn default_usage_report_interval() -> u64 { 300 }
fn default_known_endpoints() -> Vec<String> { vec!["http://127.0.0.1:9099".into()] }
fn default_registration_token() -> String {
    // Dev-only default. MUST be overridden in production.
    "xergon-dev-provider-token-change-me".into()
}
fn default_cost_per_1k() -> f64 { 0.002 }

// ── Auth settings ──

#[derive(Debug, Clone, Deserialize)]
pub struct AuthSettings {
    /// Secret key for JWT signing. REQUIRED for auth to work.
    /// Generate with: openssl rand -base64 32
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
        }
    }
}

fn default_jwt_secret() -> String {
    // Dev-only default. MUST be overridden in production.
    "xergon-dev-jwt-secret-change-me-in-production".into()
}

// ── Stripe settings ──

#[derive(Debug, Clone, Deserialize)]
pub struct StripeSettings {
    /// Stripe secret key (sk_test_... or sk_live_...)
    #[serde(default)]
    pub secret_key: String,

    /// Stripe webhook signing secret (whsec_...)
    #[serde(default)]
    pub webhook_secret: String,

    /// Base URL for success/cancel redirects (e.g. "https://xergon.ai")
    #[serde(default = "default_success_url")]
    pub success_url_base: String,
}

impl Default for StripeSettings {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            webhook_secret: String::new(),
            success_url_base: default_success_url(),
        }
    }
}

fn default_success_url() -> String {
    "http://localhost:3000".into()
}

// ── Database settings ──

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSettings {
    /// Path to the SQLite database file
    #[serde(default = "default_db_path")]
    pub path: String,
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

fn default_db_path() -> String {
    "xergon.db".into()
}

/// Check whether a CORS wildcard warning should be emitted.
/// Extracted from `load()` for testability.
#[cfg(test)]
pub(crate) fn should_warn_cors_wildcard(cors_origins: &str, is_dev: bool) -> bool {
    cors_origins == "*" && !is_dev
}

impl RelayConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = std::env::var("XERGON_RELAY_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("config.toml"));

        let settings = if config_path.exists() {
            config::Config::builder()
                .add_source(config::File::from(config_path))
                .add_source(
                    config::Environment::with_prefix("XERGON_RELAY")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?
        } else {
            config::Config::builder()
                .add_source(
                    config::Environment::with_prefix("XERGON_RELAY")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?
        };

        let config: Self = settings.try_deserialize()?;

        // ── Security: validate secrets are not defaults in production ──
        let is_dev = std::env::var("XERGON_ENV")
            .map(|v| v == "development")
            .unwrap_or(false);

        let jwt_default = "xergon-dev-jwt-secret-change-me-in-production";
        if config.auth.jwt_secret == jwt_default {
            if is_dev {
                tracing::warn!(
                    "SECURITY: Using default JWT secret. This is acceptable in development but MUST be changed in production."
                );
            } else {
                eprintln!("FATAL: JWT secret is still the default value. Refusing to start in production.");
                eprintln!("Set XERGON_ENV=development to allow defaults, or configure a secure jwt_secret.");
                std::process::exit(1);
            }
        }

        let token_default = "xergon-dev-provider-token-change-me";
        if config.providers.registration_token == token_default {
            if is_dev {
                tracing::warn!(
                    "SECURITY: Using default provider registration token. This is acceptable in development but MUST be changed in production."
                );
            } else {
                eprintln!("FATAL: Provider registration token is still the default value. Refusing to start in production.");
                eprintln!("Set XERGON_ENV=development to allow defaults, or configure a secure registration_token.");
                std::process::exit(1);
            }
        }

        if config.relay.cors_origins == "*" && !is_dev {
            tracing::warn!(
                "SECURITY: CORS allows all origins — restrict in production"
            );
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_wildcard_warns_in_production() {
        assert!(should_warn_cors_wildcard("*", false));
    }

    #[test]
    fn test_cors_wildcard_no_warn_in_development() {
        assert!(!should_warn_cors_wildcard("*", true));
    }

    #[test]
    fn test_cors_specific_origin_no_warn() {
        assert!(!should_warn_cors_wildcard("https://xergon.ai", false));
        assert!(!should_warn_cors_wildcard("https://xergon.ai", true));
    }

    #[test]
    fn test_cors_comma_separated_no_warn() {
        assert!(!should_warn_cors_wildcard("https://a.com,https://b.com", false));
    }

    #[test]
    fn test_cors_empty_no_warn() {
        assert!(!should_warn_cors_wildcard("", false));
    }
}
