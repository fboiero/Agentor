use crate::report::{ComplianceFramework, ComplianceReport, Finding, Severity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Record of an AI system in the inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSystemRecord {
    /// Unique identifier for this system record.
    pub id: Uuid,
    /// Human-readable system name.
    pub name: String,
    /// Description of the system's purpose.
    pub purpose: String,
    /// LLM provider (e.g., "anthropic", "openai").
    pub model_provider: String,
    /// Model identifier (e.g., "claude-sonnet-4-20250514").
    pub model_id: String,
    /// Assessed risk level of this AI system.
    pub risk_level: RiskLevel,
    /// UTC timestamp of when the system was registered.
    pub registered_at: DateTime<Utc>,
}

/// Risk level classification for an AI system (ISO 42001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Minimal risk; routine monitoring.
    Low,
    /// Moderate risk; periodic review required.
    Medium,
    /// High risk; continuous monitoring and safeguards required.
    High,
    /// Critical risk; requires executive approval and audit trail.
    Critical,
}

/// Bias check result for an AI system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasCheck {
    /// Unique identifier for this check.
    pub id: Uuid,
    /// AI system this check was performed on.
    pub system_id: Uuid,
    /// Type of bias check performed (e.g., "demographic_parity").
    pub check_type: String,
    /// Outcome of the bias check.
    pub result: BiasResult,
    /// Human-readable details about the check findings.
    pub details: String,
    /// UTC timestamp of when the check was performed.
    pub checked_at: DateTime<Utc>,
}

/// Outcome of a bias evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BiasResult {
    /// No significant bias detected.
    Pass,
    /// Potential bias detected; warrants review.
    Warning,
    /// Significant bias detected; remediation required.
    Fail,
}

/// Transparency log entry -- records decisions made by AI systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransparencyLog {
    /// Unique identifier for this log entry.
    pub id: Uuid,
    /// AI system that made the decision.
    pub system_id: Uuid,
    /// Action or decision taken by the system.
    pub action: String,
    /// Summary of the input that prompted the action.
    pub input_summary: String,
    /// Summary of the output produced.
    pub output_summary: String,
    /// Optional chain-of-thought or reasoning explanation.
    pub reasoning: Option<String>,
    /// UTC timestamp of when the action occurred.
    pub timestamp: DateTime<Utc>,
}

/// ISO 42001 compliance module — AI Management System (AIMS).
pub struct Iso42001Module {
    systems: Arc<RwLock<Vec<AiSystemRecord>>>,
    bias_checks: Arc<RwLock<Vec<BiasCheck>>>,
    transparency_logs: Arc<RwLock<Vec<TransparencyLog>>>,
}

