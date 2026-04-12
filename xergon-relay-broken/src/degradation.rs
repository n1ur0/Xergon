//! Graceful Degradation module for adaptive service level management.
//!
//! Under heavy load or failure conditions, the relay can automatically reduce
//! its service level to protect core functionality.
//!
//! Degradation levels:
//!   - Full: All endpoints available, no limits.
//!   - Reduced: Chat max_tokens limited, non-essential endpoints return cached data.
//!   - Minimal: Only health, metrics, chat available; everything else returns 503.
//!   - Maintenance: All endpoints return 503 with retry-after header.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{info};

// ---------------------------------------------------------------------------
// Degradation Level
// ---------------------------------------------------------------------------

/// Service degradation level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationLevel {
    /// Full service — all endpoints available, no limits.
    Full = 0,
    /// Reduced service — chat limited, non-essential endpoints cached.
    Reduced = 1,
    /// Minimal service — only health, metrics, chat available.
    Minimal = 2,
    /// Maintenance mode — all endpoints return 503.
    Maintenance = 3,
}

impl std::fmt::Display for DegradationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DegradationLevel::Full => write!(f, "full"),
            DegradationLevel::Reduced => write!(f, "reduced"),
            DegradationLevel::Minimal => write!(f, "minimal"),
            DegradationLevel::Maintenance => write!(f, "maintenance"),
        }
    }
}

