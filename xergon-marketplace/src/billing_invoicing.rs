use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// InvoiceStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum InvoiceStatus {
    Draft,
    Pending,
    Paid,
    Overdue,
    Cancelled,
    Refunded,
}

impl InvoiceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Draft => "Draft",
            Self::Pending => "Pending",
            Self::Paid => "Paid",
            Self::Overdue => "Overdue",
            Self::Cancelled => "Cancelled",
            Self::Refunded => "Refunded",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Paid | Self::Cancelled | Self::Refunded)
    }
}

impl std::fmt::Display for InvoiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// LineItem
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LineItem {
    pub description: String,
    pub quantity: u64,
    pub unit_price: f64,
    pub amount: f64,
    pub model_id: Option<String>,
    pub inference_count: Option<u64>,
}

impl LineItem {
    pub fn new(description: &str, quantity: u64, unit_price: f64) -> Self {
        let amount = quantity as f64 * unit_price;
        Self {
            description: description.to_string(),
            quantity,
            unit_price,
            amount,
            model_id: None,
            inference_count: None,
        }
    }

    pub fn with_model(mut self, model_id: &str, inference_count: u64) -> Self {
        self.model_id = Some(model_id.to_string());
        self.inference_count = Some(inference_count);
        self
    }

    pub fn recalculate(&mut self) {
        self.amount = self.quantity as f64 * self.unit_price;
    }
}

// ---------------------------------------------------------------------------
// Invoice
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Invoice {
    pub id: String,
    pub provider_id: String,
    pub user_id: String,
    pub subtotal: f64,
    pub tax: f64,
    pub late_fee: f64,
    pub total: f64,
    pub currency: String,
    pub status: InvoiceStatus,
    pub line_items: Vec<LineItem>,
    pub due_date: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub notes: Option<String>,
    pub payment_method: Option<String>,
    pub credit_applied: f64,
}

impl Invoice {
    pub fn calculate_totals(&self) -> (f64, f64, f64) {
        let subtotal: f64 = self.line_items.iter().map(|i| i.amount).sum();
        let total = subtotal + self.tax + self.late_fee - self.credit_applied;
        (subtotal, self.tax, total)
    }

    pub fn is_overdue(&self) -> bool {
        !self.status.is_terminal() && Utc::now() > self.due_date
    }

    pub fn days_until_due(&self) -> i64 {
        let now = Utc::now();
        (self.due_date - now).num_days()
    }

    pub fn days_overdue(&self) -> i64 {
        if self.is_overdue() {
            (Utc::now() - self.due_date).num_days()
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// BillingConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BillingConfig {
    pub invoice_prefix: String,
    pub payment_terms_days: i64,
    pub late_fee_pct: f64,
    pub auto_generate: bool,
    pub currency: String,
    pub tax_rate: f64,
    pub grace_period_days: i64,
    pub reminder_days_before: Vec<i64>,
    pub max_credit_pct: f64,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            invoice_prefix: "INV".to_string(),
            payment_terms_days: 30,
            late_fee_pct: 1.5,
            auto_generate: false,
            currency: "ERG".to_string(),
            tax_rate: 0.0,
            grace_period_days: 5,
            reminder_days_before: vec![7, 3, 1],
            max_credit_pct: 100.0,
        }
    }
}

// ---------------------------------------------------------------------------
// CreateInvoiceRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateInvoiceRequest {
    pub provider_id: String,
    pub user_id: String,
    pub line_items: Vec<LineItem>,
    pub notes: Option<String>,
    pub currency: Option<String>,
}

// ---------------------------------------------------------------------------
// PayInvoiceRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PayInvoiceRequest {
    pub payment_method: Option<String>,
}

