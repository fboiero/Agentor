//! Hosted dashboard adapter.
//!
//! Emits `DashboardSnapshot` payloads consumable by a SPA frontend (Next.js
//! or similar). In production this is where Server-Sent Events / WebSocket
//! push would live. For now, snapshots are synchronously computed from the
//! tenant + quota managers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::quota::QuotaEnforcer;
use crate::tenant::{Tenant, TenantManager};

/// Per-tenant metrics snapshot shown on the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TenantMetrics {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Display name.
    pub name: String,
    /// Current plan as a human-readable string.
    pub plan: String,
    /// Runs used / limit.
    pub runs: (u64, u64),
    /// Tokens used / limit.
    pub tokens: (u64, u64),
    /// Active agents / limit.
    pub active_agents: (u32, u32),
    /// Storage MB used / limit.
    pub storage_mb: (u64, u64),
    /// Health score 0.0–1.0 (1.0 = well under all quotas).
    pub health: f64,
}

/// Full dashboard snapshot for an admin view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardSnapshot {
    /// Snapshot time (UTC).
    pub generated_at: DateTime<Utc>,
    /// Per-tenant metrics.
    pub tenants: Vec<TenantMetrics>,
    /// Total tenants in this snapshot.
    pub total_tenants: usize,
    /// Total runs consumed this period across all tenants.
    pub total_runs: u64,
    /// Total tokens consumed this period across all tenants.
    pub total_tokens: u64,
}

/// Computes dashboard snapshots from in-memory state.
///
/// TODO: replace with WebSocket push driven by `argentor-gateway`. Add
/// time-series aggregation (Prometheus scrape) for charts.
pub struct DashboardAdapter {
    tenants: Arc<TenantManager>,
    quotas: Arc<QuotaEnforcer>,
}

impl DashboardAdapter {
    /// Build an adapter over the given tenant + quota managers.
    pub fn new(tenants: Arc<TenantManager>, quotas: Arc<QuotaEnforcer>) -> Self {
        Self { tenants, quotas }
    }

    /// Compute metrics for a single tenant (None if unknown).
    pub fn metrics_for(&self, tenant_id: &str) -> Option<TenantMetrics> {
        let t = self.tenants.get_tenant(tenant_id)?;
        let u = self.quotas.get_usage(tenant_id)?;
        Some(Self::metrics_from(&t, &u))
    }

    fn metrics_from(t: &Tenant, u: &crate::quota::QuotaUsage) -> TenantMetrics {
        let health = 1.0 - u.runs_pct().max(u.tokens_pct());
        TenantMetrics {
            tenant_id: t.id.clone(),
            name: t.name.clone(),
            plan: format!("{:?}", t.plan),
            runs: (u.agent_runs_used, u.agent_runs_limit),
            tokens: (u.tokens_used, u.tokens_limit),
            active_agents: (u.active_agents, u.active_agents_limit),
            storage_mb: (u.storage_mb_used, u.storage_mb_limit),
            health: health.clamp(0.0, 1.0),
        }
    }

