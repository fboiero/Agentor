//! Agent deployment manager for handling the lifecycle of deployed agent instances.
//!
//! Provides [`DeploymentManager`] which manages the full lifecycle of agent
//! deployments: creating, scaling, restarting, monitoring health, and collecting
//! metrics across all deployed agent instances.

use crate::types::AgentRole;
use argentor_core::{ArgentorError, ArgentorResult};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ResourceLimits
// ---------------------------------------------------------------------------

/// Resource constraints applied to a deployed agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum number of tasks the agent can process concurrently.
    pub max_concurrent_tasks: u32,
    /// Maximum tokens the agent may consume per hour.
    pub max_tokens_per_hour: u64,
    /// Maximum tasks the agent may complete per hour.
    pub max_tasks_per_hour: u64,
    /// Optional memory limit in megabytes.
    pub memory_limit_mb: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 4,
            max_tokens_per_hour: 100_000,
            max_tasks_per_hour: 100,
            memory_limit_mb: None,
        }
    }
}

// ---------------------------------------------------------------------------
// DeploymentConfig
// ---------------------------------------------------------------------------

/// Configuration for deploying an agent with a specific role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentConfig {
    /// The role of the agent being deployed.
    pub agent_role: AgentRole,
    /// Human-readable name for this deployment.
    pub name: String,
    /// Number of instances (replicas) to run.
    pub replicas: u32,
    /// Whether to automatically restart failed instances.
    pub auto_restart: bool,
    /// Maximum number of restart attempts before marking the deployment as failed.
    pub max_restarts: u32,
    /// Interval in seconds between health checks.
    pub health_check_interval_secs: u64,
    /// Timeout in seconds for graceful shutdown.
    pub shutdown_timeout_secs: u64,
    /// Resource limits for the agent instances.
    pub resource_limits: ResourceLimits,
    /// Environment variables passed to the agent.
    pub environment: HashMap<String, String>,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            agent_role: AgentRole::Coder,
            name: "default-deployment".to_string(),
            replicas: 1,
            auto_restart: true,
            max_restarts: 3,
            health_check_interval_secs: 30,
            shutdown_timeout_secs: 10,
            resource_limits: ResourceLimits::default(),
            environment: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Status enums
// ---------------------------------------------------------------------------

/// Overall status of a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    /// Deployment has been created but instances have not started yet.
    Pending,
    /// At least one instance is running.
    Running,
    /// Some instances are down but at least one is still running.
    Degraded,
    /// All instances have failed and restart attempts are exhausted.
    Failed,
    /// The deployment has been manually stopped.
    Stopped,
    /// The deployment is in the process of scaling up or down.
    Scaling,
}

/// Status of an individual agent instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    /// Instance is in the process of starting.
    Starting,
    /// Instance is running and healthy.
    Running,
    /// Instance has missed heartbeats and is considered unhealthy.
    Unhealthy,
    /// Instance is gracefully shutting down.
    Stopping,
    /// Instance has been stopped.
    Stopped,
    /// Instance has failed with an error message.
    Failed(String),
}

// ---------------------------------------------------------------------------
// AgentInstance
// ---------------------------------------------------------------------------

/// A single running instance (replica) of a deployed agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstance {
    /// Unique identifier for this instance.
    pub instance_id: Uuid,
    /// Zero-based index of this replica within the deployment.
    pub replica_index: u32,
    /// Current status of the instance.
    pub status: InstanceStatus,
    /// When the instance was started.
    pub started_at: DateTime<Utc>,
    /// Last heartbeat received from the instance.
    pub last_heartbeat: DateTime<Utc>,
    /// The task currently being processed, if any.
    pub current_task: Option<Uuid>,
    /// Total tasks completed by this instance.
    pub tasks_completed: u64,
    /// Total errors encountered by this instance.
    pub errors: u32,
}

