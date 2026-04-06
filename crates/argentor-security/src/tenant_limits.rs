//! Per-tenant rate limiting and quota enforcement for multi-tenant SaaS.
//!
//! Provides [`TenantLimitManager`] which tracks per-tenant usage against
//! configurable plans (Free, Pro, Enterprise, Custom) using sliding-window
//! rate limiting, daily/monthly quotas, budget caps, and model/agent restrictions.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// Plan & Limits
// ---------------------------------------------------------------------------

/// Predefined plan tiers plus a fully custom option.
#[derive(Debug, Clone, PartialEq)]
pub enum TenantPlan {
    /// Free tier with minimal quotas.
    Free,
    /// Professional tier with moderate quotas.
    Pro,
    /// Enterprise tier with high quotas.
    Enterprise,
    /// Fully custom limits defined at registration time.
    Custom {
        /// The custom limits configuration.
        limits: TenantLimits,
    },
}

impl TenantPlan {
    /// Human-readable label.
    pub fn name(&self) -> &str {
        match self {
            Self::Free => "Free",
            Self::Pro => "Pro",
            Self::Enterprise => "Enterprise",
            Self::Custom { .. } => "Custom",
        }
    }

    /// Resolve the concrete limits for this plan.
    pub fn limits(&self) -> TenantLimits {
        match self {
            Self::Free => TenantLimits::free(),
            Self::Pro => TenantLimits::pro(),
            Self::Enterprise => TenantLimits::enterprise(),
            Self::Custom { limits } => limits.clone(),
        }
    }
}

/// Configurable resource limits for a single tenant.
#[derive(Debug, Clone, PartialEq)]
pub struct TenantLimits {
    /// Maximum API requests allowed per UTC day.
    pub max_requests_per_day: u64,
    /// Maximum tokens that may be consumed in a calendar month.
    pub max_tokens_per_month: u64,
    /// Sliding-window rate limit (requests per second).
    pub max_requests_per_second: f64,
    /// Maximum in-flight requests at any given time.
    pub max_concurrent_requests: u32,
    /// Monthly spending cap in USD (0.0 = unlimited).
    pub max_cost_per_month_usd: f64,
    /// Allowlisted model identifiers. Empty means all models are allowed.
    pub allowed_models: Vec<String>,
    /// Allowlisted agent identifiers. Empty means all agents are allowed.
    pub allowed_agents: Vec<String>,
}

impl TenantLimits {
    /// Returns the default limits for the Free tier.
    pub fn free() -> Self {
        Self {
            max_requests_per_day: 100,
            max_tokens_per_month: 50_000,
            max_requests_per_second: 1.0,
            max_concurrent_requests: 2,
            max_cost_per_month_usd: 0.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        }
    }

    /// Returns the default limits for the Pro tier.
    pub fn pro() -> Self {
        Self {
            max_requests_per_day: 5_000,
            max_tokens_per_month: 2_000_000,
            max_requests_per_second: 10.0,
            max_concurrent_requests: 10,
            max_cost_per_month_usd: 50.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        }
    }

    /// Returns the default limits for the Enterprise tier.
    pub fn enterprise() -> Self {
        Self {
            max_requests_per_day: 100_000,
            max_tokens_per_month: 50_000_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 50,
            max_cost_per_month_usd: 500.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        }
    }
}

/// Return the three built-in plans keyed by lowercase name.
pub fn default_plans() -> HashMap<String, TenantLimits> {
    let mut m = HashMap::new();
    m.insert("free".to_string(), TenantLimits::free());
    m.insert("pro".to_string(), TenantLimits::pro());
    m.insert("enterprise".to_string(), TenantLimits::enterprise());
    m
}

// ---------------------------------------------------------------------------
// Check result & status types
// ---------------------------------------------------------------------------

/// Result of a `check_request` call.
#[derive(Debug, Clone)]
pub struct TenantCheckResult {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Reason the request was denied (e.g., `"daily_limit_exceeded"`).
    pub reason: Option<String>,
    /// Remaining daily request quota.
    pub remaining_daily_requests: u64,
    /// Remaining monthly token quota.
    pub remaining_monthly_tokens: u64,
    /// Remaining monthly budget in USD.
    pub remaining_monthly_budget_usd: f64,
    /// UTC time when the rate limit window resets, if applicable.
    pub rate_limit_reset_at: Option<DateTime<Utc>>,
}