impl DegradationLevel {
    /// Parse from a string (case-insensitive).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "full" => DegradationLevel::Full,
            "reduced" => DegradationLevel::Reduced,
            "minimal" => DegradationLevel::Minimal,
            "maintenance" => DegradationLevel::Maintenance,
            _ => DegradationLevel::Full,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for graceful degradation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationConfig {
    /// Enable/disable degradation management (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Automatically degrade based on system health (default: true).
    #[serde(default = "default_auto_degrade")]
    pub auto_degrade: bool,
    /// Max tokens for chat in reduced mode (default: 512).
    #[serde(default = "default_reduced_max_tokens")]
    pub reduced_max_tokens: u32,
    /// Endpoints available in minimal mode (default: health, metrics, chat).
    #[serde(default = "default_minimal_endpoints")]
    pub minimal_endpoints: Vec<String>,
    /// Message returned during maintenance mode.
    #[serde(default = "default_maintenance_message")]
    pub maintenance_message: String,
}

impl Default for DegradationConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            auto_degrade: default_auto_degrade(),
            reduced_max_tokens: default_reduced_max_tokens(),
            minimal_endpoints: default_minimal_endpoints(),
            maintenance_message: default_maintenance_message(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_auto_degrade() -> bool {
    true
}
fn default_reduced_max_tokens() -> u32 {
    512
}
fn default_minimal_endpoints() -> Vec<String> {
    vec![
        "/health".into(),
        "/ready".into(),
        "/v1/chat/completions".into(),
        "/v1/models".into(),
        "/v1/metrics".into(),
    ]
}
fn default_maintenance_message() -> String {
    "Service temporarily unavailable. Please retry later.".into()
}

// ---------------------------------------------------------------------------
// Degraded Response Config
// ---------------------------------------------------------------------------

/// Configuration applied to responses when degraded.
#[derive(Debug, Clone, Serialize)]
pub struct DegradedResponseConfig {
    /// Current degradation level.
    pub level: DegradationLevel,
    /// Max tokens for chat completions (None = no limit).
    pub max_tokens_limit: Option<u32>,
    /// Whether to use cached responses where possible.
    pub prefer_cached: bool,
    /// Suggested retry-after seconds for clients (0 = no header).
    pub retry_after_secs: u64,
}

// ---------------------------------------------------------------------------
// Degradation Manager
// ---------------------------------------------------------------------------

/// Manages the service degradation level.
pub struct DegradationManager {
    config: DegradationConfig,
    /// Current degradation level (atomic for lock-free reads).
    current_level: AtomicU8,
}

impl DegradationManager {
    /// Create a new degradation manager.
    pub fn new(config: DegradationConfig) -> Self {
        Self {
            config,
            current_level: AtomicU8::new(DegradationLevel::Full as u8),
        }
    }

    /// Get the current degradation level.
    pub fn current_level(&self) -> DegradationLevel {
        match self.current_level.load(Ordering::Relaxed) {
            0 => DegradationLevel::Full,
            1 => DegradationLevel::Reduced,
            2 => DegradationLevel::Minimal,
            3 => DegradationLevel::Maintenance,
            _ => DegradationLevel::Full,
        }
    }

    /// Manually set the degradation level.
    pub fn set_level(&self, level: DegradationLevel) {
        let old = self.current_level.swap(level as u8, Ordering::Relaxed);
        let old_level = match old {
            0 => DegradationLevel::Full,
            1 => DegradationLevel::Reduced,
            2 => DegradationLevel::Minimal,
            3 => DegradationLevel::Maintenance,
            _ => DegradationLevel::Full,
        };
        if old_level != level {
            info!(
                old_level = %old_level,
                new_level = %level,
                "Degradation level changed"
            );
        }
    }

    /// Auto-assess system health and adjust degradation level.
    ///
    /// Takes the current load factor (0.0-1.0+) and open circuit count.
    pub fn auto_assess(&self, load_factor: f64, open_circuits: usize) {
        if !self.config.enabled || !self.config.auto_degrade {
            return;
        }

        let current = self.current_level();

        // Only auto-escalate, never auto-de-escalate (manual intervention to recover)
        let new_level = match current {
            DegradationLevel::Full => {
                if load_factor > 0.95 || open_circuits > 0 {
                    DegradationLevel::Reduced
                } else {
                    DegradationLevel::Full
                }
            }
            DegradationLevel::Reduced => {
                if load_factor > 1.2 || open_circuits > 2 {
                    DegradationLevel::Minimal
                } else {
                    DegradationLevel::Reduced
                }
            }
            DegradationLevel::Minimal | DegradationLevel::Maintenance => current,
        };

        if new_level as u8 != current as u8 {
            self.set_level(new_level);
        }
    }

    /// Check whether an endpoint is available at the current degradation level.
    pub fn is_endpoint_available(&self, path: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let level = self.current_level();

        match level {
            DegradationLevel::Full => true,
            DegradationLevel::Reduced => {
                // Most endpoints available, some may return cached data
                // Block expensive endpoints like GPU listings in reduced mode
                !path.starts_with("/v1/gpu/")
            }
            DegradationLevel::Minimal => {
                // Only minimal endpoints available
                self.config
                    .minimal_endpoints
                    .iter()
                    .any(|ep| path == ep || path.starts_with(&format!("{}/", ep.trim_end_matches('/'))))
            }
            DegradationLevel::Maintenance => {
                // Only health endpoints
                path == "/health" || path == "/ready"
            }
        }
    }

    /// Get the degraded response configuration for the current level.
    pub fn get_degraded_config(&self) -> DegradedResponseConfig {
        let level = self.current_level();
        match level {
            DegradationLevel::Full => DegradedResponseConfig {
                level,
                max_tokens_limit: None,
                prefer_cached: false,
                retry_after_secs: 0,
            },
            DegradationLevel::Reduced => DegradedResponseConfig {
                level,
                max_tokens_limit: Some(self.config.reduced_max_tokens),
                prefer_cached: true,
                retry_after_secs: 0,
            },
            DegradationLevel::Minimal => DegradedResponseConfig {
                level,
                max_tokens_limit: Some(self.config.reduced_max_tokens),
                prefer_cached: true,
                retry_after_secs: 30,
            },
            DegradationLevel::Maintenance => DegradedResponseConfig {
                level,
                max_tokens_limit: None,
                prefer_cached: false,
                retry_after_secs: 60,
            },
        }
    }

    /// Get the maintenance message.
    pub fn maintenance_message(&self) -> &str {
        &self.config.maintenance_message
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> DegradationConfig {
        DegradationConfig {
            enabled: true,
            auto_degrade: true,
            reduced_max_tokens: 512,
            minimal_endpoints: vec![
                "/health".into(),
                "/ready".into(),
                "/v1/chat/completions".into(),
                "/v1/models".into(),
            ],
            maintenance_message: "Under maintenance".into(),
        }
    }

    #[test]
    fn test_default_config() {
        let cfg = DegradationConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.auto_degrade);
        assert_eq!(cfg.reduced_max_tokens, 512);
    }

    #[test]
    fn test_starts_at_full() {
        let dm = DegradationManager::new(make_config());
        assert_eq!(dm.current_level(), DegradationLevel::Full);
    }

    #[test]
    fn test_set_level() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Reduced);
        assert_eq!(dm.current_level(), DegradationLevel::Reduced);

        dm.set_level(DegradationLevel::Maintenance);
        assert_eq!(dm.current_level(), DegradationLevel::Maintenance);
    }

    #[test]
    fn test_full_all_endpoints_available() {
        let dm = DegradationManager::new(make_config());
        assert!(dm.is_endpoint_available("/v1/chat/completions"));
        assert!(dm.is_endpoint_available("/v1/models"));
        assert!(dm.is_endpoint_available("/v1/gpu/listings"));
        assert!(dm.is_endpoint_available("/v1/providers"));
    }

