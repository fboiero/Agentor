use crate::report::{ComplianceFramework, ComplianceReport, Finding, Severity};
use serde::{Deserialize, Serialize};

/// The 9 DPGA indicators for Digital Public Good certification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DpgaIndicator {
    /// 1. Open source license (OSI-approved)
    OpenSource,
    /// 2. Relevance to Sustainable Development Goals
    SdgRelevance,
    /// 3. Use of or contribution to open data
    OpenData,
    /// 4. Privacy and data protection
    Privacy,
    /// 5. Technical documentation
    Documentation,
    /// 6. Use of open standards
    OpenStandards,
    /// 7. Clear ownership and governance
    Ownership,
    /// 8. Do No Harm assessment
    DoNoHarm,
    /// 9. Platform independence and interoperability
    Interoperability,
}

impl std::fmt::Display for DpgaIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DpgaIndicator::OpenSource => write!(f, "Open Source"),
            DpgaIndicator::SdgRelevance => write!(f, "SDG Relevance"),
            DpgaIndicator::OpenData => write!(f, "Open Data"),
            DpgaIndicator::Privacy => write!(f, "Privacy"),
            DpgaIndicator::Documentation => write!(f, "Documentation"),
            DpgaIndicator::OpenStandards => write!(f, "Open Standards"),
            DpgaIndicator::Ownership => write!(f, "Ownership"),
            DpgaIndicator::DoNoHarm => write!(f, "Do No Harm"),
            DpgaIndicator::Interoperability => write!(f, "Interoperability"),
        }
    }
}

/// Assessment result for each DPGA indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpgaAssessment {
    pub indicator: DpgaIndicator,
    pub compliant: bool,
    pub evidence: String,
    pub recommendation: String,
}

/// Generate a DPGA compliance assessment from individual assessments.
pub fn assess_dpga(assessments: &[DpgaAssessment]) -> ComplianceReport {
    let findings: Vec<Finding> = assessments
        .iter()
        .enumerate()
        .map(|(i, a)| Finding {
            id: format!("DPGA-{}", i + 1),
            framework: ComplianceFramework::DPGA,
            severity: Severity::High,
            title: a.indicator.to_string(),
            description: a.evidence.clone(),
            recommendation: a.recommendation.clone(),
            compliant: a.compliant,
        })
        .collect();

    ComplianceReport::new(ComplianceFramework::DPGA, findings)
}

/// Input for a DPGA compliance assessment.
#[derive(Debug, Clone, Default)]
pub struct DpgaInput {
    pub has_open_license: bool,
    pub has_sdg_docs: bool,
    pub has_open_data: bool,
    pub has_privacy: bool,
    pub has_docs: bool,
    pub has_open_standards: bool,
    pub has_governance: bool,
    pub has_do_no_harm: bool,
    pub has_interop: bool,
}

