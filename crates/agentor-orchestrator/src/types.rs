use agentor_agent::ModelConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of each agent in the multi-agent system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    /// Decomposes tasks, delegates to workers, synthesizes results.
    Orchestrator,
    /// Analyzes requirements and generates specifications.
    Spec,
    /// Generates Rust code from specifications.
    Coder,
    /// Writes and runs tests.
    Tester,
    /// Reviews code for quality, security, and compliance.
    Reviewer,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Orchestrator => write!(f, "orchestrator"),
            AgentRole::Spec => write!(f, "spec"),
            AgentRole::Coder => write!(f, "coder"),
            AgentRole::Tester => write!(f, "tester"),
            AgentRole::Reviewer => write!(f, "reviewer"),
        }
    }
}

/// Configuration for a specialized agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub role: AgentRole,
    pub model: ModelConfig,
    pub system_prompt: String,
    /// Individual skill names this agent may use (legacy / fine-grained control).
    pub allowed_skills: Vec<String>,
    /// Tool group name — when set, the agent receives all skills in this group
    /// instead of the individual `allowed_skills` list.
    /// Takes precedence over `allowed_skills`.
    pub tool_group: Option<String>,
    pub max_turns: u32,
}

/// Status of a task in the execution queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    NeedsHumanReview,
    Completed,
    Failed { reason: String },
}

/// Kind of artifact produced by an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Spec,
    Code,
    Test,
    Review,
    Report,
}

/// An artifact produced by an agent during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub content: String,
    pub file_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Artifact {
    pub fn new(kind: ArtifactKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            file_path: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }
}

/// A task in the orchestration queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub description: String,
    pub assigned_to: AgentRole,
    pub status: TaskStatus,
    pub dependencies: Vec<Uuid>,
    pub artifacts: Vec<Artifact>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Parent task ID if this is a sub-task spawned by another task.
    #[serde(default)]
    pub parent_task: Option<Uuid>,
    /// Depth in the task hierarchy (0 = root task).
    #[serde(default)]
    pub depth: u32,
}

impl Task {
    pub fn new(description: impl Into<String>, assigned_to: AgentRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            assigned_to,
            status: TaskStatus::Pending,
            dependencies: Vec::new(),
            artifacts: Vec::new(),
            created_at: Utc::now(),
            completed_at: None,
            parent_task: None,
            depth: 0,
        }
    }

    pub fn with_dependencies(mut self, deps: Vec<Uuid>) -> Self {
        self.dependencies = deps;
        self
    }

    pub fn is_ready(&self, completed_ids: &[Uuid]) -> bool {
        self.status == TaskStatus::Pending
            && self
                .dependencies
                .iter()
                .all(|dep| completed_ids.contains(dep))
    }

    pub fn add_artifact(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }
}

/// Metrics tracked per agent execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub total_turns: u32,
    pub total_tool_calls: u32,
    pub errors: u32,
    pub duration_ms: u64,
    pub tokens_used: u64,
}

/// Real-time snapshot of an agent worker's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub role: AgentRole,
    pub current_task: Option<Uuid>,
    pub status: WorkerStatus,
    pub metrics: AgentMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    Idle,
    Working,
    WaitingForApproval,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = Task::new("Implement auth module", AgentRole::Coder);
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.assigned_to, AgentRole::Coder);
        assert!(task.dependencies.is_empty());
        assert!(task.artifacts.is_empty());
    }

    #[test]
    fn test_task_is_ready_no_deps() {
        let task = Task::new("Simple task", AgentRole::Spec);
        assert!(task.is_ready(&[]));
    }

    #[test]
    fn test_task_is_ready_with_deps() {
        let dep_id = Uuid::new_v4();
        let task = Task::new("Dependent task", AgentRole::Coder).with_dependencies(vec![dep_id]);
        assert!(!task.is_ready(&[]));
        assert!(task.is_ready(&[dep_id]));
    }

    #[test]
    fn test_task_not_ready_when_running() {
        let mut task = Task::new("Running task", AgentRole::Tester);
        task.status = TaskStatus::Running;
        assert!(!task.is_ready(&[]));
    }

    #[test]
    fn test_artifact_creation() {
        let artifact = Artifact::new(ArtifactKind::Code, "fn main() {}").with_path("src/main.rs");
        assert_eq!(artifact.kind, ArtifactKind::Code);
        assert_eq!(artifact.file_path.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(AgentRole::Orchestrator.to_string(), "orchestrator");
        assert_eq!(AgentRole::Coder.to_string(), "coder");
        assert_eq!(AgentRole::Reviewer.to_string(), "reviewer");
    }

    #[test]
    fn test_task_add_artifact() {
        let mut task = Task::new("Code task", AgentRole::Coder);
        task.add_artifact(Artifact::new(ArtifactKind::Code, "let x = 1;"));
        assert_eq!(task.artifacts.len(), 1);
    }

    #[test]
    fn test_agent_metrics_default() {
        let metrics = AgentMetrics::default();
        assert_eq!(metrics.total_turns, 0);
        assert_eq!(metrics.errors, 0);
    }

    #[test]
    fn test_task_status_serialization() {
        let status = TaskStatus::Failed {
            reason: "timeout".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("timeout"));
        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }
}