impl Iso42001Module {
    /// Create a new, empty ISO 42001 module.
    pub fn new() -> Self {
        Self {
            systems: Arc::new(RwLock::new(Vec::new())),
            bias_checks: Arc::new(RwLock::new(Vec::new())),
            transparency_logs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register an AI system in the inventory and return the created record.
    pub async fn register_system(
        &self,
        name: &str,
        purpose: &str,
        model_provider: &str,
        model_id: &str,
        risk_level: RiskLevel,
    ) -> AiSystemRecord {
        let record = AiSystemRecord {
            id: Uuid::new_v4(),
            name: name.to_string(),
            purpose: purpose.to_string(),
            model_provider: model_provider.to_string(),
            model_id: model_id.to_string(),
            risk_level,
            registered_at: Utc::now(),
        };
        self.systems.write().await.push(record.clone());
        record
    }

    /// Record a bias evaluation for the given AI system.
    pub async fn record_bias_check(
        &self,
        system_id: Uuid,
        check_type: &str,
        result: BiasResult,
        details: &str,
    ) -> BiasCheck {
        let check = BiasCheck {
            id: Uuid::new_v4(),
            system_id,
            check_type: check_type.to_string(),
            result,
            details: details.to_string(),
            checked_at: Utc::now(),
        };
        self.bias_checks.write().await.push(check.clone());
        check
    }

    /// Record a transparency log entry for an AI decision.
    pub async fn log_transparency(
        &self,
        system_id: Uuid,
        action: &str,
        input_summary: &str,
        output_summary: &str,
        reasoning: Option<&str>,
    ) -> TransparencyLog {
        let log = TransparencyLog {
            id: Uuid::new_v4(),
            system_id,
            action: action.to_string(),
            input_summary: input_summary.to_string(),
            output_summary: output_summary.to_string(),
            reasoning: reasoning.map(std::string::ToString::to_string),
            timestamp: Utc::now(),
        };
        self.transparency_logs.write().await.push(log.clone());
        log
    }

    /// Return the number of registered AI systems.
    pub async fn system_count(&self) -> usize {
        self.systems.read().await.len()
    }

    /// Return all bias checks that resulted in a `Fail` outcome.
    pub async fn bias_check_failures(&self) -> Vec<BiasCheck> {
        let checks = self.bias_checks.read().await;
        checks
            .iter()
            .filter(|c| c.result == BiasResult::Fail)
            .cloned()
            .collect()
    }

    /// Get the total number of transparency log entries.
    pub async fn transparency_log_count(&self) -> usize {
        self.transparency_logs.read().await.len()
    }

    /// Return all registered AI systems.
    pub async fn all_systems(&self) -> Vec<AiSystemRecord> {
        self.systems.read().await.clone()
    }

    /// Return all bias checks.
    pub async fn all_bias_checks(&self) -> Vec<BiasCheck> {
        self.bias_checks.read().await.clone()
    }

    /// Return all transparency log entries.
    pub async fn all_transparency_logs(&self) -> Vec<TransparencyLog> {
        self.transparency_logs.read().await.clone()
    }

    /// Generate an ISO 42001 compliance assessment.
    pub fn assess(
        &self,
        has_ai_inventory: bool,
        has_risk_assessment: bool,
        has_bias_monitoring: bool,
        has_transparency: bool,
        has_human_oversight: bool,
    ) -> ComplianceReport {
        let mut findings = Vec::new();

        findings.push(Finding {
            id: "ISO42001-6.1".to_string(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Critical,
            title: "AI System Inventory".to_string(),
            description: "Documented inventory of all AI systems (6.1)".to_string(),
            recommendation: if has_ai_inventory {
                String::new()
            } else {
                "Register all AI systems with purpose, model, and risk level".to_string()
            },
            compliant: has_ai_inventory,
        });

        findings.push(Finding {
            id: "ISO42001-6.2".to_string(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Critical,
            title: "AI Risk Assessment".to_string(),
            description: "Risk assessment for AI systems (6.2)".to_string(),
            recommendation: if has_risk_assessment {
                String::new()
            } else {
                "Conduct risk assessments for each AI system".to_string()
            },
            compliant: has_risk_assessment,
        });

        findings.push(Finding {
            id: "ISO42001-8.4".to_string(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::High,
            title: "Bias Monitoring".to_string(),
            description: "Monitoring AI systems for bias and fairness (8.4)".to_string(),
            recommendation: if has_bias_monitoring {
                String::new()
            } else {
                "Implement bias detection and monitoring procedures".to_string()
            },
            compliant: has_bias_monitoring,
        });

        findings.push(Finding {
            id: "ISO42001-7.5".to_string(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::High,
            title: "Transparency & Explainability".to_string(),
            description: "Transparent AI decision-making processes (7.5)".to_string(),
            recommendation: if has_transparency {
                String::new()
            } else {
                "Log AI decisions with inputs, outputs, and reasoning".to_string()
            },
            compliant: has_transparency,
        });

        findings.push(Finding {
            id: "ISO42001-9.1".to_string(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Critical,
            title: "Human Oversight".to_string(),
            description: "Human-in-the-loop for high-risk AI decisions (9.1)".to_string(),
            recommendation: if has_human_oversight {
                String::new()
            } else {
                "Implement HITL for critical AI operations".to_string()
            },
            compliant: has_human_oversight,
        });

        ComplianceReport::new(ComplianceFramework::ISO42001, findings)
    }
}

impl Default for Iso42001Module {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_system() {
        let module = Iso42001Module::new();
        let system = module
            .register_system(
                "Argentor Chat",
                "Customer support",
                "Anthropic",
                "claude-sonnet-4-20250514",
                RiskLevel::Medium,
            )
            .await;
        assert_eq!(system.name, "Argentor Chat");
        assert_eq!(module.system_count().await, 1);
    }

    #[tokio::test]
    async fn test_bias_check() {
        let module = Iso42001Module::new();
        let system = module
            .register_system("Test", "Test", "Test", "test", RiskLevel::Low)
            .await;

        module
            .record_bias_check(
                system.id,
                "gender_bias",
                BiasResult::Pass,
                "No bias detected",
            )
            .await;
        module
            .record_bias_check(system.id, "age_bias", BiasResult::Fail, "Age bias detected")
            .await;

        let failures = module.bias_check_failures().await;
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].check_type, "age_bias");
    }

    #[tokio::test]
    async fn test_transparency_log() {
        let module = Iso42001Module::new();
        let system = module
            .register_system("Test", "Test", "Test", "test", RiskLevel::Low)
            .await;

        let log = module
            .log_transparency(
                system.id,
                "classify_ticket",
                "User complaint about billing",
                "Category: billing",
                Some("Matched keywords: billing, charge, payment"),
            )
            .await;
        assert_eq!(log.action, "classify_ticket");
        assert!(log.reasoning.is_some());
    }

    #[test]
    fn test_iso42001_assessment_compliant() {
        let module = Iso42001Module::new();
        let report = module.assess(true, true, true, true, true);
        assert_eq!(report.status, crate::report::ComplianceStatus::Compliant);
    }

    #[test]
    fn test_iso42001_assessment_missing_hitl() {
        let module = Iso42001Module::new();
        let report = module.assess(true, true, true, true, false);
        assert_eq!(
            report.status,
            crate::report::ComplianceStatus::PartiallyCompliant
        );
        assert_eq!(report.critical_findings().len(), 1);
    }
}