/// Full usage snapshot for a tenant.
#[derive(Debug, Clone)]
pub struct TenantUsageStatus {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Plan name (e.g., "Free", "Pro", "Enterprise").
    pub plan: String,
    /// Requests made today.
    pub daily_requests: u64,
    /// Daily request cap.
    pub daily_limit: u64,
    /// Tokens consumed this month.
    pub monthly_tokens: u64,
    /// Monthly token cap.
    pub monthly_limit: u64,
    /// Cost accumulated this month (USD).
    pub monthly_cost_usd: f64,
    /// Monthly spending cap (USD).
    pub monthly_budget_usd: f64,
    /// In-flight request count.
    pub concurrent_requests: u32,
    /// Maximum concurrent requests allowed.
    pub concurrent_limit: u32,
    /// Highest utilization ratio across all dimensions (0.0 -- 100.0).
    pub utilization_percent: f64,
    /// Whether the tenant is currently throttled.
    pub is_throttled: bool,
    /// UTC timestamp of the most recent request.
    pub last_request_at: Option<DateTime<Utc>>,
}

/// Lightweight summary used by `list_tenants`.
#[derive(Debug, Clone)]
pub struct TenantSummary {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Plan name.
    pub plan: String,
    /// Highest utilization ratio across all dimensions (0.0 -- 100.0).
    pub utilization_percent: f64,
    /// Whether the tenant is currently throttled.
    pub is_throttled: bool,
}

// ---------------------------------------------------------------------------
// Internal per-tenant state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TenantState {
    plan: TenantPlan,
    limits: TenantLimits,

    // Daily counter — resets when the calendar day (UTC) changes.
    daily_requests: u64,
    daily_reset_date: chrono::NaiveDate,

    // Monthly counters — reset via `reset_monthly`.
    monthly_tokens: u64,
    monthly_cost_usd: f64,

    // Sliding-window timestamps for per-second rate limiting.
    recent_requests: VecDeque<DateTime<Utc>>,

    // Concurrent request tracking.
    concurrent_requests: u32,

    // Throttle flag — set when any limit is hit, cleared when under limits.
    is_throttled: bool,

    last_request_at: Option<DateTime<Utc>>,
}

impl TenantState {
    fn new(plan: TenantPlan) -> Self {
        let limits = plan.limits();
        Self {
            plan,
            limits,
            daily_requests: 0,
            daily_reset_date: Utc::now().date_naive(),
            monthly_tokens: 0,
            monthly_cost_usd: 0.0,
            recent_requests: VecDeque::new(),
            concurrent_requests: 0,
            is_throttled: false,
            last_request_at: None,
        }
    }

    /// Auto-reset daily counter if the UTC day rolled over.
    fn maybe_reset_daily(&mut self) {
        let today = Utc::now().date_naive();
        if today > self.daily_reset_date {
            self.daily_requests = 0;
            self.daily_reset_date = today;
        }
    }

    /// Prune the sliding window to only keep entries within the last second.
    fn prune_sliding_window(&mut self, now: DateTime<Utc>) {
        let cutoff = now - chrono::Duration::seconds(1);
        while self.recent_requests.front().is_some_and(|t| *t < cutoff) {
            self.recent_requests.pop_front();
        }
    }

    /// Compute the highest utilisation ratio across all dimensions.
    fn utilization_percent(&self) -> f64 {
        let mut max: f64 = 0.0;

        if self.limits.max_requests_per_day > 0 {
            let pct =
                (self.daily_requests as f64 / self.limits.max_requests_per_day as f64) * 100.0;
            max = max.max(pct);
        }
        if self.limits.max_tokens_per_month > 0 {
            let pct =
                (self.monthly_tokens as f64 / self.limits.max_tokens_per_month as f64) * 100.0;
            max = max.max(pct);
        }
        if self.limits.max_cost_per_month_usd > 0.0 {
            let pct = (self.monthly_cost_usd / self.limits.max_cost_per_month_usd) * 100.0;
            max = max.max(pct);
        }
        // Clamp to 100 — going over is still 100 % utilisation.
        max.min(100.0)
    }
}

