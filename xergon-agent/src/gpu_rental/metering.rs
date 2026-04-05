//! GPU usage metering — session tracking, hourly deduction, expiration.
//!
//! Tracks active rental sessions, computes elapsed usage against purchased
//! hours, and expires sessions when time runs out or the on-chain deadline
//! height is reached.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::gpu_rental::types::{GpuListingBox, GpuRentalBox};
use crate::gpu_rental::tunnel::TunnelInfo;

// ---------------------------------------------------------------------------
// Session data
// ---------------------------------------------------------------------------

/// A single active rental session tracked by the metering subsystem.
pub struct RentalSession {
    pub rental_box_id: String,
    pub listing_box_id: String,
    pub provider_pk: String,
    pub renter_pk: String,
    pub started_at: Instant,
    pub hours_purchased: u32,
    pub deadline_height: i32,
    /// ERG escrowed in the rental box (nanoERG)
    pub escrow_nanoerg: u64,
    /// Price per hour (nanoERG)
    pub price_per_hour_nanoerg: u64,
    /// Whether the session is still active
    pub active: AtomicBool,
    /// Optional tunnel associated with this session
    pub tunnel: Option<TunnelInfo>,
}

// ---------------------------------------------------------------------------
// Usage snapshot — returned by API / used for logging
// ---------------------------------------------------------------------------

/// Point-in-time usage snapshot for a rental session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub rental_box_id: String,
    pub listing_box_id: String,
    pub hours_purchased: u32,
    pub hours_used: f64,
    pub hours_remaining: f64,
    pub erg_spent: f64,
    pub erg_total_escrow: f64,
    pub is_expired: bool,
    pub is_active: bool,
    pub tunnel: Option<TunnelInfo>,
}

/// Data returned when a session is stopped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoppedSession {
    pub rental_box_id: String,
    pub hours_purchased: u32,
    pub hours_used: f64,
    pub erg_spent: f64,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// UsageMeter
// ---------------------------------------------------------------------------

/// Tracks GPU usage per rental and handles automatic expiration.
///
/// Lives as a shared `Arc<UsageMeter>` inside `AppState`.
pub struct UsageMeter {
    /// Active rental sessions, keyed by rental_box_id
    sessions: DashMap<String, RentalSession>,
    /// How often (in seconds) the background loop checks for expirations
    check_interval_secs: u64,
}

impl UsageMeter {
    /// Create a new usage meter.
    pub fn new(check_interval_secs: u64) -> Self {
        Self {
            sessions: DashMap::new(),
            check_interval_secs,
        }
    }

    /// Start tracking a new rental session.
    ///
    /// Returns an error if a session for the same `rental_box_id` already
    /// exists and is still active.
    pub fn start_session(
        &self,
        rental: &GpuRentalBox,
        listing: &GpuListingBox,
    ) -> Result<()> {
        if self.sessions.contains_key(&rental.box_id) {
            bail!(
                "Session already exists for rental box {}",
                rental.box_id
            );
        }

        let session = RentalSession {
            rental_box_id: rental.box_id.clone(),
            listing_box_id: rental.listing_box_id.clone(),
            provider_pk: rental.provider_pk.clone(),
            renter_pk: rental.renter_pk.clone(),
            started_at: Instant::now(),
            hours_purchased: rental.hours_rented.max(0) as u32,
            deadline_height: rental.deadline_height,
            escrow_nanoerg: rental.value_nanoerg,
            price_per_hour_nanoerg: listing.price_per_hour_nanoerg,
            active: AtomicBool::new(true),
            tunnel: None,
        };

        self.sessions.insert(rental.box_id.clone(), session);

        info!(
            rental_box_id = %rental.box_id,
            hours_purchased = rental.hours_rented.max(0),
            deadline_height = rental.deadline_height,
            escrow_erg = rental.value_nanoerg as f64 / 1e9,
            "Rental session started"
        );

        Ok(())
    }

