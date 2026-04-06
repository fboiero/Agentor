use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported compliance frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceFramework {
    /// EU General Data Protection Regulation.
    GDPR,
    /// ISO 27001 Information Security Management.
    ISO27001,
    /// ISO 42001 AI Management System.
    ISO42001,
    /// Digital Public Goods Alliance standard.
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
    /// All controls are met.
    Compliant,
    /// Some controls are met, others are not.
    PartiallyCompliant,
    /// No controls are met.
    NonCompliant,
}

/// Severity level of a compliance finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Immediate remediation required.
    Critical,
    /// Significant risk; should be addressed promptly.
    High,
    /// Moderate risk; should be scheduled for remediation.
    Medium,
    /// Low risk; informational.
    Low,
    /// Purely informational, no action needed.
    Info,
}

/// A specific compliance finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Unique finding identifier (e.g., "GDPR-ART25", "ISO27001-A.9").
    pub id: String,
    /// Framework this finding belongs to.
    pub framework: ComplianceFramework,
    /// Severity classification of the finding.
    pub severity: Severity,
    /// Short title describing the control.
    pub title: String,
    /// Detailed description of the finding.
    pub description: String,
    /// Recommended remediation action (empty if compliant).
    pub recommendation: String,
    /// Whether this control is satisfied.
    pub compliant: bool,
}

/// A full compliance report for a single framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Framework being assessed.
    pub framework: ComplianceFramework,
    /// Overall compliance status.
    pub status: ComplianceStatus,
    /// Individual findings for each control.
    pub findings: Vec<Finding>,
    /// UTC timestamp of when this report was generated.
    pub generated_at: DateTime<Utc>,
    /// Human-readable summary line.
    pub summary: String,
}

impl ComplianceReport {
    /// Create a report from a list of findings, auto-deriving status and summary.
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

        let summary = format!("{framework}: {compliant_count}/{total} controls compliant");

        Self {
            framework,
            status,
            findings,
            generated_at: Utc::now(),
            summary,
        }
    }

    /// Return all non-compliant findings with critical severity.
    pub fn critical_findings(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Critical && !f.compliant)
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
