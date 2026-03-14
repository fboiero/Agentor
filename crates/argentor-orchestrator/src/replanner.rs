//! Dynamic re-planning for failed tasks.
//!
//! When a task fails during orchestration, the [`Replanner`] analyzes the
//! failure context and deterministically selects a [`RecoveryStrategy`].
//! Strategies range from simple retries with exponential backoff to
//! decomposing a task into smaller subtasks or escalating to a human.
//!
//! The [`ReplanHistory`] tracks all re-planning decisions for auditability.

use crate::types::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Recovery types
// ---------------------------------------------------------------------------

/// A subtask generated when a failed task is decomposed into smaller pieces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryTask {
    /// Human-readable description of the subtask.
    pub description: String,
    /// The agent role that should handle this subtask.
    pub assigned_to: AgentRole,
    /// IDs of other tasks that must complete before this one can start.
    pub dependencies: Vec<Uuid>,
}

impl RecoveryTask {
    /// Create a new recovery task.
    pub fn new(description: impl Into<String>, assigned_to: AgentRole) -> Self {
        Self {
            description: description.into(),
            assigned_to,
            dependencies: Vec::new(),
        }
    }

    /// Add dependency task IDs that must complete first.
    pub fn with_dependencies(mut self, deps: Vec<Uuid>) -> Self {
        self.dependencies = deps;
        self
    }
}

/// Strategy the replanner chooses for recovering from a task failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    /// Retry the same task with exponential backoff.
    Retry {
        /// Maximum number of retry attempts.
        max_attempts: u32,
        /// Base backoff in milliseconds (actual backoff includes jitter).
        backoff_ms: u64,
    },
    /// Reassign the task to a different agent role.
    Reassign {
        /// The new role that should handle the task.
        new_role: AgentRole,
    },
    /// Break the task into smaller, more manageable subtasks.
    Decompose {
        /// The list of subtasks to execute instead.
        subtasks: Vec<RecoveryTask>,
    },
    /// Skip the task and continue the pipeline.
    Skip {
        /// Why the task was skipped.
        reason: String,
    },
    /// Abort the entire pipeline.
    Abort {
        /// Why the pipeline was aborted.
        reason: String,
    },
    /// Mark the task for human review.
    Escalate,
}

// ---------------------------------------------------------------------------
// Failure context
// ---------------------------------------------------------------------------

/// Context describing a task failure, used by the replanner to decide strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureContext {
    /// ID of the failed task.
    pub task_id: Uuid,
    /// Human-readable description of the task.
    pub description: String,
    /// The agent role that was assigned to the task.
    pub assigned_to: AgentRole,
    /// The error message produced by the failure.
    pub error_message: String,
    /// How many times the task has already been attempted.
    pub attempt_count: u32,
    /// Whether the task is critical to the pipeline.
    pub is_critical: bool,
}

// ---------------------------------------------------------------------------
// Replanner
// ---------------------------------------------------------------------------

/// Analyzes failed tasks and deterministically selects recovery strategies.
///
/// The decision logic follows a priority-ordered set of rules:
///
/// 1. If retries remain, retry with exponential backoff.
/// 2. Permission/access errors trigger reassignment to a more privileged role.
/// 3. Timeout errors trigger a retry (even if retries are exhausted) with longer backoff.
/// 4. Complexity/token-limit errors trigger decomposition into subtasks.
/// 5. Critical tasks with exhausted retries are escalated to a human.
/// 6. Non-critical tasks with exhausted retries are skipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replanner {
    /// Maximum number of retries before falling through to other strategies.
    max_retries: u32,
}

impl Replanner {
    /// Create a new replanner with the given retry limit.
    pub fn new(max_retries: u32) -> Self {
        Self { max_retries }
    }

