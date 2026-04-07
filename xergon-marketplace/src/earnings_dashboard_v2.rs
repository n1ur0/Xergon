use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EarningSource
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EarningSource {
    Inference,
    Training,
    Staking,
    Referral,
}

impl EarningSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Inference => "Inference",
            Self::Training => "Training",
            Self::Staking => "Staking",
            Self::Referral => "Referral",
        }
    }
}

impl std::fmt::Display for EarningSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ChartPeriod
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChartPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl ChartPeriod {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
        }
    }

    pub fn duration(&self) -> Duration {
        match self {
            Self::Daily => Duration::days(1),
            Self::Weekly => Duration::weeks(1),
            Self::Monthly => Duration::days(30),
        }
    }
}

// ---------------------------------------------------------------------------
// TrendDirection
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TrendDirection {
    Up,
    Down,
    Stable,
}

// ---------------------------------------------------------------------------
// WithdrawalStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum WithdrawalStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl WithdrawalStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Processing => "Processing",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

// ---------------------------------------------------------------------------
// EarningEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EarningEntry {
    pub id: String,
    pub provider_id: String,
    pub amount: f64,
    pub source: EarningSource,
    pub timestamp: DateTime<Utc>,
    pub model_id: Option<String>,
    pub request_id: Option<String>,
}

// ---------------------------------------------------------------------------
// EarningsSummary
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EarningsSummary {
    pub provider_id: String,
    pub total_earnings: f64,
    pub available_balance: f64,
    pub pending_withdrawal: f64,
    pub period_earnings: f64,
    pub growth_rate: f64,
    pub by_source: HashMap<String, f64>,
    pub entry_count: usize,
}

// ---------------------------------------------------------------------------
// DataPoint
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DataPoint {
    pub label: String,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// EarningsChart
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EarningsChart {
    pub provider_id: String,
    pub period: ChartPeriod,
    pub data_points: Vec<DataPoint>,
    pub trend: TrendDirection,
    pub total_in_period: f64,
}

// ---------------------------------------------------------------------------
// WithdrawalRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalRequest {
    pub id: String,
    pub provider_id: String,
    pub amount: f64,
    pub status: WithdrawalStatus,
    pub destination: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub tx_id: Option<String>,
    pub failure_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// ModelEarning
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelEarning {
    pub model_id: String,
    pub total_earnings: f64,
    pub request_count: u64,
    pub avg_per_request: f64,
}

// ---------------------------------------------------------------------------
// DashboardConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DashboardConfig {
    pub min_withdrawal: f64,
    pub withdrawal_fee_pct: f64,
    pub auto_withdraw: bool,
    pub auto_withdraw_threshold: f64,
    pub currency: String,
    pub max_withdrawal_per_day: f64,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            min_withdrawal: 10.0,
            withdrawal_fee_pct: 0.5,
            auto_withdraw: false,
            auto_withdraw_threshold: 1000.0,
            currency: "ERG".to_string(),
            max_withdrawal_per_day: 10000.0,
        }
    }
}

// ---------------------------------------------------------------------------
// RequestWithdrawalRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RequestWithdrawalRequest {
    pub provider_id: String,
    pub amount: f64,
    pub destination: Option<String>,
}

// ---------------------------------------------------------------------------
// EarningsDashboardV2
// ---------------------------------------------------------------------------

static EARNING_COUNTER: AtomicU64 = AtomicU64::new(0);
static WITHDRAWAL_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct EarningsDashboardV2 {
    earnings: Arc<DashMap<String, Vec<EarningEntry>>>,
    withdrawals: Arc<DashMap<String, Vec<WithdrawalRequest>>>,
    config: Arc<std::sync::RwLock<DashboardConfig>>,
}

use std::sync::Arc;

impl Default for EarningsDashboardV2 {
    fn default() -> Self {
        Self::new(DashboardConfig::default())
    }
}

