//! Agent version management for deploying, testing, and rolling back agent configurations.
//!
//! Provides [`AgentVersionManager`] which manages the full lifecycle of versioned
//! agent configurations: deploying new versions, rolling back to previous ones,
//! A/B traffic splitting between versions, and maintaining a complete audit trail
//! of all deployment actions.
//!
//! # Main types
//!
//! - [`AgentVersionManager`] — Thread-safe manager for versioned agent configurations.
//! - [`AgentVersionConfig`] — A versioned snapshot of an agent's configuration.
//! - [`VersionStatus`] — Lifecycle states for a version (Active, Inactive, Testing, Deprecated).
//! - [`TrafficSplit`] — A/B routing configuration between primary and canary versions.
//! - [`DeploymentHistory`] — Audit trail of all deployment actions.

use argentor_core::{ArgentorError, ArgentorResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// VersionStatus
// ---------------------------------------------------------------------------

/// Lifecycle state of an agent version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    /// Currently serving production traffic.
    Active,
    /// Deployed but not serving traffic.
    Inactive,
    /// Receiving test traffic only.
    Testing,
    /// Marked for removal; will not serve traffic.
    Deprecated,
}

impl std::fmt::Display for VersionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Testing => write!(f, "testing"),
            Self::Deprecated => write!(f, "deprecated"),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentVersionConfig
// ---------------------------------------------------------------------------

/// A versioned snapshot of an agent's configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVersionConfig {
    /// Identifier of the agent this version belongs to.
    pub agent_id: String,
    /// Monotonically increasing version number.
    pub version: u32,
    /// LLM model identifier (e.g. "claude-sonnet-4-20250514").
    pub model_id: String,
    /// System prompt sent to the model.
    pub system_prompt: String,
    /// Sampling temperature.
    pub temperature: f32,
    /// Maximum tokens the model may generate.
    pub max_tokens: u32,
    /// Names of skills/tools this version is allowed to use.
    pub tools: Vec<String>,
    /// Guardrail rules enabled for this version.
    pub guardrails: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
    /// When this version was created.
    pub created_at: DateTime<Utc>,
    /// Who created this version.
    pub created_by: String,
    /// Current lifecycle status.
    pub status: VersionStatus,
    /// Human-readable description of what changed in this version.
    pub change_log: String,
}

// ---------------------------------------------------------------------------
// TrafficSplit
// ---------------------------------------------------------------------------

/// A/B traffic routing configuration between a primary and optional canary version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSplit {
    /// Version receiving the majority of traffic.
    pub primary_version: u32,
    /// Weight assigned to the primary version (0.0..=1.0).
    pub primary_weight: f32,
    /// Optional canary version receiving the remaining traffic.
    pub canary_version: Option<u32>,
    /// Weight assigned to the canary version.
    pub canary_weight: f32,
}

impl TrafficSplit {
    /// Resolve which version should handle a request using weighted random selection.
    ///
    /// Returns the primary version if there is no canary, or picks between
    /// primary and canary based on their respective weights.
    pub fn resolve(&self) -> u32 {
        match self.canary_version {
            None => self.primary_version,
            Some(canary) => {
                // Use a simple deterministic threshold based on a pseudo-random value.
                // In production this would use a proper RNG; here we use a fast hash
                // of the current timestamp's nanosecond component for simplicity.
                let nanos = Utc::now().timestamp_subsec_nanos();
                let rand_val = (nanos % 1000) as f32 / 1000.0;
                if rand_val < self.primary_weight {
                    self.primary_version
                } else {
                    canary
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FieldChange / VersionDiff
// ---------------------------------------------------------------------------

/// A single field-level change between two versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    /// Name of the field that changed.
    pub field: String,
    /// Previous value (serialized as string).
    pub old_value: String,
    /// New value (serialized as string).
    pub new_value: String,
}

/// Diff between two versions of an agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    /// Version being diffed from.
    pub from_version: u32,
    /// Version being diffed to.
    pub to_version: u32,
    /// Individual field-level changes.
    pub changes: Vec<FieldChange>,
}

// ---------------------------------------------------------------------------
// DeploymentAction / DeploymentEvent / DeploymentHistory
// ---------------------------------------------------------------------------

/// Actions that can be recorded in the deployment history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentAction {
    /// A new version was deployed.
    Deploy,
    /// A rollback was performed.
    Rollback,
    /// Traffic split was configured.
    TrafficSplit,
    /// A version was deprecated.
    Deprecate,
}

impl std::fmt::Display for DeploymentAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deploy => write!(f, "deploy"),
            Self::Rollback => write!(f, "rollback"),
            Self::TrafficSplit => write!(f, "traffic_split"),
            Self::Deprecate => write!(f, "deprecate"),
        }
    }
}