// ---------------------------------------------------------------------------
// TenantLimitManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for per-tenant quotas and rate limits.
#[derive(Debug, Clone)]
pub struct TenantLimitManager {
    inner: Arc<RwLock<HashMap<String, TenantState>>>,
}

impl TenantLimitManager {
    /// Create an empty manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a tenant with the given plan. If the tenant already exists
    /// the previous state is replaced.
    pub fn register_tenant(&self, tenant_id: &str, plan: TenantPlan) {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        map.insert(tenant_id.to_string(), TenantState::new(plan));
    }

    /// Check whether the tenant may issue a new request right now.
    ///
    /// This does **not** consume a slot; call [`record_usage`] afterwards to
    /// actually account for consumed resources.
    pub fn check_request(&self, tenant_id: &str) -> TenantCheckResult {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        let state = match map.get_mut(tenant_id) {
            Some(s) => s,
            None => {
                return TenantCheckResult {
                    allowed: false,
                    reason: Some("tenant_not_found".to_string()),
                    remaining_daily_requests: 0,
                    remaining_monthly_tokens: 0,
                    remaining_monthly_budget_usd: 0.0,
                    rate_limit_reset_at: None,
                };
            }
        };

        state.maybe_reset_daily();
        let now = Utc::now();
        state.prune_sliding_window(now);

        // 1) Daily request limit
        if state.daily_requests >= state.limits.max_requests_per_day {
            state.is_throttled = true;
            // Safety: midnight (0, 0, 0) is always a valid time.
            #[allow(clippy::expect_used)]
            let tomorrow = (state.daily_reset_date + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .expect("valid date");
            let reset = DateTime::<Utc>::from_naive_utc_and_offset(tomorrow, Utc);
            return TenantCheckResult {
                allowed: false,
                reason: Some("daily_limit_exceeded".to_string()),
                remaining_daily_requests: 0,
                remaining_monthly_tokens: state
                    .limits
                    .max_tokens_per_month
                    .saturating_sub(state.monthly_tokens),
                remaining_monthly_budget_usd: (state.limits.max_cost_per_month_usd
                    - state.monthly_cost_usd)
                    .max(0.0),
                rate_limit_reset_at: Some(reset),
            };
        }

        // 2) Monthly token limit
        if state.monthly_tokens >= state.limits.max_tokens_per_month {
            state.is_throttled = true;
            return TenantCheckResult {
                allowed: false,
                reason: Some("monthly_tokens_exceeded".to_string()),
                remaining_daily_requests: state
                    .limits
                    .max_requests_per_day
                    .saturating_sub(state.daily_requests),
                remaining_monthly_tokens: 0,
                remaining_monthly_budget_usd: (state.limits.max_cost_per_month_usd
                    - state.monthly_cost_usd)
                    .max(0.0),
                rate_limit_reset_at: None,
            };
        }

        // 3) Monthly budget (only enforce when budget > 0)
        if state.limits.max_cost_per_month_usd > 0.0
            && state.monthly_cost_usd >= state.limits.max_cost_per_month_usd
        {
            state.is_throttled = true;
            return TenantCheckResult {
                allowed: false,
                reason: Some("monthly_budget_exceeded".to_string()),
                remaining_daily_requests: state
                    .limits
                    .max_requests_per_day
                    .saturating_sub(state.daily_requests),
                remaining_monthly_tokens: state
                    .limits
                    .max_tokens_per_month
                    .saturating_sub(state.monthly_tokens),
                remaining_monthly_budget_usd: 0.0,
                rate_limit_reset_at: None,
            };
        }

        // 4) Per-second sliding window
        #[allow(clippy::cast_possible_truncation)]
        let max_in_window = state.limits.max_requests_per_second.ceil() as usize;
        if state.recent_requests.len() >= max_in_window {
            state.is_throttled = true;
            let oldest = state.recent_requests.front().copied();
            let reset_at = oldest.map(|t| t + chrono::Duration::seconds(1));
            return TenantCheckResult {
                allowed: false,
                reason: Some("rate_limit_exceeded".to_string()),
                remaining_daily_requests: state
                    .limits
                    .max_requests_per_day
                    .saturating_sub(state.daily_requests),
                remaining_monthly_tokens: state
                    .limits
                    .max_tokens_per_month
                    .saturating_sub(state.monthly_tokens),
                remaining_monthly_budget_usd: (state.limits.max_cost_per_month_usd
                    - state.monthly_cost_usd)
                    .max(0.0),
                rate_limit_reset_at: reset_at,
            };
        }

        // 5) Concurrent requests
        if state.concurrent_requests >= state.limits.max_concurrent_requests {
            state.is_throttled = true;
            return TenantCheckResult {
                allowed: false,
                reason: Some("concurrent_limit_exceeded".to_string()),
                remaining_daily_requests: state
                    .limits
                    .max_requests_per_day
                    .saturating_sub(state.daily_requests),
                remaining_monthly_tokens: state
                    .limits
                    .max_tokens_per_month
                    .saturating_sub(state.monthly_tokens),
                remaining_monthly_budget_usd: (state.limits.max_cost_per_month_usd
                    - state.monthly_cost_usd)
                    .max(0.0),
                rate_limit_reset_at: None,
            };
        }

        // All checks pass — record the request in the sliding window and bump
        // the daily counter / concurrent count.
        state.daily_requests += 1;
        state.concurrent_requests += 1;
        state.recent_requests.push_back(now);
        state.last_request_at = Some(now);
        state.is_throttled = false;

        TenantCheckResult {
            allowed: true,
            reason: None,
            remaining_daily_requests: state
                .limits
                .max_requests_per_day
                .saturating_sub(state.daily_requests),
            remaining_monthly_tokens: state
                .limits
                .max_tokens_per_month
                .saturating_sub(state.monthly_tokens),
            remaining_monthly_budget_usd: (state.limits.max_cost_per_month_usd
                - state.monthly_cost_usd)
                .max(0.0),
            rate_limit_reset_at: None,
        }
    }

    /// Record token and cost usage **after** a request completes.
    /// Also decrements the concurrent-request counter.
    pub fn record_usage(&self, tenant_id: &str, tokens_in: u64, tokens_out: u64, cost_usd: f64) {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        if let Some(state) = map.get_mut(tenant_id) {
            state.monthly_tokens += tokens_in + tokens_out;
            state.monthly_cost_usd += cost_usd;
            state.concurrent_requests = state.concurrent_requests.saturating_sub(1);
        }
    }

    /// Return a full usage snapshot for the tenant.
    pub fn get_status(&self, tenant_id: &str) -> Option<TenantUsageStatus> {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        let state = map.get_mut(tenant_id)?;
        state.maybe_reset_daily();

        Some(TenantUsageStatus {
            tenant_id: tenant_id.to_string(),
            plan: state.plan.name().to_string(),
            daily_requests: state.daily_requests,
            daily_limit: state.limits.max_requests_per_day,
            monthly_tokens: state.monthly_tokens,
            monthly_limit: state.limits.max_tokens_per_month,
            monthly_cost_usd: state.monthly_cost_usd,
            monthly_budget_usd: state.limits.max_cost_per_month_usd,
            concurrent_requests: state.concurrent_requests,
            concurrent_limit: state.limits.max_concurrent_requests,
            utilization_percent: state.utilization_percent(),
            is_throttled: state.is_throttled,
            last_request_at: state.last_request_at,
        })
    }

    /// List all registered tenants as lightweight summaries.
    pub fn list_tenants(&self) -> Vec<TenantSummary> {
        #[allow(clippy::expect_used)] // lock poisoning
        let map = self.inner.read().expect("rwlock poisoned");
        map.iter()
            .map(|(id, state)| TenantSummary {
                tenant_id: id.clone(),
                plan: state.plan.name().to_string(),
                utilization_percent: state.utilization_percent(),
                is_throttled: state.is_throttled,
            })
            .collect()
    }

    /// Upgrade (or downgrade) a tenant to a different plan.
    /// Preserves accumulated usage counters; only the limits change.
    pub fn upgrade_plan(&self, tenant_id: &str, new_plan: TenantPlan) {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        if let Some(state) = map.get_mut(tenant_id) {
            state.limits = new_plan.limits();
            state.plan = new_plan;
            // Re-evaluate throttle flag against new limits.
            state.is_throttled = false;
        }
    }

    /// Reset monthly counters for a tenant (tokens + cost).
    pub fn reset_monthly(&self, tenant_id: &str) {
        #[allow(clippy::expect_used)] // lock poisoning
        let mut map = self.inner.write().expect("rwlock poisoned");
        if let Some(state) = map.get_mut(tenant_id) {
            state.monthly_tokens = 0;
            state.monthly_cost_usd = 0.0;
            state.is_throttled = false;
        }
    }

    /// Check whether a specific model is allowed for the tenant.
    pub fn is_model_allowed(&self, tenant_id: &str, model: &str) -> bool {
        #[allow(clippy::expect_used)] // lock poisoning
        let map = self.inner.read().expect("rwlock poisoned");
        match map.get(tenant_id) {
            Some(state) => {
                if state.limits.allowed_models.is_empty() {
                    true
                } else {
                    state.limits.allowed_models.iter().any(|m| m == model)
                }
            }
            None => false,
        }
    }

    /// Check whether a specific agent role is allowed for the tenant.
    pub fn is_agent_allowed(&self, tenant_id: &str, agent_role: &str) -> bool {
        #[allow(clippy::expect_used)] // lock poisoning
        let map = self.inner.read().expect("rwlock poisoned");
        match map.get(tenant_id) {
            Some(state) => {
                if state.limits.allowed_agents.is_empty() {
                    true
                } else {
                    state.limits.allowed_agents.iter().any(|a| a == agent_role)
                }
            }
            None => false,
        }
    }
}

impl Default for TenantLimitManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -- helpers -----------------------------------------------------------

