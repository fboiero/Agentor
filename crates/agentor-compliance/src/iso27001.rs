use crate::report::{ComplianceFramework, ComplianceReport, Finding, Severity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// An access control event (login, permission change, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlEvent {
    pub id: Uuid,
    pub subject: String,
    pub action: String,
    pub resource: String,
    pub outcome: AccessOutcome,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessOutcome {
    Granted,
    Denied,
}

/// A security incident record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIncident {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub reported_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncidentStatus {
    Open,
    Investigating,
    Resolved,
    Closed,
}

/// ISO 27001 compliance module — Information Security Management System (ISMS).
pub struct Iso27001Module {
    access_events: Arc<RwLock<Vec<AccessControlEvent>>>,
    incidents: Arc<RwLock<Vec<SecurityIncident>>>,
}

impl Iso27001Module {
    pub fn new() -> Self {
        Self {
            access_events: Arc::new(RwLock::new(Vec::new())),
            incidents: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn log_access(
        &self,
        subject: &str,
        action: &str,
        resource: &str,
        outcome: AccessOutcome,
    ) -> AccessControlEvent {
        let event = AccessControlEvent {
            id: Uuid::new_v4(),
            subject: subject.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            outcome,
            timestamp: Utc::now(),
        };
        self.access_events.write().await.push(event.clone());
        event
    }

    pub async fn report_incident(
        &self,
        title: &str,
        description: &str,
        severity: IncidentSeverity,
    ) -> SecurityIncident {
        let incident = SecurityIncident {
            id: Uuid::new_v4(),
            title: title.to_string(),
            description: description.to_string(),
            severity,
            status: IncidentStatus::Open,
            reported_at: Utc::now(),
            resolved_at: None,
        };
        self.incidents.write().await.push(incident.clone());
        incident
    }

    pub async fn resolve_incident(&self, id: Uuid) -> bool {
        let mut incidents = self.incidents.write().await;
        if let Some(inc) = incidents.iter_mut().find(|i| i.id == id) {
            inc.status = IncidentStatus::Resolved;
            inc.resolved_at = Some(Utc::now());
            true
        } else {
            false
        }
    }

    pub async fn open_incidents_count(&self) -> usize {
        let incidents = self.incidents.read().await;
        incidents
            .iter()
            .filter(|i| {
                i.status == IncidentStatus::Open || i.status == IncidentStatus::Investigating
            })
            .count()
    }

    /// Generate an ISO 27001 compliance assessment.
    pub fn assess(
        &self,
        has_access_control: bool,
        has_encryption: bool,
        has_audit_logging: bool,
        has_incident_response: bool,
        has_risk_assessment: bool,
    ) -> ComplianceReport {
        let mut findings = Vec::new();

        findings.push(Finding {
            id: "ISO27001-A.9".to_string(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::Critical,
            title: "Access Control".to_string(),
            description: "Access control policy and user management (A.9)".to_string(),
            recommendation: if has_access_control {
                String::new()
            } else {
                "Implement role-based access control".to_string()
            },
            compliant: has_access_control,
        });

        findings.push(Finding {
            id: "ISO27001-A.10".to_string(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::Critical,
            title: "Cryptography".to_string(),
            description: "Encryption of data at rest and in transit (A.10)".to_string(),
            recommendation: if has_encryption {
                String::new()
            } else {
                "Implement TLS for transit and encryption at rest".to_string()
            },
            compliant: has_encryption,
        });

        findings.push(Finding {
            id: "ISO27001-A.12".to_string(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::High,
            title: "Audit Logging".to_string(),
            description: "Event logging and monitoring (A.12.4)".to_string(),
            recommendation: if has_audit_logging {
                String::new()
            } else {
                "Implement comprehensive audit logging".to_string()
            },
            compliant: has_audit_logging,
        });

        findings.push(Finding {
            id: "ISO27001-A.16".to_string(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::High,
            title: "Incident Response".to_string(),
            description: "Security incident management (A.16)".to_string(),
            recommendation: if has_incident_response {
                String::new()
            } else {
                "Implement incident response procedures".to_string()
            },
            compliant: has_incident_response,
        });

        findings.push(Finding {
            id: "ISO27001-6.1".to_string(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::Medium,
            title: "Risk Assessment".to_string(),
            description: "Information security risk assessment (6.1.2)".to_string(),
            recommendation: if has_risk_assessment {
                String::new()
            } else {
                "Conduct and document risk assessments".to_string()
            },
            compliant: has_risk_assessment,
        });

        ComplianceReport::new(ComplianceFramework::ISO27001, findings)
    }
}

impl Default for Iso27001Module {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_access_granted() {
        let module = Iso27001Module::new();
        let event = module
            .log_access("admin", "read", "/api/users", AccessOutcome::Granted)
            .await;
        assert_eq!(event.outcome, AccessOutcome::Granted);
    }

    #[tokio::test]
    async fn test_log_access_denied() {
        let module = Iso27001Module::new();
        let event = module
            .log_access("guest", "write", "/api/admin", AccessOutcome::Denied)
            .await;
        assert_eq!(event.outcome, AccessOutcome::Denied);
    }

    #[tokio::test]
    async fn test_report_and_resolve_incident() {
        let module = Iso27001Module::new();
        let incident = module
            .report_incident(
                "SQL Injection attempt",
                "Detected in /api/search",
                IncidentSeverity::High,
            )
            .await;
        assert_eq!(incident.status, IncidentStatus::Open);
        assert_eq!(module.open_incidents_count().await, 1);

        module.resolve_incident(incident.id).await;
        assert_eq!(module.open_incidents_count().await, 0);
    }

    #[test]
    fn test_iso27001_assessment_compliant() {
        let module = Iso27001Module::new();
        let report = module.assess(true, true, true, true, true);
        assert_eq!(report.status, crate::report::ComplianceStatus::Compliant);
    }

    #[test]
    fn test_iso27001_assessment_partial() {
        let module = Iso27001Module::new();
        let report = module.assess(true, false, true, false, true);
        assert_eq!(
            report.status,
            crate::report::ComplianceStatus::PartiallyCompliant
        );
        assert_eq!(report.critical_findings().len(), 1);
    }
}