/// A single event in the deployment audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentEvent {
    /// Agent this event belongs to.
    pub agent_id: String,
    /// Version involved in the event.
    pub version: u32,
    /// The action that was performed.
    pub action: DeploymentAction,
    /// Who performed the action.
    pub actor: String,
    /// When the action was performed.
    pub timestamp: DateTime<Utc>,
    /// Additional details about the action.
    pub details: String,
}

/// Audit trail tracking all deployment actions across agents.
#[derive(Debug, Clone)]
pub struct DeploymentHistory {
    events: Vec<DeploymentEvent>,
}

impl DeploymentHistory {
    /// Create an empty deployment history.
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Record a deployment event.
    pub fn record_deployment(
        &mut self,
        agent_id: &str,
        version: u32,
        action: DeploymentAction,
        actor: &str,
        details: &str,
    ) {
        self.events.push(DeploymentEvent {
            agent_id: agent_id.to_string(),
            version,
            action,
            actor: actor.to_string(),
            timestamp: Utc::now(),
            details: details.to_string(),
        });
    }

    /// Get all events for a given agent, ordered by insertion time.
    pub fn get_history(&self, agent_id: &str) -> Vec<DeploymentEvent> {
        self.events
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .cloned()
            .collect()
    }

    /// Get all events across all agents.
    pub fn get_all(&self) -> &[DeploymentEvent] {
        &self.events
    }
}

impl Default for DeploymentHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal per-agent state
// ---------------------------------------------------------------------------

/// Internal state tracking all versions and traffic configuration for a single agent.
#[derive(Debug, Clone)]
struct AgentVersionState {
    /// All versions ordered by version number (index 0 = version 1).
    versions: Vec<AgentVersionConfig>,
    /// Currently active version number, if any.
    active_version: Option<u32>,
    /// Optional traffic split configuration.
    traffic_split: Option<TrafficSplit>,
}

impl AgentVersionState {
    fn new() -> Self {
        Self {
            versions: Vec::new(),
            active_version: None,
            traffic_split: None,
        }
    }

    /// Next version number to assign.
    fn next_version(&self) -> u32 {
        self.versions.last().map_or(1, |v| v.version + 1)
    }

    /// Get a version by its number.
    fn get_version(&self, version: u32) -> Option<&AgentVersionConfig> {
        self.versions.iter().find(|v| v.version == version)
    }

    /// Get a mutable reference to a version by its number.
    fn get_version_mut(&mut self, version: u32) -> Option<&mut AgentVersionConfig> {
        self.versions.iter_mut().find(|v| v.version == version)
    }
}

// ---------------------------------------------------------------------------
// AgentVersionManager
// ---------------------------------------------------------------------------

/// Thread-safe manager for versioned agent configurations.
///
/// Manages the full lifecycle of agent versions: deploying, rolling back,
/// A/B traffic splitting, and maintaining a deployment audit trail.
///
/// Thread-safe via internal `Arc<RwLock<>>` — can be cloned and shared
/// across async tasks safely.
#[derive(Clone)]
pub struct AgentVersionManager {
    state: Arc<RwLock<HashMap<String, AgentVersionState>>>,
    history: Arc<RwLock<DeploymentHistory>>,
}

