//! Alert rule engine for production monitoring.
//!
//! Evaluates configurable rules against metric values and triggers alerts
//! when thresholds are breached, with cooldown support to prevent alert storms.
//!
//! # Main types
//!
//! - [`AlertEngine`] — Evaluates rules against metrics and produces alerts.
//! - [`AlertRule`] — A named rule with condition, severity, and cooldown.
//! - [`AlertCondition`] — Comparison operators for threshold evaluation.
//! - [`Alert`] — A fired alert with context.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ---------------------------------------------------------------------------
// AlertSeverity
// ---------------------------------------------------------------------------

/// Severity level for fired alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    /// Low priority — informational.
    Info,
    /// Medium priority — should be investigated.
    Warning,
    /// High priority — requires attention.
    Critical,
    /// Highest priority — immediate action required.
    Emergency,
}

// ---------------------------------------------------------------------------
// AlertCondition
// ---------------------------------------------------------------------------

/// Comparison condition for evaluating a metric against a threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    /// Metric is greater than the threshold.
    GreaterThan(f64),
    /// Metric is less than the threshold.
    LessThan(f64),
    /// Metric is greater than or equal to the threshold.
    GreaterThanOrEqual(f64),
    /// Metric is less than or equal to the threshold.
    LessThanOrEqual(f64),
    /// Metric equals the threshold (exact comparison).
    Equal(f64),
    /// Metric is outside the range [low, high].
    OutsideRange(f64, f64),
    /// Metric is inside the range [low, high].
    InsideRange(f64, f64),
    /// Rate of change exceeds threshold (per second).
    RateExceeds(f64),
}

impl AlertCondition {
    /// Evaluate the condition against a metric value.
    pub fn evaluate(&self, value: f64) -> bool {
        match self {
            Self::GreaterThan(t) => value > *t,
            Self::LessThan(t) => value < *t,
            Self::GreaterThanOrEqual(t) => value >= *t,
            Self::LessThanOrEqual(t) => value <= *t,
            Self::Equal(t) => (value - t).abs() < f64::EPSILON,
            Self::OutsideRange(low, high) => value < *low || value > *high,
            Self::InsideRange(low, high) => value >= *low && value <= *high,
            Self::RateExceeds(t) => value.abs() > *t,
        }
    }

    /// Human-readable description of the condition.
    pub fn describe(&self) -> String {
        match self {
            Self::GreaterThan(t) => format!("> {t}"),
            Self::LessThan(t) => format!("< {t}"),
            Self::GreaterThanOrEqual(t) => format!(">= {t}"),
            Self::LessThanOrEqual(t) => format!("<= {t}"),
            Self::Equal(t) => format!("== {t}"),
            Self::OutsideRange(lo, hi) => format!("outside [{lo}, {hi}]"),
            Self::InsideRange(lo, hi) => format!("inside [{lo}, {hi}]"),
            Self::RateExceeds(t) => format!("rate > {t}/s"),
        }
    }
}

// ---------------------------------------------------------------------------
// AlertRule
// ---------------------------------------------------------------------------

/// A named alert rule that evaluates a metric against a condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Unique name for this rule.
    pub name: String,
    /// The metric name this rule monitors.
    pub metric: String,
    /// The condition to evaluate.
    pub condition: AlertCondition,
    /// Alert severity when the condition is met.
    pub severity: AlertSeverity,
    /// Human-readable description.
    pub description: String,
    /// Cooldown period in seconds before the rule can fire again.
    pub cooldown_seconds: u64,
    /// Whether the rule is enabled.
    pub enabled: bool,
    /// Optional labels that must match for this rule to apply.
    pub labels: HashMap<String, String>,
}

impl AlertRule {
    /// Create a new alert rule.
    pub fn new(
        name: impl Into<String>,
        metric: impl Into<String>,
        condition: AlertCondition,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            name: name.into(),
            metric: metric.into(),
            condition,
            severity,
            description: String::new(),
            cooldown_seconds: 300,
            enabled: true,
            labels: HashMap::new(),
        }
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set the cooldown period.
    pub fn with_cooldown(mut self, seconds: u64) -> Self {
        self.cooldown_seconds = seconds;
        self
    }

    /// Add a label filter.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Alert
// ---------------------------------------------------------------------------

/// A fired alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// The rule that fired.
    pub rule_name: String,
    /// The metric that triggered the alert.
    pub metric: String,
    /// The actual metric value.
    pub value: f64,
    /// The condition description.
    pub condition: String,
    /// Severity of the alert.
    pub severity: AlertSeverity,
    /// When the alert was fired.
    pub fired_at: DateTime<Utc>,
    /// Description from the rule.
    pub description: String,
    /// Whether this alert has been acknowledged.
    pub acknowledged: bool,
}

