//! Inter-agent message bus for Agent-to-Agent (A2A) communication.
//!
//! Provides a thread-safe, async-ready message bus that allows agents to
//! communicate directly during orchestration — complementing the existing
//! shared-artifact pattern with real-time messaging.
//!
//! # Features
//!
//! - **Direct messaging**: send a message to a specific agent by role.
//! - **Broadcast**: send a message to all agents.
//! - **Role-based addressing**: target any agent with a given role.
//! - **Real-time subscriptions**: subscribe via `tokio::sync::broadcast` for
//!   push-based notification.
//! - **Peek & drain**: peek at pending messages without consuming them, or
//!   drain them for processing.
//!
//! # Example
//!
//! ```rust,no_run
//! use argentor_orchestrator::message_bus::{MessageBus, AgentMessage, MessageType, BroadcastTarget};
//! use argentor_orchestrator::types::AgentRole;
//!
//! # async fn example() {
//! let bus = MessageBus::new();
//!
//! // Send a direct message
//! let msg = AgentMessage::new(
//!     AgentRole::Orchestrator,
//!     BroadcastTarget::Direct(AgentRole::Coder),
//!     "Please implement the auth module".to_string(),
//!     MessageType::Query,
//! );
//! bus.send(msg).await;
//!
//! // Receive messages for the Coder role (drains them)
//! let messages = bus.receive(&AgentRole::Coder).await;
//! # }
//! ```

use crate::types::AgentRole;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::info;
use uuid::Uuid;

/// Default capacity for the broadcast channel.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Type of message exchanged between agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// A question or request directed at another agent.
    Query,
    /// A reply to a previous query.
    Response,
    /// An informational update about the agent's progress.
    StatusUpdate,
    /// Notification that an artifact has been produced or updated.
    ArtifactNotification,
    /// Report of an error encountered during processing.
    ErrorReport,
    /// User-defined message type for extensibility.
    Custom(String),
}

/// Addressing target for a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastTarget {
    /// Send to a specific agent identified by role.
    Direct(AgentRole),
    /// Send to all agents.
    Broadcast,
    /// Send to any agent that has the specified role.
    Role(AgentRole),
}

/// A message exchanged between agents via the message bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique identifier for this message.
    pub id: Uuid,
    /// Role of the sending agent.
    pub sender: AgentRole,
    /// Intended recipient(s).
    pub recipient: BroadcastTarget,
    /// Message payload.
    pub content: String,
    /// Classification of the message.
    pub message_type: MessageType,
    /// When the message was created.
    pub timestamp: DateTime<Utc>,
    /// Optional correlation ID linking this message to a conversation thread.
    pub correlation_id: Option<Uuid>,
}

impl AgentMessage {
    /// Create a new message with auto-generated id and timestamp.
    pub fn new(
        sender: AgentRole,
        recipient: BroadcastTarget,
        content: String,
        message_type: MessageType,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            sender,
            recipient,
            content,
            message_type,
            timestamp: Utc::now(),
            correlation_id: None,
        }
    }

    /// Attach a correlation ID for threading/request-response patterns.
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }
}

/// Internal state of the message bus, protected by `RwLock`.
#[derive(Debug)]
struct BusState {
    /// Pending messages indexed by recipient role.
    mailboxes: HashMap<AgentRole, Vec<AgentMessage>>,
    /// Broadcast messages waiting for agents that have not yet collected them.
    broadcast_pending: Vec<AgentMessage>,
    /// Total number of messages ever sent through this bus.
    total_sent: usize,
}

impl BusState {
    fn new() -> Self {
        Self {
            mailboxes: HashMap::new(),
            broadcast_pending: Vec::new(),
            total_sent: 0,
        }
    }
}

/// Thread-safe inter-agent message bus.
///
/// Agents can send messages to specific roles, broadcast to all, or subscribe
/// for real-time push notifications via a `tokio::sync::broadcast` channel.
#[derive(Debug, Clone)]
pub struct MessageBus {
    state: Arc<RwLock<BusState>>,
    /// Broadcast sender for real-time push notifications.
    notifier: broadcast::Sender<AgentMessage>,
}

impl MessageBus {
    /// Create a new, empty message bus.
    pub fn new() -> Self {
        let (notifier, _) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        Self {
            state: Arc::new(RwLock::new(BusState::new())),
            notifier,
        }
    }

    /// Send a message through the bus.
    ///
    /// The message is placed in the appropriate mailbox(es) based on the
    /// recipient target and a notification is broadcast to any live subscribers.
    pub async fn send(&self, msg: AgentMessage) {
        info!(
            sender = %msg.sender,
            recipient = ?msg.recipient,
            msg_type = ?msg.message_type,
            msg_id = %msg.id,
            "message_bus: sending message"
        );

        let mut state = self.state.write().await;
        state.total_sent += 1;

        match &msg.recipient {
            BroadcastTarget::Direct(role) | BroadcastTarget::Role(role) => {
                state
                    .mailboxes
                    .entry(role.clone())
                    .or_default()
                    .push(msg.clone());
            }
            BroadcastTarget::Broadcast => {
                state.broadcast_pending.push(msg.clone());
            }
        }

        // Best-effort notification — it is fine if no one is listening.
        let _ = self.notifier.send(msg);
    }

