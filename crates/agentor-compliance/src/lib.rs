//! Compliance modules for GDPR, ISO 27001, ISO 42001, and DPGA assessments.
//!
//! Provides automated compliance assessment, event-driven hooks for runtime
//! tracking, and structured report generation across multiple regulatory
//! and standards frameworks.
//!
//! # Main types
//!
//! - [`GdprModule`] — GDPR compliance assessment (consent, data subject rights, DPIAs).
//! - [`Iso27001Module`] — ISO 27001 information security assessment.
//! - [`Iso42001Module`] — ISO 42001 AI management system assessment.
//! - [`DpgaAssessment`] — Digital Public Goods Alliance indicator evaluation.
//! - [`ComplianceReport`] — Structured report with findings and recommendations.

/// DPGA (Digital Public Goods Alliance) assessment.
pub mod dpga;
/// GDPR compliance module.
pub mod gdpr;
/// Runtime compliance event hooks.
pub mod hooks;
/// ISO 27001 information security module.
pub mod iso27001;
/// ISO 42001 AI management system module.
pub mod iso42001;
/// Report persistence to JSON files.
pub mod persistence;
/// Compliance report types and generation.
pub mod report;

pub use dpga::{DpgaAssessment, DpgaIndicator, DpgaInput};
pub use gdpr::{ConsentRecord, ConsentStore, DataSubjectRequest, GdprModule};
pub use hooks::{ComplianceEvent, ComplianceHook, ComplianceHookChain, Iso27001Hook, Iso42001Hook};
pub use iso27001::{AccessControlEvent, Iso27001Module, SecurityIncident};
pub use iso42001::{AiSystemRecord, BiasCheck, Iso42001Module, TransparencyLog};
pub use persistence::JsonReportStore;
pub use report::{ComplianceFramework, ComplianceReport, ComplianceStatus, Finding, Severity};
