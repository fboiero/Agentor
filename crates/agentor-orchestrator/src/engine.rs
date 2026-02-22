use crate::monitor::AgentMonitor;
use crate::profiles::default_profiles;
use crate::task_queue::TaskQueue;
use crate::types::{AgentProfile, AgentRole, Artifact, ArtifactKind, Task, TaskStatus};
use agentor_agent::{AgentRunner, ModelConfig};
use agentor_core::{AgentorError, AgentorResult};
use agentor_mcp::{McpProxy, ToolDiscovery};
use agentor_security::{AuditLog, PermissionSet};
use agentor_session::Session;
use agentor_skills::SkillRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Callback type for progress updates from worker agents.
pub type ProgressCallback = Arc<dyn Fn(AgentRole, &str) + Send + Sync>;

/// Factory for creating custom LLM backends (for testing or custom providers).
pub type BackendFactory =
    Arc<dyn Fn(&AgentRole) -> Box<dyn agentor_agent::LlmBackend> + Send + Sync>;

/// Shared state passed to parallel worker tasks.
struct WorkerContext {
    profiles: HashMap<AgentRole, AgentProfile>,
    queue: Arc<RwLock<TaskQueue>>,
    monitor: Arc<AgentMonitor>,
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    audit: Arc<AuditLog>,
    proxy: Arc<McpProxy>,
    on_progress: Option<ProgressCallback>,
    backend_factory: Option<BackendFactory>,
}

/// The multi-agent orchestrator engine.
/// Implements the plan → execute → synthesize pattern.
/// All tool calls are routed through the MCP Proxy for centralized logging and metrics.
pub struct Orchestrator {
    profiles: HashMap<AgentRole, AgentProfile>,
    queue: Arc<RwLock<TaskQueue>>,
    monitor: Arc<AgentMonitor>,
    skills: Arc<SkillRegistry>,
    permissions: PermissionSet,
    audit: Arc<AuditLog>,
    proxy: Arc<McpProxy>,
    output_dir: Option<PathBuf>,
    on_progress: Option<ProgressCallback>,
    backend_factory: Option<BackendFactory>,
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

        let proxy = Arc::new(McpProxy::new(skills.clone(), permissions.clone()));

        Self {
            profiles,
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            monitor: Arc::new(AgentMonitor::new()),
            skills,
            permissions,
            audit,
            proxy,
            output_dir: None,
            on_progress: None,
            backend_factory: None,
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

        let proxy = Arc::new(McpProxy::new(skills.clone(), permissions.clone()));

        Self {
            profiles,
            queue: Arc::new(RwLock::new(TaskQueue::new())),
            monitor: Arc::new(AgentMonitor::new()),
            skills,
            permissions,
            audit,
            proxy,
            output_dir: None,
            on_progress: None,
            backend_factory: None,
        }
    }

    /// Set the output directory for writing artifacts to disk.
    pub fn with_output_dir(mut self, dir: PathBuf) -> Self {
        self.output_dir = Some(dir);
        self
    }

    /// Set a progress callback for real-time status updates.
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(AgentRole, &str) + Send + Sync + 'static,
    {
        self.on_progress = Some(Arc::new(callback));
        self
    }

    /// Set a custom LLM backend factory (for testing with mock backends).
    pub fn with_backend_factory(mut self, factory: BackendFactory) -> Self {
        self.backend_factory = Some(factory);
        self
    }

