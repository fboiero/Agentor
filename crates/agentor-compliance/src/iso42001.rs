use crate::report::{ComplianceFramework, ComplianceReport, Finding, Severity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Record of an AI system in the inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSystemRecord {
    pub id: Uuid,
    pub name: String,
    pub purpose: String,
    pub model_provider: String,
    pub model_id: String,
    pub risk_level: RiskLevel,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Bias check result for an AI system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasCheck {
    pub id: Uuid,
    pub system_id: Uuid,
    pub check_type: String,
    pub result: BiasResult,
    pub details: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BiasResult {
    Pass,
    Warning,
    Fail,
}

/// Transparency log entry — records decisions made by AI systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransparencyLog {
    pub id: Uuid,
    pub system_id: Uuid,
    pub action: String,
    pub input_summary: String,
    pub output_summary: String,
    pub reasoning: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// ISO 42001 compliance module — AI Management System (AIMS).
pub struct Iso42001Module {
    systems: Arc<RwLock<Vec<AiSystemRecord>>>,
    bias_checks: Arc<RwLock<Vec<BiasCheck>>>,
    transparency_logs: Arc<RwLock<Vec<TransparencyLog>>>,
}

impl Iso42001Module {
    pub fn new() -> Self {
        Self {
            systems: Arc::new(RwLock::new(Vec::new())),
            bias_checks: Arc::new(RwLock::new(Vec::new())),
            transparency_logs: Arc::new(RwLock::new(Vec::new())),
        }
    }

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
            reasoning: reasoning.map(|s| s.to_string()),
            timestamp: Utc::now(),
        };
        self.transparency_logs.write().await.push(log.clone());
        log
    }

    pub async fn system_count(&self) -> usize {
        self.systems.read().await.len()
    }

    pub async fn bias_check_failures(&self) -> Vec<BiasCheck> {
        let checks = self.bias_checks.read().await;
        checks
            .iter()
            .filter(|c| c.result == BiasResult::Fail)
            .cloned()
            .collect()
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_system() {
        let module = Iso42001Module::new();
        let system = module
            .register_system(
                "Agentor Chat",
                "Customer support",
                "Anthropic",
                "claude-sonnet-4-20250514",
                RiskLevel::Medium,
            )
            .await;
        assert_eq!(system.name, "Agentor Chat");
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
