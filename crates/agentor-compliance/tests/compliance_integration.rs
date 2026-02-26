#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for the agentor-compliance crate.
//!
//! Covers GDPR, ISO 27001, ISO 42001, DPGA assessments, report persistence,
//! hook chains, status aggregation, and multi-framework assessment.

use agentor_compliance::*;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 1. GDPR assessment
// ---------------------------------------------------------------------------

#[test]
fn test_gdpr_assessment_returns_compliance_report() {
    let module = GdprModule::new();

    // Partially compliant: consent + erasure present, portability + DPO missing
    let report = module.assess(true, true, false, false);

    assert_eq!(report.framework, ComplianceFramework::GDPR);
    assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);
    assert_eq!(report.findings.len(), 4);
    assert!(!report.summary.is_empty());

    // The two compliant findings
    assert!(report.findings[0].compliant); // consent
    assert!(report.findings[1].compliant); // erasure
    // The two non-compliant findings
    assert!(!report.findings[2].compliant); // portability
    assert!(!report.findings[3].compliant); // DPO

    // Fully compliant case
    let full_report = module.assess(true, true, true, true);
    assert_eq!(full_report.status, ComplianceStatus::Compliant);
}

// ---------------------------------------------------------------------------
// 2. ISO 27001 assessment
// ---------------------------------------------------------------------------

#[test]
fn test_iso27001_assessment_returns_compliance_report() {
    let module = Iso27001Module::new();

    // All controls present except encryption and incident response
    let report = module.assess(true, false, true, false, true);

    assert_eq!(report.framework, ComplianceFramework::ISO27001);
    assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);
    assert_eq!(report.findings.len(), 5);

    // Encryption (A.10) is critical and non-compliant
    let critical = report.critical_findings();
    assert_eq!(critical.len(), 1);
    assert_eq!(critical[0].id, "ISO27001-A.10");

    // Fully compliant case
    let full_report = module.assess(true, true, true, true, true);
    assert_eq!(full_report.status, ComplianceStatus::Compliant);
    assert!(full_report.critical_findings().is_empty());
}

// ---------------------------------------------------------------------------
// 3. ISO 42001 assessment
// ---------------------------------------------------------------------------

#[test]
fn test_iso42001_assessment_returns_compliance_report() {
    let module = Iso42001Module::new();

    // Missing human oversight (critical) and bias monitoring (high)
    let report = module.assess(true, true, false, true, false);

    assert_eq!(report.framework, ComplianceFramework::ISO42001);
    assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);
    assert_eq!(report.findings.len(), 5);

    // Human oversight is critical and missing
    let critical = report.critical_findings();
    assert_eq!(critical.len(), 1);
    assert_eq!(critical[0].id, "ISO42001-9.1");

    // Fully compliant case
    let full_report = module.assess(true, true, true, true, true);
    assert_eq!(full_report.status, ComplianceStatus::Compliant);
}

// ---------------------------------------------------------------------------
// 4. DPGA assessment with DpgaInput
// ---------------------------------------------------------------------------

#[test]
fn test_dpga_assessment_evaluates_all_nine_indicators() {
    let input = DpgaInput {
        has_open_license: true,
        has_sdg_docs: true,
        has_open_data: false,
        has_privacy: true,
        has_docs: true,
        has_open_standards: true,
        has_governance: false,
        has_do_no_harm: true,
        has_interop: true,
    };

    let report = agentor_compliance::dpga::assess_agentor_dpga(&input);

    assert_eq!(report.framework, ComplianceFramework::DPGA);
    assert_eq!(report.findings.len(), 9);
    assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);

    // Verify the two non-compliant indicators have recommendations
    let non_compliant: Vec<_> = report.findings.iter().filter(|f| !f.compliant).collect();
    assert_eq!(non_compliant.len(), 2);
    for finding in &non_compliant {
        assert!(!finding.recommendation.is_empty());
    }

    // Verify compliant indicators have empty recommendations
    let compliant: Vec<_> = report.findings.iter().filter(|f| f.compliant).collect();
    assert_eq!(compliant.len(), 7);
    for finding in &compliant {
        assert!(finding.recommendation.is_empty());
    }

    // Fully compliant case
    let all_true = DpgaInput {
        has_open_license: true,
        has_sdg_docs: true,
        has_open_data: true,
        has_privacy: true,
        has_docs: true,
        has_open_standards: true,
        has_governance: true,
        has_do_no_harm: true,
        has_interop: true,
    };
    let full_report = agentor_compliance::dpga::assess_agentor_dpga(&all_true);
    assert_eq!(full_report.status, ComplianceStatus::Compliant);
}

