//! Health check system for monitoring deployed agents.
//!
//! Provides liveness/readiness probes, heartbeat tracking, and auto-recovery
//! support for agents managed by the orchestrator.

use crate::types::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the health check system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Expected heartbeat frequency in seconds.
    pub heartbeat_interval_secs: u64,
    /// Agent is considered unhealthy after this many seconds without a heartbeat.
    pub heartbeat_timeout_secs: u64,
    /// How often liveness checks are executed (seconds).
    pub liveness_check_interval_secs: u64,
    /// Consecutive probe failures before marking the agent as dead.
    pub max_consecutive_failures: u32,
    /// Whether to automatically restart unhealthy agents.
    pub auto_restart_enabled: bool,
    /// Delay (seconds) before triggering an auto-restart.
    pub auto_restart_delay_secs: u64,
    /// Maximum number of auto-restarts before giving up.
    pub max_auto_restarts: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 90,
            liveness_check_interval_secs: 15,
            max_consecutive_failures: 3,
            auto_restart_enabled: true,
            auto_restart_delay_secs: 5,
            max_auto_restarts: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Status & Probe types
// ---------------------------------------------------------------------------

/// Overall health status of an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HealthStatus {
    /// All checks passing.
    Healthy,
    /// Some checks failing but the agent is still operational.
    Degraded {
        /// Description of what is degraded.
        reason: String,
    },
    /// Critical checks failing.
    Unhealthy {
        /// Description of the critical failure.
        reason: String,
    },
    /// Exceeded max failures; requires manual intervention.
    Dead {
        /// Description of the fatal condition.
        reason: String,
    },
    /// No data yet.
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded { reason } => write!(f, "degraded: {reason}"),
            HealthStatus::Unhealthy { reason } => write!(f, "unhealthy: {reason}"),
            HealthStatus::Dead { reason } => write!(f, "dead: {reason}"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// The kind of probe used to evaluate an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeType {
    /// Is the agent process alive?
    Liveness,
    /// Is the agent ready to accept tasks?
    Readiness,
    /// Has the agent sent a heartbeat recently?
    Heartbeat,
}

/// A single health probe attached to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthProbe {
    /// Human-readable probe name (e.g., "liveness", "readiness").
    pub name: String,
    /// Category of probe (liveness, readiness, heartbeat).
    pub probe_type: ProbeType,
    /// UTC timestamp of the last probe execution.
    pub last_check: Option<DateTime<Utc>>,
    /// UTC timestamp of the last successful probe.
    pub last_success: Option<DateTime<Utc>>,
    /// Number of failures in a row (resets on success).
    pub consecutive_failures: u32,
    /// Total number of probe executions.
    pub total_checks: u64,
    /// Total number of failed probe executions.
    pub total_failures: u64,
}

impl HealthProbe {
    fn new(name: impl Into<String>, probe_type: ProbeType) -> Self {
        Self {
            name: name.into(),
            probe_type,
            last_check: None,
            last_success: None,
            consecutive_failures: 0,
            total_checks: 0,
            total_failures: 0,
        }
    }

    fn record_success(&mut self) {
        let now = Utc::now();
        self.last_check = Some(now);
        self.last_success = Some(now);
        self.consecutive_failures = 0;
        self.total_checks += 1;
    }

    fn record_failure(&mut self) {
        self.last_check = Some(Utc::now());
        self.consecutive_failures += 1;
        self.total_checks += 1;
        self.total_failures += 1;
    }
}

// ---------------------------------------------------------------------------
// Agent health state
// ---------------------------------------------------------------------------

/// Complete health state tracked for a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealthState {
    /// Unique identifier of the agent.
    pub agent_id: Uuid,
    /// Human-readable agent name.
    pub agent_name: String,
    /// Role this agent fulfills.
    pub role: AgentRole,
    /// Current aggregated health status.
    pub status: HealthStatus,
    /// Individual health probes (liveness, readiness, heartbeat).
    pub probes: Vec<HealthProbe>,
    /// UTC timestamp of the last received heartbeat.
    pub last_heartbeat: Option<DateTime<Utc>>,
    /// Number of times this agent has been restarted.
    pub restart_count: u32,
    /// Seconds since the agent was last (re)started.
    pub uptime_secs: u64,
    /// UTC timestamp of when the agent was last (re)started.
    pub started_at: DateTime<Utc>,
}

