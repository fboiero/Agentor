use agentor_core::approval::{ApprovalChannel, ApprovalDecision, ApprovalRequest};
use agentor_core::AgentorResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock};
use tracing::{info, warn};

use crate::connection::ConnectionManager;

/// WebSocket-based approval channel for dashboard HITL workflows.
///
/// Sends approval requests to connected WebSocket clients as JSON messages
/// and waits for a matching approval response. Uses oneshot channels to
/// route responses back to the waiting skill.
pub struct WsApprovalChannel {
    connections: Arc<ConnectionManager>,
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<ApprovalDecision>>>>,
    timeout: Duration,
}

impl WsApprovalChannel {
    /// Create a new WebSocket approval channel.
    pub fn new(connections: Arc<ConnectionManager>, timeout: Duration) -> Self {
        Self {
            connections,
            pending: Arc::new(RwLock::new(HashMap::new())),
            timeout,
        }
    }

    /// Create with a default 5-minute timeout.
    pub fn default_timeout(connections: Arc<ConnectionManager>) -> Self {
        Self::new(connections, Duration::from_secs(300))
    }

    /// Handle an incoming approval response from a WebSocket client.
    ///
    /// Call this from the WebSocket message handler when you receive a
    /// message of type `approval_response`. The `task_id` must match
    /// a pending approval request.
    pub async fn handle_approval_response(&self, task_id: &str, decision: ApprovalDecision) {
        let mut pending = self.pending.write().await;
        if let Some(tx) = pending.remove(task_id) {
            if tx.send(decision).is_err() {
                warn!(task_id = %task_id, "Approval response sent but receiver dropped");
            } else {
                info!(task_id = %task_id, "Approval response delivered");
            }
        } else {
            warn!(task_id = %task_id, "No pending approval for this task_id");
        }
    }

    /// Get the number of pending approval requests.
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }
}

#[async_trait]
impl ApprovalChannel for WsApprovalChannel {
    async fn request_approval(&self, request: ApprovalRequest) -> AgentorResult<ApprovalDecision> {
        let task_id = request.task_id.clone();

        // Create oneshot channel for the response
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.write().await;
            pending.insert(task_id.clone(), tx);
        }

        // Broadcast approval request to all connected clients
        let msg = serde_json::json!({
            "type": "approval_request",
            "task_id": request.task_id,
            "description": request.description,
            "risk_level": request.risk_level,
            "context": request.context,
        });
        let msg_str = serde_json::to_string(&msg).unwrap_or_default();

        info!(task_id = %task_id, "Broadcasting approval request to WebSocket clients");
        self.connections.broadcast(&msg_str).await;

        // Wait for response with timeout
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(decision)) => {
                info!(
                    task_id = %task_id,
                    approved = decision.approved,
                    reviewer = %decision.reviewer,
                    "Approval decision received via WebSocket"
                );
                Ok(decision)
            }
            Ok(Err(_)) => {
                // Sender was dropped (shouldn't normally happen)
                self.pending.write().await.remove(&task_id);
                Ok(ApprovalDecision {
                    approved: false,
                    reason: Some("Approval channel closed unexpectedly".into()),
                    reviewer: "system".into(),
                })
            }
            Err(_) => {
                // Timeout
                self.pending.write().await.remove(&task_id);
                warn!(task_id = %task_id, timeout_secs = self.timeout.as_secs(), "Approval timed out");
                Ok(ApprovalDecision {
                    approved: false,
                    reason: Some(format!("Timed out after {}s", self.timeout.as_secs())),
                    reviewer: "system".into(),
                })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_approval_request_and_response() {
        let connections = ConnectionManager::new();
        let channel = WsApprovalChannel::new(connections, Duration::from_secs(5));
        let channel = Arc::new(channel);

        let request = ApprovalRequest {
            task_id: "test-1".into(),
            description: "Deploy prod".into(),
            risk_level: agentor_core::approval::RiskLevel::High,
            context: "".into(),
        };

        // Spawn the approval request in a task
        let ch = channel.clone();
        let handle = tokio::spawn(async move {
            ch.request_approval(request).await
        });

        // Simulate a small delay, then deliver the response
        tokio::time::sleep(Duration::from_millis(50)).await;
        channel
            .handle_approval_response(
                "test-1",
                ApprovalDecision {
                    approved: true,
                    reason: Some("Looks good".into()),
                    reviewer: "admin".into(),
                },
            )
            .await;

        let result = handle.await.unwrap().unwrap();
        assert!(result.approved);
        assert_eq!(result.reviewer, "admin");
    }

    #[tokio::test]
    async fn test_approval_timeout() {
        let connections = ConnectionManager::new();
        let channel = WsApprovalChannel::new(connections, Duration::from_millis(100));

        let request = ApprovalRequest {
            task_id: "test-timeout".into(),
            description: "Will timeout".into(),
            risk_level: agentor_core::approval::RiskLevel::Low,
            context: "".into(),
        };

        let result = channel.request_approval(request).await.unwrap();
        assert!(!result.approved);
        assert!(result.reason.unwrap().contains("Timed out"));
    }

    #[tokio::test]
    async fn test_unknown_task_id_response() {
        let connections = ConnectionManager::new();
        let channel = WsApprovalChannel::new(connections, Duration::from_secs(5));

        // No pending request — response should be silently dropped
        channel
            .handle_approval_response(
                "nonexistent",
                ApprovalDecision {
                    approved: true,
                    reason: None,
                    reviewer: "ghost".into(),
                },
            )
            .await;

        assert_eq!(channel.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_multiple_pending_requests() {
        let connections = ConnectionManager::new();
        let channel = Arc::new(WsApprovalChannel::new(connections, Duration::from_secs(5)));

        let req1 = ApprovalRequest {
            task_id: "t1".into(),
            description: "First".into(),
            risk_level: agentor_core::approval::RiskLevel::Low,
            context: "".into(),
        };
        let req2 = ApprovalRequest {
            task_id: "t2".into(),
            description: "Second".into(),
            risk_level: agentor_core::approval::RiskLevel::High,
            context: "".into(),
        };

        let ch1 = channel.clone();
        let h1 = tokio::spawn(async move { ch1.request_approval(req1).await });

        let ch2 = channel.clone();
        let h2 = tokio::spawn(async move { ch2.request_approval(req2).await });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(channel.pending_count().await, 2);

        // Respond to both
        channel
            .handle_approval_response(
                "t2",
                ApprovalDecision { approved: false, reason: Some("no".into()), reviewer: "r2".into() },
            )
            .await;
        channel
            .handle_approval_response(
                "t1",
                ApprovalDecision { approved: true, reason: None, reviewer: "r1".into() },
            )
            .await;

        let r1 = h1.await.unwrap().unwrap();
        let r2 = h2.await.unwrap().unwrap();
        assert!(r1.approved);
        assert!(!r2.approved);
    }
}