// ---------------------------------------------------------------------------
// 5. Report persistence — save + load round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_report_persistence_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let store = JsonReportStore::new(tmp.path());

    // Generate a GDPR report
    let module = GdprModule::new();
    let original = module.assess(true, true, false, true);

    // Save it
    let path = store.save_report(&original).await.unwrap();
    assert!(path.exists());
    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("gdpr_"));
    assert!(filename.ends_with(".json"));

    // Load it back
    let loaded = store
        .load_latest(ComplianceFramework::GDPR)
        .await
        .unwrap()
        .expect("should find the saved report");

    // Verify contents match
    assert_eq!(loaded.framework, original.framework);
    assert_eq!(loaded.status, original.status);
    assert_eq!(loaded.findings.len(), original.findings.len());
    assert_eq!(loaded.summary, original.summary);

    // Verify individual findings match
    for (orig, load) in original.findings.iter().zip(loaded.findings.iter()) {
        assert_eq!(orig.id, load.id);
        assert_eq!(orig.compliant, load.compliant);
        assert_eq!(orig.title, load.title);
        assert_eq!(orig.severity, load.severity);
    }

    // Verify list_reports returns the file
    let all = store.list_reports().await.unwrap();
    assert_eq!(all.len(), 1);
}

// ---------------------------------------------------------------------------
// 6. Hook chain — Iso27001Hook + Iso42001Hook process events
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hook_chain_dispatches_to_both_hooks() {
    let iso27001_module = Arc::new(Iso27001Module::new());
    let iso42001_module = Arc::new(Iso42001Module::new());

    // Register a system for ISO 42001
    let system = iso42001_module
        .register_system(
            "Agentor",
            "AI agent framework",
            "Anthropic",
            "claude-opus-4-6",
            agentor_compliance::iso42001::RiskLevel::Medium,
        )
        .await;

    let iso27001_hook = Arc::new(Iso27001Hook::new(iso27001_module.clone()));
    let iso42001_hook = Arc::new(Iso42001Hook::new(iso42001_module.clone(), system.id));

    let mut chain = ComplianceHookChain::new();
    chain.add(iso27001_hook);
    chain.add(iso42001_hook);
    assert_eq!(chain.hook_count(), 2);

    // Emit a ToolCall event (ISO 27001 hook should log access, ISO 42001 ignores it)
    chain
        .emit(ComplianceEvent::ToolCall {
            agent_id: "agent-1".into(),
            tool_name: "file_read".into(),
            timestamp: Utc::now(),
            success: true,
        })
        .await;

    assert_eq!(iso27001_module.access_event_count().await, 1);
    assert_eq!(iso42001_module.transparency_log_count().await, 0);

    // Emit a TaskStarted event (ISO 27001 hook logs access, ISO 42001 ignores it)
    chain
        .emit(ComplianceEvent::TaskStarted {
            task_id: Uuid::new_v4(),
            role: "coder".into(),
            description: "Implement feature X".into(),
            timestamp: Utc::now(),
        })
        .await;

    assert_eq!(iso27001_module.access_event_count().await, 2);
    assert_eq!(iso42001_module.transparency_log_count().await, 0);

    // Emit a TaskCompleted event (ISO 27001 ignores it, ISO 42001 logs transparency)
    chain
        .emit(ComplianceEvent::TaskCompleted {
            task_id: Uuid::new_v4(),
            role: "tester".into(),
            duration_ms: 2000,
            artifacts_count: 3,
            timestamp: Utc::now(),
        })
        .await;

    assert_eq!(iso27001_module.access_event_count().await, 2);
    assert_eq!(iso42001_module.transparency_log_count().await, 1);

    // Emit approval events (only ISO 42001 processes these)
    chain
        .emit(ComplianceEvent::ApprovalRequested {
            task_id: "deploy-42".into(),
            risk_level: "high".into(),
            timestamp: Utc::now(),
        })
        .await;

    chain
        .emit(ComplianceEvent::ApprovalDecided {
            task_id: "deploy-42".into(),
            approved: true,
            reviewer: "admin".into(),
            timestamp: Utc::now(),
        })
        .await;

    assert_eq!(iso27001_module.access_event_count().await, 2);
    assert_eq!(iso42001_module.transparency_log_count().await, 3);
}

// ---------------------------------------------------------------------------
// 7. Compliance status aggregation — mixed findings produce correct status
// ---------------------------------------------------------------------------