    #[allow(dead_code)]
    fn emit_progress(&self, role: AgentRole, msg: &str) {
        if let Some(cb) = &self.on_progress {
            cb(role, msg);
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

    /// Get a reference to the MCP proxy (for metrics/dashboard).
    pub fn proxy(&self) -> &Arc<McpProxy> {
        &self.proxy
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

        // Create output directory if specified
        if let Some(dir) = &self.output_dir {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| AgentorError::Agent(format!("Failed to create output dir: {}", e)))?;
        }

        // Phase 2: Execute — process tasks respecting dependencies
        self.execute().await?;

        // Phase 3: Synthesize — collect all artifacts
        let result = self.synthesize().await?;

        let duration = start.elapsed();
        self.monitor
            .record_duration(AgentRole::Orchestrator, duration.as_millis() as u64)
            .await;

        let proxy_stats = self.proxy.to_json().await;
        info!(
            duration_ms = duration.as_millis(),
            artifacts = result.artifacts.len(),
            proxy_total_calls = %proxy_stats["total_calls"],
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

            if ready_tasks.len() == 1 {
                // Single task: run inline (no spawn overhead)
                let (task_id, role, description) = &ready_tasks[0];
                let context = self.gather_dependency_context(*task_id).await;
                self.execute_task(*task_id, *role, description, &context)
                    .await?;
            } else {
                // Multiple tasks ready: execute in parallel with tokio::spawn
                info!(count = ready_tasks.len(), "Executing tasks in parallel");
                let mut handles = Vec::new();

                for (task_id, role, description) in ready_tasks {
                    let context = self.gather_dependency_context(task_id).await;
                    let ctx = self.worker_context();

                    let handle = tokio::spawn(async move {
                        Self::execute_task_static(task_id, role, &description, &context, &ctx).await
                    });
                    handles.push((task_id, role, handle));
                }

                // Await all parallel tasks
                for (task_id, role, handle) in handles {
                    match handle.await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            error!(task_id = %task_id, role = %role, error = %e, "Parallel task failed");
                            return Err(e);
                        }
                        Err(e) => {
                            error!(task_id = %task_id, role = %role, "Task panicked: {}", e);
                            return Err(AgentorError::Agent(format!("Task panicked: {}", e)));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Gather artifacts from a task's dependency chain to build context for the worker.
    /// This enables context flow: Spec → Coder → Tester → Reviewer.
    async fn gather_dependency_context(&self, task_id: Uuid) -> String {
        let queue = self.queue.read().await;
        let task = match queue.get(task_id) {
            Some(t) => t,
            None => return String::new(),
        };

        if task.dependencies.is_empty() {
            return String::new();
        }

        let mut context_parts = Vec::new();

        for dep_id in &task.dependencies {
            if let Some(dep_task) = queue.get(*dep_id) {
                for artifact in &dep_task.artifacts {
                    let label = match artifact.kind {
                        ArtifactKind::Spec => "SPECIFICATION",
                        ArtifactKind::Code => "CODE",
                        ArtifactKind::Test => "TESTS",
                        ArtifactKind::Review => "REVIEW",
                        ArtifactKind::Report => "REPORT",
                    };
                    context_parts.push(format!(
                        "--- {} (from {} worker) ---\n{}",
                        label, dep_task.assigned_to, artifact.content
                    ));
                }
            }
        }

        if context_parts.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n=== CONTEXT FROM PREVIOUS WORKERS ===\n{}\n=== END CONTEXT ===\n",
                context_parts.join("\n\n")
            )
        }
    }

    /// Build a WorkerContext from this orchestrator's state.
    fn worker_context(&self) -> WorkerContext {
        WorkerContext {
            profiles: self.profiles.clone(),
            queue: self.queue.clone(),
            monitor: self.monitor.clone(),
            skills: self.skills.clone(),
            permissions: self.permissions.clone(),
            audit: self.audit.clone(),
            proxy: self.proxy.clone(),
            on_progress: self.on_progress.clone(),
            backend_factory: self.backend_factory.clone(),
        }
    }

    /// Execute a single task (delegates to the static version).
    async fn execute_task(
        &self,
        task_id: Uuid,
        role: AgentRole,
        description: &str,
        dependency_context: &str,
    ) -> AgentorResult<()> {
        let ctx = self.worker_context();
        Self::execute_task_static(task_id, role, description, dependency_context, &ctx).await
    }

    /// Static task execution — can be called from tokio::spawn for parallel execution.
    #[allow(clippy::too_many_arguments)]
    async fn execute_task_static(
        task_id: Uuid,
        role: AgentRole,
        description: &str,
        dependency_context: &str,
        ctx: &WorkerContext,
    ) -> AgentorResult<()> {
        info!(task_id = %task_id, role = %role, "Executing task");

        // Mark as running
        {
            let mut q = ctx.queue.write().await;
            q.mark_running(task_id);
        }
        ctx.monitor.start_task(role, task_id).await;

        let start = Instant::now();

        // Get the profile for this role
        let profile = ctx.profiles.get(&role).ok_or_else(|| {
            AgentorError::Agent(format!("No profile configured for role: {}", role))
        })?;

        // Progressive tool disclosure — workers only see skills they need.
        // Prefer tool_group if set, fall back to allowed_skills list.
        let total_tools = ctx.skills.skill_count();
        let worker_skills = if let Some(group_name) = &profile.tool_group {
            match ctx.skills.filter_by_group(group_name) {
                Ok(filtered) => Arc::new(filtered),
                Err(e) => {
                    warn!(role = %role, group = %group_name, error = %e, "Tool group not found, falling back to allowed_skills");
                    Arc::new(ctx.skills.filter_to_new(&profile.allowed_skills))
                }
            }
        } else {
            Arc::new(ctx.skills.filter_to_new(&profile.allowed_skills))
        };

        let disclosed = worker_skills.skill_count();
        let savings = ToolDiscovery::estimate_token_savings(total_tools, disclosed);
        info!(
            role = %role,
            total_tools = total_tools,
            disclosed_tools = disclosed,
            token_savings_pct = format!("{:.1}%", savings),
            "Progressive disclosure applied"
        );

        // Create a dedicated agent runner for this worker with specialized system prompt.
        // Route tool calls through the MCP proxy for centralized logging.
        let agent_id = format!("{}:{}", role, task_id);
        let runner = if let Some(factory) = &ctx.backend_factory {
            let backend = factory(&role);
            AgentRunner::from_backend(
                backend,
                worker_skills,
                ctx.permissions.clone(),
                ctx.audit.clone(),
                profile.max_turns,
            )
        } else {
            AgentRunner::new(
                profile.model.clone(),
                worker_skills,
                ctx.permissions.clone(),
                ctx.audit.clone(),
            )
        }
        .with_system_prompt(&profile.system_prompt)
        .with_proxy(ctx.proxy.clone(), agent_id);

        let mut session = Session::new();

        // Build enriched prompt
        let enriched_prompt = if dependency_context.is_empty() {
            description.to_string()
        } else {
            format!("{}\n{}", description, dependency_context)
        };

        if let Some(cb) = &ctx.on_progress {
            cb(role, "working...");
        }
        info!(
            task_id = %task_id,
            role = %role,
            has_context = !dependency_context.is_empty(),
            "Worker starting"
        );
        let result = runner.run(&mut session, &enriched_prompt).await;

        let duration = start.elapsed();
        ctx.monitor
            .record_duration(role, duration.as_millis() as u64)
            .await;

        match result {
            Ok(response) => {
                let artifact_kind = match role {
                    AgentRole::Spec => ArtifactKind::Spec,
                    AgentRole::Coder => ArtifactKind::Code,
                    AgentRole::Tester => ArtifactKind::Test,
                    AgentRole::Reviewer => ArtifactKind::Review,
                    AgentRole::Orchestrator => ArtifactKind::Report,
                };

                // Check if the Reviewer flagged issues for human review.
                // The reviewer's response or a human_approval tool call result
                // may contain rejection signals.
                let needs_review = role == AgentRole::Reviewer
                    && Self::detect_review_flags(&response);

                if needs_review {
                    info!(task_id = %task_id, "Reviewer flagged task for human review");
                    ctx.monitor.waiting_for_approval(role).await;
                    {
                        let mut q = ctx.queue.write().await;
                        if let Some(task) = q.get_mut(task_id) {
                            task.add_artifact(Artifact::new(artifact_kind, &response));
                        }
                        q.mark_needs_review(task_id);
                    }
                    if let Some(cb) = &ctx.on_progress {
                        cb(role, "needs human review");
                    }
                } else {
                    let artifact = Artifact::new(artifact_kind, response);
                    {
                        let mut q = ctx.queue.write().await;
                        if let Some(task) = q.get_mut(task_id) {
                            task.add_artifact(artifact);
                        }
                        q.mark_completed(task_id);
                    }
                }

                ctx.monitor.finish_task(role).await;
                ctx.monitor.record_turn(role, 1, 0).await;

                if let Some(cb) = &ctx.on_progress {
                    if !needs_review {
                        cb(role, &format!("done ({:.1}s)", duration.as_secs_f64()));
                    }
                }
                info!(task_id = %task_id, role = %role, needs_review = needs_review, "Task completed");
                Ok(())
            }
            Err(e) => {
                if let Some(cb) = &ctx.on_progress {
                    cb(role, &format!("FAILED: {}", e));
                }
                error!(task_id = %task_id, role = %role, error = %e, "Task failed");
                {
                    let mut q = ctx.queue.write().await;
                    q.mark_failed(task_id, e.to_string());
                }
                ctx.monitor.record_error(role).await;
                ctx.monitor.finish_task(role).await;
                Err(e)
            }
        }
    }

    /// Phase 3: Synthesize — collect all artifacts and optionally write to disk.
    async fn synthesize(&self) -> AgentorResult<OrchestratorResult> {
        info!("Orchestrator Phase 3: Synthesizing");

        let queue = self.queue.read().await;
        let all_tasks = queue.all_tasks();

        let mut artifacts = Vec::new();
        let mut completed = 0;
        let mut failed = 0;
        let mut needs_review = 0;

        for task in &all_tasks {
            match &task.status {
                TaskStatus::Completed => {
                    completed += 1;
                    artifacts.extend(task.artifacts.clone());
                }
                TaskStatus::NeedsHumanReview => {
                    needs_review += 1;
                    // Still collect artifacts from review tasks
                    artifacts.extend(task.artifacts.clone());
                }
                TaskStatus::Failed { .. } => {
                    failed += 1;
                }
                _ => {}
            }
        }

        if needs_review > 0 {
            info!(needs_review = needs_review, "Tasks awaiting human review");
        }

        // Write artifacts to disk if output_dir is set
        let mut written_files = Vec::new();
        if let Some(dir) = &self.output_dir {
            for artifact in &artifacts {
                let (filename, content) = Self::artifact_to_file(artifact);
                let path = dir.join(&filename);

                // Create parent dirs if needed
                if let Some(parent) = path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }

                if let Err(e) = tokio::fs::write(&path, &content).await {
                    warn!(path = %path.display(), error = %e, "Failed to write artifact");
                } else {
                    info!(path = %path.display(), "Artifact written to disk");
                    written_files.push(path.display().to_string());
                }
            }
        }

        let review_suffix = if needs_review > 0 {
            format!(", {} awaiting human review", needs_review)
        } else {
            String::new()
        };

        let summary = if written_files.is_empty() {
            format!(
                "Orchestration complete: {}/{} tasks completed, {} failed, {} artifacts produced{}",
                completed,
                all_tasks.len(),
                failed,
                artifacts.len(),
                review_suffix
            )
        } else {
            format!(
                "Orchestration complete: {}/{} tasks completed, {} failed, {} artifacts written to {}{}",
                completed,
                all_tasks.len(),
                failed,
                written_files.len(),
                self.output_dir.as_ref().unwrap().display(),
                review_suffix
            )
        };

        Ok(OrchestratorResult {
            summary,
            artifacts,
            written_files,
            total_tasks: all_tasks.len(),
            completed_tasks: completed,
            failed_tasks: failed,
            needs_review_tasks: needs_review,
        })
    }

    /// Detect if a reviewer's response contains flags requesting human review.
    /// Looks for well-known markers that indicate the reviewer found critical issues.
    fn detect_review_flags(response: &str) -> bool {
        const MARKERS: &[&str] = &[
            "NEEDS_HUMAN_REVIEW",
            "HUMAN_REVIEW_REQUIRED",
            "CRITICAL_SECURITY_ISSUE",
            "\"approved\":false",
            "\"approved\": false",
        ];
        let lower = response.to_uppercase();
        MARKERS.iter().any(|m| lower.contains(&m.to_uppercase()))
    }

    /// Convert an artifact to a filename and cleaned content for disk output.
    fn artifact_to_file(artifact: &Artifact) -> (String, String) {
        let (default_name, ext) = match artifact.kind {
            ArtifactKind::Spec => ("spec", "md"),
            ArtifactKind::Code => ("code", "rs"),
            ArtifactKind::Test => ("tests", "rs"),
            ArtifactKind::Review => ("review", "md"),
            ArtifactKind::Report => ("report", "md"),
        };

        let filename = if let Some(path) = &artifact.file_path {
            path.clone()
        } else {
            format!("{}.{}", default_name, ext)
        };

        // Extract code from markdown code blocks if present
        let content = Self::extract_code_block(&artifact.content, ext);
        (filename, content)
    }

    /// Extract the first code block from markdown, or return content as-is.
    fn extract_code_block(content: &str, expected_lang: &str) -> String {
        // Look for ```rust or ``` blocks
        let markers = [format!("```{}", expected_lang), "```".to_string()];

        for marker in &markers {
            if let Some(start) = content.find(marker.as_str()) {
                let code_start = start + marker.len();
                // Skip to next line
                let code_start = content[code_start..]
                    .find('\n')
                    .map(|i| code_start + i + 1)
                    .unwrap_or(code_start);

                if let Some(end) = content[code_start..].find("```") {
                    return content[code_start..code_start + end].trim().to_string();
                }
            }
        }

        // No code block found, return as-is
        content.to_string()
    }
}

/// Result of a full orchestration pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorResult {
    pub summary: String,
    pub artifacts: Vec<Artifact>,
    pub written_files: Vec<String>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub needs_review_tasks: usize,
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
            written_files: vec![],
            total_tasks: 4,
            completed_tasks: 4,
            failed_tasks: 0,
            needs_review_tasks: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Done"));
        let parsed: OrchestratorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_tasks, 4);
    }

    #[test]
    fn test_detect_review_flags_positive() {
        assert!(Orchestrator::detect_review_flags(
            "Found issues: NEEDS_HUMAN_REVIEW before deploying"
        ));
        assert!(Orchestrator::detect_review_flags(
            "CRITICAL_SECURITY_ISSUE in auth module"
        ));
        assert!(Orchestrator::detect_review_flags(
            "Result: {\"approved\":false, \"reason\":\"too risky\"}"
        ));
        assert!(Orchestrator::detect_review_flags(
            "HUMAN_REVIEW_REQUIRED for this change"
        ));
    }

    #[test]
    fn test_detect_review_flags_negative() {
        assert!(!Orchestrator::detect_review_flags(
            "Code looks good. All tests pass."
        ));
        assert!(!Orchestrator::detect_review_flags(
            "No security issues found. Approved."
        ));
        assert!(!Orchestrator::detect_review_flags(""));
    }

    #[test]
    fn test_orchestrator_result_with_review() {
        let result = OrchestratorResult {
            summary: "Done with reviews".to_string(),
            artifacts: vec![],
            written_files: vec![],
            total_tasks: 4,
            completed_tasks: 3,
            failed_tasks: 0,
            needs_review_tasks: 1,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: OrchestratorResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.needs_review_tasks, 1);
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