// ---------------------------------------------------------------------------
// AlertEngine
// ---------------------------------------------------------------------------

/// Inner state for the alert engine.
struct Inner {
    rules: Vec<AlertRule>,
    /// Last fire time per rule name (for cooldown).
    last_fired: HashMap<String, DateTime<Utc>>,
    /// All fired alerts (bounded).
    alerts: Vec<Alert>,
    max_alerts: usize,
    total_evaluations: u64,
    total_fired: u64,
    total_suppressed: u64,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("rules", &self.rules.len())
            .field("alerts", &self.alerts.len())
            .finish()
    }
}

/// Thread-safe alert rule evaluation engine.
///
/// Clone is cheap (inner state is behind `Arc<RwLock>`).
#[derive(Debug, Clone)]
pub struct AlertEngine {
    inner: Arc<RwLock<Inner>>,
}

impl Default for AlertEngine {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl AlertEngine {
    /// Create a new alert engine with the given maximum alert history.
    pub fn new(max_alerts: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                rules: Vec::new(),
                last_fired: HashMap::new(),
                alerts: Vec::new(),
                max_alerts,
                total_evaluations: 0,
                total_fired: 0,
                total_suppressed: 0,
            })),
        }
    }

    /// Add an alert rule.
    pub fn add_rule(&self, rule: AlertRule) {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .rules
            .push(rule);
    }

    /// Remove a rule by name.
    pub fn remove_rule(&self, name: &str) -> bool {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = inner.rules.len();
        inner.rules.retain(|r| r.name != name);
        inner.rules.len() < before
    }

    /// Evaluate all rules against a metric value. Returns any newly fired alerts.
    pub fn evaluate(&self, metric: &str, value: f64) -> Vec<Alert> {
        let now = Utc::now();
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // First pass: collect matching rules (avoids borrow conflict)
        let mut pending: Vec<(String, String, AlertSeverity, String, u64)> = Vec::new();
        let mut evals = 0u64;
        let mut suppressed = 0u64;

        for rule in &inner.rules {
            if !rule.enabled || rule.metric != metric {
                continue;
            }
            evals += 1;

            if !rule.condition.evaluate(value) {
                continue;
            }

            // Check cooldown
            if let Some(last) = inner.last_fired.get(&rule.name) {
                let elapsed = (now - *last).num_seconds().unsigned_abs();
                if elapsed < rule.cooldown_seconds {
                    suppressed += 1;
                    continue;
                }
            }

            pending.push((
                rule.name.clone(),
                rule.condition.describe(),
                rule.severity,
                rule.description.clone(),
                rule.cooldown_seconds,
            ));
        }

        inner.total_evaluations += evals;
        inner.total_suppressed += suppressed;

        // Second pass: create alerts (now we can mutate freely)
        let mut fired = Vec::with_capacity(pending.len());
        for (name, condition, severity, description, _) in pending {
            let alert = Alert {
                rule_name: name.clone(),
                metric: metric.to_string(),
                value,
                condition,
                severity,
                fired_at: now,
                description,
                acknowledged: false,
            };

            inner.last_fired.insert(name, now);
            inner.total_fired += 1;

            if inner.alerts.len() >= inner.max_alerts {
                inner.alerts.remove(0);
            }
            inner.alerts.push(alert.clone());
            fired.push(alert);
        }

        fired
    }

    /// Evaluate multiple metrics at once.
    pub fn evaluate_batch(&self, metrics: &HashMap<String, f64>) -> Vec<Alert> {
        let mut all_alerts = Vec::new();
        for (metric, value) in metrics {
            all_alerts.extend(self.evaluate(metric, *value));
        }
        all_alerts
    }

    /// Get all fired alerts.
    pub fn alerts(&self) -> Vec<Alert> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .alerts
            .clone()
    }

    /// Get unacknowledged alerts.
    pub fn pending_alerts(&self) -> Vec<Alert> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .alerts
            .iter()
            .filter(|a| !a.acknowledged)
            .cloned()
            .collect()
    }

    /// Acknowledge an alert by rule name (marks the most recent).
    pub fn acknowledge(&self, rule_name: &str) -> bool {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for alert in inner.alerts.iter_mut().rev() {
            if alert.rule_name == rule_name && !alert.acknowledged {
                alert.acknowledged = true;
                return true;
            }
        }
        false
    }

    /// Get the number of registered rules.
    pub fn rule_count(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .rules
            .len()
    }

    /// Get engine statistics.
    pub fn stats(&self) -> AlertEngineStats {
        let inner = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        AlertEngineStats {
            rules: inner.rules.len(),
            total_evaluations: inner.total_evaluations,
            total_fired: inner.total_fired,
            total_suppressed: inner.total_suppressed,
            pending_alerts: inner.alerts.iter().filter(|a| !a.acknowledged).count(),
        }
    }

    /// Clear all alert history.
    pub fn clear_alerts(&self) {
        let mut inner = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.alerts.clear();
    }
}

