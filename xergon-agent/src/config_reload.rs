//! Config hot-reload: SIGHUP-based reload + file mtime polling.
//!
//! `ConfigReloader` watches the config file for changes (poll-based, checks
//! file mtime every 5 s) and also listens for Unix SIGHUP.  On either trigger
//! it re-reads, re-parses, and re-validates the config.  If valid the new
//! config is atomically swapped into the `Arc<RwLock<AgentConfig>>` that
//! subsystems read from; otherwise the old config is kept.

use crate::config::AgentConfig;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use tracing::{error, info, warn};

/// Result of the most recent reload attempt.
#[derive(Debug, Clone, Serialize)]
pub struct ReloadStatus {
    /// Absolute path of the watched config file.
    pub config_path: PathBuf,
    /// Timestamp of the last reload attempt (success or failure).
    pub last_attempt: DateTime<Utc>,
    /// `true` when the last attempt succeeded.
    pub last_success: bool,
    /// Human-readable summary of what changed (empty on failure).
    pub diff_summary: String,
    /// Error message when `last_success` is `false`.
    pub last_error: String,
    /// How many successful reloads have occurred.
    pub total_reloads: u64,
    /// How many failed reload attempts have occurred.
    pub total_failures: u64,
}

impl Default for ReloadStatus {
    fn default() -> Self {
        Self {
            config_path: PathBuf::new(),
            last_attempt: Utc::now(),
            last_success: true,
            diff_summary: String::new(),
            last_error: String::new(),
            total_reloads: 0,
            total_failures: 0,
        }
    }
}

/// Config hot-reloader.
///
/// Holds an `Arc<RwLock<AgentConfig>>` that subsystems share.  On reload the
/// inner value is replaced atomically.
pub struct ConfigReloader {
    /// Shared config that subsystems read.
    pub config: Arc<RwLock<AgentConfig>>,
    /// Path to the config file on disk.
    config_path: PathBuf,
    /// Watch channel — sent to on every successful reload so subsystems can
    /// react (rate-limiters, interval timers, etc.).
    reload_tx: watch::Sender<u64>,
    reload_rx: watch::Receiver<u64>,
    /// Mutable status metadata.
    status: Arc<RwLock<ReloadStatus>>,
    /// How often (seconds) to poll file mtime when no SIGHUP is received.
    poll_interval_secs: u64,
}

impl ConfigReloader {
    /// Create a new reloader wrapping the given initial config.
    ///
    /// `poll_interval_secs` controls the file-mtime poll cadence (default 5).
    pub fn new(
        initial_config: AgentConfig,
        config_path: PathBuf,
        poll_interval_secs: u64,
    ) -> Self {
        let (reload_tx, reload_rx) = watch::channel(0u64);
        let status = ReloadStatus {
            config_path: config_path.clone(),
            last_attempt: Utc::now(),
            last_success: true,
            diff_summary: "initial load".into(),
            ..Default::default()
        };
        Self {
            config: Arc::new(RwLock::new(initial_config)),
            config_path,
            reload_tx,
            reload_rx,
            status: Arc::new(RwLock::new(status)),
            poll_interval_secs: if poll_interval_secs == 0 {
                5
            } else {
                poll_interval_secs
            },
        }
    }

    /// Manually trigger a config reload (e.g. from an API endpoint).
    /// Returns the reload result status.
    pub async fn reload(&self) -> ReloadStatus {
        self.do_reload().await
    }

    /// Read-only access to current reload status.
    pub async fn get_config_status(&self) -> ReloadStatus {
        self.status.read().await.clone()
    }

