use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// VerificationLevel
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerificationLevel {
    None,
    Basic,
    Verified,
    Trusted,
    Enterprise,
}

impl VerificationLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Basic => "Basic",
            Self::Verified => "Verified",
            Self::Trusted => "Trusted",
            Self::Enterprise => "Enterprise",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "None" => Some(Self::None),
            "Basic" => Some(Self::Basic),
            "Verified" => Some(Self::Verified),
            "Trusted" => Some(Self::Trusted),
            "Enterprise" => Some(Self::Enterprise),
            _ => None,
        }
    }

    pub fn level_value(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Basic => 1,
            Self::Verified => 2,
            Self::Trusted => 3,
            Self::Enterprise => 4,
        }
    }
}

// ---------------------------------------------------------------------------
// CriterionStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CriterionStatus {
    Pending,
    Passed,
    Failed,
}

impl CriterionStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Passed => "Passed",
            Self::Failed => "Failed",
        }
    }
}

// ---------------------------------------------------------------------------
// VerificationCriterion
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerificationCriterion {
    pub criterion_id: String,
    pub name: String,
    pub description: String,
    pub required_for_level: VerificationLevel,
    pub status: CriterionStatus,
    pub evidence: Option<String>,
}

impl VerificationCriterion {
    pub fn new(
        criterion_id: &str,
        name: &str,
        description: &str,
        required_for_level: VerificationLevel,
    ) -> Self {
        Self {
            criterion_id: criterion_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            required_for_level,
            status: CriterionStatus::Pending,
            evidence: None,
        }
    }

    pub fn pass(&mut self, evidence: &str) {
        self.status = CriterionStatus::Passed;
        self.evidence = Some(evidence.to_string());
    }

    pub fn fail(&mut self, reason: &str) {
        self.status = CriterionStatus::Failed;
        self.evidence = Some(reason.to_string());
    }

    pub fn reset(&mut self) {
        self.status = CriterionStatus::Pending;
        self.evidence = None;
    }
}

// ---------------------------------------------------------------------------
// DocumentType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DocumentType {
    IdDocument,
    BusinessLicense,
    TaxCertificate,
    BankStatement,
    Portfolio,
    Certification,
    Insurance,
    Other,
}

impl DocumentType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::IdDocument => "IdDocument",
            Self::BusinessLicense => "BusinessLicense",
            Self::TaxCertificate => "TaxCertificate",
            Self::BankStatement => "BankStatement",
            Self::Portfolio => "Portfolio",
            Self::Certification => "Certification",
            Self::Insurance => "Insurance",
            Self::Other => "Other",
        }
    }
}

// ---------------------------------------------------------------------------
// DocumentStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    Pending,
    UnderReview,
    Approved,
    Rejected,
    Expired,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::UnderReview => "UnderReview",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Expired => "Expired",
        }
    }
}

// ---------------------------------------------------------------------------
// VerificationDocument
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerificationDocument {
    pub doc_id: String,
    pub provider_id: String,
    pub doc_type: DocumentType,
    pub status: DocumentStatus,
    pub submitted_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewer_id: Option<String>,
    pub notes: Option<String>,
}

impl VerificationDocument {
    pub fn new(provider_id: &str, doc_type: DocumentType) -> Self {
        Self {
            doc_id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.to_string(),
            doc_type,
            status: DocumentStatus::Pending,
            submitted_at: Utc::now(),
            reviewed_at: None,
            reviewer_id: None,
            notes: None,
        }
    }
}

