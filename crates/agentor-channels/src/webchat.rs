use crate::channel::{Channel, ChannelMessage};
use agentor_core::AgentorResult;
use async_trait::async_trait;
use tokio::sync::broadcast;

/// Built-in WebSocket-based chat channel.
/// Messages are forwarded via a broadcast channel to connected WS clients.
pub struct WebChatChannel {
    tx: broadcast::Sender<ChannelMessage>,
}

impl WebChatChannel {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChannelMessage> {
        self.tx.subscribe()
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    fn name(&self) -> &str {
        "webchat"
    }

    async fn send(&self, message: ChannelMessage) -> AgentorResult<()> {
        self.tx
            .send(message)
            .map_err(|e| agentor_core::AgentorError::Channel(format!("Send failed: {e}")))?;
        Ok(())
    }
}