impl AgentVersionManager {
    /// Create a new, empty version manager.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(DeploymentHistory::new())),
        }
    }

    /// Deploy a new version of an agent configuration.
    ///
    /// Creates version N+1 for the given agent, marks it as `Active`, and
    /// deactivates any previously active version.
    pub async fn deploy(
        &self,
        agent_id: &str,
        mut config: AgentVersionConfig,
    ) -> ArgentorResult<u32> {
        let mut state = self.state.write().await;
        let agent_state = state
            .entry(agent_id.to_string())
            .or_insert_with(AgentVersionState::new);

        let version = agent_state.next_version();
        config.version = version;
        config.agent_id = agent_id.to_string();
        config.status = VersionStatus::Active;
        config.created_at = Utc::now();

        // Deactivate the previously active version.
        if let Some(prev_version) = agent_state.active_version {
            if let Some(prev) = agent_state.get_version_mut(prev_version) {
                prev.status = VersionStatus::Inactive;
            }
        }

        info!(
            agent_id = agent_id,
            version = version,
            model_id = %config.model_id,
            "Deploying agent version"
        );

        agent_state.versions.push(config);
        agent_state.active_version = Some(version);

        // Record in history.
        let mut history = self.history.write().await;
        history.record_deployment(
            agent_id,
            version,
            DeploymentAction::Deploy,
            "system",
            &format!("Deployed version {version}"),
        );

        Ok(version)
    }

    /// Roll back to the previous version.
    ///
    /// If the active version is N, this activates version N-1 and marks N as Inactive.
    pub async fn rollback(&self, agent_id: &str) -> ArgentorResult<u32> {
        let mut state = self.state.write().await;
        let agent_state = state.get_mut(agent_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' not found"))
        })?;

        let active = agent_state.active_version.ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' has no active version"))
        })?;

        if active <= 1 {
            return Err(ArgentorError::Orchestrator(format!(
                "Agent '{agent_id}' has no previous version to roll back to"
            )));
        }

        let previous = active - 1;
        self.do_rollback(agent_state, agent_id, previous).await?;

        // Record in history (need separate lock).
        let mut history = self.history.write().await;
        history.record_deployment(
            agent_id,
            previous,
            DeploymentAction::Rollback,
            "system",
            &format!("Rolled back from version {active} to {previous}"),
        );

        Ok(previous)
    }

    /// Roll back to a specific version.
    pub async fn rollback_to(&self, agent_id: &str, version: u32) -> ArgentorResult<u32> {
        let mut state = self.state.write().await;
        let agent_state = state.get_mut(agent_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' not found"))
        })?;

        let active = agent_state.active_version;

        // Validate that the target version exists.
        if agent_state.get_version(version).is_none() {
            return Err(ArgentorError::Orchestrator(format!(
                "Version {version} not found for agent '{agent_id}'"
            )));
        }

        self.do_rollback(agent_state, agent_id, version).await?;

        // Record in history.
        let mut history = self.history.write().await;
        history.record_deployment(
            agent_id,
            version,
            DeploymentAction::Rollback,
            "system",
            &format!(
                "Rolled back from version {} to {version}",
                active.map_or("none".to_string(), |v| v.to_string())
            ),
        );

        Ok(version)
    }

    /// Internal helper to perform a rollback to a target version.
    async fn do_rollback(
        &self,
        agent_state: &mut AgentVersionState,
        agent_id: &str,
        target: u32,
    ) -> ArgentorResult<()> {
        // Deactivate the current active version.
        if let Some(current_active) = agent_state.active_version {
            if let Some(current) = agent_state.get_version_mut(current_active) {
                current.status = VersionStatus::Inactive;
            }
        }

        // Activate the target version.
        let target_config = agent_state.get_version_mut(target).ok_or_else(|| {
            ArgentorError::Orchestrator(format!(
                "Version {target} not found for agent '{agent_id}'"
            ))
        })?;
        target_config.status = VersionStatus::Active;
        agent_state.active_version = Some(target);

        warn!(
            agent_id = agent_id,
            target_version = target,
            "Rolled back agent version"
        );

        Ok(())
    }

    /// Get the currently active configuration for an agent.
    pub async fn get_active(&self, agent_id: &str) -> Option<AgentVersionConfig> {
        let state = self.state.read().await;
        let agent_state = state.get(agent_id)?;
        let active = agent_state.active_version?;
        agent_state.get_version(active).cloned()
    }

    /// Get a specific version of an agent's configuration.
    pub async fn get_version(&self, agent_id: &str, version: u32) -> Option<AgentVersionConfig> {
        let state = self.state.read().await;
        let agent_state = state.get(agent_id)?;
        agent_state.get_version(version).cloned()
    }

    /// List all versions of an agent's configuration.
    pub async fn list_versions(&self, agent_id: &str) -> Vec<AgentVersionConfig> {
        let state = self.state.read().await;
        state
            .get(agent_id)
            .map(|s| s.versions.clone())
            .unwrap_or_default()
    }

    /// Configure A/B traffic splitting between versions of an agent.
    ///
    /// Validates that both versions exist and that weights sum to approximately 1.0.
    pub async fn set_traffic_split(
        &self,
        agent_id: &str,
        split: TrafficSplit,
    ) -> ArgentorResult<()> {
        // Validate weights.
        let total = split.primary_weight + split.canary_weight;
        if (total - 1.0).abs() > 0.01 {
            return Err(ArgentorError::Orchestrator(format!(
                "Traffic split weights must sum to 1.0, got {total:.2}"
            )));
        }
        if split.primary_weight < 0.0 || split.canary_weight < 0.0 {
            return Err(ArgentorError::Orchestrator(
                "Traffic split weights must be non-negative".to_string(),
            ));
        }

        let mut state = self.state.write().await;
        let agent_state = state.get_mut(agent_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' not found"))
        })?;

        // Validate that referenced versions exist.
        if agent_state.get_version(split.primary_version).is_none() {
            return Err(ArgentorError::Orchestrator(format!(
                "Primary version {} not found for agent '{agent_id}'",
                split.primary_version
            )));
        }
        if let Some(canary) = split.canary_version {
            if agent_state.get_version(canary).is_none() {
                return Err(ArgentorError::Orchestrator(format!(
                    "Canary version {canary} not found for agent '{agent_id}'"
                )));
            }
        }

        info!(
            agent_id = agent_id,
            primary_version = split.primary_version,
            primary_weight = split.primary_weight,
            canary_version = ?split.canary_version,
            canary_weight = split.canary_weight,
            "Setting traffic split"
        );

        agent_state.traffic_split = Some(split.clone());

        // Record in history.
        let mut history = self.history.write().await;
        history.record_deployment(
            agent_id,
            split.primary_version,
            DeploymentAction::TrafficSplit,
            "system",
            &format!(
                "Traffic split: v{} ({:.0}%) / v{} ({:.0}%)",
                split.primary_version,
                split.primary_weight * 100.0,
                split
                    .canary_version
                    .map_or("none".to_string(), |v| v.to_string()),
                split.canary_weight * 100.0,
            ),
        );

        Ok(())
    }

    /// Get the current traffic split configuration for an agent.
    pub async fn get_traffic_split(&self, agent_id: &str) -> Option<TrafficSplit> {
        let state = self.state.read().await;
        state.get(agent_id)?.traffic_split.clone()
    }

    /// Compute the diff between two versions of an agent's configuration.
    pub async fn diff_versions(
        &self,
        agent_id: &str,
        from_version: u32,
        to_version: u32,
    ) -> ArgentorResult<VersionDiff> {
        let state = self.state.read().await;
        let agent_state = state.get(agent_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' not found"))
        })?;

        let from = agent_state.get_version(from_version).ok_or_else(|| {
            ArgentorError::Orchestrator(format!(
                "Version {from_version} not found for agent '{agent_id}'"
            ))
        })?;

        let to = agent_state.get_version(to_version).ok_or_else(|| {
            ArgentorError::Orchestrator(format!(
                "Version {to_version} not found for agent '{agent_id}'"
            ))
        })?;

        let mut changes = Vec::new();

        if from.model_id != to.model_id {
            changes.push(FieldChange {
                field: "model_id".to_string(),
                old_value: from.model_id.clone(),
                new_value: to.model_id.clone(),
            });
        }
        if from.system_prompt != to.system_prompt {
            changes.push(FieldChange {
                field: "system_prompt".to_string(),
                old_value: from.system_prompt.clone(),
                new_value: to.system_prompt.clone(),
            });
        }
        if (from.temperature - to.temperature).abs() > f32::EPSILON {
            changes.push(FieldChange {
                field: "temperature".to_string(),
                old_value: from.temperature.to_string(),
                new_value: to.temperature.to_string(),
            });
        }
        if from.max_tokens != to.max_tokens {
            changes.push(FieldChange {
                field: "max_tokens".to_string(),
                old_value: from.max_tokens.to_string(),
                new_value: to.max_tokens.to_string(),
            });
        }
        if from.tools != to.tools {
            changes.push(FieldChange {
                field: "tools".to_string(),
                old_value: format!("{:?}", from.tools),
                new_value: format!("{:?}", to.tools),
            });
        }
        if from.guardrails != to.guardrails {
            changes.push(FieldChange {
                field: "guardrails".to_string(),
                old_value: format!("{:?}", from.guardrails),
                new_value: format!("{:?}", to.guardrails),
            });
        }
        if from.created_by != to.created_by {
            changes.push(FieldChange {
                field: "created_by".to_string(),
                old_value: from.created_by.clone(),
                new_value: to.created_by.clone(),
            });
        }

        Ok(VersionDiff {
            from_version,
            to_version,
            changes,
        })
    }

    /// Deprecate a specific version, marking it as no longer usable.
    pub async fn deprecate_version(
        &self,
        agent_id: &str,
        version: u32,
    ) -> ArgentorResult<()> {
        let mut state = self.state.write().await;
        let agent_state = state.get_mut(agent_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Agent '{agent_id}' not found"))
        })?;

        let config = agent_state.get_version_mut(version).ok_or_else(|| {
            ArgentorError::Orchestrator(format!(
                "Version {version} not found for agent '{agent_id}'"
            ))
        })?;

        if config.status == VersionStatus::Active {
            return Err(ArgentorError::Orchestrator(format!(
                "Cannot deprecate active version {version} of agent '{agent_id}'"
            )));
        }

        config.status = VersionStatus::Deprecated;

        info!(
            agent_id = agent_id,
            version = version,
            "Deprecated agent version"
        );

        let mut history = self.history.write().await;
        history.record_deployment(
            agent_id,
            version,
            DeploymentAction::Deprecate,
            "system",
            &format!("Deprecated version {version}"),
        );

        Ok(())
    }

    /// Access the deployment history for auditing.
    pub async fn get_deployment_history(&self, agent_id: &str) -> Vec<DeploymentEvent> {
        let history = self.history.read().await;
        history.get_history(agent_id)
    }

    /// Get the full deployment history across all agents.
    pub async fn get_all_history(&self) -> Vec<DeploymentEvent> {
        let history = self.history.read().await;
        history.get_all().to_vec()
    }

    /// Total number of tracked agents.
    pub async fn agent_count(&self) -> usize {
        let state = self.state.read().await;
        state.len()
    }

    /// Total number of versions across all agents.
    pub async fn total_version_count(&self) -> usize {
        let state = self.state.read().await;
        state.values().map(|s| s.versions.len()).sum()
    }
}

