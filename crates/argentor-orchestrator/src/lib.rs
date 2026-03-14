//! Multi-agent orchestration engine with task queue, monitoring, and scheduling.
//!
//! Implements the Orchestrator-Workers pattern for decomposing complex tasks
//! into sub-tasks, dispatching them to specialized worker agents, and
//! aggregating results with progress tracking and compliance hooks.
//!
//! # Main types
//!
//! - [`Orchestrator`] — Top-level engine that decomposes and executes multi-agent pipelines.
//! - [`TaskQueue`] — Priority queue for distributing tasks to worker agents.
//! - [`AgentMonitor`] — Tracks agent health, metrics, and lifecycle.
//! - [`Scheduler`] — Cron-based job scheduler for recurring tasks.
//! - [`AgentProfile`] — Configuration profile defining an agent's role and capabilities.

/// Orchestration engine and pipeline execution.
pub mod engine;
/// Agent health and metrics monitoring.
pub mod monitor;
/// Default agent profiles and role definitions.
pub mod profiles;
/// Cron-based job scheduler.
pub mod scheduler;
/// Sub-agent spawning utilities.
pub mod spawner;
/// Priority task queue.
pub mod task_queue;
/// Shared orchestration types (Task, AgentProfile, Artifact, etc.).
pub mod types;

pub use engine::{BackendFactory, Orchestrator, OrchestratorResult};
pub use monitor::AgentMonitor;
pub use profiles::default_profiles;
pub use scheduler::{ScheduledJob, Scheduler};
pub use spawner::{SpawnRequest, SubAgentSpawner};
pub use task_queue::TaskQueue;
pub use types::{
    AgentMetrics, AgentProfile, AgentRole, AgentState, Artifact, ArtifactKind, Task, TaskStatus,
    WorkerStatus,
};
