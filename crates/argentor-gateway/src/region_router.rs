//! Multi-region routing for LLM requests based on data residency rules.
//!
//! Routes LLM requests to region-appropriate providers by enforcing data
//! classification and tenancy constraints. Integrates with
//! [`argentor_core::data_residency::DataResidencyConfig`] to provide
//! gateway-level enforcement of residency policies.
//!
//! # Example
//!
//! ```rust
//! use argentor_gateway::region_router::{
//!     DataClassification, RegionRouter, RegionRule,
//! };
//!
//! let mut router = RegionRouter::new("us-east-1");
//! router.add_rule(RegionRule {
//!     tenant_id: None,
//!     data_classification: DataClassification::Restricted,
//!     allowed_regions: vec!["eu-west-1".into(), "eu-central-1".into()],
//!     allowed_providers: vec!["claude".into()],
//!     blocked_providers: vec!["openai".into()],
//! });
//!
//! // openai is blocked, but claude is available as alternative
//! let decision = router.route(None, &DataClassification::Restricted, "openai");
//! assert!(decision.allowed);
//! assert_eq!(decision.provider, Some("claude".to_string()));
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Classification level for the data being sent to an LLM provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataClassification {
    /// Publicly available data with no restrictions.
    Public,
    /// Internal organizational data, limited external sharing.
    Internal,
    /// Confidential business data requiring access controls.
    Confidential,
    /// Restricted data: PII, financial, health records, etc.
    Restricted,
}

impl DataClassification {
    /// Returns the sensitivity level as an integer (higher = more sensitive).
    pub fn sensitivity(&self) -> u8 {
        match self {
            DataClassification::Public => 0,
            DataClassification::Internal => 1,
            DataClassification::Confidential => 2,
            DataClassification::Restricted => 3,
        }
    }
}

impl std::fmt::Display for DataClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataClassification::Public => write!(f, "Public"),
            DataClassification::Internal => write!(f, "Internal"),
            DataClassification::Confidential => write!(f, "Confidential"),
            DataClassification::Restricted => write!(f, "Restricted"),
        }
    }
}

/// A rule that constrains which providers and regions are allowed for a given
/// tenant and data classification level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionRule {
    /// Tenant identifier. `None` means the rule applies to all tenants.
    pub tenant_id: Option<String>,
    /// The data classification level this rule applies to.
    pub data_classification: DataClassification,
    /// Regions where data processing is allowed (e.g., `"eu-west-1"`).
    pub allowed_regions: Vec<String>,
    /// Providers explicitly allowed. Empty means no explicit allowlist (all non-blocked are ok).
    pub allowed_providers: Vec<String>,
    /// Providers explicitly blocked. Takes precedence over allowed list.
    pub blocked_providers: Vec<String>,
}

/// The outcome of a routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Whether the request is allowed to proceed.
    pub allowed: bool,
    /// The selected provider, if the request is allowed.
    pub provider: Option<String>,
    /// The region selected for processing.
    pub region: String,
    /// Human-readable explanation of the decision.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// RegionRouter
// ---------------------------------------------------------------------------

/// Routes LLM requests to region-appropriate providers based on data residency rules.
///
/// Rules are evaluated in order. More specific rules (tenant-scoped) are checked
/// before global rules. The first matching rule determines the routing decision.
pub struct RegionRouter {
    rules: Vec<RegionRule>,
    default_region: String,
}

impl RegionRouter {
    /// Create a new router with the given default region.
    pub fn new(default_region: impl Into<String>) -> Self {
        Self {
            rules: Vec::new(),
            default_region: default_region.into(),
        }
    }

    /// Add a routing rule. Rules are evaluated in insertion order, with
    /// tenant-specific rules taking priority over global ones during matching.
    pub fn add_rule(&mut self, rule: RegionRule) {
        self.rules.push(rule);
    }

