//! Built-in skills for the Agentor framework.
//!
//! Provides ready-to-use skills covering shell execution, file I/O, HTTP fetching,
//! semantic memory, artifact storage, browser automation, Docker sandboxing,
//! human-in-the-loop approval, and agent delegation.
//!
//! # Main entry points
//!
//! - [`register_builtins()`] — Register the standard set of built-in skills.
//! - [`register_builtins_with_memory()`] — Register builtins including memory skills.
//! - [`register_builtins_with_approval()`] — Register builtins with a custom approval channel.
//! - [`register_all()`] — Register builtins with memory and approval.
//! - [`register_orchestration_builtins()`] — Register orchestration-specific skills.
//! - [`register_builtins_with_browser()`] — Register builtins with browser automation.

/// Agent delegation skill for sub-agent spawning.
pub mod agent_delegate;
/// Artifact storage skill and backends.
pub mod artifact_store;
/// Simple browser skill (URL fetching).
pub mod browser;
/// WebDriver-based browser automation skill.
pub mod browser_automation;
/// Docker-sandboxed shell execution.
pub mod docker_sandbox;
/// File read skill.
pub mod file_read;
/// File write skill.
pub mod file_write;
/// HTTP fetch skill.
pub mod http_fetch;
/// Human-in-the-loop approval skill and channels.
pub mod human_approval;
/// Semantic memory store and search skills.
pub mod memory;
/// Shell command execution skill.
pub mod shell;
/// Stdin-based interactive approval channel.
pub mod stdin_approval;
/// Task status reporting skill.
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