impl AgentInstance {
    /// Create a new instance with the given replica index.
    fn new(replica_index: u32) -> Self {
        let now = Utc::now();
        Self {
            instance_id: Uuid::new_v4(),
            replica_index,
            status: InstanceStatus::Running,
            started_at: now,
            last_heartbeat: now,
            current_task: None,
            tasks_completed: 0,
            errors: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// DeployedAgent
// ---------------------------------------------------------------------------

/// A deployed agent with one or more running instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedAgent {
    /// Unique deployment identifier.
    pub id: Uuid,
    /// Configuration used for this deployment.
    pub config: DeploymentConfig,
    /// Current deployment status.
    pub status: DeploymentStatus,
    /// Active instances of this agent.
    pub instances: Vec<AgentInstance>,
    /// When the deployment was created.
    pub created_at: DateTime<Utc>,
    /// When the deployment was last updated.
    pub updated_at: DateTime<Utc>,
    /// Total number of restart attempts performed.
    pub restart_count: u32,
    /// Total tasks completed across all instances.
    pub total_tasks_completed: u64,
    /// Total tasks that failed across all instances.
    pub total_tasks_failed: u64,
}

impl DeployedAgent {
    /// Create a new deployment from the given config.
    fn from_config(config: DeploymentConfig) -> Self {
        let now = Utc::now();
        let instances: Vec<AgentInstance> = (0..config.replicas).map(AgentInstance::new).collect();

        Self {
            id: Uuid::new_v4(),
            config,
            status: DeploymentStatus::Running,
            instances,
            created_at: now,
            updated_at: now,
            restart_count: 0,
            total_tasks_completed: 0,
            total_tasks_failed: 0,
        }
    }

    /// Recompute the deployment status based on instance states.
    fn recompute_status(&mut self) {
        if self.instances.is_empty() {
            self.status = DeploymentStatus::Stopped;
            return;
        }

        let running = self
            .instances
            .iter()
            .filter(|i| i.status == InstanceStatus::Running)
            .count();
        let total = self.instances.len();

        if running == total {
            self.status = DeploymentStatus::Running;
        } else if running > 0 {
            self.status = DeploymentStatus::Degraded;
        } else {
            // All instances are down — check if we can still restart.
            if self.restart_count >= self.config.max_restarts {
                self.status = DeploymentStatus::Failed;
            } else {
                self.status = DeploymentStatus::Degraded;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Health types
// ---------------------------------------------------------------------------

/// Severity of a detected health issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    /// A non-critical issue that should be monitored.
    Warning,
    /// A critical issue requiring immediate attention.
    Critical,
    /// A fatal issue — the deployment cannot recover.
    Fatal,
}

/// A health issue detected during a health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthIssue {
    /// The deployment this issue belongs to.
    pub deployment_id: Uuid,
    /// The specific instance affected, if applicable.
    pub instance_id: Option<Uuid>,
    /// Severity of the issue.
    pub severity: IssueSeverity,
    /// Human-readable description of the issue.
    pub description: String,
    /// When the issue was detected.
    pub detected_at: DateTime<Utc>,
}

/// Aggregate statistics across all deployments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentSummary {
    /// Total number of active deployments.
    pub total_deployments: usize,
    /// Total number of instances across all deployments.
    pub total_instances: usize,
    /// Number of instances currently running.
    pub running_instances: usize,
    /// Number of unhealthy instances.
    pub unhealthy_instances: usize,
    /// Total tasks completed across all deployments.
    pub total_tasks_completed: u64,
    /// Total tasks failed across all deployments.
    pub total_tasks_failed: u64,
    /// Currently active health issues.
    pub health_issues: Vec<HealthIssue>,
}

// ---------------------------------------------------------------------------
// DeploymentManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of deployed agent instances.
///
/// Thread-safe via internal `Arc<RwLock<>>` — can be cloned and shared across
/// async tasks safely.
#[derive(Clone)]
pub struct DeploymentManager {
    deployments: Arc<RwLock<HashMap<Uuid, DeployedAgent>>>,
}

impl DeploymentManager {
    /// Create a new, empty deployment manager.
    pub fn new() -> Self {
        Self {
            deployments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Deploy a new agent with the given configuration.
    ///
    /// Creates the deployment, initializes all replica instances, and returns
    /// the deployment ID.
    pub async fn deploy(&self, config: DeploymentConfig) -> ArgentorResult<Uuid> {
        if config.replicas == 0 {
            return Err(ArgentorError::Orchestrator(
                "Cannot deploy with 0 replicas".to_string(),
            ));
        }

        let deployed = DeployedAgent::from_config(config);
        let id = deployed.id;

        info!(
            deployment_id = %id,
            name = %deployed.config.name,
            role = %deployed.config.agent_role,
            replicas = deployed.config.replicas,
            "Deploying agent"
        );

        let mut deployments = self.deployments.write().await;
        deployments.insert(id, deployed);

        Ok(id)
    }

    /// Gracefully stop all instances and remove the deployment.
    pub async fn undeploy(&self, deployment_id: Uuid) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        info!(deployment_id = %deployment_id, "Undeploying agent");

        for instance in &mut deployed.instances {
            instance.status = InstanceStatus::Stopped;
        }
        deployed.status = DeploymentStatus::Stopped;
        deployed.updated_at = Utc::now();

        Ok(())
    }

    /// Scale a deployment to the given number of replicas.
    ///
    /// - Scaling up adds new instances.
    /// - Scaling down removes instances from the end (highest replica index first).
    /// - Scaling to 0 stops the deployment.
    pub async fn scale(&self, deployment_id: Uuid, replicas: u32) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        let current = deployed.instances.len() as u32;

        info!(
            deployment_id = %deployment_id,
            current_replicas = current,
            target_replicas = replicas,
            "Scaling deployment"
        );

        deployed.status = DeploymentStatus::Scaling;

        if replicas > current {
            // Scale up — add new instances.
            for i in current..replicas {
                deployed.instances.push(AgentInstance::new(i));
            }
        } else if replicas < current {
            // Scale down — remove from end, mark as stopped.
            deployed.instances.truncate(replicas as usize);
        }

        if replicas == 0 {
            deployed.status = DeploymentStatus::Stopped;
        } else {
            deployed.recompute_status();
        }

        deployed.config.replicas = replicas;
        deployed.updated_at = Utc::now();

        Ok(())
    }

    /// Restart all instances in a deployment.
    pub async fn restart(&self, deployment_id: Uuid) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        if deployed.restart_count >= deployed.config.max_restarts {
            return Err(ArgentorError::Orchestrator(format!(
                "Deployment {deployment_id} has exhausted max restarts ({})",
                deployed.config.max_restarts,
            )));
        }

        info!(
            deployment_id = %deployment_id,
            restart_count = deployed.restart_count + 1,
            "Restarting deployment"
        );

        deployed.restart_count += 1;
        let now = Utc::now();

        for instance in &mut deployed.instances {
            instance.status = InstanceStatus::Running;
            instance.started_at = now;
            instance.last_heartbeat = now;
            instance.current_task = None;
            instance.errors = 0;
        }

        deployed.recompute_status();
        deployed.updated_at = now;

        Ok(())
    }

    /// Restart a specific instance within a deployment.
    pub async fn restart_instance(
        &self,
        deployment_id: Uuid,
        instance_id: Uuid,
    ) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        let instance = deployed
            .instances
            .iter_mut()
            .find(|i| i.instance_id == instance_id)
            .ok_or_else(|| {
                ArgentorError::Orchestrator(format!(
                    "Instance {instance_id} not found in deployment {deployment_id}"
                ))
            })?;

        info!(
            deployment_id = %deployment_id,
            instance_id = %instance_id,
            "Restarting instance"
        );

        let now = Utc::now();
        instance.status = InstanceStatus::Running;
        instance.started_at = now;
        instance.last_heartbeat = now;
        instance.current_task = None;
        instance.errors = 0;

        deployed.recompute_status();
        deployed.updated_at = now;

        Ok(())
    }

    /// Get a clone of a deployment's current state.
    pub async fn get_deployment(&self, id: Uuid) -> Option<DeployedAgent> {
        let deployments = self.deployments.read().await;
        deployments.get(&id).cloned()
    }

    /// List all deployments.
    pub async fn list_deployments(&self) -> Vec<DeployedAgent> {
        let deployments = self.deployments.read().await;
        deployments.values().cloned().collect()
    }

    /// Get the status of a specific deployment.
    pub async fn get_status(&self, id: Uuid) -> Option<DeploymentStatus> {
        let deployments = self.deployments.read().await;
        deployments.get(&id).map(|d| d.status.clone())
    }

    /// Record a heartbeat from a specific instance.
    pub async fn record_heartbeat(
        &self,
        deployment_id: Uuid,
        instance_id: Uuid,
    ) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        let instance = deployed
            .instances
            .iter_mut()
            .find(|i| i.instance_id == instance_id)
            .ok_or_else(|| {
                ArgentorError::Orchestrator(format!(
                    "Instance {instance_id} not found in deployment {deployment_id}"
                ))
            })?;

        instance.last_heartbeat = Utc::now();
        if instance.status == InstanceStatus::Unhealthy {
            instance.status = InstanceStatus::Running;
            deployed.recompute_status();
        }
        deployed.updated_at = Utc::now();

        Ok(())
    }

