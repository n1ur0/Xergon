use std::time::Instant;

use axum::{extract::{Path, State}, response::Json};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ErgoPayQrRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ErgoPayQrRequest {
    pub request_id: String,
    pub amount_nanoerg: u64,
    pub recipient: String,
    pub description: Option<String>,
}

impl ErgoPayQrRequest {
    pub fn new(amount_nanoerg: u64, recipient: &str, description: Option<&str>) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            amount_nanoerg,
            recipient: recipient.to_string(),
            description: description.map(|d| d.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// ErgoPayQrResponse
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ErgoPayQrResponse {
    pub request_id: String,
    pub ergopay_uri: String,
    pub qr_data: String,
    pub expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ErgoPayStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ErgoPayStatus {
    Pending,
    Completed,
    Expired,
    Failed,
}

impl ErgoPayStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Completed => "Completed",
            Self::Expired => "Expired",
            Self::Failed => "Failed",
        }
    }
}

impl std::fmt::Display for ErgoPayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// StoredErgoPayRequest
// ---------------------------------------------------------------------------

#[derive(Debug)]
#[allow(dead_code)]
struct StoredErgoPayRequest {
    request: ErgoPayQrRequest,
    response: ErgoPayQrResponse,
    status: ErgoPayStatus,
    created_at: Instant,
    ttl_secs: u64,
}

impl StoredErgoPayRequest {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= self.ttl_secs
    }

    fn check_and_update_status(&mut self) -> ErgoPayStatus {
        if self.status == ErgoPayStatus::Pending && self.is_expired() {
            self.status = ErgoPayStatus::Expired;
        }
        self.status.clone()
    }
}

// ---------------------------------------------------------------------------
// ErgoPayQrManager
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ErgoPayQrManager {
    requests: DashMap<String, StoredErgoPayRequest>,
    default_ttl_secs: u64,
}

impl Default for ErgoPayQrManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ErgoPayQrManager {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
            default_ttl_secs: 600, // 10 minutes
        }
    }

    /// Set the default TTL for new requests.
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.default_ttl_secs = ttl_secs;
        self
    }

    /// Build an ergopay: URI from a request.
    pub fn build_ergopay_qr(req: &ErgoPayQrRequest) -> String {
        // ergopay: format:
        // ergopay:<recipient>?amount=<nanoerg>&description=<desc>&id=<request_id>
        let mut uri = format!("ergopay:{}?amount={}", req.recipient, req.amount_nanoerg);
        if let Some(desc) = &req.description {
            uri.push_str(&format!("&description={}", urlencoding::encode(desc)));
        }
        uri.push_str(&format!("&id={}", req.request_id));
        uri
    }

    /// Generate SVG QR code placeholder (returns the URI data as text for now).
    /// In production, this would call a QR code library to generate an actual SVG.
    pub fn generate_qr_svg(ergopay_uri: &str) -> String {
        // Placeholder SVG that renders the URI as text.
        // Future: replace with actual QR code generation using a library like `qrcode`.
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"300\" height=\"300\" viewBox=\"0 0 300 300\">\
  <rect width=\"300\" height=\"300\" fill=\"#ffffff\"/>\
  <text x=\"150\" y=\"150\" font-family=\"monospace\" font-size=\"10\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"#333333\">{}</text>\
</svg>",
            ergopay_uri
        )
    }

    /// Create a new ErgoPay QR request.
    pub fn create_request(&self, req: ErgoPayQrRequest) -> Result<ErgoPayQrResponse, String> {
        if req.recipient.is_empty() {
            return Err("Recipient address cannot be empty".to_string());
        }

        if req.amount_nanoerg == 0 {
            return Err("Amount must be greater than zero".to_string());
        }

        let request_id = req.request_id.clone();
        let ergopay_uri = Self::build_ergopay_qr(&req);
        let qr_data = Self::generate_qr_svg(&ergopay_uri);
        let expires_at = Utc::now() + Duration::seconds(self.default_ttl_secs as i64);

        let response = ErgoPayQrResponse {
            request_id: request_id.clone(),
            ergopay_uri: ergopay_uri.clone(),
            qr_data: qr_data.clone(),
            expires_at,
        };

        let stored = StoredErgoPayRequest {
            request: req,
            response: response.clone(),
            status: ErgoPayStatus::Pending,
            created_at: Instant::now(),
            ttl_secs: self.default_ttl_secs,
        };

        self.requests.insert(request_id.clone(), stored);

        Ok(response)
    }

    /// Retrieve an existing QR request by ID.
    pub fn get_request(&self, request_id: &str) -> Option<ErgoPayQrResponse> {
        if let Some(mut stored) = self.requests.get_mut(request_id) {
            stored.check_and_update_status();
            if stored.status == ErgoPayStatus::Expired {
                return None;
            }
            Some(stored.response.clone())
        } else {
            None
        }
    }

    /// Check the status of an ErgoPay request.
    pub fn get_status(&self, request_id: &str) -> Option<ErgoPayStatusResponse> {
        if let Some(mut stored) = self.requests.get_mut(request_id) {
            let status = stored.check_and_update_status();
            Some(ErgoPayStatusResponse {
                request_id: request_id.to_string(),
                status,
                expires_at: stored.response.expires_at,
            })
        } else {
            None
        }
    }

    /// Manually mark a request as completed.
    pub fn mark_completed(&self, request_id: &str) -> Result<(), String> {
        if let Some(mut stored) = self.requests.get_mut(request_id) {
            if stored.status == ErgoPayStatus::Expired {
                return Err("Request has expired".to_string());
            }
            stored.status = ErgoPayStatus::Completed;
            Ok(())
        } else {
            Err("Request not found".to_string())
        }
    }

    /// Manually mark a request as failed.
    pub fn mark_failed(&self, request_id: &str, reason: &str) -> Result<(), String> {
        if let Some(mut stored) = self.requests.get_mut(request_id) {
            if stored.status == ErgoPayStatus::Expired {
                return Err("Request has expired".to_string());
            }
            stored.status = ErgoPayStatus::Failed;
            stored.response.qr_data = format!("FAILED: {}", reason);
            Ok(())
        } else {
            Err("Request not found".to_string())
        }
    }

    /// Clean up expired requests and return count of removed entries.
    pub fn cleanup_expired(&self) -> usize {
        let keys: Vec<String> = self
            .requests
            .iter()
            .filter(|entry| entry.value().is_expired())
            .map(|entry| entry.key().clone())
            .collect();

        let count = keys.len();
        for key in keys {
            self.requests.remove(&key);
        }
        count
    }

    /// Get total number of active (non-expired) requests.
    pub fn active_count(&self) -> usize {
        self.requests
            .iter()
            .filter(|entry| !entry.value().is_expired())
            .count()
    }
}