impl AgentHealthState {
    fn new(agent_id: Uuid, name: String, role: AgentRole) -> Self {
        let now = Utc::now();
        Self {
            agent_id,
            agent_name: name,
            role,
            status: HealthStatus::Unknown,
            probes: vec![
                HealthProbe::new("liveness", ProbeType::Liveness),
                HealthProbe::new("readiness", ProbeType::Readiness),
                HealthProbe::new("heartbeat", ProbeType::Heartbeat),
            ],
            last_heartbeat: None,
            restart_count: 0,
            uptime_secs: 0,
            started_at: now,
        }
    }

    /// Recalculate `uptime_secs` based on `started_at`.
    fn refresh_uptime(&mut self) {
        let elapsed = Utc::now().signed_duration_since(self.started_at);
        self.uptime_secs = elapsed.num_seconds().max(0) as u64;
    }

    fn probe_mut(&mut self, pt: &ProbeType) -> Option<&mut HealthProbe> {
        self.probes.iter_mut().find(|p| p.probe_type == *pt)
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// Events emitted by the health check system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HealthEvent {
    /// Agent transitioned to healthy status.
    AgentBecameHealthy {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
    },
    /// Agent transitioned to degraded status.
    AgentBecameDegraded {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
        /// Description of the degradation.
        reason: String,
    },
    /// Agent transitioned to unhealthy status.
    AgentBecameUnhealthy {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
        /// Description of the failure.
        reason: String,
    },
    /// Agent exceeded max consecutive failures and is now dead.
    AgentDied {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
        /// Description of the terminal failure.
        reason: String,
    },
    /// Agent was automatically or manually restarted.
    AgentRestarted {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
        /// Cumulative restart count.
        restart_count: u32,
    },
    /// Agent did not send a heartbeat within the configured timeout.
    HeartbeatMissed {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Human-readable agent name.
        agent_name: String,
        /// Seconds since the last heartbeat was received.
        last_seen_secs_ago: u64,
    },
    /// A specific probe failed for an agent.
    ProbeFailure {
        /// Unique agent identifier.
        agent_id: Uuid,
        /// Name of the probe that failed.
        probe_name: String,
        /// Error message from the probe.
        error: String,
    },
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

/// Aggregate health summary across all tracked agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    /// Total number of agents being monitored.
    pub total_agents: usize,
    /// Agents in healthy status.
    pub healthy: usize,
    /// Agents in degraded status.
    pub degraded: usize,
    /// Agents in unhealthy status.
    pub unhealthy: usize,
    /// Agents in dead status.
    pub dead: usize,
    /// Agents in unknown status.
    pub unknown: usize,
    /// Sum of restarts across all agents.
    pub total_restarts: u32,
    /// Events generated during this check cycle.
    pub events: Vec<HealthEvent>,
    /// UTC timestamp of when this summary was produced.
    pub checked_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// HealthChecker
// ---------------------------------------------------------------------------

/// Central health check system for monitoring deployed agents.
///
/// Uses `Arc<RwLock<…>>` so it can be shared across async tasks.
pub struct HealthChecker {
    config: HealthCheckConfig,
    agents: Arc<RwLock<HashMap<Uuid, AgentHealthState>>>,
}

impl HealthChecker {
    /// Create a new `HealthChecker` with the given configuration.
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start tracking health for a new agent.
    pub async fn register_agent(&self, agent_id: Uuid, name: String, role: AgentRole) {
        let state = AgentHealthState::new(agent_id, name, role);
        let mut agents = self.agents.write().await;
        agents.insert(agent_id, state);
    }

    /// Stop tracking an agent.
    pub async fn unregister_agent(&self, agent_id: Uuid) {
        let mut agents = self.agents.write().await;
        agents.remove(&agent_id);
    }

    /// Record that an agent sent a heartbeat.
    pub async fn record_heartbeat(&self, agent_id: Uuid) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        let state = agents
            .get_mut(&agent_id)
            .ok_or_else(|| format!("agent {agent_id} not registered"))?;
        state.last_heartbeat = Some(Utc::now());
        if let Some(probe) = state.probe_mut(&ProbeType::Heartbeat) {
            probe.record_success();
        }
        Ok(())
    }

    /// Record a successful liveness probe.
    pub async fn record_liveness_success(&self, agent_id: Uuid) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        let state = agents
            .get_mut(&agent_id)
            .ok_or_else(|| format!("agent {agent_id} not registered"))?;
        if let Some(probe) = state.probe_mut(&ProbeType::Liveness) {
            probe.record_success();
        }
        Ok(())
    }

