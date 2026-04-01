//! SLA compliance tracking with uptime, availability, and incident windows.
//!
//! Monitors service-level agreements by tracking uptime percentages,
//! response time compliance, and incident durations.
//!
//! # Main types
//!
//! - [`SlaTracker`] — Thread-safe SLA state tracker.
//! - [`SlaDefinition`] — Defines an SLA target (uptime %, response time).
//! - [`SlaStatus`] — Current compliance status of an SLA.
//! - [`Incident`] — Tracks a period of SLA violation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// SlaDefinition
// ---------------------------------------------------------------------------

/// Defines a Service Level Agreement target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaDefinition {
    /// Unique name for this SLA.
    pub name: String,
    /// Target uptime percentage (e.g. 99.9).
    pub target_uptime_percent: f64,
    /// Maximum acceptable response time in milliseconds.
    pub max_response_time_ms: u64,
    /// Measurement window in seconds.
    pub window_seconds: u64,
    /// Description of this SLA.
    pub description: String,
}

impl SlaDefinition {
    /// Create a new SLA definition.
    pub fn new(name: impl Into<String>, target_uptime: f64, max_response_ms: u64) -> Self {
        Self {
            name: name.into(),
            target_uptime_percent: target_uptime,
            max_response_time_ms: max_response_ms,
            window_seconds: 86400, // 24 hours default
            description: String::new(),
        }
    }

    /// Set the measurement window.
    pub fn with_window(mut self, seconds: u64) -> Self {
        self.window_seconds = seconds;
        self
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Incident
// ---------------------------------------------------------------------------

/// A period during which an SLA was violated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    /// SLA this incident relates to.
    pub sla_name: String,
    /// When the incident started.
    pub started_at: DateTime<Utc>,
    /// When the incident ended (None if still ongoing).
    pub ended_at: Option<DateTime<Utc>>,
    /// Duration in seconds (computed or ongoing).
    pub duration_seconds: u64,
    /// Description of what happened.
    pub description: String,
    /// Whether this incident is still active.
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// SlaStatus
// ---------------------------------------------------------------------------

/// Current compliance status for an SLA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaStatus {
    /// SLA definition name.
    pub name: String,
    /// Current uptime percentage.
    pub current_uptime_percent: f64,
    /// Target uptime percentage.
    pub target_uptime_percent: f64,
    /// Whether the SLA is currently met.
    pub is_compliant: bool,
    /// Total downtime in the current window (seconds).
    pub total_downtime_seconds: u64,
    /// Number of incidents in the current window.
    pub incident_count: u64,
    /// Average response time in ms.
    pub avg_response_time_ms: f64,
    /// Whether response time SLA is met.
    pub response_time_compliant: bool,
    /// Total number of health checks performed.
    pub total_checks: u64,
    /// Number of failed health checks.
    pub failed_checks: u64,
}

// ---------------------------------------------------------------------------
// Inner state per SLA
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SlaState {
    definition: SlaDefinition,
    incidents: Vec<Incident>,
    total_checks: u64,
    failed_checks: u64,
    total_response_time_ms: u64,
    response_time_samples: u64,
    _tracking_since: DateTime<Utc>,
}

impl SlaState {
    fn new(definition: SlaDefinition) -> Self {
        Self {
            definition,
            incidents: Vec::new(),
            total_checks: 0,
            failed_checks: 0,
            total_response_time_ms: 0,
            response_time_samples: 0,
            _tracking_since: Utc::now(),
        }
    }

    fn uptime_percent(&self) -> f64 {
        if self.total_checks == 0 {
            return 100.0;
        }
        let successful = self.total_checks - self.failed_checks;
        (successful as f64 / self.total_checks as f64) * 100.0
    }

    fn avg_response_time_ms(&self) -> f64 {
        if self.response_time_samples == 0 {
            return 0.0;
        }
        self.total_response_time_ms as f64 / self.response_time_samples as f64
    }

    fn total_downtime_seconds(&self) -> u64 {
        self.incidents.iter().map(|i| i.duration_seconds).sum()
    }

    fn status(&self) -> SlaStatus {
        let uptime = self.uptime_percent();
        let avg_rt = self.avg_response_time_ms();
        SlaStatus {
            name: self.definition.name.clone(),
            current_uptime_percent: uptime,
            target_uptime_percent: self.definition.target_uptime_percent,
            is_compliant: uptime >= self.definition.target_uptime_percent,
            total_downtime_seconds: self.total_downtime_seconds(),
            incident_count: self.incidents.len() as u64,
            avg_response_time_ms: avg_rt,
            response_time_compliant: avg_rt <= self.definition.max_response_time_ms as f64
                || self.response_time_samples == 0,
            total_checks: self.total_checks,
            failed_checks: self.failed_checks,
        }
    }
}

// ---------------------------------------------------------------------------
// SlaTracker
// ---------------------------------------------------------------------------

/// Thread-safe SLA compliance tracker.
///
/// Clone is cheap (inner state is behind `Arc<RwLock>`).
#[derive(Debug, Clone)]
pub struct SlaTracker {
    state: Arc<RwLock<HashMap<String, SlaState>>>,
}

impl Default for SlaTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SlaTracker {
    /// Create a new SLA tracker.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an SLA definition to track.
    pub fn register(&self, definition: SlaDefinition) {
        let name = definition.name.clone();
        self.state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name, SlaState::new(definition));
    }