// ---------------------------------------------------------------------------
// ErgoPayStatusResponse
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ErgoPayStatusResponse {
    pub request_id: String,
    pub status: ErgoPayStatus,
    pub expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// CreateErgoPayQrBody
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct CreateErgoPayQrBody {
    pub amount_nanoerg: u64,
    pub recipient: String,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

pub async fn ergopay_create_qr_handler(
    State(state): State<super::proxy::AppState>,
    Json(body): Json<CreateErgoPayQrBody>,
) -> Json<serde_json::Value> {
    let req = ErgoPayQrRequest::new(body.amount_nanoerg, &body.recipient, body.description.as_deref());
    match state.ergopay_qr_manager.create_request(req) {
        Ok(response) => Json(serde_json::to_value(response).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn ergopay_get_qr_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.ergopay_qr_manager.get_request(&id) {
        Some(response) => Json(serde_json::to_value(response).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found_or_expired" })),
    }
}

pub async fn ergopay_status_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.ergopay_qr_manager.get_status(&id) {
        Some(status) => Json(serde_json::to_value(status).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> ErgoPayQrManager {
        ErgoPayQrManager::new()
    }

    fn make_sample_request() -> ErgoPayQrRequest {
        ErgoPayQrRequest::new(
            1_000_000_000,
            "9fB6zCFUzMZ7kPcBHcY6w7hjVjZzGjUcVb7cDKYFB7R9mCp3NsR",
            Some("Test payment"),
        )
    }

    #[test]
    fn test_ergopay_status_display() {
        assert_eq!(ErgoPayStatus::Pending.as_str(), "Pending");
        assert_eq!(ErgoPayStatus::Completed.as_str(), "Completed");
        assert_eq!(ErgoPayStatus::Expired.as_str(), "Expired");
        assert_eq!(ErgoPayStatus::Failed.as_str(), "Failed");
    }

    #[test]
    fn test_create_request_success() {
        let manager = make_manager();
        let req = make_sample_request();
        let id = req.request_id.clone();
        let response = manager.create_request(req).unwrap();

        assert_eq!(response.request_id, id);
        assert!(response.ergopay_uri.starts_with("ergopay:"));
        assert!(response.qr_data.contains("<svg"));
        assert!(response.expires_at > Utc::now());
    }

    #[test]
    fn test_create_request_empty_recipient() {
        let manager = make_manager();
        let req = ErgoPayQrRequest::new(100, "", None);
        let result = manager.create_request(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_create_request_zero_amount() {
        let manager = make_manager();
        let req = ErgoPayQrRequest::new(0, "9abc", None);
        let result = manager.create_request(req);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero"));
    }

    #[test]
    fn test_get_request() {
        let manager = make_manager();
        let req = make_sample_request();
        let id = req.request_id.clone();
        manager.create_request(req).unwrap();

        let retrieved = manager.get_request(&id).unwrap();
        assert_eq!(retrieved.request_id, id);
        assert!(retrieved.ergopay_uri.contains("ergopay:"));
    }

    #[test]
    fn test_get_request_not_found() {
        let manager = make_manager();
        let result = manager.get_request("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_status() {
        let manager = make_manager();
        let req = make_sample_request();
        let id = req.request_id.clone();
        manager.create_request(req).unwrap();

        let status = manager.get_status(&id).unwrap();
        assert_eq!(status.status, ErgoPayStatus::Pending);
    }

    #[test]
    fn test_mark_completed() {
        let manager = make_manager();
        let req = make_sample_request();
        let id = req.request_id.clone();
        manager.create_request(req).unwrap();

        manager.mark_completed(&id).unwrap();
        let status = manager.get_status(&id).unwrap();
        assert_eq!(status.status, ErgoPayStatus::Completed);
    }

    #[test]
    fn test_mark_failed() {
        let manager = make_manager();
        let req = make_sample_request();
        let id = req.request_id.clone();
        manager.create_request(req).unwrap();

        manager.mark_failed(&id, "User rejected").unwrap();
        let status = manager.get_status(&id).unwrap();
        assert_eq!(status.status, ErgoPayStatus::Failed);
    }

    #[test]
    fn test_mark_completed_not_found() {
        let manager = make_manager();
        let result = manager.mark_completed("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_active_count() {
        let manager = make_manager();
        assert_eq!(manager.active_count(), 0);

        let req = make_sample_request();
        manager.create_request(req).unwrap();
        assert_eq!(manager.active_count(), 1);

        let req2 = make_sample_request();
        manager.create_request(req2).unwrap();
        assert_eq!(manager.active_count(), 2);
    }

    #[test]
    fn test_cleanup_expired() {
        let manager = ErgoPayQrManager::new().with_ttl(0); // TTL = 0 seconds
        let req = make_sample_request();
        let id = req.request_id.clone();
        manager.create_request(req).unwrap();

        // Wait briefly to ensure TTL has passed
        std::thread::sleep(std::time::Duration::from_millis(10));

        let removed = manager.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_build_ergopay_uri() {
        let req = ErgoPayQrRequest::new(
            5_000_000_000,
            "9fB6zCFUzMZ7kPcBHcY6w7hjVjZzGjUcVb7cDKYFB7R9mCp3NsR",
            Some("Test payment"),
        );
        let uri = ErgoPayQrManager::build_ergopay_qr(&req);
        assert!(uri.starts_with("ergopay:9fB6zCFUzMZ7kPcBHcY6w7hjVjZzGjUcVb7cDKYFB7R9mCp3NsR"));
        assert!(uri.contains("amount=5000000000"));
        assert!(uri.contains("description="));
        assert!(uri.contains(&format!("id={}", req.request_id)));
    }

    #[test]
    fn test_build_ergopay_uri_no_description() {
        let req = ErgoPayQrRequest::new(
            1_000_000,
            "9abc123",
            None,
        );
        let uri = ErgoPayQrManager::build_ergopay_qr(&req);
        assert!(!uri.contains("description="));
    }

    #[test]
    fn test_generate_qr_svg() {
        let uri = "ergopay:test?amount=100";
        let svg = ErgoPayQrManager::generate_qr_svg(uri);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("ergopay:test"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let req = make_sample_request();
        let json = serde_json::to_value(&req).unwrap();
        let deserialized: ErgoPayQrRequest = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.request_id, req.request_id);
        assert_eq!(deserialized.amount_nanoerg, req.amount_nanoerg);
        assert_eq!(deserialized.recipient, req.recipient);
    }
}
