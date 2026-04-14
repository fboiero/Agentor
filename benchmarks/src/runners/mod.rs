//! Framework runners. Each framework that participates in benchmarks
//! implements [`Runner`], producing a [`TaskResult`] from a [`Task`].

use crate::task::{Task, TaskResult};
use async_trait::async_trait;
use std::path::Path;

pub mod argentor;
pub mod external;
pub mod mock;

pub use argentor::ArgentorRunner;
pub use external::ExternalRunner;
pub use mock::MockRunner;

/// Identifies the framework under test — used for result tagging and reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    Argentor,
    Langchain,
    Crewai,
    PydanticAi,
    ClaudeAgentSdk,
    OpenaiAgentsSdk,
    Mock,
}

impl RunnerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Argentor => "argentor",
            Self::Langchain => "langchain",
            Self::Crewai => "crewai",
            Self::PydanticAi => "pydantic_ai",
            Self::ClaudeAgentSdk => "claude_agent_sdk",
            Self::OpenaiAgentsSdk => "openai_agents_sdk",
            Self::Mock => "mock",
        }
    }
}

/// A framework runner capable of executing a [`Task`].
#[async_trait]
pub trait Runner: Send + Sync {
    fn kind(&self) -> RunnerKind;

    /// Human-readable name (version included).
    fn name(&self) -> String;

    /// Execute the task, resolving any file references relative to `task_dir`.
    async fn run(&self, task: &Task, task_dir: &Path) -> anyhow::Result<TaskResult>;
}