    #[test]
    fn test_reduced_blocks_gpu() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Reduced);

        assert!(dm.is_endpoint_available("/v1/chat/completions"));
        assert!(dm.is_endpoint_available("/v1/models"));
        assert!(!dm.is_endpoint_available("/v1/gpu/listings"));
        assert!(!dm.is_endpoint_available("/v1/gpu/rent"));
    }

    #[test]
    fn test_minimal_only_allowed_endpoints() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Minimal);

        assert!(dm.is_endpoint_available("/health"));
        assert!(dm.is_endpoint_available("/ready"));
        assert!(dm.is_endpoint_available("/v1/chat/completions"));
        assert!(dm.is_endpoint_available("/v1/models"));
        assert!(!dm.is_endpoint_available("/v1/providers"));
        assert!(!dm.is_endpoint_available("/v1/gpu/listings"));
        assert!(!dm.is_endpoint_available("/v1/leaderboard"));
    }

    #[test]
    fn test_maintenance_only_health() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Maintenance);

        assert!(dm.is_endpoint_available("/health"));
        assert!(dm.is_endpoint_available("/ready"));
        assert!(!dm.is_endpoint_available("/v1/chat/completions"));
        assert!(!dm.is_endpoint_available("/v1/models"));
    }

    #[test]
    fn test_degraded_config_full() {
        let dm = DegradationManager::new(make_config());
        let cfg = dm.get_degraded_config();
        assert_eq!(cfg.level, DegradationLevel::Full);
        assert!(cfg.max_tokens_limit.is_none());
        assert!(!cfg.prefer_cached);
        assert_eq!(cfg.retry_after_secs, 0);
    }

    #[test]
    fn test_degraded_config_reduced() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Reduced);
        let cfg = dm.get_degraded_config();
        assert_eq!(cfg.level, DegradationLevel::Reduced);
        assert_eq!(cfg.max_tokens_limit, Some(512));
        assert!(cfg.prefer_cached);
        assert_eq!(cfg.retry_after_secs, 0);
    }

    #[test]
    fn test_degraded_config_maintenance() {
        let dm = DegradationManager::new(make_config());
        dm.set_level(DegradationLevel::Maintenance);
        let cfg = dm.get_degraded_config();
        assert_eq!(cfg.retry_after_secs, 60);
    }

    #[test]
    fn test_auto_assess_escalates() {
        let dm = DegradationManager::new(make_config());
        assert_eq!(dm.current_level(), DegradationLevel::Full);

        // High load should escalate to Reduced
        dm.auto_assess(0.99, 0);
        assert_eq!(dm.current_level(), DegradationLevel::Reduced);

        // Very high load should escalate to Minimal
        dm.auto_assess(1.5, 5);
        assert_eq!(dm.current_level(), DegradationLevel::Minimal);
    }

    #[test]
    fn test_auto_assess_no_escalate_normal_load() {
        let dm = DegradationManager::new(make_config());
        dm.auto_assess(0.5, 0);
        assert_eq!(dm.current_level(), DegradationLevel::Full);
    }

    #[test]
    fn test_auto_assess_disabled() {
        let config = DegradationConfig {
            auto_degrade: false,
            ..make_config()
        };
        let dm = DegradationManager::new(config);
        dm.auto_assess(2.0, 10);
        assert_eq!(dm.current_level(), DegradationLevel::Full);
    }

    #[test]
    fn test_degradation_level_display() {
        assert_eq!(format!("{}", DegradationLevel::Full), "full");
        assert_eq!(format!("{}", DegradationLevel::Reduced), "reduced");
        assert_eq!(format!("{}", DegradationLevel::Minimal), "minimal");
        assert_eq!(format!("{}", DegradationLevel::Maintenance), "maintenance");
    }

    #[test]
    fn test_degradation_level_serialize() {
        assert_eq!(
            serde_json::to_string(&DegradationLevel::Full).unwrap(),
            "\"full\""
        );
        assert_eq!(
            serde_json::to_string(&DegradationLevel::Maintenance).unwrap(),
            "\"maintenance\""
        );
    }

    #[test]
    fn test_from_str_lossy() {
        assert_eq!(DegradationLevel::from_str_lossy("full"), DegradationLevel::Full);
        assert_eq!(DegradationLevel::from_str_lossy("REDUCED"), DegradationLevel::Reduced);
        assert_eq!(DegradationLevel::from_str_lossy("Minimal"), DegradationLevel::Minimal);
        assert_eq!(DegradationLevel::from_str_lossy("unknown"), DegradationLevel::Full);
    }

    #[test]
    fn test_maintenance_message() {
        let dm = DegradationManager::new(make_config());
        assert_eq!(dm.maintenance_message(), "Under maintenance");
    }

    #[test]
    fn test_disabled_all_endpoints_available() {
        let config = DegradationConfig {
            enabled: false,
            ..make_config()
        };
        let dm = DegradationManager::new(config);
        dm.set_level(DegradationLevel::Maintenance);
        // Even in maintenance, if disabled, all endpoints are available
        assert!(dm.is_endpoint_available("/v1/chat/completions"));
    }
}
