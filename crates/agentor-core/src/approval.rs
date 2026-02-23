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
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
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
    pub task_id: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub context: String,
}

/// The decision made by a human reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub reason: Option<String>,
    pub reviewer: String,
}

/// Channel through which approval requests are sent and decisions are received.
/// Implementations can be CLI prompts, WebSocket handlers, Slack bots, etc.
#[async_trait]
pub trait ApprovalChannel: Send + Sync {
    async fn request_approval(&self, request: ApprovalRequest) -> AgentorResult<ApprovalDecision>;
}
