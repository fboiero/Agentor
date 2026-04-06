use crate::report::{ComplianceFramework, ComplianceReport, Finding, Severity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A consent record for GDPR compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// Unique identifier for this consent record.
    pub id: Uuid,
    /// Identifier of the data subject (e.g., user id or email hash).
    pub subject_id: String,
    /// Processing purpose for which consent was sought.
    pub purpose: String,
    /// Whether consent was granted (`true`) or withdrawn (`false`).
    pub granted: bool,
    /// UTC timestamp of when consent was recorded.
    pub timestamp: DateTime<Utc>,
    /// Optional expiry after which the consent is no longer valid.
    pub expiry: Option<DateTime<Utc>>,
}

/// A data subject request (erasure, portability, access).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSubjectRequest {
    /// Unique identifier for this request.
    pub id: Uuid,
    /// Identifier of the data subject who made the request.
    pub subject_id: String,
    /// Type of data subject right being exercised.
    pub request_type: DataSubjectRequestType,
    /// Current processing status of the request.
    pub status: RequestStatus,
    /// UTC timestamp of when the request was submitted.
    pub created_at: DateTime<Utc>,
    /// UTC timestamp of when the request was fulfilled, if applicable.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Type of GDPR data subject right being exercised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSubjectRequestType {
    /// Right to access personal data (Art. 15).
    Access,
    /// Right to erasure / "right to be forgotten" (Art. 17).
    Erasure,
    /// Right to data portability (Art. 20).
    Portability,
    /// Right to rectification of inaccurate data (Art. 16).
    Rectification,
}

/// Processing status of a data subject request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    /// Request received, not yet started.
    Pending,
    /// Request is being processed.
    InProgress,
    /// Request has been fulfilled.
    Completed,
    /// Request was denied (with documented reason).
    Denied,
}

/// In-memory consent and data subject request store for GDPR compliance.
pub struct ConsentStore {
    records: Arc<RwLock<Vec<ConsentRecord>>>,
    requests: Arc<RwLock<Vec<DataSubjectRequest>>>,
}

impl ConsentStore {
    /// Create a new, empty consent store.
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            requests: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record a consent grant or withdrawal for a data subject.
    pub async fn record_consent(
        &self,
        subject_id: &str,
        purpose: &str,
        granted: bool,
    ) -> ConsentRecord {
        let record = ConsentRecord {
            id: Uuid::new_v4(),
            subject_id: subject_id.to_string(),
            purpose: purpose.to_string(),
            granted,
            timestamp: Utc::now(),
            expiry: None,
        };
        self.records.write().await.push(record.clone());
        record
    }

    /// Check whether the subject has active consent for the given purpose.
    pub async fn check_consent(&self, subject_id: &str, purpose: &str) -> bool {
        let records = self.records.read().await;
        records
            .iter()
            .rev()
            .find(|r| r.subject_id == subject_id && r.purpose == purpose)
            .map(|r| r.granted)
            .unwrap_or(false)
    }

    /// Revoke consent for a subject and purpose by recording a withdrawal.
    pub async fn revoke_consent(&self, subject_id: &str, purpose: &str) {
        self.record_consent(subject_id, purpose, false).await;
    }

    /// Create a new data subject request (access, erasure, portability, rectification).
    pub async fn create_request(
        &self,
        subject_id: &str,
        request_type: DataSubjectRequestType,
    ) -> DataSubjectRequest {
        let request = DataSubjectRequest {
            id: Uuid::new_v4(),
            subject_id: subject_id.to_string(),
            request_type,
            status: RequestStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
        };
        self.requests.write().await.push(request.clone());
        request
    }

    /// Mark a data subject request as completed. Returns `true` if found.
    pub async fn complete_request(&self, request_id: Uuid) -> bool {
        let mut requests = self.requests.write().await;
        if let Some(req) = requests.iter_mut().find(|r| r.id == request_id) {
            req.status = RequestStatus::Completed;
            req.completed_at = Some(Utc::now());
            true
        } else {
            false
        }
    }

    /// Return all consent records.
    pub async fn all_records(&self) -> Vec<ConsentRecord> {
        self.records.read().await.clone()
    }

    /// Return all data subject requests.
    pub async fn all_requests(&self) -> Vec<DataSubjectRequest> {
        self.requests.read().await.clone()
    }