    /// Record a failed liveness probe.
    pub async fn record_liveness_failure(&self, agent_id: Uuid, error: &str) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        let state = agents
            .get_mut(&agent_id)
            .ok_or_else(|| format!("agent {agent_id} not registered"))?;
        if let Some(probe) = state.probe_mut(&ProbeType::Liveness) {
            probe.record_failure();
        }
        // Store the error context inside the status if the agent was previously healthy.
        if state.status == HealthStatus::Healthy || state.status == HealthStatus::Unknown {
            state.status = HealthStatus::Degraded {
                reason: error.to_string(),
            };
        }
        Ok(())
    }

    /// Record a readiness probe result.
    pub async fn record_readiness(&self, agent_id: Uuid, ready: bool) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        let state = agents
            .get_mut(&agent_id)
            .ok_or_else(|| format!("agent {agent_id} not registered"))?;
        if let Some(probe) = state.probe_mut(&ProbeType::Readiness) {
            if ready {
                probe.record_success();
            } else {
                probe.record_failure();
            }
        }
        Ok(())
    }

    /// Increment the restart counter for an agent and reset its `started_at`.
    pub async fn record_restart(&self, agent_id: Uuid) -> Result<(), String> {
        let mut agents = self.agents.write().await;
        let state = agents
            .get_mut(&agent_id)
            .ok_or_else(|| format!("agent {agent_id} not registered"))?;
        state.restart_count += 1;
        state.started_at = Utc::now();
        state.uptime_secs = 0;
        // Reset probes on restart.
        for probe in &mut state.probes {
            probe.consecutive_failures = 0;
            probe.last_check = None;
            probe.last_success = None;
        }
        state.status = HealthStatus::Unknown;
        Ok(())
    }

    /// Get the health state of a single agent.
    pub async fn get_health(&self, agent_id: Uuid) -> Option<AgentHealthState> {
        let mut agents = self.agents.write().await;
        if let Some(state) = agents.get_mut(&agent_id) {
            state.refresh_uptime();
            Some(state.clone())
        } else {
            None
        }
    }

    /// Get the health state of all tracked agents.
    pub async fn get_all_health(&self) -> Vec<AgentHealthState> {
        let mut agents = self.agents.write().await;
        agents
            .values_mut()
            .map(|s| {
                s.refresh_uptime();
                s.clone()
            })
            .collect()
    }

    /// Run health evaluation on all agents and return generated events.
    ///
    /// For each registered agent the method:
    /// 1. Checks whether the heartbeat is stale.
    /// 2. Checks consecutive liveness failures against the configured maximum.
    /// 3. Transitions the status through Healthy -> Degraded -> Unhealthy -> Dead.
    /// 4. Generates [`HealthEvent`]s for every transition.
    pub async fn check_all(&self) -> Vec<HealthEvent> {
        let mut events = Vec::new();
        let now = Utc::now();
        let mut agents = self.agents.write().await;

        for state in agents.values_mut() {
            state.refresh_uptime();
            let previous_status = state.status.clone();

            // --- heartbeat staleness -------------------------------------------
            let heartbeat_stale = if let Some(last) = state.last_heartbeat {
                let elapsed = now.signed_duration_since(last).num_seconds().max(0) as u64;
                if elapsed >= self.config.heartbeat_timeout_secs {
                    // Record heartbeat probe failure.
                    if let Some(probe) = state.probe_mut(&ProbeType::Heartbeat) {
                        probe.record_failure();
                    }
                    events.push(HealthEvent::HeartbeatMissed {
                        agent_id: state.agent_id,
                        agent_name: state.agent_name.clone(),
                        last_seen_secs_ago: elapsed,
                    });
                    true
                } else {
                    false
                }
            } else {
                // No heartbeat has ever been received; not necessarily stale if
                // the agent was just registered (Unknown status).
                false
            };

            // --- liveness failures ---------------------------------------------
            let liveness_failures = state
                .probes
                .iter()
                .find(|p| p.probe_type == ProbeType::Liveness)
                .map_or(0, |p| p.consecutive_failures);

            // --- determine new status ------------------------------------------
            let new_status = if liveness_failures >= self.config.max_consecutive_failures {
                HealthStatus::Dead {
                    reason: format!("liveness probe failed {liveness_failures} consecutive times"),
                }
            } else if heartbeat_stale && liveness_failures > 0 {
                HealthStatus::Unhealthy {
                    reason: "heartbeat stale and liveness probe failing".to_string(),
                }
            } else if heartbeat_stale {
                HealthStatus::Unhealthy {
                    reason: "heartbeat timeout exceeded".to_string(),
                }
            } else if liveness_failures > 0 {
                HealthStatus::Degraded {
                    reason: format!("liveness probe failed {liveness_failures} time(s)"),
                }
            } else {
                // Check readiness probe — if it is failing, mark degraded.
                let readiness_failures = state
                    .probes
                    .iter()
                    .find(|p| p.probe_type == ProbeType::Readiness)
                    .map_or(0, |p| p.consecutive_failures);
                if readiness_failures > 0 {
                    HealthStatus::Degraded {
                        reason: format!("readiness probe failed {readiness_failures} time(s)"),
                    }
                } else if state.last_heartbeat.is_some() || previous_status == HealthStatus::Healthy
                {
                    HealthStatus::Healthy
                } else {
                    // Keep Unknown if we have never received any signal.
                    previous_status.clone()
                }
            };

            state.status = new_status.clone();

            // --- emit transition events ----------------------------------------
            if new_status != previous_status {
                match &new_status {
                    HealthStatus::Healthy => {
                        events.push(HealthEvent::AgentBecameHealthy {
                            agent_id: state.agent_id,
                            agent_name: state.agent_name.clone(),
                        });
                    }
                    HealthStatus::Degraded { reason } => {
                        events.push(HealthEvent::AgentBecameDegraded {
                            agent_id: state.agent_id,
                            agent_name: state.agent_name.clone(),
                            reason: reason.clone(),
                        });
                    }
                    HealthStatus::Unhealthy { reason } => {
                        events.push(HealthEvent::AgentBecameUnhealthy {
                            agent_id: state.agent_id,
                            agent_name: state.agent_name.clone(),
                            reason: reason.clone(),
                        });
                    }
                    HealthStatus::Dead { reason } => {
                        events.push(HealthEvent::AgentDied {
                            agent_id: state.agent_id,
                            agent_name: state.agent_name.clone(),
                            reason: reason.clone(),
                        });
                    }
                    HealthStatus::Unknown => {}
                }
            }

            // --- probe-level failure events ------------------------------------
            for probe in &state.probes {
                if probe.consecutive_failures > 0 {
                    if let Some(last_check) = probe.last_check {
                        // Only emit if the failure happened in the current check window.
                        let since = now.signed_duration_since(last_check).num_seconds().abs();
                        if since < self.config.liveness_check_interval_secs as i64 {
                            events.push(HealthEvent::ProbeFailure {
                                agent_id: state.agent_id,
                                probe_name: probe.name.clone(),
                                error: format!(
                                    "{} consecutive failures",
                                    probe.consecutive_failures
                                ),
                            });
                        }
                    }
                }
            }
        }

        events
    }

    /// Get agents that are unhealthy, degraded, or dead.
    pub async fn get_unhealthy(&self) -> Vec<AgentHealthState> {
        let mut agents = self.agents.write().await;
        agents
            .values_mut()
            .filter(|s| {
                matches!(
                    s.status,
                    HealthStatus::Degraded { .. }
                        | HealthStatus::Unhealthy { .. }
                        | HealthStatus::Dead { .. }
                )
            })
            .map(|s| {
                s.refresh_uptime();
                s.clone()
            })
            .collect()
    }

    /// Build an aggregate health summary.
    pub async fn get_summary(&self) -> HealthSummary {
        let events = self.check_all().await;
        let agents = self.agents.read().await;

        let mut summary = HealthSummary {
            total_agents: agents.len(),
            healthy: 0,
            degraded: 0,
            unhealthy: 0,
            dead: 0,
            unknown: 0,
            total_restarts: 0,
            events,
            checked_at: Utc::now(),
        };

        for state in agents.values() {
            match &state.status {
                HealthStatus::Healthy => summary.healthy += 1,
                HealthStatus::Degraded { .. } => summary.degraded += 1,
                HealthStatus::Unhealthy { .. } => summary.unhealthy += 1,
                HealthStatus::Dead { .. } => summary.dead += 1,
                HealthStatus::Unknown => summary.unknown += 1,
            }
            summary.total_restarts += state.restart_count;
        }

        summary
    }

    /// Whether the given agent should be auto-restarted based on the current
    /// configuration and its state.
    pub async fn should_restart(&self, agent_id: Uuid) -> bool {
        if !self.config.auto_restart_enabled {
            return false;
        }
        let agents = self.agents.read().await;
        let Some(state) = agents.get(&agent_id) else {
            return false;
        };
        if state.restart_count >= self.config.max_auto_restarts {
            return false;
        }
        matches!(
            state.status,
            HealthStatus::Unhealthy { .. } | HealthStatus::Dead { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn default_checker() -> HealthChecker {
        HealthChecker::new(HealthCheckConfig::default())
    }

    // 1. Register and get health
    #[tokio::test]
    async fn test_register_and_get_health() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-1".into(), AgentRole::Coder)
            .await;

        let health = checker.get_health(id).await.unwrap();
        assert_eq!(health.agent_id, id);
        assert_eq!(health.agent_name, "agent-1");
        assert_eq!(health.role, AgentRole::Coder);
        assert_eq!(health.status, HealthStatus::Unknown);
        assert_eq!(health.restart_count, 0);
    }

    // 2. Heartbeat updates timestamp
    #[tokio::test]
    async fn test_heartbeat_updates_timestamp() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-hb".into(), AgentRole::Tester)
            .await;

        assert!(checker
            .get_health(id)
            .await
            .unwrap()
            .last_heartbeat
            .is_none());
        checker.record_heartbeat(id).await.unwrap();
        let health = checker.get_health(id).await.unwrap();
        assert!(health.last_heartbeat.is_some());
    }

    // 3. Missed heartbeat generates event
    #[tokio::test]
    async fn test_missed_heartbeat_generates_event() {
        let checker = HealthChecker::new(HealthCheckConfig {
            heartbeat_timeout_secs: 0, // immediate timeout
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-miss".into(), AgentRole::Spec)
            .await;
        checker.record_heartbeat(id).await.unwrap();

        // After recording the heartbeat with a 0-second timeout, any check
        // should detect the stale heartbeat.
        let events = checker.check_all().await;
        let missed = events
            .iter()
            .any(|e| matches!(e, HealthEvent::HeartbeatMissed { agent_id, .. } if *agent_id == id));
        assert!(missed, "expected HeartbeatMissed event");
    }

    // 4. Liveness failure increments counter
    #[tokio::test]
    async fn test_liveness_failure_increments_counter() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-lf".into(), AgentRole::Coder)
            .await;

        checker
            .record_liveness_failure(id, "timeout")
            .await
            .unwrap();
        let health = checker.get_health(id).await.unwrap();
        let liveness = health
            .probes
            .iter()
            .find(|p| p.probe_type == ProbeType::Liveness)
            .unwrap();
        assert_eq!(liveness.consecutive_failures, 1);
        assert_eq!(liveness.total_failures, 1);
    }

    // 5. Consecutive failures transition to unhealthy
    #[tokio::test]
    async fn test_consecutive_failures_transition_to_unhealthy() {
        let checker = HealthChecker::new(HealthCheckConfig {
            heartbeat_timeout_secs: 0,
            max_consecutive_failures: 3,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-uh".into(), AgentRole::Reviewer)
            .await;
        checker.record_heartbeat(id).await.unwrap();

        // Two liveness failures -> should not be dead yet.
        checker.record_liveness_failure(id, "err1").await.unwrap();
        checker.record_liveness_failure(id, "err2").await.unwrap();
        let events = checker.check_all().await;
        let became_unhealthy = events.iter().any(
            |e| matches!(e, HealthEvent::AgentBecameUnhealthy { agent_id, .. } if *agent_id == id),
        );
        assert!(
            became_unhealthy,
            "expected AgentBecameUnhealthy (heartbeat stale + liveness failures)"
        );
    }

    // 6. Max failures transition to dead
    #[tokio::test]
    async fn test_max_failures_transition_to_dead() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_consecutive_failures: 2,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-dead".into(), AgentRole::DevOps)
            .await;

        for _ in 0..2 {
            checker.record_liveness_failure(id, "crash").await.unwrap();
        }

        let events = checker.check_all().await;
        let died = events
            .iter()
            .any(|e| matches!(e, HealthEvent::AgentDied { agent_id, .. } if *agent_id == id));
        assert!(died, "expected AgentDied event");

        let health = checker.get_health(id).await.unwrap();
        assert!(matches!(health.status, HealthStatus::Dead { .. }));
    }

    // 7. Record restart increments counter
    #[tokio::test]
    async fn test_record_restart_increments_counter() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-rs".into(), AgentRole::Coder)
            .await;

        checker.record_restart(id).await.unwrap();
        let health = checker.get_health(id).await.unwrap();
        assert_eq!(health.restart_count, 1);
        assert_eq!(health.status, HealthStatus::Unknown); // reset on restart

        checker.record_restart(id).await.unwrap();
        let health = checker.get_health(id).await.unwrap();
        assert_eq!(health.restart_count, 2);
    }

    // 8. Should_restart returns true when conditions met
    #[tokio::test]
    async fn test_should_restart_true() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-sr".into(), AgentRole::Tester)
            .await;

        // Make the agent unhealthy.
        {
            let mut agents = checker.agents.write().await;
            let state = agents.get_mut(&id).unwrap();
            state.status = HealthStatus::Unhealthy {
                reason: "test".into(),
            };
        }

        assert!(checker.should_restart(id).await);
    }

    // 9. Should_restart returns false when max restarts exceeded
    #[tokio::test]
    async fn test_should_restart_false_max_exceeded() {
        let checker = HealthChecker::new(HealthCheckConfig {
            max_auto_restarts: 2,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "agent-nsr".into(), AgentRole::Coder)
            .await;

        // Exhaust restarts.
        checker.record_restart(id).await.unwrap();
        checker.record_restart(id).await.unwrap();

        // Make unhealthy.
        {
            let mut agents = checker.agents.write().await;
            let state = agents.get_mut(&id).unwrap();
            state.status = HealthStatus::Unhealthy {
                reason: "stuck".into(),
            };
        }

        assert!(!checker.should_restart(id).await);
    }

    // 10. Get unhealthy filters correctly
    #[tokio::test]
    async fn test_get_unhealthy_filters() {
        let checker = default_checker();
        let healthy_id = Uuid::new_v4();
        let sick_id = Uuid::new_v4();

        checker
            .register_agent(healthy_id, "healthy".into(), AgentRole::Coder)
            .await;
        checker
            .register_agent(sick_id, "sick".into(), AgentRole::Tester)
            .await;

        // Make one healthy via heartbeat, run check.
        checker.record_heartbeat(healthy_id).await.unwrap();
        checker.record_liveness_success(healthy_id).await.unwrap();
        checker
            .record_liveness_failure(sick_id, "down")
            .await
            .unwrap();
        // Trigger status evaluation.
        checker.check_all().await;

        let unhealthy = checker.get_unhealthy().await;
        assert_eq!(unhealthy.len(), 1);
        assert_eq!(unhealthy[0].agent_id, sick_id);
    }

    // 11. Summary aggregates counts
    #[tokio::test]
    async fn test_summary_aggregates_counts() {
        let checker = default_checker();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        checker
            .register_agent(id1, "a1".into(), AgentRole::Coder)
            .await;
        checker
            .register_agent(id2, "a2".into(), AgentRole::Tester)
            .await;

        checker.record_heartbeat(id1).await.unwrap();
        checker.record_liveness_success(id1).await.unwrap();
        checker.record_restart(id2).await.unwrap();

        let summary = checker.get_summary().await;
        assert_eq!(summary.total_agents, 2);
        assert_eq!(summary.total_restarts, 1);
    }

    // 12. Check_all detects stale heartbeats
    #[tokio::test]
    async fn test_check_all_detects_stale_heartbeats() {
        let checker = HealthChecker::new(HealthCheckConfig {
            heartbeat_timeout_secs: 0,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "stale".into(), AgentRole::Architect)
            .await;
        checker.record_heartbeat(id).await.unwrap();

        let events = checker.check_all().await;
        let has_missed = events
            .iter()
            .any(|e| matches!(e, HealthEvent::HeartbeatMissed { .. }));
        assert!(has_missed);
    }

    // 13. Check_all generates HeartbeatMissed event
    #[tokio::test]
    async fn test_check_all_heartbeat_missed_event_fields() {
        let checker = HealthChecker::new(HealthCheckConfig {
            heartbeat_timeout_secs: 0,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "hb-miss".into(), AgentRole::Spec)
            .await;
        checker.record_heartbeat(id).await.unwrap();

        let events = checker.check_all().await;
        let missed_event = events.iter().find(
            |e| matches!(e, HealthEvent::HeartbeatMissed { agent_id, .. } if *agent_id == id),
        );
        assert!(missed_event.is_some());
        if let Some(HealthEvent::HeartbeatMissed {
            agent_name,
            last_seen_secs_ago,
            ..
        }) = missed_event
        {
            assert_eq!(agent_name, "hb-miss");
            // With timeout=0 the last_seen_secs_ago should be >= 0.
            assert!(*last_seen_secs_ago <= 5);
        }
    }

    // 14. Check_all generates AgentBecameUnhealthy event
    #[tokio::test]
    async fn test_check_all_agent_became_unhealthy_event() {
        let checker = HealthChecker::new(HealthCheckConfig {
            heartbeat_timeout_secs: 0,
            max_consecutive_failures: 5, // high so liveness alone won't trigger dead
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "will-degrade".into(), AgentRole::SecurityAuditor)
            .await;
        // Give it a heartbeat so it's not Unknown, then let it go stale.
        checker.record_heartbeat(id).await.unwrap();

        let events = checker.check_all().await;
        let became_unhealthy = events.iter().any(
            |e| matches!(e, HealthEvent::AgentBecameUnhealthy { agent_id, .. } if *agent_id == id),
        );
        assert!(
            became_unhealthy,
            "expected AgentBecameUnhealthy due to stale heartbeat"
        );
    }

    // 15. Readiness probe tracking
    #[tokio::test]
    async fn test_readiness_probe_tracking() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "ready-test".into(), AgentRole::DocumentWriter)
            .await;

        checker.record_readiness(id, true).await.unwrap();
        let health = checker.get_health(id).await.unwrap();
        let readiness = health
            .probes
            .iter()
            .find(|p| p.probe_type == ProbeType::Readiness)
            .unwrap();
        assert_eq!(readiness.consecutive_failures, 0);
        assert_eq!(readiness.total_checks, 1);

        checker.record_readiness(id, false).await.unwrap();
        let health = checker.get_health(id).await.unwrap();
        let readiness = health
            .probes
            .iter()
            .find(|p| p.probe_type == ProbeType::Readiness)
            .unwrap();
        assert_eq!(readiness.consecutive_failures, 1);
        assert_eq!(readiness.total_failures, 1);
        assert_eq!(readiness.total_checks, 2);
    }

    // 16. Unregister removes agent
    #[tokio::test]
    async fn test_unregister_removes_agent() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "ephemeral".into(), AgentRole::Coder)
            .await;
        assert!(checker.get_health(id).await.is_some());

        checker.unregister_agent(id).await;
        assert!(checker.get_health(id).await.is_none());
    }

    // 17. Default config values
    #[test]
    fn test_default_config_values() {
        let cfg = HealthCheckConfig::default();
        assert_eq!(cfg.heartbeat_interval_secs, 30);
        assert_eq!(cfg.heartbeat_timeout_secs, 90);
        assert_eq!(cfg.liveness_check_interval_secs, 15);
        assert_eq!(cfg.max_consecutive_failures, 3);
        assert!(cfg.auto_restart_enabled);
        assert_eq!(cfg.auto_restart_delay_secs, 5);
        assert_eq!(cfg.max_auto_restarts, 5);
    }

    // 18. HealthStatus serialize/deserialize
    #[test]
    fn test_health_status_serde() {
        let statuses = vec![
            HealthStatus::Healthy,
            HealthStatus::Degraded {
                reason: "slow".into(),
            },
            HealthStatus::Unhealthy {
                reason: "crash".into(),
            },
            HealthStatus::Dead {
                reason: "oom".into(),
            },
            HealthStatus::Unknown,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let parsed: HealthStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, status);
        }
    }

    // 19. Heartbeat on unregistered agent returns error
    #[tokio::test]
    async fn test_heartbeat_unregistered_agent_error() {
        let checker = default_checker();
        let result = checker.record_heartbeat(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    // 20. Should_restart false when auto_restart_enabled is false
    #[tokio::test]
    async fn test_should_restart_disabled() {
        let checker = HealthChecker::new(HealthCheckConfig {
            auto_restart_enabled: false,
            ..HealthCheckConfig::default()
        });
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "no-restart".into(), AgentRole::Coder)
            .await;

        // Make unhealthy.
        {
            let mut agents = checker.agents.write().await;
            let state = agents.get_mut(&id).unwrap();
            state.status = HealthStatus::Unhealthy {
                reason: "err".into(),
            };
        }

        assert!(!checker.should_restart(id).await);
    }

    // 21. Liveness success resets consecutive failures
    #[tokio::test]
    async fn test_liveness_success_resets_failures() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "recover".into(), AgentRole::Coder)
            .await;

        checker.record_liveness_failure(id, "blip").await.unwrap();
        checker.record_liveness_success(id).await.unwrap();

        let health = checker.get_health(id).await.unwrap();
        let liveness = health
            .probes
            .iter()
            .find(|p| p.probe_type == ProbeType::Liveness)
            .unwrap();
        assert_eq!(liveness.consecutive_failures, 0);
        assert_eq!(liveness.total_checks, 2);
        assert_eq!(liveness.total_failures, 1);
    }

    // 22. Get all health returns every registered agent
    #[tokio::test]
    async fn test_get_all_health() {
        let checker = default_checker();
        for i in 0..4 {
            checker
                .register_agent(Uuid::new_v4(), format!("a{i}"), AgentRole::Coder)
                .await;
        }
        let all = checker.get_all_health().await;
        assert_eq!(all.len(), 4);
    }

    // 23. Record restart resets probes
    #[tokio::test]
    async fn test_restart_resets_probes() {
        let checker = default_checker();
        let id = Uuid::new_v4();
        checker
            .register_agent(id, "rst".into(), AgentRole::Tester)
            .await;

        checker.record_liveness_failure(id, "err").await.unwrap();
        checker.record_restart(id).await.unwrap();

        let health = checker.get_health(id).await.unwrap();
        for probe in &health.probes {
            assert_eq!(probe.consecutive_failures, 0);
            assert!(probe.last_check.is_none());
        }
    }
}