impl EarningsDashboardV2 {
    pub fn new(config: DashboardConfig) -> Self {
        Self {
            earnings: Arc::new(DashMap::new()),
            withdrawals: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    // ---- record_earning ----

    pub fn record_earning(
        &self,
        provider_id: &str,
        amount: f64,
        source: EarningSource,
        model_id: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<EarningEntry, String> {
        if amount <= 0.0 {
            return Err("Amount must be positive".to_string());
        }

        let id = EARNING_COUNTER.fetch_add(1, Ordering::Relaxed).to_string();
        let entry = EarningEntry {
            id,
            provider_id: provider_id.to_string(),
            amount,
            source,
            timestamp: Utc::now(),
            model_id: model_id.map(|s| s.to_string()),
            request_id: request_id.map(|s| s.to_string()),
        };

        self.earnings
            .entry(provider_id.to_string())
            .or_default()
            .push(entry.clone());

        Ok(entry)
    }

    // ---- get_summary ----

    pub fn get_summary(&self, provider_id: &str) -> Result<EarningsSummary, String> {
        let entries = self
            .earnings
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let withdrawals = self
            .withdrawals
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let total_earnings: f64 = entries.iter().map(|e| e.amount).sum();
        let pending_withdrawal: f64 = withdrawals
            .iter()
            .filter(|w| w.status == WithdrawalStatus::Pending || w.status == WithdrawalStatus::Processing)
            .map(|w| w.amount)
            .sum();
        let available_balance = total_earnings - pending_withdrawal;

        // Period earnings: last 30 days
        let cutoff = Utc::now() - Duration::days(30);
        let period_earnings: f64 = entries
            .iter()
            .filter(|e| e.timestamp >= cutoff)
            .map(|e| e.amount)
            .sum();

        // Growth rate: compare last 30 days vs previous 30 days
        let prev_cutoff = cutoff - Duration::days(30);
        let prev_period: f64 = entries
            .iter()
            .filter(|e| e.timestamp >= prev_cutoff && e.timestamp < cutoff)
            .map(|e| e.amount)
            .sum();

        let growth_rate = if prev_period > 0.0 {
            ((period_earnings - prev_period) / prev_period) * 100.0
        } else if period_earnings > 0.0 {
            100.0
        } else {
            0.0
        };

        let mut by_source: HashMap<String, f64> = HashMap::new();
        for entry in &entries {
            *by_source
                .entry(entry.source.as_str().to_string())
                .or_insert(0.0) += entry.amount;
        }

        Ok(EarningsSummary {
            provider_id: provider_id.to_string(),
            total_earnings,
            available_balance,
            pending_withdrawal,
            period_earnings,
            growth_rate,
            by_source,
            entry_count: entries.len(),
        })
    }

    // ---- get_chart ----

    pub fn get_chart(
        &self,
        provider_id: &str,
        period: &ChartPeriod,
        limit: usize,
    ) -> Result<EarningsChart, String> {
        let entries = self
            .earnings
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let duration = period.duration();
        let mut buckets: Vec<(DateTime<Utc>, f64)> = Vec::new();
        let now = Utc::now();

        for i in (0..limit).rev() {
            let bucket_start = now - duration * (i as i32 + 1);
            let bucket_end = now - duration * (i as i32);
            let sum: f64 = entries
                .iter()
                .filter(|e| e.timestamp >= bucket_start && e.timestamp < bucket_end)
                .map(|e| e.amount)
                .sum();
            buckets.push((bucket_end, sum));
        }

        let total_in_period: f64 = buckets.iter().map(|(_, v)| *v).sum();
        let trend = if buckets.len() >= 2 {
            let first_half: f64 = buckets[..buckets.len() / 2].iter().map(|(_, v)| *v).sum();
            let second_half: f64 = buckets[buckets.len() / 2..].iter().map(|(_, v)| *v).sum();
            let diff = second_half - first_half;
            let threshold = first_half * 0.05;
            if diff > threshold {
                TrendDirection::Up
            } else if diff < -threshold {
                TrendDirection::Down
            } else {
                TrendDirection::Stable
            }
        } else {
            TrendDirection::Stable
        };

        let data_points: Vec<DataPoint> = buckets
            .iter()
            .map(|(ts, val)| DataPoint {
                label: ts.format("%Y-%m-%d %H:%M").to_string(),
                value: *val,
                timestamp: *ts,
            })
            .collect();

        Ok(EarningsChart {
            provider_id: provider_id.to_string(),
            period: period.clone(),
            data_points,
            trend,
            total_in_period,
        })
    }

    // ---- request_withdrawal ----

    pub fn request_withdrawal(
        &self,
        req: &RequestWithdrawalRequest,
    ) -> Result<WithdrawalRequest, String> {
        let config = self.config.read().map_err(|e| e.to_string())?;

        if req.amount < config.min_withdrawal {
            return Err(format!(
                "Amount below minimum withdrawal of {} {}",
                config.min_withdrawal, config.currency
            ));
        }

        let summary = self.get_summary(&req.provider_id)?;
        if req.amount > summary.available_balance {
            return Err("Insufficient available balance".to_string());
        }

        // Check daily limit
        let today = Utc::now().date_naive();
        let today_withdrawals: f64 = self
            .withdrawals
            .get(&req.provider_id)
            .map(|r| {
                r.value()
                    .iter()
                    .filter(|w| {
                        w.requested_at.date_naive() == today
                            && w.status != WithdrawalStatus::Cancelled
                            && w.status != WithdrawalStatus::Failed
                    })
                    .map(|w| w.amount)
                    .sum()
            })
            .unwrap_or(0.0);

        if today_withdrawals + req.amount > config.max_withdrawal_per_day {
            return Err("Daily withdrawal limit exceeded".to_string());
        }

        let id = WITHDRAWAL_COUNTER.fetch_add(1, Ordering::Relaxed).to_string();
        let withdrawal = WithdrawalRequest {
            id,
            provider_id: req.provider_id.clone(),
            amount: req.amount,
            status: WithdrawalStatus::Pending,
            destination: req.destination.clone(),
            requested_at: Utc::now(),
            processed_at: None,
            tx_id: None,
            failure_reason: None,
        };

        self.withdrawals
            .entry(req.provider_id.clone())
            .or_default()
            .push(withdrawal.clone());

        Ok(withdrawal)
    }

    // ---- process_withdrawal ----

    pub fn process_withdrawal(
        &self,
        withdrawal_id: &str,
        provider_id: &str,
        approve: bool,
        tx_id: Option<&str>,
    ) -> Result<WithdrawalRequest, String> {
        let mut withdrawals = self
            .withdrawals
            .get_mut(provider_id)
            .ok_or_else(|| "No withdrawals found for provider".to_string())?;

        let withdrawal = withdrawals
            .iter_mut()
            .find(|w| w.id == withdrawal_id)
            .ok_or_else(|| "Withdrawal not found".to_string())?;

        if withdrawal.status != WithdrawalStatus::Pending {
            return Err(format!(
                "Cannot process withdrawal in {} status",
                withdrawal.status.as_str()
            ));
        }

        if approve {
            withdrawal.status = WithdrawalStatus::Completed;
            withdrawal.processed_at = Some(Utc::now());
            withdrawal.tx_id = tx_id.map(|s| s.to_string());
        } else {
            withdrawal.status = WithdrawalStatus::Failed;
            withdrawal.processed_at = Some(Utc::now());
            withdrawal.failure_reason = Some("Rejected by admin".to_string());
        }

        Ok(withdrawal.clone())
    }

    // ---- get_withdrawal_history ----

    pub fn get_withdrawal_history(
        &self,
        provider_id: &str,
        limit: usize,
    ) -> Vec<WithdrawalRequest> {
        self.withdrawals
            .get(provider_id)
            .map(|r| {
                let mut list = r.value().clone();
                list.sort_by(|a, b| b.requested_at.cmp(&a.requested_at));
                list.into_iter().take(limit).collect()
            })
            .unwrap_or_default()
    }

    // ---- get_top_models ----

    pub fn get_top_models(
        &self,
        provider_id: &str,
        limit: usize,
    ) -> Vec<ModelEarning> {
        let entries = self
            .earnings
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let mut by_model: HashMap<String, (f64, u64)> = HashMap::new();
        for entry in &entries {
            if let Some(ref mid) = entry.model_id {
                let (total, count) = by_model.entry(mid.clone()).or_insert((0.0, 0));
                *total += entry.amount;
                *count += 1;
            }
        }

        let mut models: Vec<ModelEarning> = by_model
            .into_iter()
            .map(|(model_id, (total, count))| ModelEarning {
                model_id,
                total_earnings: total,
                request_count: count,
                avg_per_request: if count > 0 { total / count as f64 } else { 0.0 },
            })
            .collect();

        models.sort_by(|a, b| b.total_earnings.partial_cmp(&a.total_earnings).unwrap());
        models.into_iter().take(limit).collect()
    }

    // ---- get_config ----

    pub fn get_config(&self) -> Result<DashboardConfig, String> {
        self.config.read().map(|c| c.clone()).map_err(|e| e.to_string())
    }

    // ---- update_config ----

    pub fn update_config(&self, new_config: DashboardConfig) -> Result<(), String> {
        let mut config = self.config.write().map_err(|e| e.to_string())?;
        *config = new_config;
        Ok(())
    }

    // ---- cancel_withdrawal ----

    pub fn cancel_withdrawal(
        &self,
        withdrawal_id: &str,
        provider_id: &str,
    ) -> Result<WithdrawalRequest, String> {
        let mut withdrawals = self
            .withdrawals
            .get_mut(provider_id)
            .ok_or_else(|| "No withdrawals found for provider".to_string())?;

        let withdrawal = withdrawals
            .iter_mut()
            .find(|w| w.id == withdrawal_id)
            .ok_or_else(|| "Withdrawal not found".to_string())?;

        if withdrawal.status != WithdrawalStatus::Pending {
            return Err("Only pending withdrawals can be cancelled".to_string());
        }

        withdrawal.status = WithdrawalStatus::Cancelled;
        Ok(withdrawal.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> EarningsDashboardV2 {
        EarningsDashboardV2::default()
    }

    #[test]
    fn test_record_earning() {
        let dash = setup();
        let entry = dash
            .record_earning("prov-1", 10.0, EarningSource::Inference, Some("model-A"), None)
            .unwrap();
        assert_eq!(entry.provider_id, "prov-1");
        assert!((entry.amount - 10.0).abs() < f64::EPSILON);
        assert_eq!(entry.source, EarningSource::Inference);
    }

    #[test]
    fn test_record_earning_negative_amount() {
        let dash = setup();
        let result = dash.record_earning("prov-1", -5.0, EarningSource::Inference, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_summary() {
        let dash = setup();
        dash.record_earning("prov-1", 50.0, EarningSource::Inference, None, None)
            .unwrap();
        dash.record_earning("prov-1", 30.0, EarningSource::Staking, None, None)
            .unwrap();
        let summary = dash.get_summary("prov-1").unwrap();
        assert!((summary.total_earnings - 80.0).abs() < f64::EPSILON);
        assert_eq!(summary.entry_count, 2);
    }

    #[test]
    fn test_get_summary_empty() {
        let dash = setup();
        let summary = dash.get_summary("nonexistent").unwrap();
        assert!((summary.total_earnings - 0.0).abs() < f64::EPSILON);
        assert_eq!(summary.entry_count, 0);
    }

    #[test]
    fn test_get_chart() {
        let dash = setup();
        dash.record_earning("prov-1", 10.0, EarningSource::Inference, None, None)
            .unwrap();
        let chart = dash
            .get_chart("prov-1", &ChartPeriod::Daily, 7)
            .unwrap();
        assert_eq!(chart.period, ChartPeriod::Daily);
        assert!(!chart.data_points.is_empty());
    }

    #[test]
    fn test_request_withdrawal() {
        let dash = setup();
        dash.record_earning("prov-1", 100.0, EarningSource::Inference, None, None)
            .unwrap();
        let req = RequestWithdrawalRequest {
            provider_id: "prov-1".to_string(),
            amount: 50.0,
            destination: Some("wallet-addr".to_string()),
        };
        let w = dash.request_withdrawal(&req).unwrap();
        assert_eq!(w.status, WithdrawalStatus::Pending);
        assert!((w.amount - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_request_withdrawal_insufficient() {
        let dash = setup();
        let req = RequestWithdrawalRequest {
            provider_id: "prov-1".to_string(),
            amount: 50.0,
            destination: None,
        };
        let result = dash.request_withdrawal(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_process_withdrawal() {
        let dash = setup();
        dash.record_earning("prov-1", 100.0, EarningSource::Inference, None, None)
            .unwrap();
        let req = RequestWithdrawalRequest {
            provider_id: "prov-1".to_string(),
            amount: 50.0,
            destination: None,
        };
        let w = dash.request_withdrawal(&req).unwrap();
        let processed = dash
            .process_withdrawal(&w.id, "prov-1", true, Some("tx-123"))
            .unwrap();
        assert_eq!(processed.status, WithdrawalStatus::Completed);
        assert_eq!(processed.tx_id.as_deref(), Some("tx-123"));
    }

    #[test]
    fn test_get_withdrawal_history() {
        let dash = setup();
        dash.record_earning("prov-1", 200.0, EarningSource::Inference, None, None)
            .unwrap();
        let req = RequestWithdrawalRequest {
            provider_id: "prov-1".to_string(),
            amount: 50.0,
            destination: None,
        };
        dash.request_withdrawal(&req).unwrap();
        let history = dash.get_withdrawal_history("prov-1", 10);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_get_top_models() {
        let dash = setup();
        dash.record_earning("prov-1", 30.0, EarningSource::Inference, Some("model-A"), None)
            .unwrap();
        dash.record_earning("prov-1", 70.0, EarningSource::Inference, Some("model-B"), None)
            .unwrap();
        dash.record_earning("prov-1", 10.0, EarningSource::Inference, Some("model-A"), None)
            .unwrap();
        let models = dash.get_top_models("prov-1", 10);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_id, "model-B"); // 70 > 40
    }

    #[test]
    fn test_cancel_withdrawal() {
        let dash = setup();
        dash.record_earning("prov-1", 100.0, EarningSource::Inference, None, None)
            .unwrap();
        let req = RequestWithdrawalRequest {
            provider_id: "prov-1".to_string(),
            amount: 50.0,
            destination: None,
        };
        let w = dash.request_withdrawal(&req).unwrap();
        let cancelled = dash.cancel_withdrawal(&w.id, "prov-1").unwrap();
        assert_eq!(cancelled.status, WithdrawalStatus::Cancelled);
    }
}