    /// Analyze a failure and return the recommended recovery strategy.
    ///
    /// The decision is fully deterministic given the same [`FailureContext`].
    pub fn analyze_failure(&self, ctx: &FailureContext) -> RecoveryStrategy {
        let error_lower = ctx.error_message.to_lowercase();

        // Rule 1 — retries remaining
        if ctx.attempt_count < self.max_retries {
            let backoff = Self::calculate_backoff(ctx.attempt_count, 500);
            info!(
                task_id = %ctx.task_id,
                attempt = ctx.attempt_count,
                backoff_ms = backoff,
                "Replanner: scheduling retry"
            );
            return RecoveryStrategy::Retry {
                max_attempts: self.max_retries,
                backoff_ms: backoff,
            };
        }

        // Rule 2 — permission / access denied
        if error_lower.contains("permission") || error_lower.contains("denied") {
            let new_role = Self::suggest_alternate_role(&ctx.assigned_to, &ctx.error_message);
            info!(
                task_id = %ctx.task_id,
                from = %ctx.assigned_to,
                to = %new_role,
                "Replanner: reassigning to role with higher permissions"
            );
            return RecoveryStrategy::Reassign { new_role };
        }

        // Rule 3 — timeout
        if error_lower.contains("timeout") {
            let backoff = Self::calculate_backoff(ctx.attempt_count, 2000);
            info!(
                task_id = %ctx.task_id,
                backoff_ms = backoff,
                "Replanner: retrying after timeout with longer backoff"
            );
            return RecoveryStrategy::Retry {
                max_attempts: ctx.attempt_count + 1,
                backoff_ms: backoff,
            };
        }

        // Rule 4 — too complex / token limit
        if error_lower.contains("too complex") || error_lower.contains("token limit") {
            let subtasks = vec![
                RecoveryTask::new(
                    format!("Part 1 of: {}", ctx.description),
                    ctx.assigned_to.clone(),
                ),
                RecoveryTask::new(
                    format!("Part 2 of: {}", ctx.description),
                    ctx.assigned_to.clone(),
                ),
            ];
            info!(
                task_id = %ctx.task_id,
                subtask_count = subtasks.len(),
                "Replanner: decomposing complex task"
            );
            return RecoveryStrategy::Decompose { subtasks };
        }

        // Rule 5 — critical task, retries exhausted → escalate
        if ctx.is_critical {
            info!(
                task_id = %ctx.task_id,
                "Replanner: escalating critical task for human review"
            );
            return RecoveryStrategy::Escalate;
        }

        // Rule 6 — non-critical, retries exhausted → skip
        info!(
            task_id = %ctx.task_id,
            "Replanner: skipping non-critical task after exhausting retries"
        );
        RecoveryStrategy::Skip {
            reason: format!(
                "Non-critical task failed after {} attempts: {}",
                ctx.attempt_count, ctx.error_message
            ),
        }
    }

    /// Suggest an alternate agent role when the current one lacks permissions.
    ///
    /// Heuristic: certain roles naturally have broader permissions (e.g.
    /// `DevOps` for infrastructure, `SecurityAuditor` for security-related
    /// errors, `Architect` for design-level issues).
    pub fn suggest_alternate_role(current: &AgentRole, error: &str) -> AgentRole {
        let error_lower = error.to_lowercase();

        if error_lower.contains("security") || error_lower.contains("vulnerability") {
            return AgentRole::SecurityAuditor;
        }
        if error_lower.contains("deploy") || error_lower.contains("infrastructure") {
            return AgentRole::DevOps;
        }
        if error_lower.contains("design") || error_lower.contains("architecture") {
            return AgentRole::Architect;
        }

        // Default escalation path based on current role.
        match current {
            AgentRole::Coder => AgentRole::Reviewer,
            AgentRole::Tester => AgentRole::Coder,
            AgentRole::Reviewer => AgentRole::Architect,
            AgentRole::Spec => AgentRole::Architect,
            AgentRole::DocumentWriter => AgentRole::Reviewer,
            _ => AgentRole::Orchestrator,
        }
    }

    /// Calculate exponential backoff with deterministic jitter.
    ///
    /// Formula: `base_ms * 2^attempt + attempt * 100` (capped at 60 seconds).
    pub fn calculate_backoff(attempt: u32, base_ms: u64) -> u64 {
        let exponential = base_ms.saturating_mul(1u64 << attempt.min(10));
        let jitter = u64::from(attempt).saturating_mul(100);
        exponential.saturating_add(jitter).min(60_000)
    }
}

