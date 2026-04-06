use crate::channel::{Channel, ChannelMessage};
use argentor_core::ArgentorResult;
use async_trait::async_trait;
use tokio::sync::broadcast;

/// Built-in WebSocket-based chat channel.
/// Messages are forwarded via a broadcast channel to connected WS clients.
pub struct WebChatChannel {
    tx: broadcast::Sender<ChannelMessage>,
}

impl WebChatChannel {
    /// Create a new web chat channel with the given broadcast capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Subscribe to incoming messages on this channel.
    pub fn subscribe(&self) -> broadcast::Receiver<ChannelMessage> {
        self.tx.subscribe()
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    fn name(&self) -> &str {
        "webchat"
    }

    async fn send(&self, message: ChannelMessage) -> ArgentorResult<()> {
        self.tx
            .send(message)
            .map_err(|e| argentor_core::ArgentorError::Channel(format!("Send failed: {e}")))?;
        Ok(())
    }
}