    /// Record a successful health check with response time.
    pub fn record_success(&self, sla_name: &str, response_time_ms: u64) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sla) = state.get_mut(sla_name) {
            sla.total_checks += 1;
            sla.total_response_time_ms += response_time_ms;
            sla.response_time_samples += 1;

            // Close any active incident
            let now = Utc::now();
            for incident in &mut sla.incidents {
                if incident.is_active {
                    incident.is_active = false;
                    incident.ended_at = Some(now);
                    incident.duration_seconds =
                        (now - incident.started_at).num_seconds().unsigned_abs();
                }
            }
        }
    }

    /// Record a failed health check.
    pub fn record_failure(&self, sla_name: &str, description: impl Into<String>) {
        let now = Utc::now();
        let desc = description.into();
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sla) = state.get_mut(sla_name) {
            sla.total_checks += 1;
            sla.failed_checks += 1;

            // Start new incident if none active
            let has_active = sla.incidents.iter().any(|i| i.is_active);
            if !has_active {
                sla.incidents.push(Incident {
                    sla_name: sla_name.to_string(),
                    started_at: now,
                    ended_at: None,
                    duration_seconds: 0,
                    description: desc,
                    is_active: true,
                });
            }
        }
    }

    /// Get the status of a specific SLA.
    pub fn status(&self, sla_name: &str) -> Option<SlaStatus> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(sla_name)
            .map(|s| s.status())
    }

    /// Get the status of all tracked SLAs.
    pub fn all_statuses(&self) -> Vec<SlaStatus> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|s| s.status())
            .collect()
    }

    /// Get incidents for a specific SLA.
    pub fn incidents(&self, sla_name: &str) -> Vec<Incident> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(sla_name)
            .map(|s| s.incidents.clone())
            .unwrap_or_default()
    }

    /// Get all active (ongoing) incidents.
    pub fn active_incidents(&self) -> Vec<Incident> {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .flat_map(|s| s.incidents.iter().filter(|i| i.is_active).cloned())
            .collect()
    }

    /// Get the number of tracked SLAs.
    pub fn sla_count(&self) -> usize {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Generate a compliance summary report.
    pub fn compliance_report(&self) -> SlaComplianceReport {
        let statuses = self.all_statuses();
        let compliant = statuses.iter().filter(|s| s.is_compliant).count();
        let total = statuses.len();
        SlaComplianceReport {
            total_slas: total,
            compliant_slas: compliant,
            non_compliant_slas: total - compliant,
            overall_compliant: compliant == total,
            statuses,
        }
    }
}