// ---------------------------------------------------------------------------
// Replan history
// ---------------------------------------------------------------------------

/// A single re-planning decision record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanEntry {
    /// The task that was re-planned.
    pub task_id: Uuid,
    /// When the decision was made.
    pub timestamp: DateTime<Utc>,
    /// The strategy that was chosen.
    pub strategy_chosen: RecoveryStrategy,
    /// Human-readable reason for the decision.
    pub reason: String,
}

/// Audit log of all re-planning decisions made during a pipeline run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplanHistory {
    entries: Vec<ReplanEntry>,
}

impl ReplanHistory {
    /// Create an empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new re-planning decision.
    pub fn record(&mut self, entry: ReplanEntry) {
        info!(
            task_id = %entry.task_id,
            reason = %entry.reason,
            "Replan decision recorded"
        );
        self.entries.push(entry);
    }

    /// Return all re-planning entries for a given task.
    pub fn entries_for_task(&self, task_id: Uuid) -> Vec<&ReplanEntry> {
        self.entries
            .iter()
            .filter(|e| e.task_id == task_id)
            .collect()
    }

    /// Total number of re-planning decisions recorded.
    pub fn total_replans(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_ctx(error: &str, attempts: u32, critical: bool) -> FailureContext {
        FailureContext {
            task_id: Uuid::new_v4(),
            description: "Test task".to_string(),
            assigned_to: AgentRole::Coder,
            error_message: error.to_string(),
            attempt_count: attempts,
            is_critical: critical,
        }
    }

    // -- Strategy selection tests --

    #[test]
    fn test_retry_when_attempts_remain() {
        let replanner = Replanner::new(3);
        let ctx = make_ctx("some random error", 1, false);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Retry {
                max_attempts,
                backoff_ms,
            } => {
                assert_eq!(max_attempts, 3);
                assert!(backoff_ms > 0);
            }
            other => panic!("Expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn test_retry_first_attempt() {
        let replanner = Replanner::new(5);
        let ctx = make_ctx("connection reset", 0, true);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Retry {
                max_attempts,
                backoff_ms,
            } => {
                assert_eq!(max_attempts, 5);
                // attempt 0: 500 * 2^0 + 0*100 = 500
                assert_eq!(backoff_ms, 500);
            }
            other => panic!("Expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn test_reassign_on_permission_error() {
        let replanner = Replanner::new(2);
        let ctx = make_ctx("permission denied: cannot access /etc/secrets", 3, false);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Reassign { new_role } => {
                // Coder → Reviewer by default escalation
                assert_eq!(new_role, AgentRole::Reviewer);
            }
            other => panic!("Expected Reassign, got {other:?}"),
        }
    }

    #[test]
    fn test_reassign_on_denied_error() {
        let replanner = Replanner::new(1);
        let ctx = make_ctx("access denied for resource X", 2, false);
        let strategy = replanner.analyze_failure(&ctx);
        assert!(matches!(strategy, RecoveryStrategy::Reassign { .. }));
    }

    #[test]
    fn test_retry_on_timeout() {
        let replanner = Replanner::new(2);
        let ctx = make_ctx("operation timeout after 30s", 3, false);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Retry {
                max_attempts,
                backoff_ms,
            } => {
                assert_eq!(max_attempts, 4); // attempt_count + 1
                assert!(backoff_ms >= 2000);
            }
            other => panic!("Expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn test_decompose_on_too_complex() {
        let replanner = Replanner::new(2);
        let ctx = make_ctx("task is too complex for single agent", 3, false);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Decompose { subtasks } => {
                assert_eq!(subtasks.len(), 2);
                assert!(subtasks[0].description.contains("Part 1"));
                assert!(subtasks[1].description.contains("Part 2"));
            }
            other => panic!("Expected Decompose, got {other:?}"),
        }
    }

    #[test]
    fn test_decompose_on_token_limit() {
        let replanner = Replanner::new(1);
        let ctx = make_ctx("token limit exceeded", 2, false);
        let strategy = replanner.analyze_failure(&ctx);
        assert!(matches!(strategy, RecoveryStrategy::Decompose { .. }));
    }

    #[test]
    fn test_escalate_critical_exhausted() {
        let replanner = Replanner::new(2);
        let ctx = make_ctx("unknown internal error", 5, true);
        let strategy = replanner.analyze_failure(&ctx);
        assert!(matches!(strategy, RecoveryStrategy::Escalate));
    }

    #[test]
    fn test_skip_non_critical_exhausted() {
        let replanner = Replanner::new(2);
        let ctx = make_ctx("some transient glitch", 5, false);
        let strategy = replanner.analyze_failure(&ctx);
        match strategy {
            RecoveryStrategy::Skip { reason } => {
                assert!(reason.contains("Non-critical"));
                assert!(reason.contains("5 attempts"));
            }
            other => panic!("Expected Skip, got {other:?}"),
        }
    }

    // -- Alternate role suggestion --

    #[test]
    fn test_suggest_alternate_role_security() {
        let role =
            Replanner::suggest_alternate_role(&AgentRole::Coder, "security vulnerability found");
        assert_eq!(role, AgentRole::SecurityAuditor);
    }

    #[test]
    fn test_suggest_alternate_role_deploy() {
        let role = Replanner::suggest_alternate_role(&AgentRole::Coder, "deploy pipeline failed");
        assert_eq!(role, AgentRole::DevOps);
    }

    #[test]
    fn test_suggest_alternate_role_default_escalation() {
        let role = Replanner::suggest_alternate_role(&AgentRole::Coder, "generic error");
        assert_eq!(role, AgentRole::Reviewer);

        let role2 = Replanner::suggest_alternate_role(&AgentRole::Tester, "generic error");
        assert_eq!(role2, AgentRole::Coder);

        let role3 = Replanner::suggest_alternate_role(&AgentRole::Reviewer, "generic error");
        assert_eq!(role3, AgentRole::Architect);
    }

    // -- Backoff calculation --

    #[test]
    fn test_calculate_backoff_exponential() {
        // attempt 0: 1000 * 1 + 0 = 1000
        assert_eq!(Replanner::calculate_backoff(0, 1000), 1000);
        // attempt 1: 1000 * 2 + 100 = 2100
        assert_eq!(Replanner::calculate_backoff(1, 1000), 2100);
        // attempt 2: 1000 * 4 + 200 = 4200
        assert_eq!(Replanner::calculate_backoff(2, 1000), 4200);
    }

    #[test]
    fn test_calculate_backoff_capped() {
        // Very high attempt should cap at 60_000 ms
        let backoff = Replanner::calculate_backoff(20, 5000);
        assert_eq!(backoff, 60_000);
    }

    // -- ReplanHistory --

    #[test]
    fn test_replan_history_record_and_query() {
        let mut history = ReplanHistory::new();
        let task_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();

        history.record(ReplanEntry {
            task_id,
            timestamp: Utc::now(),
            strategy_chosen: RecoveryStrategy::Escalate,
            reason: "critical failure".to_string(),
        });
        history.record(ReplanEntry {
            task_id: other_id,
            timestamp: Utc::now(),
            strategy_chosen: RecoveryStrategy::Skip {
                reason: "non-critical".to_string(),
            },
            reason: "skipped".to_string(),
        });
        history.record(ReplanEntry {
            task_id,
            timestamp: Utc::now(),
            strategy_chosen: RecoveryStrategy::Retry {
                max_attempts: 3,
                backoff_ms: 500,
            },
            reason: "second attempt".to_string(),
        });

        assert_eq!(history.total_replans(), 3);
        assert_eq!(history.entries_for_task(task_id).len(), 2);
        assert_eq!(history.entries_for_task(other_id).len(), 1);
        assert_eq!(history.entries_for_task(Uuid::new_v4()).len(), 0);
    }
}
