//! Download progress tracking for model pulls.
//!
//! Provides real-time progress tracking with:
//! - Per-model download progress (bytes, percentage, speed, ETA)
//! - Broadcast event channel for SSE streaming
//! - Cancel support via cancellation tokens

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::debug;

/// Status of a model download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    /// Download is queued but not yet started.
    Pending,
    /// Actively downloading bytes.
    Downloading,
    /// Verifying download integrity.
    Verifying,
    /// Download completed successfully.
    Completed,
    /// Download failed.
    Failed,
    /// Download was cancelled by user.
    Cancelled,
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Downloading => write!(f, "downloading"),
            Self::Verifying => write!(f, "verifying"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Snapshot of download progress for a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgressSnapshot {
    /// Model name (normalized to lowercase).
    pub model: String,
    /// Current download status.
    pub status: DownloadStatus,
    /// Bytes downloaded so far.
    pub bytes_downloaded: u64,
    /// Total bytes to download (0 if unknown).
    pub total_bytes: u64,
    /// Download percentage (0.0 - 100.0). -1.0 if total_bytes is unknown.
    pub percentage: f64,
    /// Current download speed in bytes/sec.
    pub speed_bytes_per_sec: f64,
    /// Estimated seconds remaining (0 if unknown or completed).
    pub eta_secs: u64,
    /// Seconds elapsed since download started.
    pub elapsed_secs: u64,
    /// Error message (only set when status == Failed).
    pub error: Option<String>,
}

/// A progress event broadcast to all SSE subscribers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Model name.
    pub model: String,
    /// Current status.
    pub status: DownloadStatus,
    /// Bytes downloaded.
    pub bytes_downloaded: u64,
    /// Total bytes.
    pub total_bytes: u64,
    /// Percentage (0.0-100.0, or -1.0 if unknown).
    pub percentage: f64,
    /// Speed in bytes/sec.
    pub speed_bytes_per_sec: f64,
    /// ETA in seconds.
    pub eta_secs: u64,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Internal per-model download state.
struct DownloadState {
    status: DownloadStatus,
    bytes_downloaded: u64,
    total_bytes: u64,
    started_at: Instant,
    /// Timestamp and byte count for speed calculation.
    last_speed_sample: Instant,
    last_speed_bytes: u64,
    speed_bytes_per_sec: f64,
    error: Option<String>,
    cancelled: Arc<AtomicBool>,
}

impl DownloadState {
    fn new(model: &str) -> Self {
        Self {
            status: DownloadStatus::Pending,
            bytes_downloaded: 0,
            total_bytes: 0,
            started_at: Instant::now(),
            last_speed_sample: Instant::now(),
            last_speed_bytes: 0,
            speed_bytes_per_sec: 0.0,
            error: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn snapshot(&self, model: &str) -> DownloadProgressSnapshot {
        let percentage = if self.total_bytes > 0 {
            (self.bytes_downloaded as f64 / self.total_bytes as f64) * 100.0
        } else {
            -1.0
        };

        let eta_secs = if self.speed_bytes_per_sec > 0.0 && self.total_bytes > self.bytes_downloaded {
            let remaining = self.total_bytes - self.bytes_downloaded;
            (remaining as f64 / self.speed_bytes_per_sec) as u64
        } else {
            0
        };

        DownloadProgressSnapshot {
            model: model.to_string(),
            status: self.status,
            bytes_downloaded: self.bytes_downloaded,
            total_bytes: self.total_bytes,
            percentage,
            speed_bytes_per_sec: self.speed_bytes_per_sec,
            eta_secs,
            elapsed_secs: self.started_at.elapsed().as_secs(),
            error: self.error.clone(),
        }
    }

    fn to_event(&self, model: &str) -> ProgressEvent {
        let percentage = if self.total_bytes > 0 {
            (self.bytes_downloaded as f64 / self.total_bytes as f64) * 100.0
        } else {
            -1.0
        };

        let eta_secs = if self.speed_bytes_per_sec > 0.0 && self.total_bytes > self.bytes_downloaded {
            let remaining = self.total_bytes - self.bytes_downloaded;
            (remaining as f64 / self.speed_bytes_per_sec) as u64
        } else {
            0
        };

        ProgressEvent {
            model: model.to_string(),
            status: self.status,
            bytes_downloaded: self.bytes_downloaded,
            total_bytes: self.total_bytes,
            percentage,
            speed_bytes_per_sec: self.speed_bytes_per_sec,
            eta_secs,
            error: self.error.clone(),
        }
    }

    /// Update speed calculation. Should be called periodically.
    fn update_speed(&mut self) {
        let elapsed = self.last_speed_sample.elapsed().as_secs_f64();
        if elapsed >= 0.5 {
            // Recalculate speed every 0.5s minimum
            let bytes_delta = self.bytes_downloaded.saturating_sub(self.last_speed_bytes);
            self.speed_bytes_per_sec = bytes_delta as f64 / elapsed;
            self.last_speed_sample = Instant::now();
            self.last_speed_bytes = self.bytes_downloaded;
        }
    }
}

/// Progress tracker for all active model downloads.
///
/// Thread-safe, uses DashMap for concurrent access and broadcast channel
/// for real-time event streaming to SSE subscribers.
#[derive(Clone)]
pub struct ProgressTracker {
    /// Map from model name (lowercase) to download state.
    downloads: Arc<DashMap<String, DownloadState>>,
    /// Broadcast channel for progress events.
    tx: broadcast::Sender<ProgressEvent>,
    /// Broadcast channel capacity.
    _capacity: usize,
}

impl ProgressTracker {
    /// Create a new progress tracker.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            downloads: Arc::new(DashMap::new()),
            tx,
            _capacity: 256,
        }
    }

    /// Register a new download for tracking. Returns a cancellation token.
    pub fn start_download(&self, model: &str) -> Arc<AtomicBool> {
        let model_lower = model.to_lowercase();
        let state = DownloadState::new(&model_lower);
        let cancel_token = state.cancelled.clone();
        self.downloads.insert(model_lower.clone(), state);

        debug!(model = %model_lower, "Started tracking download progress");
        cancel_token
    }

    /// Update download progress with new byte counts.
    pub fn update_progress(
        &self,
        model: &str,
        bytes_downloaded: u64,
        total_bytes: u64,
    ) {
        let model_lower = model.to_lowercase();
        if let Some(mut state) = self.downloads.get_mut(&model_lower) {
            if state.cancelled.load(Ordering::Relaxed) {
                return;
            }

            state.status = DownloadStatus::Downloading;
            state.bytes_downloaded = bytes_downloaded;
            state.total_bytes = total_bytes;
            state.update_speed();

            let event = state.to_event(&model_lower);
            let _ = self.tx.send(event);
        }
    }

    /// Set the download status to Verifying.
    pub fn set_verifying(&self, model: &str) {
        let model_lower = model.to_lowercase();
        if let Some(mut state) = self.downloads.get_mut(&model_lower) {
            state.status = DownloadStatus::Verifying;
            let event = state.to_event(&model_lower);
            let _ = self.tx.send(event);
        }
    }

    /// Mark a download as completed.
    pub fn mark_completed(&self, model: &str) {
        let model_lower = model.to_lowercase();
        if let Some(mut state) = self.downloads.get_mut(&model_lower) {
            state.status = DownloadStatus::Completed;
            // If total_bytes was unknown, set it to bytes_downloaded for 100%
            if state.total_bytes == 0 {
                state.total_bytes = state.bytes_downloaded;
            }
            state.bytes_downloaded = state.total_bytes; // ensure 100%
            state.update_speed();

            let event = state.to_event(&model_lower);
            let _ = self.tx.send(event);
            debug!(model = %model_lower, elapsed_secs = state.started_at.elapsed().as_secs(), "Download completed");
        }
    }

    /// Mark a download as failed.
    pub fn mark_failed(&self, model: &str, error: &str) {
        let model_lower = model.to_lowercase();
        if let Some(mut state) = self.downloads.get_mut(&model_lower) {
            state.status = DownloadStatus::Failed;
            state.error = Some(error.to_string());
            state.update_speed();

            let event = state.to_event(&model_lower);
            let _ = self.tx.send(event);
            debug!(model = %model_lower, error = %error, "Download failed");
        }
    }

    /// Mark a download as cancelled.
    pub fn mark_cancelled(&self, model: &str) {
        let model_lower = model.to_lowercase();
        if let Some(mut state) = self.downloads.get_mut(&model_lower) {
            state.status = DownloadStatus::Cancelled;
            state.cancelled.store(true, Ordering::Relaxed);
            state.error = Some("Cancelled by user".to_string());

            let event = state.to_event(&model_lower);
            let _ = self.tx.send(event);
            debug!(model = %model_lower, "Download cancelled");
        }
    }

    /// Request cancellation of a download. Returns true if an active download was found.
    pub fn request_cancel(&self, model: &str) -> bool {
        let model_lower = model.to_lowercase();
        if let Some(state) = self.downloads.get(&model_lower) {
            let current_status = state.status;
            let is_active = matches!(
                current_status,
                DownloadStatus::Pending | DownloadStatus::Downloading | DownloadStatus::Verifying
            );
            if is_active {
                state.cancelled.store(true, Ordering::Relaxed);
                drop(state); // release borrow before calling mark_cancelled
                self.mark_cancelled(model);
                return true;
            }
        }
        false
    }

    /// Check if a download has been cancelled.
    pub fn is_cancelled(&self, model: &str) -> bool {
        let model_lower = model.to_lowercase();
        self.downloads
            .get(&model_lower)
            .map(|s| s.cancelled.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Get progress snapshot for a specific model.
    pub fn get_progress(&self, model: &str) -> Option<DownloadProgressSnapshot> {
        let model_lower = model.to_lowercase();
        self.downloads.get(&model_lower).map(|s| s.snapshot(&model_lower))
    }

    /// Get progress snapshots for all tracked downloads.
    pub fn get_all_progress(&self) -> Vec<DownloadProgressSnapshot> {
        self.downloads
            .iter()
            .map(|entry| entry.value().snapshot(entry.key()))
            .collect()
    }

    /// Subscribe to progress events for SSE streaming.
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressEvent> {
        self.tx.subscribe()
    }

    /// Remove a completed/failed download from tracking (cleanup).
    pub fn remove(&self, model: &str) {
        let model_lower = model.to_lowercase();
        self.downloads.remove(&model_lower);
    }

    /// Remove all completed/failed downloads older than the given duration.
    pub fn cleanup(&self, max_age: std::time::Duration) {
        let to_remove: Vec<String> = self
            .downloads
            .iter()
            .filter(|entry| {
                let state = entry.value();
                matches!(state.status, DownloadStatus::Completed | DownloadStatus::Failed | DownloadStatus::Cancelled)
                    && state.started_at.elapsed() > max_age
            })
            .map(|entry| entry.key().clone())
            .collect();

        for model in to_remove {
            self.downloads.remove(&model);
        }
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_tracker_lifecycle() {
        let tracker = ProgressTracker::new();
        let cancel = tracker.start_download("llama3:8b");

        let progress = tracker.get_progress("llama3:8b").unwrap();
        assert_eq!(progress.status, DownloadStatus::Pending);
        assert_eq!(progress.bytes_downloaded, 0);

        tracker.update_progress("llama3:8b", 500_000_000, 4_000_000_000);
        let progress = tracker.get_progress("llama3:8b").unwrap();
        assert_eq!(progress.status, DownloadStatus::Downloading);
        assert_eq!(progress.bytes_downloaded, 500_000_000);
        assert!((progress.percentage - 12.5).abs() < 0.1);

        tracker.mark_completed("llama3:8b");
        let progress = tracker.get_progress("llama3:8b").unwrap();
        assert_eq!(progress.status, DownloadStatus::Completed);

        // Cancel token should still work
        assert!(!cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn test_cancel_flow() {
        let tracker = ProgressTracker::new();
        tracker.start_download("test-model");
        tracker.update_progress("test-model", 100, 1000);

        assert!(tracker.request_cancel("test-model"));

        let progress = tracker.get_progress("test-model").unwrap();
        assert_eq!(progress.status, DownloadStatus::Cancelled);
        assert!(tracker.is_cancelled("test-model"));

        // Requesting cancel again should return false
        assert!(!tracker.request_cancel("test-model"));
    }

    #[test]
    fn test_broadcast_events() {
        let tracker = ProgressTracker::new();
        let mut rx = tracker.subscribe();

        tracker.start_download("event-model");
        // Initial start doesn't broadcast (no speed data yet)

        tracker.update_progress("event-model", 100, 1000);
        let event = rx.try_recv().unwrap();
        assert_eq!(event.model, "event-model");
        assert_eq!(event.status, DownloadStatus::Downloading);

        tracker.mark_failed("event-model", "network error");
        let event = rx.try_recv().unwrap();
        assert_eq!(event.status, DownloadStatus::Failed);
        assert_eq!(event.error.as_deref(), Some("network error"));
    }

    #[test]
    fn test_get_all_progress() {
        let tracker = ProgressTracker::new();
        tracker.start_download("model-a");
        tracker.start_download("model-b");

        let all = tracker.get_all_progress();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_snapshot_serialization() {
        let tracker = ProgressTracker::new();
        tracker.start_download("llama3:8b");
        tracker.update_progress("llama3:8b", 1_000_000_000, 4_000_000_000);

        let snapshot = tracker.get_progress("llama3:8b").unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("downloading"));
        assert!(json.contains("llama3:8b"));
    }
}
