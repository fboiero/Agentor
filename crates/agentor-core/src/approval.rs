//! Approval types for human-in-the-loop (HITL) workflows.
//!
//! These types live in `agentor-core` so that both `agentor-builtins` (which
//! implements the `HumanApprovalSkill`) and `agentor-gateway` (which
//! implements the `WsApprovalChannel`) can share them without circular deps.

use crate::AgentorResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Risk level for a human-in-the-loop approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Low-risk action that rarely needs human review.
    Low,
    /// Medium-risk action (default when risk cannot be determined).
    Medium,
    /// High-risk action that should be reviewed before execution.
    High,
    /// Critical action that must always be approved by a human.
    Critical,
}

impl RiskLevel {
    /// Parses a string into a [`RiskLevel`], defaulting to [`RiskLevel::Medium`] for unknown values.
    pub fn parse_level(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => RiskLevel::Low,
            "medium" => RiskLevel::Medium,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Medium,
        }
    }
}

/// A request sent to a human reviewer for approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Identifier of the task that requires approval.
    pub task_id: String,
    /// Human-readable description of the action to be approved.
    pub description: String,
    /// The assessed risk level of the action.
    pub risk_level: RiskLevel,
    /// Additional context to help the reviewer make a decision.
    pub context: String,
}

/// The decision made by a human reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    /// Whether the action was approved.
    pub approved: bool,
    /// Optional reason provided by the reviewer.
    pub reason: Option<String>,
    /// Identifier of the human reviewer who made the decision.
    pub reviewer: String,
}

/// Channel through which approval requests are sent and decisions are received.
///
/// Implementations can be CLI prompts, WebSocket handlers, Slack bots, etc.
#[async_trait]
pub trait ApprovalChannel: Send + Sync {
    /// Sends an approval request and waits for the reviewer's decision.
    async fn request_approval(&self, request: ApprovalRequest) -> AgentorResult<ApprovalDecision>;
}
