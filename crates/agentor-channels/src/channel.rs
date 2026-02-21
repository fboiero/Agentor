use agentor_core::AgentorResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub channel_id: String,
    pub sender_id: String,
    pub content: String,
    pub session_id: Option<Uuid>,
}

#[derive(Debug)]
pub enum ChannelEvent {
    MessageReceived(ChannelMessage),
    Connected(String),
    Disconnected(String),
}

#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn send(&self, message: ChannelMessage) -> AgentorResult<()>;
}