    /// Stop a session (user disconnects, rental expires, or is cancelled).
    ///
    /// Returns the final usage stats. If the session doesn't exist or is
    /// already stopped, returns an error.
    pub fn stop_session(&self, rental_box_id: &str, reason: &str) -> Result<StoppedSession> {
        let entry = self
            .sessions
            .get_mut(rental_box_id)
            .ok_or_else(|| anyhow::anyhow!("No session found for rental box {}", rental_box_id))?;

        if !entry.active.load(Ordering::Relaxed) {
            bail!("Session for {} is already stopped", rental_box_id);
        }

        entry.active.store(false, Ordering::Relaxed);

        let hours_used = entry.started_at.elapsed().as_secs_f64() / 3600.0;
        let hours_purchased = entry.hours_purchased;
        let erg_spent = if hours_purchased > 0 && entry.price_per_hour_nanoerg > 0 {
            // Charge for the lesser of used vs purchased
            let chargeable = hours_used.min(hours_purchased as f64);
            chargeable * entry.price_per_hour_nanoerg as f64 / 1e9
        } else {
            0.0
        };

        let stopped = StoppedSession {
            rental_box_id: rental_box_id.to_string(),
            hours_purchased,
            hours_used,
            erg_spent,
            reason: reason.to_string(),
        };

        info!(
            rental_box_id = %rental_box_id,
            hours_used = hours_used,
            erg_spent = erg_spent,
            reason = %reason,
            "Rental session stopped"
        );

        Ok(stopped)
    }

    /// Get current usage for a specific rental session.
    pub fn get_usage(&self, rental_box_id: &str) -> Option<UsageSnapshot> {
        self.sessions
            .get(rental_box_id)
            .map(|entry| self.snapshot_for(entry.value()))
    }

    /// List all active sessions (includes recently expired ones still in map).
    pub fn active_sessions(&self) -> Vec<UsageSnapshot> {
        self.sessions
            .iter()
            .filter(|e| e.value().active.load(Ordering::Relaxed))
            .map(|e| self.snapshot_for(e.value()))
            .collect()
    }

    /// List all sessions (active and inactive).
    pub fn all_sessions(&self) -> Vec<UsageSnapshot> {
        self.sessions
            .iter()
            .map(|e| self.snapshot_for(e.value()))
            .collect()
    }

    /// Associate a tunnel with a session.
    pub fn attach_tunnel(&self, rental_box_id: &str, tunnel: TunnelInfo) -> Result<()> {
        let mut entry = self
            .sessions
            .get_mut(rental_box_id)
            .ok_or_else(|| anyhow::anyhow!("No session found for rental box {}", rental_box_id))?;

        entry.tunnel = Some(tunnel);
        Ok(())
    }