    /// Build a full snapshot over all known tenants.
    pub fn snapshot(&self) -> DashboardSnapshot {
        let tenants = self.tenants.list_tenants();
        let mut metrics = Vec::with_capacity(tenants.len());
        let mut total_runs = 0u64;
        let mut total_tokens = 0u64;
        for t in &tenants {
            if let Some(u) = self.quotas.get_usage(&t.id) {
                total_runs = total_runs.saturating_add(u.agent_runs_used);
                total_tokens = total_tokens.saturating_add(u.tokens_used);
                metrics.push(Self::metrics_from(t, &u));
            }
        }
        DashboardSnapshot {
            generated_at: Utc::now(),
            total_tenants: metrics.len(),
            tenants: metrics,
            total_runs,
            total_tokens,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tenant::{DataRegion, TenantPlan};

    fn setup() -> (
        Arc<TenantManager>,
        Arc<QuotaEnforcer>,
        String,
        DashboardAdapter,
    ) {
        let tenants = Arc::new(TenantManager::new());
        let quotas = Arc::new(QuotaEnforcer::new());
        let t = tenants.create_tenant("Acme".into(), TenantPlan::Free, DataRegion::UsEast);
        quotas.register(t.id.clone(), TenantPlan::Free);
        let adapter = DashboardAdapter::new(tenants.clone(), quotas.clone());
        (tenants, quotas, t.id, adapter)
    }

    #[test]
    fn metrics_for_existing_tenant() {
        let (_, _, tid, adapter) = setup();
        let m = adapter.metrics_for(&tid).unwrap();
        assert_eq!(m.tenant_id, tid);
        assert_eq!(m.runs, (0, 1_000));
    }

    #[test]
    fn metrics_for_unknown_returns_none() {
        let (_, _, _, adapter) = setup();
        assert!(adapter.metrics_for("ghost").is_none());
    }

    #[test]
    fn metrics_health_full_when_fresh() {
        let (_, _, tid, adapter) = setup();
        let m = adapter.metrics_for(&tid).unwrap();
        assert!((m.health - 1.0).abs() < 1e-9);
    }

    #[test]
    fn metrics_health_drops_with_usage() {
        let (_, quotas, tid, adapter) = setup();
        for _ in 0..500 {
            quotas.record_run(&tid, 0);
        }
        let m = adapter.metrics_for(&tid).unwrap();
        assert!(m.health < 0.51 && m.health > 0.49);
    }

    #[test]
    fn metrics_plan_renders_as_debug() {
        let (_, _, tid, adapter) = setup();
        let m = adapter.metrics_for(&tid).unwrap();
        assert_eq!(m.plan, "Free");
    }

    #[test]
    fn snapshot_counts_tenants() {
        let (_, _, _, adapter) = setup();
        let snap = adapter.snapshot();
        assert_eq!(snap.total_tenants, 1);
    }

    #[test]
    fn snapshot_aggregates_runs() {
        let (_, quotas, tid, adapter) = setup();
        quotas.record_run(&tid, 100);
        quotas.record_run(&tid, 200);
        let snap = adapter.snapshot();
        assert_eq!(snap.total_runs, 2);
        assert_eq!(snap.total_tokens, 300);
    }

    #[test]
    fn snapshot_has_timestamp() {
        let (_, _, _, adapter) = setup();
        let before = Utc::now();
        let snap = adapter.snapshot();
        let after = Utc::now();
        assert!(snap.generated_at >= before && snap.generated_at <= after);
    }

    #[test]
    fn snapshot_empty_for_empty_manager() {
        let tenants = Arc::new(TenantManager::new());
        let quotas = Arc::new(QuotaEnforcer::new());
        let adapter = DashboardAdapter::new(tenants, quotas);
        let snap = adapter.snapshot();
        assert_eq!(snap.total_tenants, 0);
        assert!(snap.tenants.is_empty());
    }

    #[test]
    fn snapshot_skips_tenants_without_quota_record() {
        let tenants = Arc::new(TenantManager::new());
        let quotas = Arc::new(QuotaEnforcer::new());
        // tenant created but quota not registered
        tenants.create_tenant("A".into(), TenantPlan::Free, DataRegion::UsEast);
        let adapter = DashboardAdapter::new(tenants, quotas);
        assert_eq!(adapter.snapshot().total_tenants, 0);
    }

    #[test]
    fn snapshot_serde_roundtrip() {
        let (_, _, _, adapter) = setup();
        let snap = adapter.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        let back: DashboardSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_tenants, snap.total_tenants);
    }

    #[test]
    fn tenant_metrics_serde_roundtrip() {
        let (_, _, tid, adapter) = setup();
        let m = adapter.metrics_for(&tid).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let back: TenantMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn multiple_tenants_in_snapshot() {
        let tenants = Arc::new(TenantManager::new());
        let quotas = Arc::new(QuotaEnforcer::new());
        for i in 0..3 {
            let t = tenants.create_tenant(format!("T{i}"), TenantPlan::Free, DataRegion::UsEast);
            quotas.register(t.id, TenantPlan::Free);
        }
        let adapter = DashboardAdapter::new(tenants, quotas);
        assert_eq!(adapter.snapshot().total_tenants, 3);
    }

    #[test]
    fn storage_reported_in_metrics() {
        let (_, quotas, tid, adapter) = setup();
        quotas.add_storage(&tid, 25).unwrap();
        let m = adapter.metrics_for(&tid).unwrap();
        assert_eq!(m.storage_mb, (25, 100));
    }
}
