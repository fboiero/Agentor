pub mod agent_delegate;
pub mod artifact_store;
pub mod browser;
pub mod browser_automation;
pub mod docker_sandbox;
pub mod file_read;
pub mod file_write;
pub mod http_fetch;
pub mod human_approval;
pub mod memory;
pub mod shell;
pub mod stdin_approval;
pub mod task_status;

pub use agent_delegate::{AgentDelegateSkill, TaskInfo, TaskQueueHandle, TaskSummary};
pub use artifact_store::{ArtifactBackend, ArtifactStoreSkill, InMemoryArtifactBackend};
pub use browser::BrowserSkill;
pub use browser_automation::{BrowserAction, BrowserAutomationSkill, BrowserConfig, BrowserResult};
pub use file_read::FileReadSkill;
pub use file_write::FileWriteSkill;
pub use http_fetch::HttpFetchSkill;
pub use human_approval::{
    ApprovalChannel, ApprovalDecision, ApprovalRequest, AutoApproveChannel,
    CallbackApprovalChannel, HumanApprovalSkill, RiskLevel,
};
pub use memory::{MemorySearchSkill, MemoryStoreSkill};
pub use shell::ShellSkill;
pub use stdin_approval::StdinApprovalChannel;
pub use task_status::TaskStatusSkill;

pub use docker_sandbox::{DockerSandboxConfig, ExecResult};

#[cfg(feature = "docker")]
pub use docker_sandbox::{DockerSandbox, DockerShellSkill};

#[cfg(feature = "browser")]
pub use browser_automation::BrowserAutomation;

use agentor_memory::{EmbeddingProvider, VectorStore};
use agentor_skills::SkillRegistry;
use std::sync::Arc;

/// Register all built-in skills into the given registry.
/// Uses the provided vector store and embedding provider for memory skills.
pub fn register_builtins_with_memory(
    registry: &mut SkillRegistry,
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn EmbeddingProvider>,
) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(MemoryStoreSkill::new(
        store.clone(),
        embedder.clone(),
    )));
    registry.register(Arc::new(MemorySearchSkill::new(store, embedder)));
    registry.register(Arc::new(HumanApprovalSkill::auto_approve()));
}

/// Register built-in skills without memory (backwards compatible).
pub fn register_builtins(registry: &mut SkillRegistry) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(HumanApprovalSkill::auto_approve()));
}

/// Register built-in skills with a custom approval channel for HITL.
pub fn register_builtins_with_approval(
    registry: &mut SkillRegistry,
    approval_channel: Arc<dyn ApprovalChannel>,
) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(HumanApprovalSkill::new(approval_channel)));
}

/// Register all built-in skills including memory and a custom approval channel.
pub fn register_all(
    registry: &mut SkillRegistry,
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn EmbeddingProvider>,
    approval_channel: Arc<dyn ApprovalChannel>,
) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(MemoryStoreSkill::new(
        store.clone(),
        embedder.clone(),
    )));
    registry.register(Arc::new(MemorySearchSkill::new(store, embedder)));
    registry.register(Arc::new(HumanApprovalSkill::new(approval_channel)));
}

/// Register orchestration-specific skills (artifact_store, agent_delegate, task_status).
/// These require a TaskQueueHandle and ArtifactBackend from the orchestrator.
pub fn register_orchestration_builtins(
    registry: &mut SkillRegistry,
    queue: Arc<dyn TaskQueueHandle>,
    artifact_backend: Arc<dyn ArtifactBackend>,
) {
    registry.register(Arc::new(ArtifactStoreSkill::new(artifact_backend)));
    registry.register(Arc::new(AgentDelegateSkill::new(queue.clone())));
    registry.register(Arc::new(TaskStatusSkill::new(queue)));
}

/// Register built-in skills plus the browser automation skill.
///
/// This registers all the standard builtins and adds `BrowserAutomationSkill`
/// configured with the given `BrowserConfig`. The actual WebDriver connection
/// is established lazily when the skill is first invoked, and only when the
/// `browser` feature is enabled.
pub fn register_builtins_with_browser(registry: &mut SkillRegistry, config: BrowserConfig) {
    registry.register(Arc::new(ShellSkill::new()));
    registry.register(Arc::new(FileReadSkill::new()));
    registry.register(Arc::new(FileWriteSkill::new()));
    registry.register(Arc::new(HttpFetchSkill::new()));
    registry.register(Arc::new(BrowserSkill::new()));
    registry.register(Arc::new(HumanApprovalSkill::auto_approve()));
    registry.register(Arc::new(BrowserAutomationSkill::new(config)));
}