// ---------------------------------------------------------------------------
// ApplyCreditRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApplyCreditRequest {
    pub amount: f64,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Statement
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Statement {
    pub provider_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub invoices: Vec<Invoice>,
    pub total_invoiced: f64,
    pub total_paid: f64,
    pub total_outstanding: f64,
    pub total_overdue: f64,
    pub generated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// OutstandingSummary
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OutstandingSummary {
    pub total_outstanding: f64,
    pub total_overdue: f64,
    pub invoice_count: usize,
    pub overdue_count: usize,
    pub by_status: HashMap<String, usize>,
    pub by_provider: HashMap<String, f64>,
}

// ---------------------------------------------------------------------------
// PaymentRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PaymentRecord {
    pub id: String,
    pub invoice_id: String,
    pub amount: f64,
    pub method: String,
    pub paid_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// CreditRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreditRecord {
    pub id: String,
    pub invoice_id: String,
    pub amount: f64,
    pub reason: String,
    pub applied_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ReminderRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReminderRecord {
    pub invoice_id: String,
    pub sent_at: DateTime<Utc>,
    pub reminder_type: String,
}

// ---------------------------------------------------------------------------
// BillingEngine
// ---------------------------------------------------------------------------

static INVOICE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct BillingEngine {
    invoices: Arc<DashMap<String, Invoice>>,
    payments: Arc<DashMap<String, Vec<PaymentRecord>>>,
    credits: Arc<DashMap<String, Vec<CreditRecord>>>,
    reminders: Arc<DashMap<String, Vec<ReminderRecord>>>,
    config: Arc<std::sync::RwLock<BillingConfig>>,
}

use std::sync::Arc;

impl Default for BillingEngine {
    fn default() -> Self {
        Self::new(BillingConfig::default())
    }
}

impl BillingEngine {
    pub fn new(config: BillingConfig) -> Self {
        Self {
            invoices: Arc::new(DashMap::new()),
            payments: Arc::new(DashMap::new()),
            credits: Arc::new(DashMap::new()),
            reminders: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    fn next_invoice_id(&self, prefix: &str) -> String {
        let seq = INVOICE_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
        format!("{}-{:08}", prefix, seq)
    }

    // ---- create_invoice ----

    pub fn create_invoice(&self, req: &CreateInvoiceRequest) -> Result<Invoice, String> {
        let config = self.config.read().map_err(|e| e.to_string())?;
        let now = Utc::now();
        let due_date = now + Duration::days(config.payment_terms_days);
        let currency = req.currency.clone().unwrap_or_else(|| config.currency.clone());

        let subtotal: f64 = req.line_items.iter().map(|i| i.amount).sum();
        let tax = subtotal * config.tax_rate;
        let total = subtotal + tax;

        let id = self.next_invoice_id(&config.invoice_prefix);

        let invoice = Invoice {
            id: id.clone(),
            provider_id: req.provider_id.clone(),
            user_id: req.user_id.clone(),
            subtotal,
            tax,
            late_fee: 0.0,
            total,
            currency,
            status: InvoiceStatus::Pending,
            line_items: req.line_items.clone(),
            due_date,
            paid_at: None,
            created_at: now,
            updated_at: now,
            notes: req.notes.clone(),
            payment_method: None,
            credit_applied: 0.0,
        };

        self.invoices.insert(id.clone(), invoice.clone());
        Ok(invoice)
    }

    // ---- get_invoice ----

    pub fn get_invoice(&self, id: &str) -> Option<Invoice> {
        self.invoices.get(id).map(|r| r.value().clone())
    }

    // ---- list_invoices ----

    pub fn list_invoices(
        &self,
        provider_id: Option<&str>,
        user_id: Option<&str>,
        status: Option<&InvoiceStatus>,
        limit: usize,
        offset: usize,
    ) -> Vec<Invoice> {
        self.invoices
            .iter()
            .filter(|r| {
                if let Some(pid) = provider_id {
                    if r.value().provider_id != pid {
                        return false;
                    }
                }
                if let Some(uid) = user_id {
                    if r.value().user_id != uid {
                        return false;
                    }
                }
                if let Some(s) = status {
                    if r.value().status != *s {
                        return false;
                    }
                }
                true
            })
            .map(|r| r.value().clone())
            .skip(offset)
            .take(limit)
            .collect()
    }

    // ---- mark_paid ----

    pub fn mark_paid(&self, id: &str, payment_method: Option<&str>) -> Result<Invoice, String> {
        let mut inv = self
            .invoices
            .get_mut(id)
            .ok_or_else(|| "Invoice not found".to_string())?;

        if inv.status.is_terminal() {
            return Err(format!("Cannot pay invoice in {} status", inv.status));
        }

        inv.status = InvoiceStatus::Paid;
        inv.paid_at = Some(Utc::now());
        inv.updated_at = Utc::now();
        inv.payment_method = payment_method.map(|s| s.to_string());

        let record = PaymentRecord {
            id: uuid::Uuid::new_v4().to_string(),
            invoice_id: id.to_string(),
            amount: inv.total,
            method: payment_method.unwrap_or("unknown").to_string(),
            paid_at: inv.paid_at.unwrap(),
        };

        self.payments
            .entry(id.to_string())
            .or_default()
            .push(record);

        Ok(inv.clone())
    }

    // ---- apply_credit ----

    pub fn apply_credit(
        &self,
        id: &str,
        amount: f64,
        reason: &str,
    ) -> Result<Invoice, String> {
        let config = self.config.read().map_err(|e| e.to_string())?;
        let mut inv = self
            .invoices
            .get_mut(id)
            .ok_or_else(|| "Invoice not found".to_string())?;

        if inv.status.is_terminal() {
            return Err(format!("Cannot apply credit to {} invoice", inv.status));
        }

        let max_credit = inv.total * (config.max_credit_pct / 100.0);
        let credit = amount.min(max_credit);
        let new_credit = inv.credit_applied + credit;

        if new_credit > max_credit {
            return Err("Credit exceeds maximum allowed".to_string());
        }

        inv.credit_applied = new_credit;
        inv.total = inv.subtotal + inv.tax + inv.late_fee - inv.credit_applied;
        inv.updated_at = Utc::now();

        let record = CreditRecord {
            id: uuid::Uuid::new_v4().to_string(),
            invoice_id: id.to_string(),
            amount: credit,
            reason: reason.to_string(),
            applied_at: Utc::now(),
        };

        self.credits
            .entry(id.to_string())
            .or_default()
            .push(record);

        Ok(inv.clone())
    }

    // ---- calculate_late_fees ----

    pub fn calculate_late_fees(&self) -> Vec<Invoice> {
        let config = match self.config.read() {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let mut updated = Vec::new();
        for mut entry in self.invoices.iter_mut() {
            let inv = entry.value_mut();
            if inv.is_overdue() && inv.late_fee == 0.0 {
                let days_overdue = inv.days_overdue();
                let fee = inv.subtotal * (config.late_fee_pct / 100.0) * days_overdue as f64;
                inv.late_fee = fee;
                inv.total = inv.subtotal + inv.tax + inv.late_fee - inv.credit_applied;
                inv.status = InvoiceStatus::Overdue;
                inv.updated_at = Utc::now();
                updated.push(inv.clone());
            }
        }
        updated
    }

    // ---- generate_statement ----

    pub fn generate_statement(
        &self,
        provider_id: &str,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Result<Statement, String> {
        let invoices: Vec<Invoice> = self
            .invoices
            .iter()
            .filter(|r| {
                let inv = r.value();
                inv.provider_id == provider_id
                    && inv.created_at >= period_start
                    && inv.created_at < period_end
            })
            .map(|r| r.value().clone())
            .collect();

        let total_invoiced: f64 = invoices.iter().map(|i| i.subtotal + i.tax).sum();
        let total_paid: f64 = invoices
            .iter()
            .filter(|i| i.status == InvoiceStatus::Paid)
            .map(|i| i.total)
            .sum();
        let total_outstanding: f64 = invoices
            .iter()
            .filter(|i| !i.status.is_terminal())
            .map(|i| i.total)
            .sum();
        let total_overdue: f64 = invoices
            .iter()
            .filter(|i| i.status == InvoiceStatus::Overdue)
            .map(|i| i.total)
            .sum();

        Ok(Statement {
            provider_id: provider_id.to_string(),
            period_start,
            period_end,
            invoices,
            total_invoiced,
            total_paid,
            total_outstanding,
            total_overdue,
            generated_at: Utc::now(),
        })
    }

    // ---- calculate_totals ----

    pub fn calculate_totals(&self, invoice_id: &str) -> Result<(f64, f64, f64), String> {
        let inv = self
            .invoices
            .get(invoice_id)
            .ok_or_else(|| "Invoice not found".to_string())?;
        Ok(inv.calculate_totals())
    }

    // ---- get_outstanding ----

    pub fn get_outstanding(&self) -> OutstandingSummary {
        let mut total_outstanding = 0.0;
        let mut total_overdue = 0.0;
        let mut invoice_count = 0usize;
        let mut overdue_count = 0usize;
        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut by_provider: HashMap<String, f64> = HashMap::new();

        for entry in self.invoices.iter() {
            let inv = entry.value();
            if inv.status.is_terminal() {
                continue;
            }
            total_outstanding += inv.total;
            invoice_count += 1;

            *by_status.entry(inv.status.as_str().to_string()).or_insert(0) += 1;
            *by_provider
                .entry(inv.provider_id.clone())
                .or_insert(0.0) += inv.total;

            if inv.is_overdue() {
                total_overdue += inv.total;
                overdue_count += 1;
            }
        }

        OutstandingSummary {
            total_outstanding,
            total_overdue,
            invoice_count,
            overdue_count,
            by_status,
            by_provider,
        }
    }

    // ---- send_reminder ----

    pub fn send_reminder(&self, id: &str) -> Result<ReminderRecord, String> {
        let inv = self
            .invoices
            .get(id)
            .ok_or_else(|| "Invoice not found".to_string())?;

        if inv.status.is_terminal() {
            return Err(format!("Cannot send reminder for {} invoice", inv.status));
        }

        let reminder_type = if inv.is_overdue() {
            "overdue_notice"
        } else {
            let days = inv.days_until_due();
            if days <= 1 {
                "final_notice"
            } else if days <= 3 {
                "urgent_reminder"
            } else {
                "friendly_reminder"
            }
        };

        let record = ReminderRecord {
            invoice_id: id.to_string(),
            sent_at: Utc::now(),
            reminder_type: reminder_type.to_string(),
        };

        self.reminders
            .entry(id.to_string())
            .or_default()
            .push(record.clone());

        Ok(record)
    }

    // ---- cancel_invoice ----

    pub fn cancel_invoice(&self, id: &str) -> Result<Invoice, String> {
        let mut inv = self
            .invoices
            .get_mut(id)
            .ok_or_else(|| "Invoice not found".to_string())?;

        if inv.status.is_terminal() {
            return Err(format!("Cannot cancel {} invoice", inv.status));
        }

        inv.status = InvoiceStatus::Cancelled;
        inv.updated_at = Utc::now();
        Ok(inv.clone())
    }

    // ---- refund_invoice ----

    pub fn refund_invoice(&self, id: &str) -> Result<Invoice, String> {
        let mut inv = self
            .invoices
            .get_mut(id)
            .ok_or_else(|| "Invoice not found".to_string())?;

        if inv.status != InvoiceStatus::Paid {
            return Err("Only paid invoices can be refunded".to_string());
        }

        inv.status = InvoiceStatus::Refunded;
        inv.updated_at = Utc::now();
        Ok(inv.clone())
    }

    // ---- get_config ----

    pub fn get_config(&self) -> Result<BillingConfig, String> {
        self.config.read().map(|c| c.clone()).map_err(|e| e.to_string())
    }

    // ---- update_config ----

    pub fn update_config(&self, new_config: BillingConfig) -> Result<(), String> {
        let mut config = self.config.write().map_err(|e| e.to_string())?;
        *config = new_config;
        Ok(())
    }

    // ---- get_invoice_count ----

    pub fn get_invoice_count(&self) -> usize {
        self.invoices.len()
    }

    // ---- get_payment_history ----

    pub fn get_payment_history(&self, invoice_id: &str) -> Vec<PaymentRecord> {
        self.payments
            .get(invoice_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    // ---- get_credit_history ----

    pub fn get_credit_history(&self, invoice_id: &str) -> Vec<CreditRecord> {
        self.credits
            .get(invoice_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    // ---- get_reminder_history ----

    pub fn get_reminder_history(&self, invoice_id: &str) -> Vec<ReminderRecord> {
        self.reminders
            .get(invoice_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> BillingEngine {
        BillingEngine::default()
    }

    fn make_line_items() -> Vec<LineItem> {
        vec![
            LineItem::new("Model A inference", 1000, 0.01),
            LineItem::new("Model B inference", 500, 0.02),
        ]
    }

    fn make_request() -> CreateInvoiceRequest {
        CreateInvoiceRequest {
            provider_id: "provider-1".to_string(),
            user_id: "user-1".to_string(),
            line_items: make_line_items(),
            notes: None,
            currency: None,
        }
    }

    #[test]
    fn test_create_invoice() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        assert_eq!(inv.status, InvoiceStatus::Pending);
        assert_eq!(inv.provider_id, "provider-1");
        assert!(!inv.id.is_empty());
        // 1000*0.01 + 500*0.02 = 10 + 10 = 20
        assert!((inv.subtotal - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_invoice() {
        let engine = setup();
        let created = engine.create_invoice(&make_request()).unwrap();
        let fetched = engine.get_invoice(&created.id).unwrap();
        assert_eq!(created.id, fetched.id);
        assert_eq!(created.total, fetched.total);
    }

    #[test]
    fn test_get_invoice_not_found() {
        let engine = setup();
        assert!(engine.get_invoice("nonexistent").is_none());
    }

    #[test]
    fn test_list_invoices_by_provider() {
        let engine = setup();
        let mut req1 = make_request();
        req1.provider_id = "prov-A".to_string();
        let mut req2 = make_request();
        req2.provider_id = "prov-B".to_string();

        engine.create_invoice(&req1).unwrap();
        engine.create_invoice(&req2).unwrap();

        let list_a = engine.list_invoices(Some("prov-A"), None, None, 100, 0);
        assert_eq!(list_a.len(), 1);
        assert_eq!(list_a[0].provider_id, "prov-A");
    }

    #[test]
    fn test_mark_paid() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        let paid = engine.mark_paid(&inv.id, Some("wallet")).unwrap();
        assert_eq!(paid.status, InvoiceStatus::Paid);
        assert!(paid.paid_at.is_some());
        assert_eq!(paid.payment_method.as_deref(), Some("wallet"));
    }

    #[test]
    fn test_mark_paid_already_paid() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        engine.mark_paid(&inv.id, None).unwrap();
        let result = engine.mark_paid(&inv.id, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_credit() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        let original_total = inv.total;
        let credited = engine.apply_credit(&inv.id, 5.0, "Promotional credit").unwrap();
        assert!((credited.credit_applied - 5.0).abs() < f64::EPSILON);
        assert!((credited.total - (original_total - 5.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_generate_statement() {
        let engine = setup();
        let now = Utc::now();
        engine.create_invoice(&make_request()).unwrap();
        let start = now - Duration::hours(1);
        let end = now + Duration::hours(1);
        let stmt = engine
            .generate_statement("provider-1", start, end)
            .unwrap();
        assert_eq!(stmt.invoices.len(), 1);
        assert!((stmt.total_invoiced - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_outstanding() {
        let engine = setup();
        engine.create_invoice(&make_request()).unwrap();
        let summary = engine.get_outstanding();
        assert_eq!(summary.invoice_count, 1);
        assert_eq!(summary.overdue_count, 0);
        assert!(summary.total_outstanding > 0.0);
    }

    #[test]
    fn test_send_reminder() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        let reminder = engine.send_reminder(&inv.id).unwrap();
        assert!(!reminder.reminder_type.is_empty());
    }

    #[test]
    fn test_cancel_invoice() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        let cancelled = engine.cancel_invoice(&inv.id).unwrap();
        assert_eq!(cancelled.status, InvoiceStatus::Cancelled);
    }

    #[test]
    fn test_calculate_totals() {
        let engine = setup();
        let inv = engine.create_invoice(&make_request()).unwrap();
        let (subtotal, _tax, total) = engine.calculate_totals(&inv.id).unwrap();
        assert!((subtotal - 20.0).abs() < f64::EPSILON);
        assert!((total - 20.0).abs() < f64::EPSILON);
    }
}