    /// Receive (drain) all pending messages for the given role.
    ///
    /// This consumes the messages so that subsequent calls will not return them.
    /// Broadcast messages addressed to all agents are also included and removed
    /// from the broadcast pending list for this recipient.
    pub async fn receive(&self, role: &AgentRole) -> Vec<AgentMessage> {
        let mut state = self.state.write().await;

        let mut messages: Vec<AgentMessage> = state.mailboxes.remove(role).unwrap_or_default();

        // Drain broadcast messages as well.
        messages.append(&mut state.broadcast_pending);

        if !messages.is_empty() {
            info!(
                role = %role,
                count = messages.len(),
                "message_bus: delivering messages"
            );
        }

        messages
    }

    /// Peek at pending messages for the given role without consuming them.
    ///
    /// Returns cloned messages so the originals remain in the bus.
    pub async fn peek(&self, role: &AgentRole) -> Vec<AgentMessage> {
        let state = self.state.read().await;

        let mut messages: Vec<AgentMessage> =
            state.mailboxes.get(role).cloned().unwrap_or_default();

        // Include broadcast pending (cloned, not drained).
        messages.extend(state.broadcast_pending.iter().cloned());

        messages
    }

    /// Subscribe to real-time message notifications via a broadcast channel.
    ///
    /// The returned receiver will get every message sent through the bus,
    /// regardless of recipient. Callers should filter by role as needed.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentMessage> {
        info!("message_bus: new subscriber registered");
        self.notifier.subscribe()
    }

    /// Broadcast a message from `sender` to all agents.
    pub async fn broadcast(&self, sender: AgentRole, content: String, msg_type: MessageType) {
        let msg = AgentMessage::new(sender, BroadcastTarget::Broadcast, content, msg_type);
        self.send(msg).await;
    }

    /// Return the total number of messages ever sent through this bus.
    pub async fn message_count(&self) -> usize {
        self.state.read().await.total_sent
    }

    /// Clear all pending messages and reset the send counter.
    pub async fn clear(&self) {
        info!("message_bus: clearing all pending messages");
        let mut state = self.state.write().await;
        state.mailboxes.clear();
        state.broadcast_pending.clear();
        state.total_sent = 0;
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_send_and_receive_direct() {
        let bus = MessageBus::new();

        let msg = AgentMessage::new(
            AgentRole::Orchestrator,
            BroadcastTarget::Direct(AgentRole::Coder),
            "implement auth".to_string(),
            MessageType::Query,
        );
        bus.send(msg).await;

        let received = bus.receive(&AgentRole::Coder).await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].content, "implement auth");
        assert_eq!(received[0].sender, AgentRole::Orchestrator);
    }

    #[tokio::test]
    async fn test_receive_drains_messages() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Orchestrator,
            BroadcastTarget::Direct(AgentRole::Tester),
            "run tests".to_string(),
            MessageType::Query,
        ))
        .await;

        let first = bus.receive(&AgentRole::Tester).await;
        assert_eq!(first.len(), 1);

        let second = bus.receive(&AgentRole::Tester).await;
        assert!(
            second.is_empty(),
            "messages should be drained after receive"
        );
    }

    #[tokio::test]
    async fn test_peek_does_not_drain() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Reviewer,
            BroadcastTarget::Direct(AgentRole::Coder),
            "fix clippy warnings".to_string(),
            MessageType::Query,
        ))
        .await;

        let peeked = bus.peek(&AgentRole::Coder).await;
        assert_eq!(peeked.len(), 1);

        // Peek again — should still be there.
        let peeked_again = bus.peek(&AgentRole::Coder).await;
        assert_eq!(peeked_again.len(), 1);

        // Now drain.
        let drained = bus.receive(&AgentRole::Coder).await;
        assert_eq!(drained.len(), 1);
    }

    #[tokio::test]
    async fn test_broadcast_delivered_to_receiver() {
        let bus = MessageBus::new();

        bus.broadcast(
            AgentRole::Orchestrator,
            "system shutting down".to_string(),
            MessageType::StatusUpdate,
        )
        .await;

        let coder_msgs = bus.receive(&AgentRole::Coder).await;
        assert_eq!(coder_msgs.len(), 1);
        assert_eq!(coder_msgs[0].content, "system shutting down");
        assert_eq!(coder_msgs[0].recipient, BroadcastTarget::Broadcast);
    }

    #[tokio::test]
    async fn test_message_count() {
        let bus = MessageBus::new();
        assert_eq!(bus.message_count().await, 0);

        bus.send(AgentMessage::new(
            AgentRole::Spec,
            BroadcastTarget::Direct(AgentRole::Coder),
            "spec ready".to_string(),
            MessageType::ArtifactNotification,
        ))
        .await;

        bus.send(AgentMessage::new(
            AgentRole::Coder,
            BroadcastTarget::Direct(AgentRole::Tester),
            "code ready".to_string(),
            MessageType::ArtifactNotification,
        ))
        .await;

        assert_eq!(bus.message_count().await, 2);
    }

    #[tokio::test]
    async fn test_clear_resets_everything() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Orchestrator,
            BroadcastTarget::Direct(AgentRole::Coder),
            "task A".to_string(),
            MessageType::Query,
        ))
        .await;

        bus.broadcast(
            AgentRole::Orchestrator,
            "announcement".to_string(),
            MessageType::StatusUpdate,
        )
        .await;

        assert_eq!(bus.message_count().await, 2);

        bus.clear().await;

        assert_eq!(bus.message_count().await, 0);
        assert!(bus.receive(&AgentRole::Coder).await.is_empty());
    }

    #[tokio::test]
    async fn test_correlation_id() {
        let bus = MessageBus::new();
        let corr_id = Uuid::new_v4();

        let query = AgentMessage::new(
            AgentRole::Orchestrator,
            BroadcastTarget::Direct(AgentRole::Architect),
            "design question".to_string(),
            MessageType::Query,
        )
        .with_correlation_id(corr_id);
        let query_id = query.id;
        bus.send(query).await;

        let received = bus.receive(&AgentRole::Architect).await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].correlation_id, Some(corr_id));

        // Reply with same correlation_id.
        let response = AgentMessage::new(
            AgentRole::Architect,
            BroadcastTarget::Direct(AgentRole::Orchestrator),
            "use hexagonal arch".to_string(),
            MessageType::Response,
        )
        .with_correlation_id(corr_id);
        bus.send(response).await;

        let reply = bus.receive(&AgentRole::Orchestrator).await;
        assert_eq!(reply.len(), 1);
        assert_eq!(reply[0].correlation_id, Some(corr_id));
        assert_ne!(reply[0].id, query_id, "reply should have its own id");
    }

    #[tokio::test]
    async fn test_subscribe_receives_notifications() {
        let bus = MessageBus::new();
        let mut rx = bus.subscribe();

        bus.send(AgentMessage::new(
            AgentRole::Coder,
            BroadcastTarget::Direct(AgentRole::Reviewer),
            "PR ready for review".to_string(),
            MessageType::StatusUpdate,
        ))
        .await;

        let result = timeout(Duration::from_secs(1), rx.recv()).await;
        assert!(result.is_ok(), "should receive notification within timeout");
        let notification = result.unwrap().unwrap();
        assert_eq!(notification.content, "PR ready for review");
    }

    #[tokio::test]
    async fn test_role_based_targeting() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Orchestrator,
            BroadcastTarget::Role(AgentRole::Coder),
            "priority task".to_string(),
            MessageType::Query,
        ))
        .await;

        let msgs = bus.receive(&AgentRole::Coder).await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "priority task");
        assert_eq!(msgs[0].recipient, BroadcastTarget::Role(AgentRole::Coder));
    }

    #[tokio::test]
    async fn test_error_report_message_type() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Tester,
            BroadcastTarget::Direct(AgentRole::Orchestrator),
            "3 tests failed".to_string(),
            MessageType::ErrorReport,
        ))
        .await;

        let msgs = bus.receive(&AgentRole::Orchestrator).await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type, MessageType::ErrorReport);
    }

    #[tokio::test]
    async fn test_custom_message_type() {
        let bus = MessageBus::new();

        bus.send(AgentMessage::new(
            AgentRole::Custom("metrics".to_string()),
            BroadcastTarget::Direct(AgentRole::Orchestrator),
            r#"{"cpu": 42}"#.to_string(),
            MessageType::Custom("telemetry".to_string()),
        ))
        .await;

        let msgs = bus.receive(&AgentRole::Orchestrator).await;
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].message_type,
            MessageType::Custom("telemetry".to_string())
        );
        assert_eq!(msgs[0].sender, AgentRole::Custom("metrics".to_string()));
    }

    #[tokio::test]
    async fn test_serialization_roundtrip() {
        let msg = AgentMessage::new(
            AgentRole::SecurityAuditor,
            BroadcastTarget::Direct(AgentRole::Coder),
            "vulnerability found in auth".to_string(),
            MessageType::ErrorReport,
        )
        .with_correlation_id(Uuid::new_v4());

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, msg.id);
        assert_eq!(deserialized.sender, msg.sender);
        assert_eq!(deserialized.content, msg.content);
        assert_eq!(deserialized.message_type, msg.message_type);
        assert_eq!(deserialized.correlation_id, msg.correlation_id);
    }
}