    /// Record a successful task completion for an instance.
    pub async fn record_task_completed(
        &self,
        deployment_id: Uuid,
        instance_id: Uuid,
    ) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        let instance = deployed
            .instances
            .iter_mut()
            .find(|i| i.instance_id == instance_id)
            .ok_or_else(|| {
                ArgentorError::Orchestrator(format!(
                    "Instance {instance_id} not found in deployment {deployment_id}"
                ))
            })?;

        instance.tasks_completed += 1;
        instance.current_task = None;
        deployed.total_tasks_completed += 1;
        deployed.updated_at = Utc::now();

        Ok(())
    }

    /// Record a task failure for an instance.
    pub async fn record_task_failed(
        &self,
        deployment_id: Uuid,
        instance_id: Uuid,
        reason: &str,
    ) -> ArgentorResult<()> {
        let mut deployments = self.deployments.write().await;
        let deployed = deployments.get_mut(&deployment_id).ok_or_else(|| {
            ArgentorError::Orchestrator(format!("Deployment {deployment_id} not found"))
        })?;

        let instance = deployed
            .instances
            .iter_mut()
            .find(|i| i.instance_id == instance_id)
            .ok_or_else(|| {
                ArgentorError::Orchestrator(format!(
                    "Instance {instance_id} not found in deployment {deployment_id}"
                ))
            })?;

        warn!(
            deployment_id = %deployment_id,
            instance_id = %instance_id,
            reason = reason,
            "Task failed"
        );

        instance.errors += 1;
        instance.current_task = None;
        deployed.total_tasks_failed += 1;
        deployed.updated_at = Utc::now();

        Ok(())
    }

    /// Scan all deployments for health issues.
    ///
    /// Detects missed heartbeats (based on the deployment's configured
    /// `health_check_interval_secs`), failed instances, and fully-failed
    /// deployments.
    pub async fn check_health(&self) -> Vec<HealthIssue> {
        let deployments = self.deployments.read().await;
        let now = Utc::now();
        let mut issues = Vec::new();

        for deployed in deployments.values() {
            if deployed.status == DeploymentStatus::Stopped {
                continue;
            }

            let heartbeat_threshold =
                Duration::seconds(deployed.config.health_check_interval_secs as i64 * 2);

            let mut unhealthy_count = 0u32;
            let mut failed_count = 0u32;

            for instance in &deployed.instances {
                match &instance.status {
                    InstanceStatus::Failed(reason) => {
                        failed_count += 1;
                        issues.push(HealthIssue {
                            deployment_id: deployed.id,
                            instance_id: Some(instance.instance_id),
                            severity: IssueSeverity::Critical,
                            description: format!(
                                "Instance {} (replica {}) failed: {}",
                                instance.instance_id, instance.replica_index, reason
                            ),
                            detected_at: now,
                        });
                    }
                    InstanceStatus::Running | InstanceStatus::Unhealthy => {
                        let since_heartbeat = now.signed_duration_since(instance.last_heartbeat);

                        if since_heartbeat > heartbeat_threshold {
                            unhealthy_count += 1;
                            let severity = if since_heartbeat > heartbeat_threshold * 3 {
                                IssueSeverity::Critical
                            } else {
                                IssueSeverity::Warning
                            };

                            issues.push(HealthIssue {
                                deployment_id: deployed.id,
                                instance_id: Some(instance.instance_id),
                                severity,
                                description: format!(
                                    "Instance {} (replica {}) missed heartbeat — last seen {} seconds ago",
                                    instance.instance_id,
                                    instance.replica_index,
                                    since_heartbeat.num_seconds()
                                ),
                                detected_at: now,
                            });
                        }
                    }
                    _ => {}
                }
            }

            let total = deployed.instances.len() as u32;
            let down = unhealthy_count + failed_count;

            if down == total && total > 0 {
                let severity = if deployed.restart_count >= deployed.config.max_restarts {
                    IssueSeverity::Fatal
                } else {
                    IssueSeverity::Critical
                };

                issues.push(HealthIssue {
                    deployment_id: deployed.id,
                    instance_id: None,
                    severity,
                    description: format!(
                        "All {} instances in deployment '{}' are down",
                        total, deployed.config.name
                    ),
                    detected_at: now,
                });
            }
        }

        issues
    }

    /// Produce an aggregate summary across all deployments.
    pub async fn summary(&self) -> DeploymentSummary {
        let deployments = self.deployments.read().await;
        let mut total_instances = 0usize;
        let mut running_instances = 0usize;
        let mut unhealthy_instances = 0usize;
        let mut total_tasks_completed = 0u64;
        let mut total_tasks_failed = 0u64;

        for deployed in deployments.values() {
            total_instances += deployed.instances.len();
            total_tasks_completed += deployed.total_tasks_completed;
            total_tasks_failed += deployed.total_tasks_failed;

            for instance in &deployed.instances {
                match &instance.status {
                    InstanceStatus::Running => running_instances += 1,
                    InstanceStatus::Unhealthy | InstanceStatus::Failed(_) => {
                        unhealthy_instances += 1;
                    }
                    _ => {}
                }
            }
        }

        // Release the read lock before calling check_health (which also acquires it).
        drop(deployments);

        let health_issues = self.check_health().await;

        DeploymentSummary {
            total_deployments: {
                let d = self.deployments.read().await;
                d.len()
            },
            total_instances,
            running_instances,
            unhealthy_instances,
            total_tasks_completed,
            total_tasks_failed,
            health_issues,
        }
    }
}

