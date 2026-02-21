use crate::monitor::AgentMonitor;
use crate::profiles::default_profiles;
use crate::task_queue::TaskQueue;
use crate::types::{AgentProfile, AgentRole, Artifact, ArtifactKind, Task, TaskStatus};
use agentor_agent::{AgentRunner, ModelConfig};
use agentor_core::{AgentorError, AgentorResult};
use agentor_security::{AuditLog, PermissionSet};
use agentor_session::Session;
use agentor_skills::SkillRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

/// The multi-agent orchestrator engine.
/// Implements the plan → execute → synthesize pattern.
pub struct Orchestrator {
    profiles: HashMap<AgentRole, AgentProfile>,
    queue: Arc<RwLock<TaskQueue>>,
    monitor: Arc<AgentMonitor>,
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    audit: Arc<AuditLog>,
}

impl Orchestrator {
    /// Create a new orchestrator with default profiles based on the given model config.
    pub fn new(
        base_config: &ModelConfig,
        skills: Arc<SkillRegistry>,
        permissions: PermissionSet,
        audit: Arc<AuditLog>,
    ) -> Self {
        let profiles: HashMap<AgentRole, AgentProfile> = default_profiles(base_config)
            .into_iter()
            .map(|p| (p.role, p))
            .collect();

        Self {
            profiles,
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            monitor: Arc::new(AgentMonitor::new()),
            skills,
            permissions,
            audit,
        }
    }

    /// Create with custom profiles.
    pub fn with_profiles(
        profiles: Vec<AgentProfile>,
        skills: Arc<SkillRegistry>,
        permissions: PermissionSet,
        audit: Arc<AuditLog>,
    ) -> Self {
        let profiles: HashMap<AgentRole, AgentProfile> =
            profiles.into_iter().map(|p| (p.role, p)).collect();

        Self {
            profiles,
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            monitor: Arc::new(AgentMonitor::new()),
            skills,
            permissions,
            audit,
        }
    }

    /// Get a reference to the monitor.
    pub fn monitor(&self) -> &Arc<AgentMonitor> {
        &self.monitor
    }

    /// Get a reference to the task queue.
    pub fn queue(&self) -> &Arc<RwLock<TaskQueue>> {
        &self.queue
    }

    /// Run the full orchestration pipeline for a high-level task.
    ///
    /// Phase 1 (Plan): The orchestrator agent decomposes the task.
    /// Phase 2 (Execute): Workers execute subtasks respecting dependencies.
    /// Phase 3 (Synthesize): The orchestrator combines all artifacts.
    pub async fn run(&self, task_description: &str) -> AgentorResult<OrchestratorResult> {
        let start = Instant::now();

        info!(task = %task_description, "Orchestrator: starting pipeline");

        // Phase 1: Plan — decompose into subtasks
        let subtasks = self.plan(task_description).await?;

        info!(
            subtask_count = subtasks.len(),
            "Orchestrator: plan complete"
        );

        // Add subtasks to queue
        {
            let mut queue = self.queue.write().await;
            for task in subtasks {
                queue.add(task);
            }

            // Validate no cycles
            if queue.has_cycle() {
                return Err(AgentorError::Agent(
                    "Dependency cycle detected in task graph".to_string(),
                ));
            }
        }

        // Phase 2: Execute — process tasks respecting dependencies
        self.execute().await?;

        // Phase 3: Synthesize — collect all artifacts
        let result = self.synthesize().await?;

        let duration = start.elapsed();
        self.monitor
            .record_duration(AgentRole::Orchestrator, duration.as_millis() as u64)
            .await;

        info!(
            duration_ms = duration.as_millis(),
            artifacts = result.artifacts.len(),
            "Orchestrator: pipeline complete"
        );

        Ok(result)
    }

    /// Phase 1: Plan — decompose a high-level task into subtasks.
    async fn plan(&self, task_description: &str) -> AgentorResult<Vec<Task>> {
        info!("Orchestrator Phase 1: Planning");
        self.monitor
            .start_task(AgentRole::Orchestrator, Uuid::new_v4())
            .await;

        // Create a spec task first, then code, test, review chain
        let spec_task = Task::new(
            format!("Analyze and specify: {}", task_description),
            AgentRole::Spec,
        );
        let spec_id = spec_task.id;

        let code_task = Task::new(format!("Implement: {}", task_description), AgentRole::Coder)
            .with_dependencies(vec![spec_id]);
        let code_id = code_task.id;

        let test_task = Task::new(
            format!("Write tests for: {}", task_description),
            AgentRole::Tester,
        )
        .with_dependencies(vec![code_id]);
        let test_id = test_task.id;

        let review_task = Task::new(format!("Review: {}", task_description), AgentRole::Reviewer)
            .with_dependencies(vec![code_id, test_id]);

        self.monitor.finish_task(AgentRole::Orchestrator).await;

        Ok(vec![spec_task, code_task, test_task, review_task])
    }

