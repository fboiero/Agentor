use argentor_core::ArgentorResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A message routed through a communication channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    /// Identifier of the channel this message belongs to.
    pub channel_id: String,
    /// Identifier of the message sender (user or bot).
    pub sender_id: String,
    /// Textual content of the message.
    pub content: String,
    /// Optional session to associate this message with.
    pub session_id: Option<Uuid>,
}

/// Events emitted by a channel implementation.
#[derive(Debug)]
pub enum ChannelEvent {
    /// A new message was received on the channel.
    MessageReceived(ChannelMessage),
    /// A client connected (carries the client identifier).
    Connected(String),
    /// A client disconnected (carries the client identifier).
    Disconnected(String),
}

/// Trait for platform-specific communication channels (Slack, Discord, web, etc.).
#[async_trait]
pub trait Channel: Send + Sync {
    /// Human-readable name of this channel (e.g., "slack", "webchat").
    fn name(&self) -> &str;
    /// Send a message through this channel.
    async fn send(&self, message: ChannelMessage) -> ArgentorResult<()>;
}