    /// Export all stored data for a subject (for data portability requests).
    pub async fn get_subject_data(&self, subject_id: &str) -> HashMap<String, serde_json::Value> {
        let records = self.records.read().await;
        let subject_records: Vec<&ConsentRecord> = records
            .iter()
            .filter(|r| r.subject_id == subject_id)
            .collect();

        let mut data = HashMap::new();
        data.insert(
            "consent_records".to_string(),
            serde_json::to_value(&subject_records).unwrap_or_default(),
        );
        data
    }
}

impl Default for ConsentStore {
    fn default() -> Self {
        Self::new()
    }
}

/// GDPR compliance module.
pub struct GdprModule {
    /// Consent storage backend.
    pub consent_store: ConsentStore,
}

impl GdprModule {
    /// Create a new GDPR compliance module with a fresh consent store.
    pub fn new() -> Self {
        Self {
            consent_store: ConsentStore::new(),
        }
    }

    /// Generate a GDPR compliance assessment.
    pub fn assess(
        &self,
        has_consent_mechanism: bool,
        has_erasure: bool,
        has_portability: bool,
        has_dpo: bool,
    ) -> ComplianceReport {
        let mut findings = Vec::new();

        findings.push(Finding {
            id: "GDPR-1".to_string(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::Critical,
            title: "Consent Mechanism".to_string(),
            description: "Lawful basis for processing (Art. 6)".to_string(),
            recommendation: if has_consent_mechanism {
                String::new()
            } else {
                "Implement consent collection before data processing".to_string()
            },
            compliant: has_consent_mechanism,
        });

        findings.push(Finding {
            id: "GDPR-2".to_string(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::Critical,
            title: "Right to Erasure".to_string(),
            description: "Data subject right to deletion (Art. 17)".to_string(),
            recommendation: if has_erasure {
                String::new()
            } else {
                "Implement data erasure endpoint and process".to_string()
            },
            compliant: has_erasure,
        });

        findings.push(Finding {
            id: "GDPR-3".to_string(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::High,
            title: "Data Portability".to_string(),
            description: "Right to data portability (Art. 20)".to_string(),
            recommendation: if has_portability {
                String::new()
            } else {
                "Implement data export in machine-readable format".to_string()
            },
            compliant: has_portability,
        });

        findings.push(Finding {
            id: "GDPR-4".to_string(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::Medium,
            title: "Data Protection Officer".to_string(),
            description: "DPO designation (Art. 37)".to_string(),
            recommendation: if has_dpo {
                String::new()
            } else {
                "Designate a Data Protection Officer".to_string()
            },
            compliant: has_dpo,
        });

        ComplianceReport::new(ComplianceFramework::GDPR, findings)
    }
}

impl Default for GdprModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consent_record() {
        let store = ConsentStore::new();
        let record = store.record_consent("user-1", "analytics", true).await;
        assert!(record.granted);
        assert_eq!(record.subject_id, "user-1");
    }

    #[tokio::test]
    async fn test_check_consent() {
        let store = ConsentStore::new();
        store.record_consent("user-1", "analytics", true).await;
        assert!(store.check_consent("user-1", "analytics").await);
        assert!(!store.check_consent("user-1", "marketing").await);
    }

    #[tokio::test]
    async fn test_revoke_consent() {
        let store = ConsentStore::new();
        store.record_consent("user-1", "analytics", true).await;
        assert!(store.check_consent("user-1", "analytics").await);
        store.revoke_consent("user-1", "analytics").await;
        assert!(!store.check_consent("user-1", "analytics").await);
    }

    #[tokio::test]
    async fn test_data_subject_request() {
        let store = ConsentStore::new();
        let req = store
            .create_request("user-1", DataSubjectRequestType::Erasure)
            .await;
        assert_eq!(req.status, RequestStatus::Pending);

        let completed = store.complete_request(req.id).await;
        assert!(completed);
    }

    #[tokio::test]
    async fn test_get_subject_data() {
        let store = ConsentStore::new();
        store.record_consent("user-1", "analytics", true).await;
        let data = store.get_subject_data("user-1").await;
        assert!(data.contains_key("consent_records"));
    }

    #[test]
    fn test_gdpr_assessment_compliant() {
        let module = GdprModule::new();
        let report = module.assess(true, true, true, true);
        assert_eq!(report.status, crate::report::ComplianceStatus::Compliant);
    }

    #[test]
    fn test_gdpr_assessment_partial() {
        let module = GdprModule::new();
        let report = module.assess(true, false, true, false);
        assert_eq!(
            report.status,
            crate::report::ComplianceStatus::PartiallyCompliant
        );
    }
}