// ---------------------------------------------------------------------------
// VerificationConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VerificationConfig {
    pub auto_approve_basic: bool,
    pub required_documents: Vec<DocumentType>,
    pub enterprise_requirements: Vec<String>,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            auto_approve_basic: true,
            required_documents: vec![DocumentType::IdDocument, DocumentType::Portfolio],
            enterprise_requirements: vec![
                "business_license".to_string(),
                "tax_certificate".to_string(),
                "insurance".to_string(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// ProviderVerificationRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderVerificationRecord {
    pub provider_id: String,
    pub current_level: VerificationLevel,
    pub target_level: VerificationLevel,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: String,
    pub criteria: Vec<VerificationCriterion>,
    pub document_ids: Vec<String>,
}

impl ProviderVerificationRecord {
    pub fn new(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            current_level: VerificationLevel::None,
            target_level: VerificationLevel::None,
            submitted_at: Utc::now(),
            updated_at: Utc::now(),
            status: "Not Submitted".to_string(),
            criteria: Vec::new(),
            document_ids: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// SubmitVerificationRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmitVerificationRequest {
    pub provider_id: String,
    pub target_level: VerificationLevel,
    pub documents: Vec<DocumentType>,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// ReviewDocumentRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewDocumentRequest {
    pub doc_id: String,
    pub approved: bool,
    pub reviewer_id: String,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// ProviderVerificationV2
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ProviderVerificationV2 {
    verifications: DashMap<String, ProviderVerificationRecord>,
    documents: DashMap<String, VerificationDocument>,
    criteria: DashMap<String, VerificationCriterion>,
    config: VerificationConfig,
}

impl ProviderVerificationV2 {
    pub fn new(config: VerificationConfig) -> Self {
        Self {
            verifications: DashMap::new(),
            documents: DashMap::new(),
            criteria: DashMap::new(),
            config,
        }
    }

    pub fn default() -> Self {
        Self::new(VerificationConfig::default())
    }

    // ---- Verification submission ----

    pub fn submit_verification(&self, request: &SubmitVerificationRequest) -> Result<String, String> {
        let existing = self.verifications.get(&request.provider_id);
        let current_level = existing
            .as_ref()
            .map(|e| e.value().current_level.clone())
            .unwrap_or(VerificationLevel::None);

        if request.target_level.level_value() <= current_level.level_value() {
            return Err(format!(
                "Target level {:?} must be higher than current level {:?}",
                request.target_level, current_level
            ));
        }

        // Create documents
        let mut doc_ids = Vec::new();
        for doc_type in &request.documents {
            let doc = VerificationDocument::new(&request.provider_id, doc_type.clone());
            doc_ids.push(doc.doc_id.clone());
            self.documents.insert(doc.doc_id.clone(), doc);
        }

        // Create criteria for the target level
        let level_criteria = self.get_criteria_for_level(&request.target_level);
        let mut criteria = Vec::new();
        for c in &level_criteria {
            let criterion = VerificationCriterion::new(
                &c.criterion_id,
                &c.name,
                &c.description,
                request.target_level.clone(),
            );
            self.criteria.insert(criterion.criterion_id.clone(), criterion.clone());
            criteria.push(criterion);
        }

        let existing_doc_ids = existing
            .as_ref()
            .map(|e| e.value().document_ids.clone())
            .unwrap_or_default();

        let mut all_doc_ids = existing_doc_ids;
        all_doc_ids.extend(doc_ids);

        let mut record = ProviderVerificationRecord::new(&request.provider_id);
        record.current_level = current_level;
        record.target_level = request.target_level.clone();
        record.status = "Under Review".to_string();
        record.criteria = criteria;
        record.document_ids = all_doc_ids;

        if let Some(existing_record) = existing {
            record.submitted_at = existing_record.submitted_at;
            record.criteria = existing_record.criteria.clone();
        }

        record.updated_at = Utc::now();

        self.verifications.insert(request.provider_id.clone(), record);

        // Auto-approve basic if configured
        if self.config.auto_approve_basic
            && request.target_level == VerificationLevel::Basic
        {
            self.approve_verification(&request.provider_id, "system", "Auto-approved basic verification")?;
        }

        Ok(request.provider_id.clone())
    }

    // ---- Document review ----

    pub fn review_document(&self, request: &ReviewDocumentRequest) -> Result<bool, String> {
        let mut doc = self
            .documents
            .get_mut(&request.doc_id)
            .ok_or("Document not found")?;

        doc.status = if request.approved {
            DocumentStatus::Approved
        } else {
            DocumentStatus::Rejected
        };
        doc.reviewed_at = Some(Utc::now());
        doc.reviewer_id = Some(request.reviewer_id.clone());
        doc.notes = request.notes.clone();

        Ok(true)
    }

    // ---- Verification approval/rejection ----

    pub fn approve_verification(
        &self,
        provider_id: &str,
        reviewer_id: &str,
        notes: &str,
    ) -> Result<bool, String> {
        let mut record = self
            .verifications
            .get_mut(provider_id)
            .ok_or("Verification record not found")?;

        // Check all documents are approved
        let all_docs_approved = record.document_ids.iter().all(|doc_id| {
            self.documents
                .get(doc_id)
                .map(|d| d.status == DocumentStatus::Approved)
                .unwrap_or(true) // Missing docs don't block
        });

        if !all_docs_approved {
            return Err("Not all documents are approved".to_string());
        }

        // Mark all criteria as passed
        for criterion in &mut record.criteria {
            if criterion.status == CriterionStatus::Pending {
                criterion.status = CriterionStatus::Passed;
                criterion.evidence = Some(format!("Approved by {}", reviewer_id));
            }
        }

        record.current_level = record.target_level.clone();
        record.status = format!("Approved: {}", notes);
        record.updated_at = Utc::now();

        Ok(true)
    }

    pub fn reject_verification(
        &self,
        provider_id: &str,
        reviewer_id: &str,
        reason: &str,
    ) -> Result<bool, String> {
        let mut record = self
            .verifications
            .get_mut(provider_id)
            .ok_or("Verification record not found")?;

        for criterion in &mut record.criteria {
            if criterion.status == CriterionStatus::Pending {
                criterion.status = CriterionStatus::Failed;
                criterion.evidence = Some(format!("Rejected by {}: {}", reviewer_id, reason));
            }
        }

        record.status = format!("Rejected: {}", reason);
        record.updated_at = Utc::now();

        Ok(true)
    }

    // ---- Queries ----

    pub fn get_verification(&self, provider_id: &str) -> Option<ProviderVerificationRecord> {
        self.verifications.get(provider_id).map(|r| r.clone())
    }

    pub fn get_level(&self, provider_id: &str) -> VerificationLevel {
        self.verifications
            .get(provider_id)
            .map(|r| r.value().current_level.clone())
            .unwrap_or(VerificationLevel::None)
    }

    pub fn get_document(&self, doc_id: &str) -> Option<VerificationDocument> {
        self.documents.get(doc_id).map(|d| d.clone())
    }

    pub fn get_provider_documents(&self, provider_id: &str) -> Vec<VerificationDocument> {
        self.documents
            .iter()
            .filter(|e| e.value().provider_id == provider_id)
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn get_pending(&self, limit: usize, offset: usize) -> Vec<ProviderVerificationRecord> {
        let pending: Vec<ProviderVerificationRecord> = self
            .verifications
            .iter()
            .filter(|e| {
                let r = e.value();
                r.status == "Under Review" || r.status.starts_with("Submitted")
            })
            .map(|e| e.value().clone())
            .collect();

        let end = (offset + limit).min(pending.len());
        if offset < pending.len() {
            pending[offset..end].to_vec()
        } else {
            Vec::new()
        }
    }

    pub fn get_all_levels() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "level": "None",
                "value": 0,
                "description": "No verification",
            }),
            serde_json::json!({
                "level": "Basic",
                "value": 1,
                "description": "Basic identity verification",
            }),
            serde_json::json!({
                "level": "Verified",
                "value": 2,
                "description": "Full provider verification",
            }),
            serde_json::json!({
                "level": "Trusted",
                "value": 3,
                "description": "Trusted provider with history",
            }),
            serde_json::json!({
                "level": "Enterprise",
                "value": 4,
                "description": "Enterprise-grade verification",
            }),
        ]
    }

    // ---- Criteria management ----

    fn get_criteria_for_level(&self, level: &VerificationLevel) -> Vec<VerificationCriterion> {
        match level {
            VerificationLevel::Basic => vec![
                VerificationCriterion::new("basic-id", "Identity Verification", "Valid government-issued ID", VerificationLevel::Basic),
                VerificationCriterion::new("basic-email", "Email Verification", "Verified email address", VerificationLevel::Basic),
            ],
            VerificationLevel::Verified => vec![
                VerificationCriterion::new("verified-portfolio", "Portfolio Review", "Demonstrated expertise via portfolio", VerificationLevel::Verified),
                VerificationCriterion::new("verified-history", "Provider History", "Minimum 30 days active", VerificationLevel::Verified),
                VerificationCriterion::new("verified-quality", "Quality Score", "Minimum quality score of 3.5", VerificationLevel::Verified),
            ],
            VerificationLevel::Trusted => vec![
                VerificationCriterion::new("trusted-reputation", "Reputation Score", "Minimum reputation score of 4.0", VerificationLevel::Trusted),
                VerificationCriterion::new("trusted-volume", "Volume Requirements", "Minimum 1000 successful transactions", VerificationLevel::Trusted),
                VerificationCriterion::new("trusted-sla", "SLA Compliance", "99% SLA compliance over 90 days", VerificationLevel::Trusted),
            ],
            VerificationLevel::Enterprise => vec![
                VerificationCriterion::new("enterprise-license", "Business License", "Valid business license", VerificationLevel::Enterprise),
                VerificationCriterion::new("enterprise-insurance", "Insurance", "Professional liability insurance", VerificationLevel::Enterprise),
                VerificationCriterion::new("enterprise-dedication", "Dedicated Resources", "Committed dedicated resources", VerificationLevel::Enterprise),
            ],
            VerificationLevel::None => vec![],
        }
    }

    // ---- Config ----

    pub fn get_config(&self) -> &VerificationConfig {
        &self.config
    }

    // ---- Stats ----

    pub fn get_stats(&self) -> serde_json::Value {
        let total = self.verifications.len();
        let mut level_counts: HashMap<String, usize> = HashMap::new();
        let mut pending_count = 0usize;

        for entry in self.verifications.iter() {
            let r = entry.value();
            *level_counts.entry(r.current_level.as_str().to_string()).or_insert(0) += 1;
            if r.status == "Under Review" {
                pending_count += 1;
            }
        }

        let total_docs = self.documents.len();
        let mut doc_status_counts: HashMap<String, usize> = HashMap::new();
        for entry in self.documents.iter() {
            *doc_status_counts.entry(entry.value().status.as_str().to_string()).or_insert(0) += 1;
        }

        serde_json::json!({
            "total_providers": total,
            "pending_reviews": pending_count,
            "level_distribution": level_counts,
            "total_documents": total_docs,
            "document_status_distribution": doc_status_counts,
            "auto_approve_basic": self.config.auto_approve_basic,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ProviderVerificationV2 {
        ProviderVerificationV2::default()
    }

    #[test]
    fn test_submit_verification_basic() {
        let engine = make_engine();
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Basic,
            documents: vec![DocumentType::IdDocument],
            notes: None,
        };
        let result = engine.submit_verification(&request);
        assert!(result.is_ok());
        let record = engine.get_verification("prov-1").unwrap();
        // Auto-approved since auto_approve_basic is true
        assert_eq!(record.current_level, VerificationLevel::Basic);
    }

    #[test]
    fn test_submit_verification_verified() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio],
            notes: None,
        };
        let result = engine.submit_verification(&request);
        assert!(result.is_ok());
        let record = engine.get_verification("prov-1").unwrap();
        assert_eq!(record.status, "Under Review");
        assert_eq!(record.target_level, VerificationLevel::Verified);
    }

    #[test]
    fn test_submit_invalid_level() {
        let engine = make_engine();
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::None,
            documents: vec![],
            notes: None,
        };
        let result = engine.submit_verification(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_review_document() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio],
            notes: None,
        };
        engine.submit_verification(&request);

        let record = engine.get_verification("prov-1").unwrap();
        let doc_id = &record.document_ids[0];

        let review = ReviewDocumentRequest {
            doc_id: doc_id.clone(),
            approved: true,
            reviewer_id: "rev-1".to_string(),
            notes: Some("Looks good".to_string()),
        };
        let ok = engine.review_document(&review).unwrap();
        assert!(ok);

        let doc = engine.get_document(doc_id).unwrap();
        assert_eq!(doc.status, DocumentStatus::Approved);
    }

    #[test]
    fn test_approve_verification() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio],
            notes: None,
        };
        engine.submit_verification(&request);

        let record = engine.get_verification("prov-1").unwrap();
        let doc_id = &record.document_ids[0];

        engine.review_document(&ReviewDocumentRequest {
            doc_id: doc_id.clone(),
            approved: true,
            reviewer_id: "rev-1".to_string(),
            notes: None,
        });

        let ok = engine.approve_verification("prov-1", "rev-1", "All good").unwrap();
        assert!(ok);

        let record = engine.get_verification("prov-1").unwrap();
        assert_eq!(record.current_level, VerificationLevel::Verified);
    }

    #[test]
    fn test_reject_verification() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio],
            notes: None,
        };
        engine.submit_verification(&request);

        let ok = engine.reject_verification("prov-1", "rev-1", "Incomplete portfolio").unwrap();
        assert!(ok);

        let record = engine.get_verification("prov-1").unwrap();
        assert!(record.status.starts_with("Rejected"));
    }

    #[test]
    fn test_get_level() {
        let engine = make_engine();
        assert_eq!(engine.get_level("nonexistent"), VerificationLevel::None);

        let request = SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Basic,
            documents: vec![DocumentType::IdDocument],
            notes: None,
        };
        engine.submit_verification(&request);
        assert_eq!(engine.get_level("prov-1"), VerificationLevel::Basic);
    }

    #[test]
    fn test_get_pending() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        engine.submit_verification(&SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio],
            notes: None,
        });

        let pending = engine.get_pending(10, 0);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].provider_id, "prov-1");
    }

    #[test]
    fn test_get_all_levels() {
        let levels = ProviderVerificationV2::get_all_levels();
        assert_eq!(levels.len(), 5);
        assert_eq!(levels[0]["level"], "None");
        assert_eq!(levels[4]["level"], "Enterprise");
    }

    #[test]
    fn test_get_stats() {
        let engine = make_engine();
        engine.submit_verification(&SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Basic,
            documents: vec![DocumentType::IdDocument],
            notes: None,
        });

        let stats = engine.get_stats();
        assert_eq!(stats["total_providers"], 1);
        assert!(stats["level_distribution"]["Basic"].as_i64().unwrap_or(0) >= 1);
    }

    #[test]
    fn test_get_provider_documents() {
        let engine = ProviderVerificationV2::new(VerificationConfig {
            auto_approve_basic: false,
            ..Default::default()
        });
        engine.submit_verification(&SubmitVerificationRequest {
            provider_id: "prov-1".to_string(),
            target_level: VerificationLevel::Verified,
            documents: vec![DocumentType::Portfolio, DocumentType::IdDocument],
            notes: None,
        });

        let docs = engine.get_provider_documents("prov-1");
        assert_eq!(docs.len(), 2);
    }
}