impl Default for DeploymentManager {
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

    fn test_config() -> DeploymentConfig {
        DeploymentConfig {
            agent_role: AgentRole::Coder,
            name: "test-coder".to_string(),
            replicas: 3,
            auto_restart: true,
            max_restarts: 3,
            health_check_interval_secs: 30,
            shutdown_timeout_secs: 10,
            resource_limits: ResourceLimits::default(),
            environment: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_deploy_creates_deployment_with_correct_status() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.status, DeploymentStatus::Running);
        assert_eq!(deployed.config.name, "test-coder");
        assert_eq!(deployed.config.agent_role, AgentRole::Coder);
        assert_eq!(deployed.restart_count, 0);
        assert_eq!(deployed.total_tasks_completed, 0);
        assert_eq!(deployed.total_tasks_failed, 0);
    }

    #[tokio::test]
    async fn test_deploy_creates_correct_number_of_instances() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.instances.len(), 3);

        for (i, instance) in deployed.instances.iter().enumerate() {
            assert_eq!(instance.replica_index, i as u32);
            assert_eq!(instance.status, InstanceStatus::Running);
            assert_eq!(instance.tasks_completed, 0);
            assert_eq!(instance.errors, 0);
            assert!(instance.current_task.is_none());
        }
    }

    #[tokio::test]
    async fn test_deploy_zero_replicas_fails() {
        let mgr = DeploymentManager::new();
        let mut config = test_config();
        config.replicas = 0;

        let result = mgr.deploy(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_undeploy_transitions_to_stopped() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        mgr.undeploy(id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.status, DeploymentStatus::Stopped);
        for instance in &deployed.instances {
            assert_eq!(instance.status, InstanceStatus::Stopped);
        }
    }

    #[tokio::test]
    async fn test_scale_up_adds_instances() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        mgr.scale(id, 5).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.instances.len(), 5);
        assert_eq!(deployed.config.replicas, 5);
        // New instances should be running.
        assert_eq!(deployed.instances[3].replica_index, 3);
        assert_eq!(deployed.instances[4].replica_index, 4);
        assert_eq!(deployed.instances[3].status, InstanceStatus::Running);
    }

    #[tokio::test]
    async fn test_scale_down_removes_instances() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        mgr.scale(id, 1).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.instances.len(), 1);
        assert_eq!(deployed.config.replicas, 1);
        assert_eq!(deployed.instances[0].replica_index, 0);
    }

    #[tokio::test]
    async fn test_scale_to_zero_stops_deployment() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        mgr.scale(id, 0).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.status, DeploymentStatus::Stopped);
        assert!(deployed.instances.is_empty());
        assert_eq!(deployed.config.replicas, 0);
    }

    #[tokio::test]
    async fn test_restart_resets_instance_status() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        // Simulate a failure on the first instance.
        {
            let mut deployments = mgr.deployments.write().await;
            let deployed = deployments.get_mut(&id).unwrap();
            deployed.instances[0].status = InstanceStatus::Failed("crash".to_string());
            deployed.instances[0].errors = 5;
        }

        mgr.restart(id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.restart_count, 1);
        for instance in &deployed.instances {
            assert_eq!(instance.status, InstanceStatus::Running);
            assert_eq!(instance.errors, 0);
            assert!(instance.current_task.is_none());
        }
    }

    #[tokio::test]
    async fn test_restart_instance_only_affects_one() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let instance_id;
        {
            let mut deployments = mgr.deployments.write().await;
            let deployed = deployments.get_mut(&id).unwrap();
            deployed.instances[1].status = InstanceStatus::Failed("crash".to_string());
            deployed.instances[1].errors = 3;
            instance_id = deployed.instances[1].instance_id;
        }

        mgr.restart_instance(id, instance_id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        let restarted = deployed
            .instances
            .iter()
            .find(|i| i.instance_id == instance_id)
            .unwrap();
        assert_eq!(restarted.status, InstanceStatus::Running);
        assert_eq!(restarted.errors, 0);

        // Other instances should be unchanged.
        let other = &deployed.instances[0];
        assert_eq!(other.status, InstanceStatus::Running);
    }

    #[tokio::test]
    async fn test_record_heartbeat_updates_timestamp() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let instance_id;
        let old_heartbeat;
        {
            let deployments = mgr.deployments.read().await;
            let deployed = deployments.get(&id).unwrap();
            instance_id = deployed.instances[0].instance_id;
            old_heartbeat = deployed.instances[0].last_heartbeat;
        }

        // Record a new heartbeat (time moves forward even by nanoseconds).
        mgr.record_heartbeat(id, instance_id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        let instance = deployed
            .instances
            .iter()
            .find(|i| i.instance_id == instance_id)
            .unwrap();
        assert!(instance.last_heartbeat >= old_heartbeat);
    }

    #[tokio::test]
    async fn test_record_task_completed_increments_counters() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let instance_id = {
            let deployments = mgr.deployments.read().await;
            deployments.get(&id).unwrap().instances[0].instance_id
        };

        mgr.record_task_completed(id, instance_id).await.unwrap();
        mgr.record_task_completed(id, instance_id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.total_tasks_completed, 2);

        let instance = deployed
            .instances
            .iter()
            .find(|i| i.instance_id == instance_id)
            .unwrap();
        assert_eq!(instance.tasks_completed, 2);
    }

    #[tokio::test]
    async fn test_record_task_failed_increments_error_counters() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let instance_id = {
            let deployments = mgr.deployments.read().await;
            deployments.get(&id).unwrap().instances[0].instance_id
        };

        mgr.record_task_failed(id, instance_id, "timeout")
            .await
            .unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.total_tasks_failed, 1);

        let instance = deployed
            .instances
            .iter()
            .find(|i| i.instance_id == instance_id)
            .unwrap();
        assert_eq!(instance.errors, 1);
    }

    #[tokio::test]
    async fn test_check_health_detects_missed_heartbeats() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        // Set an instance's heartbeat far in the past.
        {
            let mut deployments = mgr.deployments.write().await;
            let deployed = deployments.get_mut(&id).unwrap();
            deployed.instances[0].last_heartbeat = Utc::now() - Duration::seconds(300);
        }

        let issues = mgr.check_health().await;
        assert!(!issues.is_empty());

        let stale_issue = issues
            .iter()
            .find(|i| i.deployment_id == id && i.description.contains("missed heartbeat"))
            .expect("should detect missed heartbeat");

        assert!(
            stale_issue.severity == IssueSeverity::Warning
                || stale_issue.severity == IssueSeverity::Critical
        );
    }

    #[tokio::test]
    async fn test_check_health_returns_empty_when_all_healthy() {
        let mgr = DeploymentManager::new();
        let _id = mgr.deploy(test_config()).await.unwrap();

        let issues = mgr.check_health().await;
        assert!(
            issues.is_empty(),
            "Expected no health issues for fresh deployment"
        );
    }

    #[tokio::test]
    async fn test_summary_aggregates_across_deployments() {
        let mgr = DeploymentManager::new();

        let id1 = mgr.deploy(test_config()).await.unwrap();
        let mut config2 = test_config();
        config2.name = "test-tester".to_string();
        config2.agent_role = AgentRole::Tester;
        config2.replicas = 2;
        let id2 = mgr.deploy(config2).await.unwrap();

        // Record some tasks.
        let inst1 = {
            let d = mgr.deployments.read().await;
            d.get(&id1).unwrap().instances[0].instance_id
        };
        let inst2 = {
            let d = mgr.deployments.read().await;
            d.get(&id2).unwrap().instances[0].instance_id
        };

        mgr.record_task_completed(id1, inst1).await.unwrap();
        mgr.record_task_failed(id2, inst2, "error").await.unwrap();

        let summary = mgr.summary().await;
        assert_eq!(summary.total_deployments, 2);
        assert_eq!(summary.total_instances, 5); // 3 + 2
        assert_eq!(summary.running_instances, 5);
        assert_eq!(summary.total_tasks_completed, 1);
        assert_eq!(summary.total_tasks_failed, 1);
    }

    #[tokio::test]
    async fn test_max_restarts_prevents_infinite_restarts() {
        let mgr = DeploymentManager::new();
        let mut config = test_config();
        config.max_restarts = 2;
        let id = mgr.deploy(config).await.unwrap();

        // Use up all restarts.
        mgr.restart(id).await.unwrap();
        mgr.restart(id).await.unwrap();

        // Third restart should fail.
        let result = mgr.restart(id).await;
        assert!(result.is_err());

        let deployed = mgr.get_deployment(id).await.unwrap();
        assert_eq!(deployed.restart_count, 2);
    }

    #[tokio::test]
    async fn test_get_deployment_returns_none_for_unknown_id() {
        let mgr = DeploymentManager::new();
        let result = mgr.get_deployment(Uuid::new_v4()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_deployments_returns_all() {
        let mgr = DeploymentManager::new();

        let _id1 = mgr.deploy(test_config()).await.unwrap();
        let mut config2 = test_config();
        config2.name = "second".to_string();
        let _id2 = mgr.deploy(config2).await.unwrap();

        let list = mgr.list_deployments().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_deployment_config_defaults() {
        let config = DeploymentConfig::default();
        assert_eq!(config.agent_role, AgentRole::Coder);
        assert_eq!(config.name, "default-deployment");
        assert_eq!(config.replicas, 1);
        assert!(config.auto_restart);
        assert_eq!(config.max_restarts, 3);
        assert_eq!(config.health_check_interval_secs, 30);
        assert_eq!(config.shutdown_timeout_secs, 10);
        assert!(config.environment.is_empty());

        // ResourceLimits defaults
        assert_eq!(config.resource_limits.max_concurrent_tasks, 4);
        assert_eq!(config.resource_limits.max_tokens_per_hour, 100_000);
        assert_eq!(config.resource_limits.max_tasks_per_hour, 100);
        assert!(config.resource_limits.memory_limit_mb.is_none());
    }

    #[tokio::test]
    async fn test_serialize_deserialize_roundtrip_deployment_status() {
        let statuses = vec![
            DeploymentStatus::Pending,
            DeploymentStatus::Running,
            DeploymentStatus::Degraded,
            DeploymentStatus::Failed,
            DeploymentStatus::Stopped,
            DeploymentStatus::Scaling,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: DeploymentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[tokio::test]
    async fn test_serialize_deserialize_roundtrip_instance_status() {
        let statuses = vec![
            InstanceStatus::Starting,
            InstanceStatus::Running,
            InstanceStatus::Unhealthy,
            InstanceStatus::Stopping,
            InstanceStatus::Stopped,
            InstanceStatus::Failed("some error".to_string()),
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: InstanceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[tokio::test]
    async fn test_get_status_returns_none_for_unknown() {
        let mgr = DeploymentManager::new();
        assert!(mgr.get_status(Uuid::new_v4()).await.is_none());
    }

    #[tokio::test]
    async fn test_get_status_returns_correct_status() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        assert_eq!(mgr.get_status(id).await, Some(DeploymentStatus::Running));

        mgr.undeploy(id).await.unwrap();
        assert_eq!(mgr.get_status(id).await, Some(DeploymentStatus::Stopped));
    }

    #[tokio::test]
    async fn test_heartbeat_recovers_unhealthy_instance() {
        let mgr = DeploymentManager::new();
        let id = mgr.deploy(test_config()).await.unwrap();

        let instance_id;
        {
            let mut deployments = mgr.deployments.write().await;
            let deployed = deployments.get_mut(&id).unwrap();
            deployed.instances[0].status = InstanceStatus::Unhealthy;
            instance_id = deployed.instances[0].instance_id;
        }

        mgr.record_heartbeat(id, instance_id).await.unwrap();

        let deployed = mgr.get_deployment(id).await.unwrap();
        let instance = deployed
            .instances
            .iter()
            .find(|i| i.instance_id == instance_id)
            .unwrap();
        assert_eq!(instance.status, InstanceStatus::Running);
    }
}