    /// Phase 2: Execute — process all tasks in dependency order.
    async fn execute(&self) -> AgentorResult<()> {
        info!("Orchestrator Phase 2: Executing");

        loop {
            // Get next ready tasks
            let ready_tasks: Vec<(Uuid, AgentRole, String)> = {
                let queue = self.queue.read().await;
                if queue.is_done() {
                    break;
                }
                queue
                    .all_ready()
                    .iter()
                    .map(|t| (t.id, t.assigned_to, t.description.clone()))
                    .collect()
            };

            if ready_tasks.is_empty() {
                // Check for deadlock (all remaining tasks are blocked or failed)
                let queue = self.queue.read().await;
                let has_pending = queue.pending_count() > 0;
                if has_pending {
                    warn!("Orchestrator: possible deadlock — pending tasks with unresolvable deps");
                    return Err(AgentorError::Agent(
                        "Task deadlock: pending tasks with unmet dependencies".to_string(),
                    ));
                }
                break;
            }

            // Execute ready tasks (could be parallelized with tokio::spawn)
            for (task_id, role, description) in ready_tasks {
                self.execute_task(task_id, role, &description).await?;
            }
        }

        Ok(())
    }

    /// Execute a single task by running the appropriate agent.
    async fn execute_task(
        &self,
        task_id: Uuid,
        role: AgentRole,
        description: &str,
    ) -> AgentorResult<()> {
        info!(task_id = %task_id, role = %role, "Executing task");

        // Mark as running
        {
            let mut queue = self.queue.write().await;
            queue.mark_running(task_id);
        }
        self.monitor.start_task(role, task_id).await;

        let start = Instant::now();

        // Get the profile for this role
        let profile = self.profiles.get(&role).ok_or_else(|| {
            AgentorError::Agent(format!("No profile configured for role: {}", role))
        })?;

        // Create a dedicated agent runner for this worker
        let runner = AgentRunner::new(
            profile.model.clone(),
            self.skills.clone(),
            self.permissions.clone(),
            self.audit.clone(),
        );

        // Create a temporary session for this worker
        let mut session = Session::new();

        // Run the agent with the task description
        let result = runner.run(&mut session, description).await;

        let duration = start.elapsed();
        self.monitor
            .record_duration(role, duration.as_millis() as u64)
            .await;

        match result {
            Ok(response) => {
                // Create artifact from response
                let artifact_kind = match role {
                    AgentRole::Spec => ArtifactKind::Spec,
                    AgentRole::Coder => ArtifactKind::Code,
                    AgentRole::Tester => ArtifactKind::Test,
                    AgentRole::Reviewer => ArtifactKind::Review,
                    AgentRole::Orchestrator => ArtifactKind::Report,
                };

                let artifact = Artifact::new(artifact_kind, response);

                {
                    let mut queue = self.queue.write().await;
                    if let Some(task) = queue.get_mut(task_id) {
                        task.add_artifact(artifact);
                    }
                    queue.mark_completed(task_id);
                }

                self.monitor.finish_task(role).await;
                self.monitor.record_turn(role, 1, 0).await;

                info!(task_id = %task_id, role = %role, "Task completed");
                Ok(())
            }
            Err(e) => {
                error!(task_id = %task_id, role = %role, error = %e, "Task failed");
                {
                    let mut queue = self.queue.write().await;
                    queue.mark_failed(task_id, e.to_string());
                }
                self.monitor.record_error(role).await;
                self.monitor.finish_task(role).await;
                Err(e)
            }
        }
    }

    /// Phase 3: Synthesize — collect all artifacts from completed tasks.
    async fn synthesize(&self) -> AgentorResult<OrchestratorResult> {
        info!("Orchestrator Phase 3: Synthesizing");

        let queue = self.queue.read().await;
        let all_tasks = queue.all_tasks();

        let mut artifacts = Vec::new();
        let mut completed = 0;
        let mut failed = 0;

        for task in &all_tasks {
            match &task.status {
                TaskStatus::Completed => {
                    completed += 1;
                    artifacts.extend(task.artifacts.clone());
                }
                TaskStatus::Failed { .. } => {
                    failed += 1;
                }
                _ => {}
            }
        }

        let summary = format!(
            "Orchestration complete: {}/{} tasks completed, {} failed, {} artifacts produced",
            completed,
            all_tasks.len(),
            failed,
            artifacts.len()
        );

        Ok(OrchestratorResult {
            summary,
            artifacts,
            total_tasks: all_tasks.len(),
            completed_tasks: completed,
            failed_tasks: failed,
        })
    }
}

/// Result of a full orchestration pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResult {
    pub summary: String,
    pub artifacts: Vec<Artifact>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_queue::TaskQueue;
    use crate::types::AgentRole;

    #[test]
    fn test_orchestrator_result_serialization() {
        let result = OrchestratorResult {
            summary: "Done".to_string(),
            artifacts: vec![Artifact::new(ArtifactKind::Code, "fn main() {}")],
            total_tasks: 4,
            completed_tasks: 4,
            failed_tasks: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Done"));
        let parsed: OrchestratorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_tasks, 4);
    }

    #[test]
    fn test_task_queue_plan_pattern() {
        // Simulate the plan phase output
        let spec = Task::new("Spec task", AgentRole::Spec);
        let spec_id = spec.id;
        let code = Task::new("Code task", AgentRole::Coder).with_dependencies(vec![spec_id]);
        let code_id = code.id;
        let test = Task::new("Test task", AgentRole::Tester).with_dependencies(vec![code_id]);
        let test_id = test.id;
        let review =
            Task::new("Review task", AgentRole::Reviewer).with_dependencies(vec![code_id, test_id]);

        let mut queue = TaskQueue::new();
        queue.add(spec);
        queue.add(code);
        queue.add(test);
        queue.add(review);

        assert!(!queue.has_cycle());
        assert_eq!(queue.total_count(), 4);

        // Only spec is ready initially
        let ready = queue.all_ready();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].assigned_to, AgentRole::Spec);
    }
}