/// Summary report of SLA compliance across all tracked SLAs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaComplianceReport {
    /// Total number of tracked SLAs.
    pub total_slas: usize,
    /// Number of SLAs currently meeting their targets.
    pub compliant_slas: usize,
    /// Number of SLAs not meeting their targets.
    pub non_compliant_slas: usize,
    /// Whether all SLAs are compliant.
    pub overall_compliant: bool,
    /// Status of each SLA.
    pub statuses: Vec<SlaStatus>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn tracker() -> SlaTracker {
        SlaTracker::new()
    }

    fn api_sla() -> SlaDefinition {
        SlaDefinition::new("api", 99.9, 500)
    }

    // 1. New tracker is empty
    #[test]
    fn test_new_tracker_empty() {
        let t = tracker();
        assert_eq!(t.sla_count(), 0);
    }

    // 2. Register SLA
    #[test]
    fn test_register() {
        let t = tracker();
        t.register(api_sla());
        assert_eq!(t.sla_count(), 1);
    }

    // 3. Initial status is 100% uptime
    #[test]
    fn test_initial_status() {
        let t = tracker();
        t.register(api_sla());
        let status = t.status("api").unwrap();
        assert_eq!(status.current_uptime_percent, 100.0);
        assert!(status.is_compliant);
    }

    // 4. Record success
    #[test]
    fn test_record_success() {
        let t = tracker();
        t.register(api_sla());
        t.record_success("api", 100);
        t.record_success("api", 200);

        let status = t.status("api").unwrap();
        assert_eq!(status.total_checks, 2);
        assert_eq!(status.failed_checks, 0);
        assert_eq!(status.current_uptime_percent, 100.0);
        assert!((status.avg_response_time_ms - 150.0).abs() < 0.01);
    }

    // 5. Record failure creates incident
    #[test]
    fn test_record_failure() {
        let t = tracker();
        t.register(api_sla());
        t.record_failure("api", "timeout");

        let status = t.status("api").unwrap();
        assert_eq!(status.failed_checks, 1);
        assert_eq!(status.incident_count, 1);
        assert!(!status.is_compliant); // 0% uptime
    }

    // 6. Uptime calculation with mixed results
    #[test]
    fn test_uptime_calculation() {
        let t = tracker();
        t.register(SlaDefinition::new("test", 99.0, 1000));
        for _ in 0..99 {
            t.record_success("test", 50);
        }
        t.record_failure("test", "error");

        let status = t.status("test").unwrap();
        assert_eq!(status.total_checks, 100);
        assert!((status.current_uptime_percent - 99.0).abs() < 0.01);
    }

    // 7. Active incidents tracked
    #[test]
    fn test_active_incidents() {
        let t = tracker();
        t.register(api_sla());
        t.record_failure("api", "down");

        let active = t.active_incidents();
        assert_eq!(active.len(), 1);
        assert!(active[0].is_active);
    }

    // 8. Success closes active incident
    #[test]
    fn test_success_closes_incident() {
        let t = tracker();
        t.register(api_sla());
        t.record_failure("api", "down");
        assert_eq!(t.active_incidents().len(), 1);

        t.record_success("api", 100);
        assert_eq!(t.active_incidents().len(), 0);

        let incidents = t.incidents("api");
        assert!(!incidents[0].is_active);
        assert!(incidents[0].ended_at.is_some());
    }

    // 9. Multiple failures don't create duplicate incidents
    #[test]
    fn test_no_duplicate_incidents() {
        let t = tracker();
        t.register(api_sla());
        t.record_failure("api", "down");
        t.record_failure("api", "still down");
        t.record_failure("api", "still down");

        let incidents = t.incidents("api");
        assert_eq!(incidents.len(), 1);
    }

    // 10. Compliance report
    #[test]
    fn test_compliance_report() {
        let t = tracker();
        t.register(SlaDefinition::new("api", 99.9, 500));
        t.register(SlaDefinition::new("db", 99.99, 100));

        t.record_success("api", 100);
        t.record_failure("db", "slow query");

        let report = t.compliance_report();
        assert_eq!(report.total_slas, 2);
        assert_eq!(report.compliant_slas, 1);
        assert_eq!(report.non_compliant_slas, 1);
        assert!(!report.overall_compliant);
    }

    // 11. Response time compliance
    #[test]
    fn test_response_time_compliance() {
        let t = tracker();
        t.register(SlaDefinition::new("fast-api", 99.9, 100));
        t.record_success("fast-api", 200); // exceeds 100ms limit

        let status = t.status("fast-api").unwrap();
        assert!(!status.response_time_compliant);
    }

    // 12. Response time within limits
    #[test]
    fn test_response_time_ok() {
        let t = tracker();
        t.register(SlaDefinition::new("api", 99.9, 500));
        t.record_success("api", 200);

        let status = t.status("api").unwrap();
        assert!(status.response_time_compliant);
    }

    // 13. Missing SLA returns None
    #[test]
    fn test_missing_sla() {
        let t = tracker();
        assert!(t.status("nonexistent").is_none());
    }

    // 14. SLA definition with window
    #[test]
    fn test_sla_with_window() {
        let sla = SlaDefinition::new("api", 99.9, 500).with_window(3600);
        assert_eq!(sla.window_seconds, 3600);
    }

    // 15. SLA definition with description
    #[test]
    fn test_sla_with_description() {
        let sla = SlaDefinition::new("api", 99.9, 500).with_description("Production API SLA");
        assert_eq!(sla.description, "Production API SLA");
    }

    // 16. All statuses
    #[test]
    fn test_all_statuses() {
        let t = tracker();
        t.register(SlaDefinition::new("a", 99.0, 500));
        t.register(SlaDefinition::new("b", 99.9, 100));

        let statuses = t.all_statuses();
        assert_eq!(statuses.len(), 2);
    }

    // 17. Clone shares state
    #[test]
    fn test_clone_shares_state() {
        let t1 = tracker();
        let t2 = t1.clone();
        t1.register(api_sla());
        assert_eq!(t2.sla_count(), 1);
    }

    // 18. Default tracker
    #[test]
    fn test_default() {
        let t = SlaTracker::default();
        assert_eq!(t.sla_count(), 0);
    }

    // 19. Status serializable
    #[test]
    fn test_status_serializable() {
        let t = tracker();
        t.register(api_sla());
        t.record_success("api", 100);
        let status = t.status("api").unwrap();
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"name\":\"api\""));
    }

    // 20. Report serializable
    #[test]
    fn test_report_serializable() {
        let t = tracker();
        t.register(api_sla());
        let report = t.compliance_report();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"total_slas\":1"));
    }

    // 21. Downtime tracking
    #[test]
    fn test_downtime_seconds() {
        let t = tracker();
        t.register(api_sla());
        t.record_failure("api", "outage");
        t.record_success("api", 100); // closes incident

        let status = t.status("api").unwrap();
        // Downtime is very small (near-instant test)
        assert!(status.total_downtime_seconds < 5);
    }

    // 22. Empty incidents for registered SLA
    #[test]
    fn test_empty_incidents() {
        let t = tracker();
        t.register(api_sla());
        assert!(t.incidents("api").is_empty());
    }
}