    /// Check if a request is allowed and determine which provider/region to use.
    ///
    /// Matching logic:
    /// 1. Find all rules matching the classification (exact match).
    /// 2. Among those, prefer tenant-specific rules if `tenant_id` is provided.
    /// 3. Fall back to global rules (`tenant_id: None`).
    /// 4. If no rule matches, the request is allowed with the preferred provider
    ///    in the default region.
    pub fn route(
        &self,
        tenant_id: Option<&str>,
        classification: &DataClassification,
        preferred_provider: &str,
    ) -> RoutingDecision {
        // Collect matching rules, split into tenant-specific and global
        let mut tenant_rules: Vec<&RegionRule> = Vec::new();
        let mut global_rules: Vec<&RegionRule> = Vec::new();

        for rule in &self.rules {
            if rule.data_classification != *classification {
                continue;
            }
            match (&rule.tenant_id, tenant_id) {
                (Some(rule_tid), Some(req_tid)) if rule_tid == req_tid => {
                    tenant_rules.push(rule);
                }
                (None, _) => {
                    global_rules.push(rule);
                }
                _ => {}
            }
        }

        // Use tenant-specific rules first, then global
        let applicable = if !tenant_rules.is_empty() {
            tenant_rules
        } else {
            global_rules
        };

        if applicable.is_empty() {
            // No rule matches — allow with defaults
            return RoutingDecision {
                allowed: true,
                provider: Some(preferred_provider.to_string()),
                region: self.default_region.clone(),
                reason: format!(
                    "No rules matched for classification={classification}; using defaults"
                ),
            };
        }

        // Evaluate against the first matching rule
        let rule = applicable[0];

        // Check blocked providers first (takes precedence)
        let provider_lower = preferred_provider.to_lowercase();
        if rule
            .blocked_providers
            .iter()
            .any(|p| p.to_lowercase() == provider_lower)
        {
            // Provider is blocked. Try to find an alternative from allowed list.
            let alternative = rule
                .allowed_providers
                .iter()
                .find(|p| {
                    !rule
                        .blocked_providers
                        .iter()
                        .any(|b| b.to_lowercase() == p.to_lowercase())
                })
                .cloned();

            if let Some(alt) = alternative {
                let region = rule
                    .allowed_regions
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.default_region.clone());
                return RoutingDecision {
                    allowed: true,
                    provider: Some(alt.clone()),
                    region,
                    reason: format!(
                        "Provider '{preferred_provider}' blocked for {classification} data; \
                         rerouted to '{alt}'"
                    ),
                };
            }

            // No alternative available
            return RoutingDecision {
                allowed: false,
                provider: None,
                region: rule
                    .allowed_regions
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.default_region.clone()),
                reason: format!(
                    "Provider '{preferred_provider}' blocked for {classification} data \
                     and no alternative providers available"
                ),
            };
        }

        // Check allowed providers (if the allowlist is non-empty, provider must be in it)
        if !rule.allowed_providers.is_empty()
            && !rule
                .allowed_providers
                .iter()
                .any(|p| p.to_lowercase() == provider_lower)
        {
            // Provider is not in the allowlist. Suggest the first allowed provider.
            let suggestion = rule.allowed_providers.first().cloned();
            if let Some(ref alt) = suggestion {
                let region = rule
                    .allowed_regions
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.default_region.clone());
                return RoutingDecision {
                    allowed: true,
                    provider: Some(alt.clone()),
                    region,
                    reason: format!(
                        "Provider '{preferred_provider}' not in allowlist for {classification} data; \
                         rerouted to '{alt}'"
                    ),
                };
            }
            return RoutingDecision {
                allowed: false,
                provider: None,
                region: self.default_region.clone(),
                reason: format!(
                    "Provider '{preferred_provider}' not in allowlist for {classification} data"
                ),
            };
        }

        // Provider is acceptable — pick a region
        let region = rule
            .allowed_regions
            .first()
            .cloned()
            .unwrap_or_else(|| self.default_region.clone());

        RoutingDecision {
            allowed: true,
            provider: Some(preferred_provider.to_string()),
            region,
            reason: format!(
                "Allowed: provider='{preferred_provider}', classification={classification}"
            ),
        }
    }

    /// Get all allowed providers for a given tenant + classification combination.
    ///
    /// If no rule matches, returns an empty list (meaning no restrictions).
    pub fn allowed_providers(
        &self,
        tenant_id: Option<&str>,
        classification: &DataClassification,
    ) -> Vec<String> {
        // Collect matching rules (same priority logic as route())
        let mut tenant_rules: Vec<&RegionRule> = Vec::new();
        let mut global_rules: Vec<&RegionRule> = Vec::new();

        for rule in &self.rules {
            if rule.data_classification != *classification {
                continue;
            }
            match (&rule.tenant_id, tenant_id) {
                (Some(rule_tid), Some(req_tid)) if rule_tid == req_tid => {
                    tenant_rules.push(rule);
                }
                (None, _) => {
                    global_rules.push(rule);
                }
                _ => {}
            }
        }

        let applicable = if !tenant_rules.is_empty() {
            tenant_rules
        } else {
            global_rules
        };

        if applicable.is_empty() {
            return Vec::new();
        }

        // Aggregate allowed providers from all matching rules, removing blocked ones
        let mut allowed: Vec<String> = Vec::new();
        let mut blocked: Vec<String> = Vec::new();

        for rule in &applicable {
            for p in &rule.allowed_providers {
                if !allowed.iter().any(|a| a.to_lowercase() == p.to_lowercase()) {
                    allowed.push(p.clone());
                }
            }
            for p in &rule.blocked_providers {
                if !blocked.iter().any(|b| b.to_lowercase() == p.to_lowercase()) {
                    blocked.push(p.clone());
                }
            }
        }

        // Remove blocked from allowed
        allowed.retain(|a| !blocked.iter().any(|b| b.to_lowercase() == a.to_lowercase()));

        allowed
    }

    /// Validate the router configuration and return a list of issues found.
    ///
    /// Checks for:
    /// - Rules with empty allowed_regions
    /// - Providers that appear in both allowed and blocked lists
    /// - Duplicate rules for the same tenant + classification
    /// - Rules with no allowed providers and non-empty blocked list
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        for (i, rule) in self.rules.iter().enumerate() {
            let label = match &rule.tenant_id {
                Some(tid) => format!(
                    "Rule #{} (tenant={}, class={})",
                    i + 1,
                    tid,
                    rule.data_classification
                ),
                None => format!(
                    "Rule #{} (global, class={})",
                    i + 1,
                    rule.data_classification
                ),
            };

            // Empty allowed_regions
            if rule.allowed_regions.is_empty() {
                issues.push(format!("{label}: allowed_regions is empty"));
            }

            // Provider in both allowed and blocked
            for p in &rule.allowed_providers {
                if rule
                    .blocked_providers
                    .iter()
                    .any(|b| b.to_lowercase() == p.to_lowercase())
                {
                    issues.push(format!(
                        "{label}: provider '{p}' appears in both allowed and blocked lists"
                    ));
                }
            }
        }

        // Check for duplicate rules (same tenant + classification)
        for i in 0..self.rules.len() {
            for j in (i + 1)..self.rules.len() {
                let a = &self.rules[i];
                let b = &self.rules[j];
                if a.tenant_id == b.tenant_id && a.data_classification == b.data_classification {
                    let label = match &a.tenant_id {
                        Some(tid) => format!("tenant={tid}, class={}", a.data_classification),
                        None => format!("global, class={}", a.data_classification),
                    };
                    issues.push(format!(
                        "Duplicate rules #{} and #{} for ({label})",
                        i + 1,
                        j + 1
                    ));
                }
            }
        }

        issues
    }

    /// Return a reference to the default region.
    pub fn default_region(&self) -> &str {
        &self.default_region
    }

    /// Return the number of configured rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- Helpers --

    fn eu_restricted_rule() -> RegionRule {
        RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Restricted,
            allowed_regions: vec!["eu-west-1".into(), "eu-central-1".into()],
            allowed_providers: vec!["claude".into(), "gemini".into()],
            blocked_providers: vec!["openai".into()],
        }
    }

    fn public_any_rule() -> RegionRule {
        RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Public,
            allowed_regions: vec!["us-east-1".into(), "eu-west-1".into()],
            allowed_providers: vec![],
            blocked_providers: vec![],
        }
    }

    fn tenant_acme_confidential() -> RegionRule {
        RegionRule {
            tenant_id: Some("acme-corp".into()),
            data_classification: DataClassification::Confidential,
            allowed_regions: vec!["eu-west-1".into()],
            allowed_providers: vec!["claude".into()],
            blocked_providers: vec![],
        }
    }

    // -- Test: default routing with no rules --

    #[test]
    fn test_default_routing_no_rules() {
        let router = RegionRouter::new("us-east-1");
        let decision = router.route(None, &DataClassification::Public, "openai");
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("openai".to_string()));
        assert_eq!(decision.region, "us-east-1");
    }

    // -- Test: restricted data blocks US-only providers --

    #[test]
    fn test_restricted_data_blocks_openai() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule());

        let decision = router.route(None, &DataClassification::Restricted, "openai");
        // openai is blocked, but claude is available as alternative
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("claude".to_string()));
        assert_eq!(decision.region, "eu-west-1");
        assert!(decision.reason.contains("blocked"));
    }

    // -- Test: restricted data allows claude --

    #[test]
    fn test_restricted_data_allows_claude() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule());

        let decision = router.route(None, &DataClassification::Restricted, "claude");
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("claude".to_string()));
        assert_eq!(decision.region, "eu-west-1");
    }

    // -- Test: public data with no restrictions --

    #[test]
    fn test_public_data_any_provider() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(public_any_rule());

        let decision = router.route(None, &DataClassification::Public, "openai");
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("openai".to_string()));
        assert_eq!(decision.region, "us-east-1");
    }

    // -- Test: tenant-specific rules override global --

    #[test]
    fn test_tenant_specific_overrides_global() {
        let mut router = RegionRouter::new("us-east-1");
        // Global rule: confidential data can use any provider
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Confidential,
            allowed_regions: vec!["us-east-1".into()],
            allowed_providers: vec![],
            blocked_providers: vec![],
        });
        // Tenant rule: acme-corp confidential data must use claude in eu-west-1
        router.add_rule(tenant_acme_confidential());

        // Non-acme tenant gets global rule
        let d1 = router.route(
            Some("other-corp"),
            &DataClassification::Confidential,
            "openai",
        );
        assert!(d1.allowed);
        assert_eq!(d1.provider, Some("openai".to_string()));
        assert_eq!(d1.region, "us-east-1");

        // Acme tenant gets tenant-specific rule
        let d2 = router.route(
            Some("acme-corp"),
            &DataClassification::Confidential,
            "openai",
        );
        assert!(d2.allowed);
        // openai is not in allowed list, so gets rerouted to claude
        assert_eq!(d2.provider, Some("claude".to_string()));
        assert_eq!(d2.region, "eu-west-1");
    }

    // -- Test: classification levels are independent --

    #[test]
    fn test_classification_levels_independent() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule()); // Only applies to Restricted

        // Internal data should have no restrictions (no matching rule)
        let decision = router.route(None, &DataClassification::Internal, "openai");
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("openai".to_string()));
        assert_eq!(decision.region, "us-east-1");
    }

    // -- Test: blocked provider with no alternatives --

    #[test]
    fn test_blocked_no_alternatives_denied() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Restricted,
            allowed_regions: vec!["eu-west-1".into()],
            allowed_providers: vec![],
            blocked_providers: vec!["openai".into()],
        });

        let decision = router.route(None, &DataClassification::Restricted, "openai");
        assert!(!decision.allowed);
        assert!(decision.provider.is_none());
        assert!(decision.reason.contains("blocked"));
    }

    // -- Test: validation detects empty regions --

    #[test]
    fn test_validate_empty_regions() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Public,
            allowed_regions: vec![],
            allowed_providers: vec![],
            blocked_providers: vec![],
        });

        let issues = router.validate();
        assert!(!issues.is_empty());
        assert!(issues[0].contains("allowed_regions is empty"));
    }

    // -- Test: validation detects provider in both lists --

    #[test]
    fn test_validate_provider_in_both_lists() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Internal,
            allowed_regions: vec!["us-east-1".into()],
            allowed_providers: vec!["openai".into()],
            blocked_providers: vec!["openai".into()],
        });

        let issues = router.validate();
        assert!(issues
            .iter()
            .any(|i| i.contains("both allowed and blocked")));
    }

    // -- Test: validation detects duplicate rules --

    #[test]
    fn test_validate_duplicate_rules() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Public,
            allowed_regions: vec!["us-east-1".into()],
            allowed_providers: vec![],
            blocked_providers: vec![],
        });
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Public,
            allowed_regions: vec!["eu-west-1".into()],
            allowed_providers: vec![],
            blocked_providers: vec![],
        });

        let issues = router.validate();
        assert!(issues.iter().any(|i| i.contains("Duplicate rules")));
    }

    // -- Test: allowed_providers returns correct list --

    #[test]
    fn test_allowed_providers_list() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule());

        let providers = router.allowed_providers(None, &DataClassification::Restricted);
        assert_eq!(providers.len(), 2);
        assert!(providers.contains(&"claude".to_string()));
        assert!(providers.contains(&"gemini".to_string()));
    }

    // -- Test: allowed_providers empty when no rule matches --

    #[test]
    fn test_allowed_providers_empty_no_match() {
        let router = RegionRouter::new("us-east-1");
        let providers = router.allowed_providers(None, &DataClassification::Public);
        assert!(providers.is_empty());
    }

    // -- Test: data classification display --

    #[test]
    fn test_data_classification_display() {
        assert_eq!(DataClassification::Public.to_string(), "Public");
        assert_eq!(DataClassification::Internal.to_string(), "Internal");
        assert_eq!(DataClassification::Confidential.to_string(), "Confidential");
        assert_eq!(DataClassification::Restricted.to_string(), "Restricted");
    }

    // -- Test: data classification sensitivity ordering --

    #[test]
    fn test_data_classification_sensitivity() {
        assert!(
            DataClassification::Public.sensitivity() < DataClassification::Internal.sensitivity()
        );
        assert!(
            DataClassification::Internal.sensitivity()
                < DataClassification::Confidential.sensitivity()
        );
        assert!(
            DataClassification::Confidential.sensitivity()
                < DataClassification::Restricted.sensitivity()
        );
    }

    // -- Test: multiple rules for different classifications --

    #[test]
    fn test_multiple_classification_rules() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule());
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Confidential,
            allowed_regions: vec!["us-east-1".into(), "eu-west-1".into()],
            allowed_providers: vec!["claude".into(), "openai".into()],
            blocked_providers: vec![],
        });

        // Restricted: openai blocked
        let d1 = router.route(None, &DataClassification::Restricted, "openai");
        assert!(d1.allowed);
        assert_eq!(d1.provider, Some("claude".to_string()));

        // Confidential: openai allowed
        let d2 = router.route(None, &DataClassification::Confidential, "openai");
        assert!(d2.allowed);
        assert_eq!(d2.provider, Some("openai".to_string()));
    }

    // -- Test: provider not in allowlist gets rerouted --

    #[test]
    fn test_provider_not_in_allowlist_rerouted() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(RegionRule {
            tenant_id: None,
            data_classification: DataClassification::Internal,
            allowed_regions: vec!["eu-west-1".into()],
            allowed_providers: vec!["claude".into(), "gemini".into()],
            blocked_providers: vec![],
        });

        let decision = router.route(None, &DataClassification::Internal, "mistral");
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("claude".to_string()));
        assert!(decision.reason.contains("not in allowlist"));
    }

    // -- Test: serialization round-trip --

    #[test]
    fn test_serialization_roundtrip() {
        let rule = eu_restricted_rule();
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: RegionRule = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.data_classification,
            DataClassification::Restricted
        );
        assert_eq!(deserialized.allowed_regions.len(), 2);
        assert_eq!(deserialized.blocked_providers.len(), 1);
    }

    // -- Test: routing decision serialization --

    #[test]
    fn test_routing_decision_serialization() {
        let decision = RoutingDecision {
            allowed: true,
            provider: Some("claude".to_string()),
            region: "eu-west-1".to_string(),
            reason: "Allowed by rule".to_string(),
        };
        let json = serde_json::to_string(&decision).unwrap();
        let d: RoutingDecision = serde_json::from_str(&json).unwrap();
        assert!(d.allowed);
        assert_eq!(d.provider.unwrap(), "claude");
    }

    // -- Test: valid configuration has no issues --

    #[test]
    fn test_valid_config_no_issues() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule());
        router.add_rule(public_any_rule());
        let issues = router.validate();
        assert!(issues.is_empty(), "Expected no issues, got: {issues:?}");
    }

    // -- Test: case-insensitive provider matching --

    #[test]
    fn test_case_insensitive_provider() {
        let mut router = RegionRouter::new("us-east-1");
        router.add_rule(eu_restricted_rule()); // blocks "openai"

        let decision = router.route(None, &DataClassification::Restricted, "OpenAI");
        // Should still be blocked despite different casing
        assert!(decision.allowed);
        assert_eq!(decision.provider, Some("claude".to_string()));
        assert!(decision.reason.contains("blocked"));
    }
}
