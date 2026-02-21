use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported compliance frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceFramework {
    GDPR,
    ISO27001,
    ISO42001,
    DPGA,
}

impl std::fmt::Display for ComplianceFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceFramework::GDPR => write!(f, "GDPR"),
            ComplianceFramework::ISO27001 => write!(f, "ISO 27001"),
            ComplianceFramework::ISO42001 => write!(f, "ISO 42001"),
            ComplianceFramework::DPGA => write!(f, "DPGA"),
        }
    }
}

/// Overall compliance status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplianceStatus {
    Compliant,
    PartiallyCompliant,
    NonCompliant,
}

/// Severity level of a compliance finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// A specific compliance finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub framework: ComplianceFramework,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub recommendation: String,
    pub compliant: bool,
}

/// A full compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub framework: ComplianceFramework,
    pub status: ComplianceStatus,
    pub findings: Vec<Finding>,
    pub generated_at: DateTime<Utc>,
    pub summary: String,
}

impl ComplianceReport {
    pub fn new(framework: ComplianceFramework, findings: Vec<Finding>) -> Self {
        let compliant_count = findings.iter().filter(|f| f.compliant).count();
        let total = findings.len();

        let status = if compliant_count == total {
            ComplianceStatus::Compliant
        } else if compliant_count == 0 {
            ComplianceStatus::NonCompliant
        } else {
            ComplianceStatus::PartiallyCompliant
        };

        let summary = format!(
            "{}: {}/{} controls compliant",
            framework, compliant_count, total
        );

        Self {
            framework,
            status,
            findings,
            generated_at: Utc::now(),
            summary,
        }
    }

    pub fn critical_findings(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Critical && !f.compliant)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_compliant() {
        let findings = vec![Finding {
            id: "GDPR-1".to_string(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::High,
            title: "Consent".to_string(),
            description: "Consent mechanism exists".to_string(),
            recommendation: "".to_string(),
            compliant: true,
        }];
        let report = ComplianceReport::new(ComplianceFramework::GDPR, findings);
        assert_eq!(report.status, ComplianceStatus::Compliant);
    }

    #[test]
    fn test_report_partially_compliant() {
        let findings = vec![
            Finding {
                id: "ISO-1".to_string(),
                framework: ComplianceFramework::ISO27001,
                severity: Severity::High,
                title: "Access Control".to_string(),
                description: "".to_string(),
                recommendation: "".to_string(),
                compliant: true,
            },
            Finding {
                id: "ISO-2".to_string(),
                framework: ComplianceFramework::ISO27001,
                severity: Severity::Critical,
                title: "Encryption".to_string(),
                description: "".to_string(),
                recommendation: "Add TLS".to_string(),
                compliant: false,
            },
        ];
        let report = ComplianceReport::new(ComplianceFramework::ISO27001, findings);
        assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);
        assert_eq!(report.critical_findings().len(), 1);
    }

    #[test]
    fn test_report_non_compliant() {
        let findings = vec![Finding {
            id: "D-1".to_string(),
            framework: ComplianceFramework::DPGA,
            severity: Severity::High,
            title: "Open Source".to_string(),
            description: "".to_string(),
            recommendation: "Publish repo".to_string(),
            compliant: false,
        }];
        let report = ComplianceReport::new(ComplianceFramework::DPGA, findings);
        assert_eq!(report.status, ComplianceStatus::NonCompliant);
    }

    #[test]
    fn test_framework_display() {
        assert_eq!(ComplianceFramework::GDPR.to_string(), "GDPR");
        assert_eq!(ComplianceFramework::ISO27001.to_string(), "ISO 27001");
        assert_eq!(ComplianceFramework::ISO42001.to_string(), "ISO 42001");
        assert_eq!(ComplianceFramework::DPGA.to_string(), "DPGA");
    }

    #[test]
    fn test_report_serialization() {
        let report = ComplianceReport::new(ComplianceFramework::GDPR, vec![]);
        let json = serde_json::to_string(&report).unwrap();
        let parsed: ComplianceReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.framework, ComplianceFramework::GDPR);
    }
}