impl Default for AgentVersionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper: build a config for testing
// ---------------------------------------------------------------------------

/// Build a minimal [`AgentVersionConfig`] suitable for tests and examples.
///
/// Version number and agent_id are set to placeholder values; the caller or
/// [`AgentVersionManager::deploy`] will override them.
pub fn test_version_config(model_id: &str, change_log: &str) -> AgentVersionConfig {
    AgentVersionConfig {
        agent_id: String::new(),
        version: 0,
        model_id: model_id.to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: 0.7,
        max_tokens: 4096,
        tools: vec!["memory_search".to_string(), "file_read".to_string()],
        guardrails: vec!["no-pii".to_string()],
        metadata: HashMap::new(),
        created_at: Utc::now(),
        created_by: "test-user".to_string(),
        status: VersionStatus::Inactive,
        change_log: change_log.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_config(model_id: &str, change_log: &str) -> AgentVersionConfig {
        test_version_config(model_id, change_log)
    }

    fn make_config_with_details(
        model_id: &str,
        prompt: &str,
        temp: f32,
        max_tokens: u32,
        tools: Vec<&str>,
        guardrails: Vec<&str>,
        created_by: &str,
        change_log: &str,
    ) -> AgentVersionConfig {
        AgentVersionConfig {
            agent_id: String::new(),
            version: 0,
            model_id: model_id.to_string(),
            system_prompt: prompt.to_string(),
            temperature: temp,
            max_tokens,
            tools: tools.into_iter().map(String::from).collect(),
            guardrails: guardrails.into_iter().map(String::from).collect(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            created_by: created_by.to_string(),
            status: VersionStatus::Inactive,
            change_log: change_log.to_string(),
        }
    }

    // 1. Deploy creates version 1
    #[tokio::test]
    async fn test_deploy_creates_version_1() {
        let mgr = AgentVersionManager::new();
        let v = mgr
            .deploy("agent-a", make_config("claude-sonnet", "Initial"))
            .await
            .unwrap();
        assert_eq!(v, 1);
    }

    // 2. Deploy increments version numbers
    #[tokio::test]
    async fn test_deploy_increments_version_numbers() {
        let mgr = AgentVersionManager::new();
        let v1 = mgr
            .deploy("agent-a", make_config("model-1", "v1"))
            .await
            .unwrap();
        let v2 = mgr
            .deploy("agent-a", make_config("model-2", "v2"))
            .await
            .unwrap();
        let v3 = mgr
            .deploy("agent-a", make_config("model-3", "v3"))
            .await
            .unwrap();

        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        assert_eq!(v3, 3);
    }

    // 3. Deploy sets new version as active and deactivates previous
    #[tokio::test]
    async fn test_deploy_activates_new_deactivates_old() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("model-1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("model-2", "v2"))
            .await
            .unwrap();

        let v1 = mgr.get_version("agent-a", 1).await.unwrap();
        assert_eq!(v1.status, VersionStatus::Inactive);

        let v2 = mgr.get_version("agent-a", 2).await.unwrap();
        assert_eq!(v2.status, VersionStatus::Active);
    }

    // 4. Get active returns current active version
    #[tokio::test]
    async fn test_get_active_returns_current() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("model-1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("model-2", "v2"))
            .await
            .unwrap();

        let active = mgr.get_active("agent-a").await.unwrap();
        assert_eq!(active.version, 2);
        assert_eq!(active.model_id, "model-2");
        assert_eq!(active.status, VersionStatus::Active);
    }

    // 5. Get active returns None for unknown agent
    #[tokio::test]
    async fn test_get_active_returns_none_for_unknown() {
        let mgr = AgentVersionManager::new();
        assert!(mgr.get_active("nonexistent").await.is_none());
    }

    // 6. Get version returns specific version
    #[tokio::test]
    async fn test_get_version_returns_specific() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("model-1", "first"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("model-2", "second"))
            .await
            .unwrap();

        let v1 = mgr.get_version("agent-a", 1).await.unwrap();
        assert_eq!(v1.model_id, "model-1");
        assert_eq!(v1.change_log, "first");

        let v2 = mgr.get_version("agent-a", 2).await.unwrap();
        assert_eq!(v2.model_id, "model-2");
        assert_eq!(v2.change_log, "second");
    }

    // 7. Get version returns None for missing version
    #[tokio::test]
    async fn test_get_version_returns_none_for_missing() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("model-1", "v1"))
            .await
            .unwrap();
        assert!(mgr.get_version("agent-a", 99).await.is_none());
    }

    // 8. List versions returns all versions
    #[tokio::test]
    async fn test_list_versions_returns_all() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m3", "v3"))
            .await
            .unwrap();

        let versions = mgr.list_versions("agent-a").await;
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version, 1);
        assert_eq!(versions[1].version, 2);
        assert_eq!(versions[2].version, 3);
    }

    // 9. List versions returns empty for unknown agent
    #[tokio::test]
    async fn test_list_versions_empty_for_unknown() {
        let mgr = AgentVersionManager::new();
        let versions = mgr.list_versions("nonexistent").await;
        assert!(versions.is_empty());
    }

    // 10. Rollback reverts to previous version
    #[tokio::test]
    async fn test_rollback_reverts_to_previous() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();

        let rolled = mgr.rollback("agent-a").await.unwrap();
        assert_eq!(rolled, 1);

        let active = mgr.get_active("agent-a").await.unwrap();
        assert_eq!(active.version, 1);
        assert_eq!(active.status, VersionStatus::Active);

        let v2 = mgr.get_version("agent-a", 2).await.unwrap();
        assert_eq!(v2.status, VersionStatus::Inactive);
    }

    // 11. Rollback fails when only one version exists
    #[tokio::test]
    async fn test_rollback_fails_at_version_1() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        let result = mgr.rollback("agent-a").await;
        assert!(result.is_err());
    }

    // 12. Rollback fails for unknown agent
    #[tokio::test]
    async fn test_rollback_fails_for_unknown_agent() {
        let mgr = AgentVersionManager::new();
        let result = mgr.rollback("nonexistent").await;
        assert!(result.is_err());
    }

    // 13. Rollback to specific version
    #[tokio::test]
    async fn test_rollback_to_specific_version() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m3", "v3"))
            .await
            .unwrap();

        let rolled = mgr.rollback_to("agent-a", 1).await.unwrap();
        assert_eq!(rolled, 1);

        let active = mgr.get_active("agent-a").await.unwrap();
        assert_eq!(active.version, 1);

        // v3 should be inactive now.
        let v3 = mgr.get_version("agent-a", 3).await.unwrap();
        assert_eq!(v3.status, VersionStatus::Inactive);
    }

    // 14. Rollback to nonexistent version fails
    #[tokio::test]
    async fn test_rollback_to_nonexistent_version_fails() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        let result = mgr.rollback_to("agent-a", 99).await;
        assert!(result.is_err());
    }

    // 15. Traffic split — set and get
    #[tokio::test]
    async fn test_set_and_get_traffic_split() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();

        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 0.8,
            canary_version: Some(2),
            canary_weight: 0.2,
        };

        mgr.set_traffic_split("agent-a", split).await.unwrap();

        let fetched = mgr.get_traffic_split("agent-a").await.unwrap();
        assert_eq!(fetched.primary_version, 1);
        assert!((fetched.primary_weight - 0.8).abs() < 0.001);
        assert_eq!(fetched.canary_version, Some(2));
        assert!((fetched.canary_weight - 0.2).abs() < 0.001);
    }

    // 16. Traffic split — invalid weights rejected
    #[tokio::test]
    async fn test_traffic_split_invalid_weights_rejected() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();

        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 0.5,
            canary_version: Some(2),
            canary_weight: 0.3, // sums to 0.8, not 1.0
        };

        let result = mgr.set_traffic_split("agent-a", split).await;
        assert!(result.is_err());
    }

    // 17. Traffic split — negative weights rejected
    #[tokio::test]
    async fn test_traffic_split_negative_weights_rejected() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 1.5,
            canary_version: None,
            canary_weight: -0.5,
        };

        let result = mgr.set_traffic_split("agent-a", split).await;
        assert!(result.is_err());
    }

    // 18. Traffic split — nonexistent version rejected
    #[tokio::test]
    async fn test_traffic_split_nonexistent_version_rejected() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 0.8,
            canary_version: Some(99),
            canary_weight: 0.2,
        };

        let result = mgr.set_traffic_split("agent-a", split).await;
        assert!(result.is_err());
    }

    // 19. Traffic split — resolve without canary always returns primary
    #[tokio::test]
    async fn test_traffic_split_resolve_no_canary() {
        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 1.0,
            canary_version: None,
            canary_weight: 0.0,
        };

        // Should always return primary when there's no canary.
        for _ in 0..100 {
            assert_eq!(split.resolve(), 1);
        }
    }

    // 20. Traffic split — resolve with canary returns one of the two versions
    #[tokio::test]
    async fn test_traffic_split_resolve_with_canary() {
        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 0.5,
            canary_version: Some(2),
            canary_weight: 0.5,
        };

        let result = split.resolve();
        assert!(result == 1 || result == 2);
    }

    // 21. Get traffic split returns None when not configured
    #[tokio::test]
    async fn test_get_traffic_split_returns_none() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        assert!(mgr.get_traffic_split("agent-a").await.is_none());
    }

    // 22. Diff detects model change
    #[tokio::test]
    async fn test_diff_detects_model_change() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("model-old", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("model-new", "v2"))
            .await
            .unwrap();

        let diff = mgr.diff_versions("agent-a", 1, 2).await.unwrap();
        assert_eq!(diff.from_version, 1);
        assert_eq!(diff.to_version, 2);

        let model_change = diff
            .changes
            .iter()
            .find(|c| c.field == "model_id")
            .expect("should detect model_id change");
        assert_eq!(model_change.old_value, "model-old");
        assert_eq!(model_change.new_value, "model-new");
    }

    // 23. Diff detects multiple changes
    #[tokio::test]
    async fn test_diff_detects_multiple_changes() {
        let mgr = AgentVersionManager::new();

        let c1 = make_config_with_details(
            "m1",
            "prompt-a",
            0.7,
            4096,
            vec!["tool-a"],
            vec!["guard-a"],
            "alice",
            "v1",
        );
        let c2 = make_config_with_details(
            "m2",
            "prompt-b",
            0.9,
            8192,
            vec!["tool-b"],
            vec!["guard-b"],
            "bob",
            "v2",
        );

        mgr.deploy("agent-a", c1).await.unwrap();
        mgr.deploy("agent-a", c2).await.unwrap();

        let diff = mgr.diff_versions("agent-a", 1, 2).await.unwrap();
        let fields: Vec<&str> = diff.changes.iter().map(|c| c.field.as_str()).collect();

        assert!(fields.contains(&"model_id"));
        assert!(fields.contains(&"system_prompt"));
        assert!(fields.contains(&"temperature"));
        assert!(fields.contains(&"max_tokens"));
        assert!(fields.contains(&"tools"));
        assert!(fields.contains(&"guardrails"));
        assert!(fields.contains(&"created_by"));
    }

    // 24. Diff with identical versions produces no changes
    #[tokio::test]
    async fn test_diff_identical_versions_no_changes() {
        let mgr = AgentVersionManager::new();
        let c = make_config("same-model", "same-log");
        mgr.deploy("agent-a", c.clone()).await.unwrap();
        mgr.deploy("agent-a", c).await.unwrap();

        let diff = mgr.diff_versions("agent-a", 1, 2).await.unwrap();
        assert!(
            diff.changes.is_empty(),
            "identical configs should produce no diff"
        );
    }

    // 25. Deployment history records events
    #[tokio::test]
    async fn test_deployment_history_records_events() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();
        mgr.rollback("agent-a").await.unwrap();

        let history = mgr.get_deployment_history("agent-a").await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].action, DeploymentAction::Deploy);
        assert_eq!(history[1].action, DeploymentAction::Deploy);
        assert_eq!(history[2].action, DeploymentAction::Rollback);
    }

    // 26. Deployment history is per-agent
    #[tokio::test]
    async fn test_deployment_history_per_agent() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-b", make_config("m2", "v1"))
            .await
            .unwrap();

        let history_a = mgr.get_deployment_history("agent-a").await;
        let history_b = mgr.get_deployment_history("agent-b").await;

        assert_eq!(history_a.len(), 1);
        assert_eq!(history_b.len(), 1);
        assert_eq!(history_a[0].agent_id, "agent-a");
        assert_eq!(history_b[0].agent_id, "agent-b");
    }

    // 27. Deprecate version
    #[tokio::test]
    async fn test_deprecate_version() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();

        // v1 is now Inactive, so we can deprecate it.
        mgr.deprecate_version("agent-a", 1).await.unwrap();

        let v1 = mgr.get_version("agent-a", 1).await.unwrap();
        assert_eq!(v1.status, VersionStatus::Deprecated);
    }

    // 28. Cannot deprecate active version
    #[tokio::test]
    async fn test_cannot_deprecate_active_version() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();

        let result = mgr.deprecate_version("agent-a", 1).await;
        assert!(result.is_err());
    }

    // 29. Multiple agents are independent
    #[tokio::test]
    async fn test_multiple_agents_independent() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m-a", "a-v1"))
            .await
            .unwrap();
        mgr.deploy("agent-b", make_config("m-b", "b-v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m-a2", "a-v2"))
            .await
            .unwrap();

        let active_a = mgr.get_active("agent-a").await.unwrap();
        assert_eq!(active_a.version, 2);
        assert_eq!(active_a.model_id, "m-a2");

        let active_b = mgr.get_active("agent-b").await.unwrap();
        assert_eq!(active_b.version, 1);
        assert_eq!(active_b.model_id, "m-b");

        assert_eq!(mgr.agent_count().await, 2);
    }

    // 30. Agent count and total version count
    #[tokio::test]
    async fn test_agent_count_and_total_versions() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("a", make_config("m1", "v1")).await.unwrap();
        mgr.deploy("a", make_config("m2", "v2")).await.unwrap();
        mgr.deploy("b", make_config("m3", "v1")).await.unwrap();

        assert_eq!(mgr.agent_count().await, 2);
        assert_eq!(mgr.total_version_count().await, 3);
    }

    // 31. Deploy sets correct agent_id on config
    #[tokio::test]
    async fn test_deploy_sets_agent_id_on_config() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("my-agent", make_config("m1", "v1"))
            .await
            .unwrap();

        let config = mgr.get_version("my-agent", 1).await.unwrap();
        assert_eq!(config.agent_id, "my-agent");
    }

    // 32. Serialize/deserialize VersionStatus roundtrip
    #[tokio::test]
    async fn test_version_status_serde_roundtrip() {
        let statuses = vec![
            VersionStatus::Active,
            VersionStatus::Inactive,
            VersionStatus::Testing,
            VersionStatus::Deprecated,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: VersionStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    // 33. Serialize/deserialize AgentVersionConfig roundtrip
    #[tokio::test]
    async fn test_agent_version_config_serde_roundtrip() {
        let config = make_config("test-model", "serde test");
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AgentVersionConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.model_id, config.model_id);
        assert_eq!(parsed.change_log, config.change_log);
        assert_eq!(parsed.status, config.status);
    }

    // 34. Traffic split history event is recorded
    #[tokio::test]
    async fn test_traffic_split_records_history() {
        let mgr = AgentVersionManager::new();
        mgr.deploy("agent-a", make_config("m1", "v1"))
            .await
            .unwrap();
        mgr.deploy("agent-a", make_config("m2", "v2"))
            .await
            .unwrap();

        let split = TrafficSplit {
            primary_version: 1,
            primary_weight: 0.9,
            canary_version: Some(2),
            canary_weight: 0.1,
        };
        mgr.set_traffic_split("agent-a", split).await.unwrap();

        let history = mgr.get_deployment_history("agent-a").await;
        let split_events: Vec<_> = history
            .iter()
            .filter(|e| e.action == DeploymentAction::TrafficSplit)
            .collect();
        assert_eq!(split_events.len(), 1);
    }

    // 35. Default manager is empty
    #[tokio::test]
    async fn test_default_manager_is_empty() {
        let mgr = AgentVersionManager::default();
        assert_eq!(mgr.agent_count().await, 0);
        assert_eq!(mgr.total_version_count().await, 0);
    }
}
