use crate::iso27001::{AccessOutcome, Iso27001Module};
use crate::iso42001::Iso42001Module;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Events emitted by the runtime that are relevant for compliance tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceEvent {
    /// A tool/skill was called by an agent.
    ToolCall {
        agent_id: String,
        tool_name: String,
        timestamp: DateTime<Utc>,
        success: bool,
    },
    /// An agent started working on a task.
    TaskStarted {
        task_id: Uuid,
        role: String,
        description: String,
        timestamp: DateTime<Utc>,
    },
    /// An agent completed a task.
    TaskCompleted {
        task_id: Uuid,
        role: String,
        duration_ms: u64,
        artifacts_count: usize,
        timestamp: DateTime<Utc>,
    },
    /// A human approval was requested.
    ApprovalRequested {
        task_id: String,
        risk_level: String,
        timestamp: DateTime<Utc>,
    },
    /// A human approval decision was made.
    ApprovalDecided {
        task_id: String,
        approved: bool,
        reviewer: String,
        timestamp: DateTime<Utc>,
    },
}

/// Trait for receiving compliance-relevant events from the runtime.
#[async_trait]
pub trait ComplianceHook: Send + Sync {
    async fn on_event(&self, event: &ComplianceEvent);
}

/// Composite hook that dispatches events to multiple hooks.
pub struct ComplianceHookChain {
    hooks: Vec<Arc<dyn ComplianceHook>>,
}

impl ComplianceHookChain {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Add a hook to the chain.
    pub fn add(&mut self, hook: Arc<dyn ComplianceHook>) {
        self.hooks.push(hook);
    }

    /// Emit an event to all hooks in the chain.
    pub async fn emit(&self, event: ComplianceEvent) {
        for hook in &self.hooks {
            hook.on_event(&event).await;
        }
    }

    /// Get the number of hooks in the chain.
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }
}

impl Default for ComplianceHookChain {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook that maps runtime events to ISO 27001 access control events.
pub struct Iso27001Hook {
    module: Arc<Iso27001Module>,
}

impl Iso27001Hook {
    pub fn new(module: Arc<Iso27001Module>) -> Self {
        Self { module }
    }

    /// Get a reference to the underlying module (for report generation).
    pub fn module(&self) -> &Arc<Iso27001Module> {
        &self.module
    }
}

#[async_trait]
impl ComplianceHook for Iso27001Hook {
    async fn on_event(&self, event: &ComplianceEvent) {
        match event {
            ComplianceEvent::ToolCall {
                agent_id,
                tool_name,
                success,
                ..
            } => {
                let outcome = if *success {
                    AccessOutcome::Granted
                } else {
                    AccessOutcome::Denied
                };
                self.module
                    .log_access(agent_id, tool_name, "skill_registry", outcome)
                    .await;
            }
            ComplianceEvent::TaskStarted {
                role, description, ..
            } => {
                self.module
                    .log_access(role, "task_execution", description, AccessOutcome::Granted)
                    .await;
            }
            _ => {} // Other events not relevant to ISO 27001
        }
    }
}

/// Hook that maps runtime events to ISO 42001 AI transparency logs.
pub struct Iso42001Hook {
    module: Arc<Iso42001Module>,
    /// System ID registered in the AI inventory.
    system_id: Uuid,
}

impl Iso42001Hook {
    pub fn new(module: Arc<Iso42001Module>, system_id: Uuid) -> Self {
        Self { module, system_id }
    }

    /// Get a reference to the underlying module.
    pub fn module(&self) -> &Arc<Iso42001Module> {
        &self.module
    }
}

#[async_trait]
impl ComplianceHook for Iso42001Hook {
    async fn on_event(&self, event: &ComplianceEvent) {
        match event {
            ComplianceEvent::TaskCompleted {
                role,
                duration_ms,
                artifacts_count,
                ..
            } => {
                self.module
                    .log_transparency(
                        self.system_id,
                        "task_execution",
                        &format!("Worker: {}", role),
                        &format!(
                            "Completed in {}ms, {} artifacts",
                            duration_ms, artifacts_count
                        ),
                        None,
                    )
                    .await;
            }
            ComplianceEvent::ApprovalRequested {
                task_id,
                risk_level,
                ..
            } => {
                self.module
                    .log_transparency(
                        self.system_id,
                        "hitl_approval_requested",
                        &format!("Task: {} (risk: {})", task_id, risk_level),
                        "Awaiting human decision",
                        None,
                    )
                    .await;
            }
            ComplianceEvent::ApprovalDecided {
                task_id,
                approved,
                reviewer,
                ..
            } => {
                let decision = if *approved { "approved" } else { "denied" };
                self.module
                    .log_transparency(
                        self.system_id,
                        "hitl_approval_decided",
                        &format!("Task: {}", task_id),
                        &format!("{} by {}", decision, reviewer),
                        Some(&format!("Human oversight: {}", reviewer)),
                    )
                    .await;
            }
            _ => {} // ToolCall and TaskStarted less relevant for AI transparency
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hook_chain_dispatch() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingHook(Arc<AtomicUsize>);

        #[async_trait]
        impl ComplianceHook for CountingHook {
            async fn on_event(&self, _event: &ComplianceEvent) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let count = Arc::new(AtomicUsize::new(0));
        let mut chain = ComplianceHookChain::new();
        chain.add(Arc::new(CountingHook(count.clone())));
        chain.add(Arc::new(CountingHook(count.clone())));

        chain
            .emit(ComplianceEvent::ToolCall {
                agent_id: "test".into(),
                tool_name: "shell".into(),
                timestamp: Utc::now(),
                success: true,
            })
            .await;

        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_iso27001_hook_tool_call() {
        let module = Arc::new(Iso27001Module::new());
        let hook = Iso27001Hook::new(module.clone());

        hook.on_event(&ComplianceEvent::ToolCall {
            agent_id: "coder:task-1".into(),
            tool_name: "file_write".into(),
            timestamp: Utc::now(),
            success: true,
        })
        .await;

        let count = module.access_event_count().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_iso27001_hook_task_started() {
        let module = Arc::new(Iso27001Module::new());
        let hook = Iso27001Hook::new(module.clone());

        hook.on_event(&ComplianceEvent::TaskStarted {
            task_id: Uuid::new_v4(),
            role: "coder".into(),
            description: "Implement auth".into(),
            timestamp: Utc::now(),
        })
        .await;

        let count = module.access_event_count().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_iso42001_hook_task_completed() {
        let module = Arc::new(Iso42001Module::new());
        let system_id = Uuid::new_v4();
        let hook = Iso42001Hook::new(module.clone(), system_id);

        hook.on_event(&ComplianceEvent::TaskCompleted {
            task_id: Uuid::new_v4(),
            role: "tester".into(),
            duration_ms: 1500,
            artifacts_count: 2,
            timestamp: Utc::now(),
        })
        .await;

        let count = module.transparency_log_count().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_iso42001_hook_approval_flow() {
        let module = Arc::new(Iso42001Module::new());
        let system_id = Uuid::new_v4();
        let hook = Iso42001Hook::new(module.clone(), system_id);

        hook.on_event(&ComplianceEvent::ApprovalRequested {
            task_id: "deploy-1".into(),
            risk_level: "high".into(),
            timestamp: Utc::now(),
        })
        .await;

        hook.on_event(&ComplianceEvent::ApprovalDecided {
            task_id: "deploy-1".into(),
            approved: true,
            reviewer: "admin".into(),
            timestamp: Utc::now(),
        })
        .await;

        let count = module.transparency_log_count().await;
        assert_eq!(count, 2);
    }
}
