//! Data residency configuration for multi-region compliance.
//!
//! This module provides types and validation logic to enforce data sovereignty
//! requirements such as GDPR (EU), HIPAA (US healthcare), and regional LLM
//! routing policies. It is designed for enterprise deployments where data must
//! stay within a specific jurisdiction.
//!
//! # Quick start
//!
//! ```rust
//! use argentor_core::data_residency::{eu_gdpr_config, ResidencyValidator};
//!
//! let config = eu_gdpr_config();
//! let issues = ResidencyValidator::validate_config(&config);
//! assert!(issues.is_empty());
//! assert!(ResidencyValidator::is_compliant(&config, "gdpr"));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// DataRegion
// ---------------------------------------------------------------------------

/// Supported deployment regions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataRegion {
    /// European Union — forces EU-based LLM endpoints and EU storage.
    EU,
    /// United States — standard US endpoints.
    US,
    /// Latin America — LATAM-compatible, Spanish defaults.
    LATAM,
    /// Asia-Pacific.
    APAC,
    /// A custom region identifier (e.g. `"me-south-1"`).
    Custom(String),
}

impl fmt::Display for DataRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataRegion::EU => write!(f, "EU"),
            DataRegion::US => write!(f, "US"),
            DataRegion::LATAM => write!(f, "LATAM"),
            DataRegion::APAC => write!(f, "APAC"),
            DataRegion::Custom(s) => write!(f, "Custom({s})"),
        }
    }
}

// ---------------------------------------------------------------------------
// LlmRoutingPolicy
// ---------------------------------------------------------------------------

/// How LLM API calls are routed based on data residency requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmRoutingPolicy {
    /// No restrictions — any provider endpoint is acceptable.
    AnyRegion,
    /// Only use providers with endpoints in the configured region.
    SameRegion,
    /// Prefer same-region endpoints, fall back to others when unavailable.
    PreferRegion,
    /// Only use these specific endpoint URLs.
    ExplicitEndpoints(Vec<String>),
}

// ---------------------------------------------------------------------------
// PiiHandlingPolicy
// ---------------------------------------------------------------------------

/// Policy for personally identifiable information (PII) in LLM requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiHandlingPolicy {
    /// Strip PII fields before sending data to the LLM.
    Redact,
    /// Encrypt PII fields in-place (reversible with key).
    Encrypt,
    /// No special handling — suitable for fully on-premises deployments.
    Allow,
    /// Reject any request that contains detected PII.
    Reject,
}

// ---------------------------------------------------------------------------
// DataResidencyConfig
// ---------------------------------------------------------------------------

/// Top-level data residency configuration.
///
/// Controls where data is stored, how LLM calls are routed, and which
/// privacy/compliance policies are enforced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataResidencyConfig {
    /// The deployment region.
    pub region: DataRegion,
    /// Filesystem path or S3-compatible URL for primary data storage.
    pub data_storage_location: String,
    /// Policy governing LLM endpoint selection.
    pub llm_routing_policy: LlmRoutingPolicy,
    /// Whether data at rest must be encrypted.
    pub encryption_at_rest: bool,
    /// Whether data in transit must be encrypted (TLS).
    pub encryption_in_transit: bool,
    /// Number of days to retain data before automatic purging.
    pub data_retention_days: u64,
    /// How PII is handled in LLM-bound requests.
    pub pii_handling: PiiHandlingPolicy,
    /// Whether cross-border data transfer is permitted.
    pub cross_border_transfer: bool,
    /// Filesystem path or URL for audit log storage.
    pub audit_data_location: String,
}

impl Default for DataResidencyConfig {
    fn default() -> Self {
        Self {
            region: DataRegion::US,
            data_storage_location: "/var/lib/argentor/data".to_string(),
            llm_routing_policy: LlmRoutingPolicy::AnyRegion,
            encryption_at_rest: true,
            encryption_in_transit: true,
            data_retention_days: 90,
            pii_handling: PiiHandlingPolicy::Redact,
            cross_border_transfer: false,
            audit_data_location: "/var/lib/argentor/audit".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// ResidencyIssue
// ---------------------------------------------------------------------------

/// Severity level for a residency configuration issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    /// The configuration will not satisfy compliance requirements.
    Error,
    /// The configuration is risky but may be acceptable in some contexts.
    Warning,
    /// An informational note.
    Info,
}

/// A single issue found during residency configuration validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidencyIssue {
    /// Severity of the issue.
    pub severity: IssueSeverity,
    /// Which config field is affected.
    pub field: String,
    /// Human-readable description.
    pub message: String,
}

impl fmt::Display for ResidencyIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            IssueSeverity::Error => "ERROR",
            IssueSeverity::Warning => "WARN",
            IssueSeverity::Info => "INFO",
        };
        write!(f, "[{}] {}: {}", level, self.field, self.message)
    }
}