    /// Get a receiver that is notified on every successful reload.
    /// The value is a monotonically increasing counter.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.reload_rx.clone()
    }

    /// Spawn two background tasks:
    ///   1. SIGHUP listener (Unix only)
    ///   2. File-mtime poll loop
    pub fn spawn_tasks(self: &Arc<Self>) {
        // SIGHUP handler
        let reloader_hup = Arc::clone(self);
        tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix;
                let mut sighup = match unix::signal(unix::SignalKind::hangup()) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(error = %e, "Failed to install SIGHUP handler");
                        return;
                    }
                };
                info!("SIGHUP handler installed -- send SIGHUP to reload config");
                loop {
                    sighup.recv().await;
                    info!("SIGHUP received -- reloading config");
                    reloader_hup.do_reload().await;
                }
            }
            #[cfg(not(unix))]
            {
                // No SIGHUP on non-Unix; just park this task.
                std::future::pending::<()>().await;
            }
        });

        // File-mtime poll loop
        let reloader_poll = Arc::clone(self);
        let interval = self.poll_interval_secs;
        tokio::spawn(async move {
            let mut last_mtime = reloader_poll.get_file_mtime();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                let current_mtime = reloader_poll.get_file_mtime();
                if current_mtime != last_mtime {
                    info!(
                        old_mtime = ?last_mtime,
                        new_mtime = ?current_mtime,
                        "Config file mtime changed -- reloading config"
                    );
                    reloader_poll.do_reload().await;
                    last_mtime = current_mtime;
                }
            }
        });
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    async fn do_reload(&self) -> ReloadStatus {
        // 1. Read & parse
        let new_config = match AgentConfig::load_from(Some(self.config_path.clone())) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!(error = %e, "Config reload failed: unable to parse");
                let mut st = self.status.write().await;
                st.last_attempt = Utc::now();
                st.last_success = false;
                st.last_error = format!("parse error: {e}");
                st.total_failures += 1;
                return st.clone();
            }
        };

        // 2. Validate
        if let Err(e) = new_config.validate() {
            warn!(error = %e, "Config reload failed: validation error");
            let mut st = self.status.write().await;
            st.last_attempt = Utc::now();
            st.last_success = false;
            st.last_error = format!("validation error: {e}");
            st.total_failures += 1;
            return st.clone();
        }

        // 3. Diff against old config
        let old_config = self.config.read().await;
        let diff = diff_configs(&old_config, &new_config);
        drop(old_config);

        // 4. Atomic swap
        *self.config.write().await = new_config;

        // 5. Update status
        let mut st = self.status.write().await;
        st.last_attempt = Utc::now();
        st.last_success = true;
        st.last_error = String::new();
        st.diff_summary = diff.summary.clone();
        st.total_reloads += 1;
        let changed_keys = diff.changed_keys.clone();

        info!(
            changed_keys = ?changed_keys,
            summary = %diff.summary,
            "Config reloaded successfully"
        );

        // 6. Notify watchers
        let _ = self.reload_tx.send(st.total_reloads);

        st.clone()
    }

    fn get_file_mtime(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(&self.config_path)
            .ok()
            .and_then(|m| m.modified().ok())
    }
}

// ------------------------------------------------------------------
// Config diffing helpers
// ------------------------------------------------------------------

struct ConfigDiff {
    changed_keys: Vec<String>,
    summary: String,
}

/// Produce a simple text diff between two configs by comparing their
/// serialised JSON representations key-by-key (top-level).
fn diff_configs(old: &AgentConfig, new: &AgentConfig) -> ConfigDiff {
    let old_json = serde_json::to_value(old).unwrap_or_default();
    let new_json = serde_json::to_value(new).unwrap_or_default();

    let mut changed_keys: Vec<String> = Vec::new();

    if let (Some(old_obj), Some(new_obj)) = (old_json.as_object(), new_json.as_object()) {
        // Collect all keys
        let all_keys: std::collections::HashSet<&String> = old_obj
            .keys()
            .chain(new_obj.keys())
            .collect();

        for key in all_keys {
            let old_val = old_obj.get(key);
            let new_val = new_obj.get(key);
            if old_val != new_val {
                changed_keys.push(key.clone());
            }
        }
    }

    let summary = if changed_keys.is_empty() {
        "no changes detected".into()
    } else {
        format!("changed: {}", changed_keys.join(", "))
    };

    ConfigDiff {
        changed_keys,
        summary,
    }
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_empty() {
        // Two identical default configs
        let a = AgentConfig::load_from(None).unwrap();
        let b = AgentConfig::load_from(None).unwrap();
        let diff = diff_configs(&a, &b);
        assert!(diff.changed_keys.is_empty());
        assert_eq!(diff.summary, "no changes detected");
    }
}