    /// Get the number of active sessions.
    pub fn active_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|e| e.value().active.load(Ordering::Relaxed))
            .count()
    }

    // -----------------------------------------------------------------------
    // Background metering loop
    // -----------------------------------------------------------------------

    /// Spawn a background tokio task that periodically checks all sessions
    /// for expiration (time exceeded or chain deadline reached).
    ///
    /// On expiration the session is marked inactive and any associated
    /// tunnel is closed.
    pub fn spawn_metering_loop(self: &Arc<Self>, chain_client: ErgoNodeClient) {
        let meter = Arc::clone(self);
        let interval = self.check_interval_secs;

        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval));

            loop {
                tick.tick().await;

                // 1. Get current chain height (best-effort)
                let current_height = match chain_client.get_height().await {
                    Ok(h) => h,
                    Err(e) => {
                        warn!(error = %e, "Failed to fetch chain height for metering check");
                        continue;
                    }
                };

                // 2. Iterate sessions and expire overdue ones
                let mut to_stop: Vec<(String, String)> = Vec::new();

                for entry in meter.sessions.iter() {
                    let session: &RentalSession = entry.value();
                    if !session.active.load(Ordering::Relaxed) {
                        continue;
                    }

                    let hours_used = session.started_at.elapsed().as_secs_f64() / 3600.0;
                    let hours_purchased = session.hours_purchased as f64;

                    if hours_used >= hours_purchased {
                        to_stop.push((
                            entry.key().clone(),
                            "time_expired".to_string(),
                        ));
                    } else if current_height >= session.deadline_height {
                        to_stop.push((
                            entry.key().clone(),
                            "deadline_height_reached".to_string(),
                        ));
                    }
                }

                // 3. Stop expired sessions (outside the DashMap iterator)
                for (rental_box_id, reason) in &to_stop {
                    if let Err(e) = meter.stop_session(rental_box_id, reason) {
                        warn!(
                            rental_box_id = %rental_box_id,
                            error = %e,
                            "Failed to stop expired session"
                        );
                    }
                }
            }
        });

        info!(
            interval_secs = interval,
            "GPU usage metering loop started"
        );
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn snapshot_for(&self, s: &RentalSession) -> UsageSnapshot {
        let hours_used = s.started_at.elapsed().as_secs_f64() / 3600.0;
        let hours_purchased = s.hours_purchased as f64;
        let hours_remaining = (hours_purchased - hours_used).max(0.0);
        let erg_spent = if s.price_per_hour_nanoerg > 0 {
            let chargeable = hours_used.min(hours_purchased);
            chargeable * s.price_per_hour_nanoerg as f64 / 1e9
        } else {
            0.0
        };
        let is_expired = hours_used >= hours_purchased;
        let is_active = s.active.load(Ordering::Relaxed);

        UsageSnapshot {
            rental_box_id: s.rental_box_id.clone(),
            listing_box_id: s.listing_box_id.clone(),
            hours_purchased: s.hours_purchased,
            hours_used,
            hours_remaining,
            erg_spent,
            erg_total_escrow: s.escrow_nanoerg as f64 / 1e9,
            is_expired,
            is_active,
            tunnel: s.tunnel.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_listing(price_per_hour_nanoerg: u64) -> GpuListingBox {
        GpuListingBox {
            box_id: "listing_1".to_string(),
            tx_id: "tx_1".to_string(),
            listing_nft_id: "nft_1".to_string(),
            provider_pk: "pk_provider".to_string(),
            gpu_type: "RTX 4090".to_string(),
            vram_gb: 24,
            price_per_hour_nanoerg,
            region: "us-east".to_string(),
            available: true,
            value_nanoerg: 1_000_000,
            creation_height: 100,
        }
    }

    fn make_rental(hours: i32, deadline: i32) -> GpuRentalBox {
        GpuRentalBox {
            box_id: "rental_1".to_string(),
            tx_id: "tx_2".to_string(),
            rental_nft_id: "rental_nft_1".to_string(),
            provider_pk: "pk_provider".to_string(),
            renter_pk: "pk_renter".to_string(),
            deadline_height: deadline,
            listing_box_id: "listing_1".to_string(),
            rental_start_height: 100,
            hours_rented: hours,
            value_nanoerg: hours as u64 * 100_000_000, // 0.1 ERG/hr
            creation_height: 100,
        }
    }

    #[test]
    fn test_start_and_stop_session() {
        let meter = UsageMeter::new(60);
        let listing = make_listing(100_000_000); // 0.1 ERG/hr
        let rental = make_rental(10, 999999);

        meter.start_session(&rental, &listing).unwrap();
        assert_eq!(meter.active_count(), 1);

        let usage = meter.get_usage("rental_1").unwrap();
        assert!(usage.is_active);
        assert_eq!(usage.hours_purchased, 10);
        assert!(usage.hours_used < 0.01); // just started

        let stopped = meter.stop_session("rental_1", "user_request").unwrap();
        assert_eq!(stopped.hours_purchased, 10);
        assert!(stopped.hours_used < 0.01);
        assert_eq!(stopped.reason, "user_request");
        assert_eq!(meter.active_count(), 0);
    }

    #[test]
    fn test_duplicate_session_rejected() {
        let meter = UsageMeter::new(60);
        let listing = make_listing(100_000_000);
        let rental = make_rental(10, 999999);

        meter.start_session(&rental, &listing).unwrap();
        let result = meter.start_session(&rental, &listing);
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_nonexistent_session() {
        let meter = UsageMeter::new(60);
        let result = meter.stop_session("nonexistent", "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_active_sessions_filtering() {
        let meter = UsageMeter::new(60);
        let listing = make_listing(100_000_000);

        let r1 = GpuRentalBox {
            box_id: "r1".to_string(),
            ..make_rental(10, 999999)
        };
        let r2 = GpuRentalBox {
            box_id: "r2".to_string(),
            ..make_rental(5, 999999)
        };

        meter.start_session(&r1, &listing).unwrap();
        meter.start_session(&r2, &listing).unwrap();
        assert_eq!(meter.active_count(), 2);

        meter.stop_session("r1", "test").unwrap();
        assert_eq!(meter.active_count(), 1);

        let active = meter.active_sessions();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].rental_box_id, "r2");

        let all = meter.all_sessions();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_erg_calculation() {
        let meter = UsageMeter::new(60);
        let listing = make_listing(500_000_000); // 0.5 ERG/hr
        let rental = make_rental(2, 999999);

        meter.start_session(&rental, &listing).unwrap();
        let usage = meter.get_usage("rental_1").unwrap();
        // Usage is near-zero since we just started
        assert!(usage.erg_spent < 0.001);
        assert_eq!(usage.erg_total_escrow, (2 * 100_000_000) as f64 / 1e9);
    }
}
