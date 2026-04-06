use argentor_agent::ModelConfig;
use argentor_security::PermissionSet;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of each agent in the multi-agent system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// System design and API design.
    Architect,
    /// Security review and vulnerability analysis.
    SecurityAuditor,
    /// Deployment, infrastructure, and CI/CD.
    DevOps,
    /// Documentation writing and maintenance.
    DocumentWriter,
    /// User-defined custom role.
    Custom(String),
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Orchestrator => write!(f, "orchestrator"),
            AgentRole::Spec => write!(f, "spec"),
            AgentRole::Coder => write!(f, "coder"),
            AgentRole::Tester => write!(f, "tester"),
            AgentRole::Reviewer => write!(f, "reviewer"),
            AgentRole::Architect => write!(f, "architect"),
            AgentRole::SecurityAuditor => write!(f, "security_auditor"),
            AgentRole::DevOps => write!(f, "devops"),
            AgentRole::DocumentWriter => write!(f, "document_writer"),
            AgentRole::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

/// Configuration for a specialized agent within the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Role this agent fulfills in the multi-agent pipeline.
    pub role: AgentRole,
    /// LLM configuration (provider, model id, API key, etc.).
    pub model: ModelConfig,
    /// System prompt injected at the start of every conversation.
    pub system_prompt: String,
    /// Individual skill names this agent may use (legacy / fine-grained control).
    pub allowed_skills: Vec<String>,
    /// Tool group name -- when set, the agent receives all skills in this group
    /// instead of the individual `allowed_skills` list.
    /// Takes precedence over `allowed_skills`.
    pub tool_group: Option<String>,
    /// Maximum number of agentic loop turns before the agent stops.
    pub max_turns: u32,
    /// Per-role permissions -- controls what capabilities this agent has.
    #[serde(default)]
    pub permissions: PermissionSet,
}

/// Status of a task in the execution queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task is waiting to be picked up by a worker agent.
    Pending,
    /// Task is currently being executed.
    Running,
    /// Task requires human approval before proceeding.
    NeedsHumanReview,
    /// Task finished successfully.
    Completed,
    /// Task terminated with an error.
    Failed {
        /// Human-readable description of the failure.
        reason: String,
    },
}

/// Kind of artifact produced by an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    /// Requirements or design specification.
    Spec,
    /// Source code.
    Code,
    /// Test code or test results.
    Test,
    /// Code review or audit output.
    Review,
    /// Summary report or compliance report.
    Report,
}

/// An artifact produced by an agent during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Category of the artifact.
    pub kind: ArtifactKind,
    /// Textual content of the artifact.
    pub content: String,
    /// Optional file path the artifact was written to.
    pub file_path: Option<String>,
    /// UTC timestamp of when the artifact was created.
    pub created_at: DateTime<Utc>,
}

impl Artifact {
    /// Create a new artifact with the given kind and content.
    pub fn new(kind: ArtifactKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            file_path: None,
            created_at: Utc::now(),
        }
    }

    /// Set the file path for this artifact (builder pattern).
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }
}

/// A task in the orchestration queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for this task.
    pub id: Uuid,
    /// Human-readable description of what needs to be done.
    pub description: String,
    /// Role of the agent this task is assigned to.
    pub assigned_to: AgentRole,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Task IDs that must complete before this task can start.
    pub dependencies: Vec<Uuid>,
    /// Artifacts produced during execution.
    pub artifacts: Vec<Artifact>,
    /// UTC timestamp of task creation.
    pub created_at: DateTime<Utc>,
    /// UTC timestamp of when the task finished (if completed or failed).
    pub completed_at: Option<DateTime<Utc>>,
    /// Parent task ID if this is a sub-task spawned by another task.
    #[serde(default)]
    pub parent_task: Option<Uuid>,
    /// Depth in the task hierarchy (0 = root task).
    #[serde(default)]
    pub depth: u32,
}

impl Task {
    /// Create a new pending task assigned to the given agent role.
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

    /// Set the dependency list (builder pattern).
    pub fn with_dependencies(mut self, deps: Vec<Uuid>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Returns `true` if the task is pending and all its dependencies have completed.
    pub fn is_ready(&self, completed_ids: &[Uuid]) -> bool {
        self.status == TaskStatus::Pending
            && self
                .dependencies
                .iter()
                .all(|dep| completed_ids.contains(dep))
    }

    /// Append an artifact to this task.
    pub fn add_artifact(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }
}

/// Metrics tracked per agent execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    /// Number of agentic loop turns completed.
    pub total_turns: u32,
    /// Number of tool calls made across all turns.
    pub total_tool_calls: u32,
    /// Number of errors encountered.
    pub errors: u32,
    /// Total wall-clock execution time in milliseconds.
    pub duration_ms: u64,
    /// Approximate token count consumed.
    pub tokens_used: u64,
}

/// Real-time snapshot of an agent worker's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Role this worker agent fulfills.
    pub role: AgentRole,
    /// Task the agent is currently executing, if any.
    pub current_task: Option<Uuid>,
    /// Current operational status.
    pub status: WorkerStatus,
    /// Cumulative metrics for this agent.
    pub metrics: AgentMetrics,
}

/// Operational status of a worker agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerStatus {
    /// Agent is idle and ready to accept tasks.
    Idle,
    /// Agent is actively executing a task.
    Working,
    /// Agent has paused and is awaiting human approval.
    WaitingForApproval,
    /// Agent encountered a fatal error.
    Error,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        assert_eq!(AgentRole::Architect.to_string(), "architect");
        assert_eq!(AgentRole::SecurityAuditor.to_string(), "security_auditor");
        assert_eq!(AgentRole::DevOps.to_string(), "devops");
        assert_eq!(AgentRole::DocumentWriter.to_string(), "document_writer");
        assert_eq!(
            AgentRole::Custom("my_role".to_string()).to_string(),
            "custom:my_role"
        );
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