#[test]
fn test_compliance_status_aggregation_with_mixed_findings() {
    // All compliant -> Compliant
    let all_pass = vec![
        Finding {
            id: "F-1".into(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::Critical,
            title: "Consent".into(),
            description: "Has consent".into(),
            recommendation: String::new(),
            compliant: true,
        },
        Finding {
            id: "F-2".into(),
            framework: ComplianceFramework::GDPR,
            severity: Severity::High,
            title: "Erasure".into(),
            description: "Has erasure".into(),
            recommendation: String::new(),
            compliant: true,
        },
    ];
    let report = ComplianceReport::new(ComplianceFramework::GDPR, all_pass);
    assert_eq!(report.status, ComplianceStatus::Compliant);
    assert!(report.critical_findings().is_empty());

    // None compliant -> NonCompliant
    let all_fail = vec![
        Finding {
            id: "F-1".into(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::Critical,
            title: "Access Control".into(),
            description: "Missing".into(),
            recommendation: "Add RBAC".into(),
            compliant: false,
        },
        Finding {
            id: "F-2".into(),
            framework: ComplianceFramework::ISO27001,
            severity: Severity::High,
            title: "Encryption".into(),
            description: "Missing".into(),
            recommendation: "Add TLS".into(),
            compliant: false,
        },
    ];
    let report = ComplianceReport::new(ComplianceFramework::ISO27001, all_fail);
    assert_eq!(report.status, ComplianceStatus::NonCompliant);
    assert_eq!(report.critical_findings().len(), 1);

    // Mixed -> PartiallyCompliant
    let mixed = vec![
        Finding {
            id: "F-1".into(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Critical,
            title: "AI Inventory".into(),
            description: "Present".into(),
            recommendation: String::new(),
            compliant: true,
        },
        Finding {
            id: "F-2".into(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Critical,
            title: "Human Oversight".into(),
            description: "Missing".into(),
            recommendation: "Add HITL".into(),
            compliant: false,
        },
        Finding {
            id: "F-3".into(),
            framework: ComplianceFramework::ISO42001,
            severity: Severity::Medium,
            title: "Documentation".into(),
            description: "Present".into(),
            recommendation: String::new(),
            compliant: true,
        },
    ];
    let report = ComplianceReport::new(ComplianceFramework::ISO42001, mixed);
    assert_eq!(report.status, ComplianceStatus::PartiallyCompliant);
    // Only one critical non-compliant finding
    assert_eq!(report.critical_findings().len(), 1);
    assert_eq!(report.critical_findings()[0].id, "F-2");

    // Empty findings -> Compliant (0/0 = all compliant)
    let empty_report = ComplianceReport::new(ComplianceFramework::DPGA, vec![]);
    assert_eq!(empty_report.status, ComplianceStatus::Compliant);
}

// ---------------------------------------------------------------------------
// 8. Multiple framework assessment — run all 4, verify each report
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_framework_assessments_sequentially() {
    // 1. GDPR assessment
    let gdpr = GdprModule::new();
    let gdpr_report = gdpr.assess(true, true, true, false);
    assert_eq!(gdpr_report.framework, ComplianceFramework::GDPR);
    assert_eq!(gdpr_report.status, ComplianceStatus::PartiallyCompliant);
    assert_eq!(gdpr_report.findings.len(), 4);
    assert!(gdpr_report.summary.contains("GDPR"));

    // 2. ISO 27001 assessment
    let iso27001 = Iso27001Module::new();
    let iso27001_report = iso27001.assess(true, true, true, true, true);
    assert_eq!(iso27001_report.framework, ComplianceFramework::ISO27001);
    assert_eq!(iso27001_report.status, ComplianceStatus::Compliant);
    assert_eq!(iso27001_report.findings.len(), 5);
    assert!(iso27001_report.summary.contains("ISO 27001"));

    // 3. ISO 42001 assessment
    let iso42001 = Iso42001Module::new();
    let iso42001_report = iso42001.assess(true, true, true, true, true);
    assert_eq!(iso42001_report.framework, ComplianceFramework::ISO42001);
    assert_eq!(iso42001_report.status, ComplianceStatus::Compliant);
    assert_eq!(iso42001_report.findings.len(), 5);
    assert!(iso42001_report.summary.contains("ISO 42001"));

    // 4. DPGA assessment
    let dpga_input = DpgaInput {
        has_open_license: true,
        has_sdg_docs: true,
        has_open_data: true,
        has_privacy: true,
        has_docs: true,
        has_open_standards: true,
        has_governance: true,
        has_do_no_harm: true,
        has_interop: true,
    };
    let dpga_report = agentor_compliance::dpga::assess_agentor_dpga(&dpga_input);
    assert_eq!(dpga_report.framework, ComplianceFramework::DPGA);
    assert_eq!(dpga_report.status, ComplianceStatus::Compliant);
    assert_eq!(dpga_report.findings.len(), 9);
    assert!(dpga_report.summary.contains("DPGA"));

    // Collect all reports and verify they are distinct frameworks
    let reports = [&gdpr_report, &iso27001_report, &iso42001_report, &dpga_report];
    let frameworks: Vec<ComplianceFramework> = reports.iter().map(|r| r.framework).collect();
    assert_eq!(frameworks.len(), 4);
    assert!(frameworks.contains(&ComplianceFramework::GDPR));
    assert!(frameworks.contains(&ComplianceFramework::ISO27001));
    assert!(frameworks.contains(&ComplianceFramework::ISO42001));
    assert!(frameworks.contains(&ComplianceFramework::DPGA));

    // Verify total finding count across all frameworks
    let total_findings: usize = reports.iter().map(|r| r.findings.len()).sum();
    assert_eq!(total_findings, 4 + 5 + 5 + 9); // 23 total
}