/// Statistics for the alert engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEngineStats {
    /// Number of registered rules.
    pub rules: usize,
    /// Total number of rule evaluations.
    pub total_evaluations: u64,
    /// Total number of alerts fired.
    pub total_fired: u64,
    /// Total number of alerts suppressed by cooldown.
    pub total_suppressed: u64,
    /// Number of pending (unacknowledged) alerts.
    pub pending_alerts: usize,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn engine() -> AlertEngine {
        AlertEngine::new(100)
    }

    // 1. New engine is empty
    #[test]
    fn test_new_engine_empty() {
        let e = engine();
        assert_eq!(e.rule_count(), 0);
        assert!(e.alerts().is_empty());
    }

    // 2. Add and count rules
    #[test]
    fn test_add_rule() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(90.0),
            AlertSeverity::Warning,
        ));
        assert_eq!(e.rule_count(), 1);
    }

    // 3. Remove rule
    #[test]
    fn test_remove_rule() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(90.0),
            AlertSeverity::Warning,
        ));
        assert!(e.remove_rule("test"));
        assert_eq!(e.rule_count(), 0);
        assert!(!e.remove_rule("nonexistent"));
    }

    // 4. Evaluate fires alert when condition met
    #[test]
    fn test_evaluate_fires() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "high-cpu",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Critical,
        ));
        let alerts = e.evaluate("cpu", 95.0);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].rule_name, "high-cpu");
        assert_eq!(alerts[0].value, 95.0);
    }

    // 5. Evaluate does not fire when condition not met
    #[test]
    fn test_evaluate_no_fire() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "high-cpu",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        let alerts = e.evaluate("cpu", 50.0);
        assert!(alerts.is_empty());
    }

    // 6. Different metric doesn't trigger
    #[test]
    fn test_wrong_metric() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "high-cpu",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        let alerts = e.evaluate("memory", 95.0);
        assert!(alerts.is_empty());
    }

    // 7. Cooldown prevents re-firing
    #[test]
    fn test_cooldown() {
        let e = engine();
        e.add_rule(
            AlertRule::new(
                "test",
                "cpu",
                AlertCondition::GreaterThan(80.0),
                AlertSeverity::Warning,
            )
            .with_cooldown(3600),
        );
        let a1 = e.evaluate("cpu", 95.0);
        let a2 = e.evaluate("cpu", 99.0);
        assert_eq!(a1.len(), 1);
        assert!(a2.is_empty()); // suppressed by cooldown
    }

    // 8. Stats tracking
    #[test]
    fn test_stats() {
        let e = engine();
        e.add_rule(
            AlertRule::new(
                "test",
                "cpu",
                AlertCondition::GreaterThan(80.0),
                AlertSeverity::Warning,
            )
            .with_cooldown(3600),
        );
        e.evaluate("cpu", 95.0);
        e.evaluate("cpu", 99.0); // suppressed

        let stats = e.stats();
        assert_eq!(stats.rules, 1);
        assert_eq!(stats.total_evaluations, 2);
        assert_eq!(stats.total_fired, 1);
        assert_eq!(stats.total_suppressed, 1);
    }

    // 9. Acknowledge alert
    #[test]
    fn test_acknowledge() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        e.evaluate("cpu", 95.0);

        assert_eq!(e.pending_alerts().len(), 1);
        assert!(e.acknowledge("test"));
        assert_eq!(e.pending_alerts().len(), 0);
    }

    // 10. LessThan condition
    #[test]
    fn test_less_than() {
        assert!(AlertCondition::LessThan(50.0).evaluate(30.0));
        assert!(!AlertCondition::LessThan(50.0).evaluate(60.0));
    }

    // 11. OutsideRange condition
    #[test]
    fn test_outside_range() {
        let cond = AlertCondition::OutsideRange(20.0, 80.0);
        assert!(cond.evaluate(10.0));
        assert!(cond.evaluate(90.0));
        assert!(!cond.evaluate(50.0));
    }

    // 12. InsideRange condition
    #[test]
    fn test_inside_range() {
        let cond = AlertCondition::InsideRange(20.0, 80.0);
        assert!(!cond.evaluate(10.0));
        assert!(cond.evaluate(50.0));
    }

    // 13. Equal condition
    #[test]
    fn test_equal() {
        assert!(AlertCondition::Equal(42.0).evaluate(42.0));
        assert!(!AlertCondition::Equal(42.0).evaluate(43.0));
    }

    // 14. GTE and LTE conditions
    #[test]
    fn test_gte_lte() {
        assert!(AlertCondition::GreaterThanOrEqual(50.0).evaluate(50.0));
        assert!(AlertCondition::GreaterThanOrEqual(50.0).evaluate(51.0));
        assert!(!AlertCondition::GreaterThanOrEqual(50.0).evaluate(49.0));

        assert!(AlertCondition::LessThanOrEqual(50.0).evaluate(50.0));
        assert!(AlertCondition::LessThanOrEqual(50.0).evaluate(49.0));
        assert!(!AlertCondition::LessThanOrEqual(50.0).evaluate(51.0));
    }

    // 15. RateExceeds condition
    #[test]
    fn test_rate_exceeds() {
        assert!(AlertCondition::RateExceeds(10.0).evaluate(15.0));
        assert!(AlertCondition::RateExceeds(10.0).evaluate(-15.0)); // abs
        assert!(!AlertCondition::RateExceeds(10.0).evaluate(5.0));
    }

    // 16. Multiple rules on same metric
    #[test]
    fn test_multiple_rules() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "warn",
            "cpu",
            AlertCondition::GreaterThan(70.0),
            AlertSeverity::Warning,
        ));
        e.add_rule(AlertRule::new(
            "crit",
            "cpu",
            AlertCondition::GreaterThan(90.0),
            AlertSeverity::Critical,
        ));

        let alerts = e.evaluate("cpu", 95.0);
        assert_eq!(alerts.len(), 2);
    }

    // 17. Disabled rule doesn't fire
    #[test]
    fn test_disabled_rule() {
        let e = engine();
        let mut rule = AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        );
        rule.enabled = false;
        e.add_rule(rule);
        let alerts = e.evaluate("cpu", 95.0);
        assert!(alerts.is_empty());
    }

    // 18. Batch evaluation
    #[test]
    fn test_batch_evaluation() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "cpu",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        e.add_rule(AlertRule::new(
            "mem",
            "memory",
            AlertCondition::GreaterThan(90.0),
            AlertSeverity::Critical,
        ));

        let mut metrics = HashMap::new();
        metrics.insert("cpu".to_string(), 95.0);
        metrics.insert("memory".to_string(), 50.0);

        let alerts = e.evaluate_batch(&metrics);
        assert_eq!(alerts.len(), 1); // only cpu fires
    }

    // 19. Clear alerts
    #[test]
    fn test_clear_alerts() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        e.evaluate("cpu", 95.0);
        assert!(!e.alerts().is_empty());

        e.clear_alerts();
        assert!(e.alerts().is_empty());
    }

    // 20. Alert serializable
    #[test]
    fn test_alert_serializable() {
        let e = engine();
        e.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Critical,
        ));
        e.evaluate("cpu", 95.0);

        let alerts = e.alerts();
        let json = serde_json::to_string(&alerts[0]).unwrap();
        assert!(json.contains("\"rule_name\":\"test\""));
    }

    // 21. Condition describe
    #[test]
    fn test_condition_describe() {
        assert_eq!(AlertCondition::GreaterThan(90.0).describe(), "> 90");
        assert_eq!(
            AlertCondition::OutsideRange(10.0, 90.0).describe(),
            "outside [10, 90]"
        );
    }

    // 22. Rule with description
    #[test]
    fn test_rule_with_description() {
        let e = engine();
        e.add_rule(
            AlertRule::new(
                "test",
                "cpu",
                AlertCondition::GreaterThan(80.0),
                AlertSeverity::Warning,
            )
            .with_description("CPU too high"),
        );
        e.evaluate("cpu", 95.0);
        let alerts = e.alerts();
        assert_eq!(alerts[0].description, "CPU too high");
    }

    // 23. Default engine
    #[test]
    fn test_default() {
        let e = AlertEngine::default();
        assert_eq!(e.rule_count(), 0);
    }

    // 24. Clone shares state
    #[test]
    fn test_clone_shares_state() {
        let e1 = engine();
        let e2 = e1.clone();
        e1.add_rule(AlertRule::new(
            "test",
            "cpu",
            AlertCondition::GreaterThan(80.0),
            AlertSeverity::Warning,
        ));
        assert_eq!(e2.rule_count(), 1);
    }
}