/// Generate a default DPGA assessment for Agentor.
pub fn assess_agentor_dpga(input: &DpgaInput) -> ComplianceReport {
    let i = input;
    let assessments = vec![
        make_assessment(
            DpgaIndicator::OpenSource,
            i.has_open_license,
            "AGPL-3.0-only license",
            "No OSI-approved license found",
            "Add AGPL-3.0-only license to repository",
        ),
        make_assessment(
            DpgaIndicator::SdgRelevance,
            i.has_sdg_docs,
            "Contributes to SDG 9 (Industry/Innovation) and SDG 16 (Institutions)",
            "No SDG relevance documented",
            "Document how the project contributes to relevant SDGs",
        ),
        make_assessment(
            DpgaIndicator::OpenData,
            i.has_open_data,
            "MCP integration enables open data interoperability",
            "No open data usage documented",
            "Document open data usage or contribution",
        ),
        make_assessment(
            DpgaIndicator::Privacy,
            i.has_privacy,
            "GDPR compliance module with consent tracking and data erasure",
            "No privacy protection mechanisms",
            "Implement GDPR-compliant data protection",
        ),
        make_assessment(
            DpgaIndicator::Documentation,
            i.has_docs,
            "README, API docs, architecture docs, CONTRIBUTING.md",
            "Insufficient documentation",
            "Add comprehensive documentation",
        ),
        make_assessment(
            DpgaIndicator::OpenStandards,
            i.has_open_standards,
            "Uses MCP (AAIF/Linux Foundation), WASM, WIT, JSON-RPC 2.0",
            "No open standards documented",
            "Document use of open standards",
        ),
        make_assessment(
            DpgaIndicator::Ownership,
            i.has_governance,
            "Clear ownership, GOVERNANCE.md, contributor guidelines",
            "No governance structure documented",
            "Add GOVERNANCE.md and define contribution process",
        ),
        make_assessment(
            DpgaIndicator::DoNoHarm,
            i.has_do_no_harm,
            "ISO 42001 AI safety, HITL, bias monitoring, capability-based sandboxing",
            "No do-no-harm assessment",
            "Conduct do-no-harm assessment with bias and safety measures",
        ),
        make_assessment(
            DpgaIndicator::Interoperability,
            i.has_interop,
            "MCP + A2A protocol support, WASM plugins, REST/WebSocket APIs",
            "Limited interoperability",
            "Implement standard protocols for interoperability",
        ),
    ];

    assess_dpga(&assessments)
}

fn make_assessment(
    indicator: DpgaIndicator,
    compliant: bool,
    evidence_yes: &str,
    evidence_no: &str,
    recommendation: &str,
) -> DpgaAssessment {
    DpgaAssessment {
        indicator,
        compliant,
        evidence: if compliant {
            evidence_yes.to_string()
        } else {
            evidence_no.to_string()
        },
        recommendation: if compliant {
            String::new()
        } else {
            recommendation.to_string()
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn all_true() -> DpgaInput {
        DpgaInput {
            has_open_license: true,
            has_sdg_docs: true,
            has_open_data: true,
            has_privacy: true,
            has_docs: true,
            has_open_standards: true,
            has_governance: true,
            has_do_no_harm: true,
            has_interop: true,
        }
    }

    #[test]
    fn test_dpga_all_compliant() {
        let report = assess_agentor_dpga(&all_true());
        assert_eq!(report.status, crate::report::ComplianceStatus::Compliant);
        assert_eq!(report.findings.len(), 9);
    }

    #[test]
    fn test_dpga_partial() {
        let mut input = all_true();
        input.has_open_data = false;
        input.has_docs = false;
        input.has_governance = false;
        let report = assess_agentor_dpga(&input);
        assert_eq!(
            report.status,
            crate::report::ComplianceStatus::PartiallyCompliant
        );
    }

    #[test]
    fn test_dpga_indicator_display() {
        assert_eq!(DpgaIndicator::OpenSource.to_string(), "Open Source");
        assert_eq!(DpgaIndicator::DoNoHarm.to_string(), "Do No Harm");
        assert_eq!(
            DpgaIndicator::Interoperability.to_string(),
            "Interoperability"
        );
    }

    #[test]
    fn test_dpga_custom_assessment() {
        let assessments = vec![DpgaAssessment {
            indicator: DpgaIndicator::OpenSource,
            compliant: true,
            evidence: "MIT license".to_string(),
            recommendation: String::new(),
        }];
        let report = assess_dpga(&assessments);
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].compliant);
    }

    #[test]
    fn test_dpga_serialization() {
        let assessment = DpgaAssessment {
            indicator: DpgaIndicator::Privacy,
            compliant: true,
            evidence: "GDPR module".to_string(),
            recommendation: String::new(),
        };
        let json = serde_json::to_string(&assessment).unwrap();
        let parsed: DpgaAssessment = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.indicator, DpgaIndicator::Privacy);
    }
}