// ---------------------------------------------------------------------------
// ResidencyValidator
// ---------------------------------------------------------------------------

/// Validates and generates [`DataResidencyConfig`] instances.
pub struct ResidencyValidator;

impl ResidencyValidator {
    /// Validate a configuration and return any issues found.
    pub fn validate_config(config: &DataResidencyConfig) -> Vec<ResidencyIssue> {
        let mut issues = Vec::new();

        // Storage location must not be empty.
        if config.data_storage_location.trim().is_empty() {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Error,
                field: "data_storage_location".into(),
                message: "Data storage location must not be empty".into(),
            });
        }

        // Audit location must not be empty.
        if config.audit_data_location.trim().is_empty() {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Error,
                field: "audit_data_location".into(),
                message: "Audit data location must not be empty".into(),
            });
        }

        // Encryption at rest should be enabled for regulated regions.
        if !config.encryption_at_rest {
            let sev = match config.region {
                DataRegion::EU | DataRegion::LATAM => IssueSeverity::Error,
                _ => IssueSeverity::Warning,
            };
            issues.push(ResidencyIssue {
                severity: sev,
                field: "encryption_at_rest".into(),
                message: "Encryption at rest is disabled — this may violate compliance requirements".into(),
            });
        }

        // Encryption in transit should always be enabled.
        if !config.encryption_in_transit {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Error,
                field: "encryption_in_transit".into(),
                message: "Encryption in transit (TLS) must be enabled for production deployments".into(),
            });
        }

        // EU region should use SameRegion or ExplicitEndpoints routing.
        if config.region == DataRegion::EU {
            match &config.llm_routing_policy {
                LlmRoutingPolicy::AnyRegion => {
                    issues.push(ResidencyIssue {
                        severity: IssueSeverity::Error,
                        field: "llm_routing_policy".into(),
                        message: "EU region requires SameRegion or ExplicitEndpoints routing for GDPR compliance".into(),
                    });
                }
                LlmRoutingPolicy::PreferRegion => {
                    issues.push(ResidencyIssue {
                        severity: IssueSeverity::Warning,
                        field: "llm_routing_policy".into(),
                        message: "PreferRegion may route outside EU under load — consider SameRegion for strict GDPR".into(),
                    });
                }
                _ => {}
            }
        }

        // Cross-border transfer warning for EU.
        if config.region == DataRegion::EU && config.cross_border_transfer {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Warning,
                field: "cross_border_transfer".into(),
                message: "Cross-border transfer enabled for EU — ensure adequate safeguards (SCCs, adequacy decisions)".into(),
            });
        }

        // PII handling: Allow is risky unless on-prem.
        if config.pii_handling == PiiHandlingPolicy::Allow {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Warning,
                field: "pii_handling".into(),
                message: "PII handling set to Allow — only appropriate for fully on-premises deployments".into(),
            });
        }

        // Very short retention may lose audit trail.
        if config.data_retention_days < 7 {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Warning,
                field: "data_retention_days".into(),
                message: "Data retention under 7 days may be insufficient for audit requirements".into(),
            });
        }

        // Zero retention is almost certainly a mistake.
        if config.data_retention_days == 0 {
            issues.push(ResidencyIssue {
                severity: IssueSeverity::Error,
                field: "data_retention_days".into(),
                message: "Data retention of 0 days means data is purged immediately — likely a misconfiguration".into(),
            });
        }

        // ExplicitEndpoints must contain at least one URL.
        if let LlmRoutingPolicy::ExplicitEndpoints(ref eps) = config.llm_routing_policy {
            if eps.is_empty() {
                issues.push(ResidencyIssue {
                    severity: IssueSeverity::Error,
                    field: "llm_routing_policy".into(),
                    message: "ExplicitEndpoints list is empty — at least one endpoint URL is required".into(),
                });
            }
        }

        issues
    }

    /// Check whether a config is compliant with a given framework.
    ///
    /// Supported framework identifiers (case-insensitive):
    /// `"gdpr"`, `"hipaa"`, `"iso27001"`, `"iso42001"`, `"dpga"`, `"sox"`.
    pub fn is_compliant(config: &DataResidencyConfig, framework: &str) -> bool {
        match framework.to_lowercase().as_str() {
            "gdpr" => {
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && matches!(
                        config.llm_routing_policy,
                        LlmRoutingPolicy::SameRegion | LlmRoutingPolicy::ExplicitEndpoints(_)
                    )
                    && matches!(
                        config.pii_handling,
                        PiiHandlingPolicy::Redact | PiiHandlingPolicy::Encrypt | PiiHandlingPolicy::Reject
                    )
                    && (config.region == DataRegion::EU || !config.cross_border_transfer)
            }
            "hipaa" => {
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && !config.cross_border_transfer
                    && matches!(
                        config.pii_handling,
                        PiiHandlingPolicy::Encrypt | PiiHandlingPolicy::Reject
                    )
                    && config.data_retention_days >= 2555 // ~7 years
            }
            "iso27001" => {
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && config.data_retention_days >= 30
                    && config.pii_handling != PiiHandlingPolicy::Allow
            }
            "iso42001" => {
                // AI management system standard — requires encryption plus PII safeguards.
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && config.pii_handling != PiiHandlingPolicy::Allow
                    && !matches!(config.llm_routing_policy, LlmRoutingPolicy::AnyRegion)
            }
            "dpga" => {
                // Digital Public Goods Alliance — open, privacy-respecting.
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && config.pii_handling != PiiHandlingPolicy::Allow
            }
            "sox" => {
                // Sarbanes-Oxley — focus on audit trail integrity.
                config.encryption_at_rest
                    && config.encryption_in_transit
                    && config.data_retention_days >= 2555
                    && !config.audit_data_location.trim().is_empty()
            }
            _ => false,
        }
    }

    /// Generate a recommended config for a given region and set of compliance frameworks.
    pub fn suggest_config(region: DataRegion, compliance_frameworks: &[&str]) -> DataResidencyConfig {
        let needs_gdpr = compliance_frameworks
            .iter()
            .any(|f| f.eq_ignore_ascii_case("gdpr"));
        let needs_hipaa = compliance_frameworks
            .iter()
            .any(|f| f.eq_ignore_ascii_case("hipaa"));
        let needs_sox = compliance_frameworks
            .iter()
            .any(|f| f.eq_ignore_ascii_case("sox"));

        let region_label = match &region.clone() {
            DataRegion::EU => "eu".to_string(),
            DataRegion::US => "us".to_string(),
            DataRegion::LATAM => "latam".to_string(),
            DataRegion::APAC => "apac".to_string(),
            DataRegion::Custom(s) => s.clone(),
        };

        let routing = if needs_gdpr || region == DataRegion::EU {
            LlmRoutingPolicy::SameRegion
        } else {
            LlmRoutingPolicy::PreferRegion
        };

        let pii = if needs_hipaa {
            PiiHandlingPolicy::Encrypt
        } else {
            PiiHandlingPolicy::Redact
        };

        let retention = if needs_hipaa || needs_sox {
            2555 // ~7 years
        } else if needs_gdpr {
            30
        } else {
            90
        };

        let cross_border = !(needs_gdpr || needs_hipaa);

        DataResidencyConfig {
            region,
            data_storage_location: format!("/var/lib/argentor/data/{region_label}"),
            llm_routing_policy: routing,
            encryption_at_rest: true,
            encryption_in_transit: true,
            data_retention_days: retention,
            pii_handling: pii,
            cross_border_transfer: cross_border,
            audit_data_location: format!("/var/lib/argentor/audit/{region_label}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Pre-built configurations
// ---------------------------------------------------------------------------

/// Strict EU configuration for GDPR compliance.
///
/// - Region: EU
/// - PII: Redact
/// - Routing: SameRegion
/// - Retention: 30 days
/// - No cross-border transfer
pub fn eu_gdpr_config() -> DataResidencyConfig {
    DataResidencyConfig {
        region: DataRegion::EU,
        data_storage_location: "/var/lib/argentor/data/eu".to_string(),
        llm_routing_policy: LlmRoutingPolicy::SameRegion,
        encryption_at_rest: true,
        encryption_in_transit: true,
        data_retention_days: 30,
        pii_handling: PiiHandlingPolicy::Redact,
        cross_border_transfer: false,
        audit_data_location: "/var/lib/argentor/audit/eu".to_string(),
    }
}

/// Standard US configuration with sensible defaults.
///
/// - Region: US
/// - PII: Redact
/// - Routing: PreferRegion
/// - Retention: 90 days
pub fn us_standard_config() -> DataResidencyConfig {
    DataResidencyConfig {
        region: DataRegion::US,
        data_storage_location: "/var/lib/argentor/data/us".to_string(),
        llm_routing_policy: LlmRoutingPolicy::PreferRegion,
        encryption_at_rest: true,
        encryption_in_transit: true,
        data_retention_days: 90,
        pii_handling: PiiHandlingPolicy::Redact,
        cross_border_transfer: false,
        audit_data_location: "/var/lib/argentor/audit/us".to_string(),
    }
}

/// LATAM configuration with Spanish defaults.
///
/// - Region: LATAM
/// - PII: Redact
/// - Routing: PreferRegion
/// - Retention: 90 days
pub fn latam_config() -> DataResidencyConfig {
    DataResidencyConfig {
        region: DataRegion::LATAM,
        data_storage_location: "/var/lib/argentor/data/latam".to_string(),
        llm_routing_policy: LlmRoutingPolicy::PreferRegion,
        encryption_at_rest: true,
        encryption_in_transit: true,
        data_retention_days: 90,
        pii_handling: PiiHandlingPolicy::Redact,
        cross_border_transfer: false,
        audit_data_location: "/var/lib/argentor/audit/latam".to_string(),
    }
}

/// US healthcare configuration for HIPAA compliance.
///
/// - Region: US
/// - PII: Encrypt
/// - Routing: SameRegion
/// - Retention: 2555 days (~7 years)
/// - No cross-border transfer
pub fn hipaa_config() -> DataResidencyConfig {
    DataResidencyConfig {
        region: DataRegion::US,
        data_storage_location: "/var/lib/argentor/data/us-hipaa".to_string(),
        llm_routing_policy: LlmRoutingPolicy::SameRegion,
        encryption_at_rest: true,
        encryption_in_transit: true,
        data_retention_days: 2555,
        pii_handling: PiiHandlingPolicy::Encrypt,
        cross_border_transfer: false,
        audit_data_location: "/var/lib/argentor/audit/us-hipaa".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Pre-built config tests --

    #[test]
    fn test_eu_gdpr_config_defaults() {
        let cfg = eu_gdpr_config();
        assert_eq!(cfg.region, DataRegion::EU);
        assert!(cfg.encryption_at_rest);
        assert!(cfg.encryption_in_transit);
        assert_eq!(cfg.data_retention_days, 30);
        assert_eq!(cfg.pii_handling, PiiHandlingPolicy::Redact);
        assert!(!cfg.cross_border_transfer);
        assert_eq!(cfg.llm_routing_policy, LlmRoutingPolicy::SameRegion);
    }

    #[test]
    fn test_us_standard_config_defaults() {
        let cfg = us_standard_config();
        assert_eq!(cfg.region, DataRegion::US);
        assert_eq!(cfg.data_retention_days, 90);
        assert_eq!(cfg.llm_routing_policy, LlmRoutingPolicy::PreferRegion);
    }

    #[test]
    fn test_latam_config_defaults() {
        let cfg = latam_config();
        assert_eq!(cfg.region, DataRegion::LATAM);
        assert!(cfg.encryption_at_rest);
        assert!(!cfg.cross_border_transfer);
    }

    #[test]
    fn test_hipaa_config_defaults() {
        let cfg = hipaa_config();
        assert_eq!(cfg.region, DataRegion::US);
        assert_eq!(cfg.pii_handling, PiiHandlingPolicy::Encrypt);
        assert_eq!(cfg.data_retention_days, 2555);
        assert!(!cfg.cross_border_transfer);
        assert_eq!(cfg.llm_routing_policy, LlmRoutingPolicy::SameRegion);
    }

    // -- Validation tests --

    #[test]
    fn test_valid_eu_config_has_no_issues() {
        let cfg = eu_gdpr_config();
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_valid_hipaa_config_has_no_issues() {
        let cfg = hipaa_config();
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.is_empty(), "Expected no issues, got: {:?}", issues);
    }

    #[test]
    fn test_empty_storage_location_is_error() {
        let mut cfg = us_standard_config();
        cfg.data_storage_location = "".to_string();
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "data_storage_location"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_empty_audit_location_is_error() {
        let mut cfg = us_standard_config();
        cfg.audit_data_location = "   ".to_string();
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "audit_data_location"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_eu_anyregion_routing_is_error() {
        let mut cfg = eu_gdpr_config();
        cfg.llm_routing_policy = LlmRoutingPolicy::AnyRegion;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "llm_routing_policy"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_eu_prefer_region_is_warning() {
        let mut cfg = eu_gdpr_config();
        cfg.llm_routing_policy = LlmRoutingPolicy::PreferRegion;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "llm_routing_policy"
            && i.severity == IssueSeverity::Warning));
    }

    #[test]
    fn test_eu_cross_border_warning() {
        let mut cfg = eu_gdpr_config();
        cfg.cross_border_transfer = true;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "cross_border_transfer"
            && i.severity == IssueSeverity::Warning));
    }

    #[test]
    fn test_pii_allow_is_warning() {
        let mut cfg = us_standard_config();
        cfg.pii_handling = PiiHandlingPolicy::Allow;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "pii_handling"
            && i.severity == IssueSeverity::Warning));
    }

    #[test]
    fn test_zero_retention_is_error() {
        let mut cfg = us_standard_config();
        cfg.data_retention_days = 0;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "data_retention_days"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_short_retention_is_warning() {
        let mut cfg = us_standard_config();
        cfg.data_retention_days = 3;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "data_retention_days"
            && i.severity == IssueSeverity::Warning));
    }

    #[test]
    fn test_empty_explicit_endpoints_is_error() {
        let mut cfg = us_standard_config();
        cfg.llm_routing_policy = LlmRoutingPolicy::ExplicitEndpoints(vec![]);
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "llm_routing_policy"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_encryption_disabled_eu_is_error() {
        let mut cfg = eu_gdpr_config();
        cfg.encryption_at_rest = false;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "encryption_at_rest"
            && i.severity == IssueSeverity::Error));
    }

    #[test]
    fn test_encryption_disabled_us_is_warning() {
        let mut cfg = us_standard_config();
        cfg.encryption_at_rest = false;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "encryption_at_rest"
            && i.severity == IssueSeverity::Warning));
    }

    #[test]
    fn test_transit_encryption_disabled_is_error() {
        let mut cfg = us_standard_config();
        cfg.encryption_in_transit = false;
        let issues = ResidencyValidator::validate_config(&cfg);
        assert!(issues.iter().any(|i| i.field == "encryption_in_transit"
            && i.severity == IssueSeverity::Error));
    }

    // -- Compliance tests --

    #[test]
    fn test_eu_gdpr_is_gdpr_compliant() {
        let cfg = eu_gdpr_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "gdpr"));
    }

    #[test]
    fn test_hipaa_config_is_hipaa_compliant() {
        let cfg = hipaa_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "hipaa"));
    }

    #[test]
    fn test_us_standard_is_not_hipaa_compliant() {
        let cfg = us_standard_config();
        assert!(!ResidencyValidator::is_compliant(&cfg, "hipaa"));
    }

    #[test]
    fn test_eu_gdpr_is_iso27001_compliant() {
        let cfg = eu_gdpr_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "iso27001"));
    }

    #[test]
    fn test_eu_gdpr_is_dpga_compliant() {
        let cfg = eu_gdpr_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "dpga"));
    }

    #[test]
    fn test_hipaa_config_is_sox_compliant() {
        let cfg = hipaa_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "sox"));
    }

    #[test]
    fn test_us_standard_is_not_sox_compliant() {
        let cfg = us_standard_config();
        assert!(!ResidencyValidator::is_compliant(&cfg, "sox"));
    }

    #[test]
    fn test_unknown_framework_is_not_compliant() {
        let cfg = eu_gdpr_config();
        assert!(!ResidencyValidator::is_compliant(&cfg, "unknown_framework"));
    }

    #[test]
    fn test_compliance_case_insensitive() {
        let cfg = eu_gdpr_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "GDPR"));
        assert!(ResidencyValidator::is_compliant(&cfg, "Gdpr"));
    }

    #[test]
    fn test_eu_gdpr_is_iso42001_compliant() {
        let cfg = eu_gdpr_config();
        assert!(ResidencyValidator::is_compliant(&cfg, "iso42001"));
    }

    // -- suggest_config tests --

    #[test]
    fn test_suggest_gdpr_config() {
        let cfg = ResidencyValidator::suggest_config(DataRegion::EU, &["gdpr"]);
        assert_eq!(cfg.region, DataRegion::EU);
        assert_eq!(cfg.llm_routing_policy, LlmRoutingPolicy::SameRegion);
        assert_eq!(cfg.pii_handling, PiiHandlingPolicy::Redact);
        assert_eq!(cfg.data_retention_days, 30);
        assert!(!cfg.cross_border_transfer);
        assert!(ResidencyValidator::is_compliant(&cfg, "gdpr"));
    }

    #[test]
    fn test_suggest_hipaa_config() {
        let cfg = ResidencyValidator::suggest_config(DataRegion::US, &["hipaa"]);
        assert_eq!(cfg.pii_handling, PiiHandlingPolicy::Encrypt);
        assert_eq!(cfg.data_retention_days, 2555);
        assert!(!cfg.cross_border_transfer);
        assert!(ResidencyValidator::is_compliant(&cfg, "hipaa"));
    }

    #[test]
    fn test_suggest_multi_framework() {
        let cfg = ResidencyValidator::suggest_config(DataRegion::US, &["hipaa", "sox"]);
        assert_eq!(cfg.data_retention_days, 2555);
        assert!(ResidencyValidator::is_compliant(&cfg, "hipaa"));
        assert!(ResidencyValidator::is_compliant(&cfg, "sox"));
    }

    #[test]
    fn test_suggest_no_frameworks() {
        let cfg = ResidencyValidator::suggest_config(DataRegion::APAC, &[]);
        assert_eq!(cfg.region, DataRegion::APAC);
        assert_eq!(cfg.data_retention_days, 90);
        assert_eq!(cfg.llm_routing_policy, LlmRoutingPolicy::PreferRegion);
    }

    #[test]
    fn test_suggest_custom_region() {
        let cfg = ResidencyValidator::suggest_config(
            DataRegion::Custom("me-south-1".to_string()),
            &[],
        );
        assert_eq!(cfg.region, DataRegion::Custom("me-south-1".to_string()));
        assert!(cfg.data_storage_location.contains("me-south-1"));
        assert!(cfg.audit_data_location.contains("me-south-1"));
    }

    // -- Serialization tests --

    #[test]
    fn test_config_roundtrip_json() {
        let cfg = eu_gdpr_config();
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: DataResidencyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.region, cfg.region);
        assert_eq!(deserialized.data_retention_days, cfg.data_retention_days);
        assert_eq!(deserialized.pii_handling, cfg.pii_handling);
    }

    #[test]
    fn test_data_region_display() {
        assert_eq!(DataRegion::EU.to_string(), "EU");
        assert_eq!(DataRegion::US.to_string(), "US");
        assert_eq!(DataRegion::LATAM.to_string(), "LATAM");
        assert_eq!(DataRegion::APAC.to_string(), "APAC");
        assert_eq!(
            DataRegion::Custom("me-south-1".to_string()).to_string(),
            "Custom(me-south-1)"
        );
    }

    #[test]
    fn test_residency_issue_display() {
        let issue = ResidencyIssue {
            severity: IssueSeverity::Error,
            field: "encryption_at_rest".into(),
            message: "Must be enabled".into(),
        };
        assert_eq!(
            issue.to_string(),
            "[ERROR] encryption_at_rest: Must be enabled"
        );
    }

    #[test]
    fn test_default_config() {
        let cfg = DataResidencyConfig::default();
        assert_eq!(cfg.region, DataRegion::US);
        assert!(cfg.encryption_at_rest);
        assert!(cfg.encryption_in_transit);
        assert_eq!(cfg.data_retention_days, 90);
        assert_eq!(cfg.pii_handling, PiiHandlingPolicy::Redact);
        assert!(!cfg.cross_border_transfer);
    }
}