    fn mgr_with_tenant(plan: TenantPlan) -> (TenantLimitManager, String) {
        let mgr = TenantLimitManager::new();
        let id = "tenant-1".to_string();
        mgr.register_tenant(&id, plan);
        (mgr, id)
    }

    // -- registration & listing -------------------------------------------

    #[test]
    fn test_register_tenant_free() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.plan, "Free");
        assert_eq!(status.daily_limit, 100);
        assert_eq!(status.monthly_limit, 50_000);
    }

    #[test]
    fn test_register_tenant_pro() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.plan, "Pro");
        assert_eq!(status.daily_limit, 5_000);
        assert_eq!(status.monthly_limit, 2_000_000);
    }

    #[test]
    fn test_register_tenant_enterprise() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Enterprise);
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.plan, "Enterprise");
        assert_eq!(status.daily_limit, 100_000);
    }

    #[test]
    fn test_register_custom_plan() {
        let limits = TenantLimits {
            max_requests_per_day: 42,
            max_tokens_per_month: 999,
            max_requests_per_second: 5.0,
            max_concurrent_requests: 3,
            max_cost_per_month_usd: 10.0,
            allowed_models: vec!["gpt-4".to_string()],
            allowed_agents: vec!["coder".to_string()],
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom {
            limits: limits.clone(),
        });
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.plan, "Custom");
        assert_eq!(status.daily_limit, 42);
    }

    #[test]
    fn test_list_tenants_empty() {
        let mgr = TenantLimitManager::new();
        assert!(mgr.list_tenants().is_empty());
    }

    #[test]
    fn test_list_tenants_multiple() {
        let mgr = TenantLimitManager::new();
        mgr.register_tenant("a", TenantPlan::Free);
        mgr.register_tenant("b", TenantPlan::Pro);
        let list = mgr.list_tenants();
        assert_eq!(list.len(), 2);
    }

    // -- check_request: allowed -------------------------------------------

    #[test]
    fn test_check_request_allowed() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let res = mgr.check_request(&id);
        assert!(res.allowed);
        assert!(res.reason.is_none());
        assert_eq!(res.remaining_daily_requests, 4_999);
    }

    #[test]
    fn test_check_request_unknown_tenant() {
        let mgr = TenantLimitManager::new();
        let res = mgr.check_request("nonexistent");
        assert!(!res.allowed);
        assert_eq!(res.reason.as_deref(), Some("tenant_not_found"));
    }

    // -- daily limit enforcement ------------------------------------------

    #[test]
    fn test_daily_limit_enforcement() {
        let limits = TenantLimits {
            max_requests_per_day: 3,
            max_tokens_per_month: 1_000_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        // First 3 should pass
        for i in 0..3 {
            let r = mgr.check_request(&id);
            assert!(r.allowed, "request {i} should be allowed");
            // Release the concurrent slot
            mgr.record_usage(&id, 0, 0, 0.0);
        }

        // 4th should fail
        let r = mgr.check_request(&id);
        assert!(!r.allowed);
        assert_eq!(r.reason.as_deref(), Some("daily_limit_exceeded"));
        assert_eq!(r.remaining_daily_requests, 0);
        assert!(r.rate_limit_reset_at.is_some());
    }

    // -- monthly token enforcement ----------------------------------------

    #[test]
    fn test_monthly_token_enforcement() {
        let limits = TenantLimits {
            max_requests_per_day: 1_000,
            max_tokens_per_month: 100,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        // Consume tokens
        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 60, 50, 0.0); // 110 total, over the 100 limit

        // Next request should be denied
        let r = mgr.check_request(&id);
        assert!(!r.allowed);
        assert_eq!(r.reason.as_deref(), Some("monthly_tokens_exceeded"));
        assert_eq!(r.remaining_monthly_tokens, 0);
    }

    // -- budget enforcement -----------------------------------------------

    #[test]
    fn test_budget_enforcement() {
        let limits = TenantLimits {
            max_requests_per_day: 1_000,
            max_tokens_per_month: 10_000_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 5.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 100, 100, 5.50);

        let r = mgr.check_request(&id);
        assert!(!r.allowed);
        assert_eq!(r.reason.as_deref(), Some("monthly_budget_exceeded"));
        assert_eq!(r.remaining_monthly_budget_usd, 0.0);
    }

    #[test]
    fn test_zero_budget_not_enforced() {
        // Free plan has $0 budget — that means "no budget cap", not "deny everything".
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        let r = mgr.check_request(&id);
        assert!(r.allowed);
    }

    // -- rate limiting (per-second) ---------------------------------------

    #[test]
    fn test_rate_limit_per_second() {
        let limits = TenantLimits {
            max_requests_per_day: 10_000,
            max_tokens_per_month: 10_000_000,
            max_requests_per_second: 2.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        // First 2 pass (ceil(2.0) = 2 per window)
        for _ in 0..2 {
            let r = mgr.check_request(&id);
            assert!(r.allowed);
            mgr.record_usage(&id, 0, 0, 0.0);
        }

        // 3rd should be rate limited
        let r = mgr.check_request(&id);
        assert!(!r.allowed);
        assert_eq!(r.reason.as_deref(), Some("rate_limit_exceeded"));
        assert!(r.rate_limit_reset_at.is_some());
    }

    // -- concurrent request tracking --------------------------------------

    #[test]
    fn test_concurrent_request_tracking() {
        let limits = TenantLimits {
            max_requests_per_day: 10_000,
            max_tokens_per_month: 10_000_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 2,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        // Open 2 concurrent slots
        let r1 = mgr.check_request(&id);
        assert!(r1.allowed);
        let r2 = mgr.check_request(&id);
        assert!(r2.allowed);

        // 3rd should fail — concurrent limit hit
        let r3 = mgr.check_request(&id);
        assert!(!r3.allowed);
        assert_eq!(r3.reason.as_deref(), Some("concurrent_limit_exceeded"));

        // Finish one request
        mgr.record_usage(&id, 10, 10, 0.1);

        // Now it should work again
        let r4 = mgr.check_request(&id);
        assert!(r4.allowed);
    }

    // -- plan upgrade -----------------------------------------------------

    #[test]
    fn test_upgrade_plan() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        let before = mgr.get_status(&id).unwrap();
        assert_eq!(before.daily_limit, 100);

        mgr.upgrade_plan(&id, TenantPlan::Pro);
        let after = mgr.get_status(&id).unwrap();
        assert_eq!(after.plan, "Pro");
        assert_eq!(after.daily_limit, 5_000);
    }

    #[test]
    fn test_upgrade_preserves_usage() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 100, 200, 1.0);

        mgr.upgrade_plan(&id, TenantPlan::Enterprise);
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.monthly_tokens, 300);
        assert!((status.monthly_cost_usd - 1.0).abs() < f64::EPSILON);
    }

    // -- model / agent restrictions ---------------------------------------

    #[test]
    fn test_model_allowed_empty_list() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        // Empty allowed_models means all models are allowed.
        assert!(mgr.is_model_allowed(&id, "gpt-4"));
        assert!(mgr.is_model_allowed(&id, "claude-3"));
    }

    #[test]
    fn test_model_restricted() {
        let limits = TenantLimits {
            allowed_models: vec!["gpt-4".to_string()],
            ..TenantLimits::free()
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });
        assert!(mgr.is_model_allowed(&id, "gpt-4"));
        assert!(!mgr.is_model_allowed(&id, "claude-3"));
    }

    #[test]
    fn test_model_unknown_tenant() {
        let mgr = TenantLimitManager::new();
        assert!(!mgr.is_model_allowed("ghost", "gpt-4"));
    }

    #[test]
    fn test_agent_allowed_empty_list() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        assert!(mgr.is_agent_allowed(&id, "coder"));
        assert!(mgr.is_agent_allowed(&id, "reviewer"));
    }

    #[test]
    fn test_agent_restricted() {
        let limits = TenantLimits {
            allowed_agents: vec!["coder".to_string(), "reviewer".to_string()],
            ..TenantLimits::pro()
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });
        assert!(mgr.is_agent_allowed(&id, "coder"));
        assert!(mgr.is_agent_allowed(&id, "reviewer"));
        assert!(!mgr.is_agent_allowed(&id, "admin"));
    }

    #[test]
    fn test_agent_unknown_tenant() {
        let mgr = TenantLimitManager::new();
        assert!(!mgr.is_agent_allowed("ghost", "coder"));
    }

    // -- reset monthly ----------------------------------------------------

    #[test]
    fn test_reset_monthly() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 500, 500, 2.5);

        let before = mgr.get_status(&id).unwrap();
        assert_eq!(before.monthly_tokens, 1_000);

        mgr.reset_monthly(&id);

        let after = mgr.get_status(&id).unwrap();
        assert_eq!(after.monthly_tokens, 0);
        assert!((after.monthly_cost_usd).abs() < f64::EPSILON);
        assert!(!after.is_throttled);
    }

    // -- throttle status --------------------------------------------------

    #[test]
    fn test_throttled_flag_set_on_deny() {
        let limits = TenantLimits {
            max_requests_per_day: 1,
            max_tokens_per_month: 1_000_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        let _ = mgr.check_request(&id);
        mgr.record_usage(&id, 0, 0, 0.0);

        // Should be denied now
        let _ = mgr.check_request(&id);
        let status = mgr.get_status(&id).unwrap();
        assert!(status.is_throttled);
    }

    #[test]
    fn test_throttled_flag_cleared_on_allow() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let r = mgr.check_request(&id);
        assert!(r.allowed);
        let status = mgr.get_status(&id).unwrap();
        assert!(!status.is_throttled);
    }

    // -- utilization calculation ------------------------------------------

    #[test]
    fn test_utilization_zero_when_fresh() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let status = mgr.get_status(&id).unwrap();
        assert!((status.utilization_percent).abs() < f64::EPSILON);
    }

    #[test]
    fn test_utilization_increases_with_usage() {
        let limits = TenantLimits {
            max_requests_per_day: 100,
            max_tokens_per_month: 1_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 10.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 250, 250, 1.0); // 500/1000 = 50% tokens

        let status = mgr.get_status(&id).unwrap();
        assert!(status.utilization_percent >= 49.0);
        assert!(status.utilization_percent <= 51.0);
    }

    #[test]
    fn test_utilization_capped_at_100() {
        let limits = TenantLimits {
            max_requests_per_day: 10_000,
            max_tokens_per_month: 100,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 100.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        let r = mgr.check_request(&id);
        assert!(r.allowed);
        mgr.record_usage(&id, 200, 200, 0.0); // 400/100 = 400%, capped to 100%

        let status = mgr.get_status(&id).unwrap();
        assert!((status.utilization_percent - 100.0).abs() < f64::EPSILON);
    }

    // -- edge cases -------------------------------------------------------

    #[test]
    fn test_record_usage_unknown_tenant() {
        let mgr = TenantLimitManager::new();
        // Should not panic.
        mgr.record_usage("ghost", 100, 100, 1.0);
    }

    #[test]
    fn test_get_status_unknown_tenant() {
        let mgr = TenantLimitManager::new();
        assert!(mgr.get_status("ghost").is_none());
    }

    #[test]
    fn test_upgrade_unknown_tenant_noop() {
        let mgr = TenantLimitManager::new();
        // Should not panic.
        mgr.upgrade_plan("ghost", TenantPlan::Enterprise);
    }

    #[test]
    fn test_reset_monthly_unknown_tenant_noop() {
        let mgr = TenantLimitManager::new();
        // Should not panic.
        mgr.reset_monthly("ghost");
    }

    #[test]
    fn test_default_plans_helper() {
        let plans = default_plans();
        assert_eq!(plans.len(), 3);
        assert!(plans.contains_key("free"));
        assert!(plans.contains_key("pro"));
        assert!(plans.contains_key("enterprise"));
        assert_eq!(plans["free"].max_requests_per_day, 100);
        assert_eq!(plans["pro"].max_requests_per_day, 5_000);
        assert_eq!(plans["enterprise"].max_requests_per_day, 100_000);
    }

    #[test]
    fn test_remaining_values_decrement() {
        let limits = TenantLimits {
            max_requests_per_day: 10,
            max_tokens_per_month: 10_000,
            max_requests_per_second: 100.0,
            max_concurrent_requests: 100,
            max_cost_per_month_usd: 50.0,
            allowed_models: Vec::new(),
            allowed_agents: Vec::new(),
        };
        let (mgr, id) = mgr_with_tenant(TenantPlan::Custom { limits });

        let r1 = mgr.check_request(&id);
        assert!(r1.allowed);
        assert_eq!(r1.remaining_daily_requests, 9);
        mgr.record_usage(&id, 100, 100, 1.0);

        let r2 = mgr.check_request(&id);
        assert!(r2.allowed);
        assert_eq!(r2.remaining_daily_requests, 8);
        assert_eq!(r2.remaining_monthly_tokens, 10_000 - 200);
        assert!((r2.remaining_monthly_budget_usd - 49.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_last_request_at_updated() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Pro);
        let before = mgr.get_status(&id).unwrap();
        assert!(before.last_request_at.is_none());

        let _ = mgr.check_request(&id);
        let after = mgr.get_status(&id).unwrap();
        assert!(after.last_request_at.is_some());
    }

    #[test]
    fn test_thread_safe_clone() {
        let mgr = TenantLimitManager::new();
        let mgr2 = mgr.clone();
        mgr.register_tenant("t1", TenantPlan::Free);
        // Both handles see the same data.
        assert!(mgr2.get_status("t1").is_some());
    }

    #[test]
    fn test_re_register_replaces_state() {
        let (mgr, id) = mgr_with_tenant(TenantPlan::Free);
        let _ = mgr.check_request(&id);
        mgr.record_usage(&id, 1000, 1000, 5.0);

        // Re-registering resets all state.
        mgr.register_tenant(&id, TenantPlan::Pro);
        let status = mgr.get_status(&id).unwrap();
        assert_eq!(status.plan, "Pro");
        assert_eq!(status.monthly_tokens, 0);
    }
}
