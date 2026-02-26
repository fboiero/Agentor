use crate::channel::{Channel, ChannelMessage};
use agentor_core::{AgentorError, AgentorResult};
use std::collections::HashMap;

/// Manages multiple communication channels.
///
/// Provides a unified interface for sending messages to specific channels
/// or broadcasting to all registered channels.
pub struct ChannelManager {
    channels: HashMap<String, Box<dyn Channel>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Add a channel to the manager.
    pub fn add_channel(&mut self, channel: Box<dyn Channel>) {
        let name = channel.name().to_string();
        self.channels.insert(name, channel);
    }

    /// Get a reference to a channel by name.
    pub fn get(&self, name: &str) -> Option<&dyn Channel> {
        self.channels.get(name).map(std::convert::AsRef::as_ref)
    }

    /// Send a message to a specific channel.
    pub async fn send_to(&self, channel_name: &str, message: ChannelMessage) -> AgentorResult<()> {
        let channel = self.channels.get(channel_name).ok_or_else(|| {
            AgentorError::Channel(format!("Channel '{channel_name}' not found"))
        })?;
        channel.send(message).await
    }

    /// Broadcast a message to all registered channels.
    /// Errors from individual channels are collected and returned together.
    pub async fn broadcast(&self, message: ChannelMessage) -> Vec<AgentorError> {
        let mut errors = Vec::new();
        for (name, channel) in &self.channels {
            if let Err(e) = channel.send(message.clone()).await {
                tracing::warn!(channel = %name, error = %e, "Broadcast send failed");
                errors.push(e);
            }
        }
        errors
    }

    /// List all registered channel names.
    pub fn channel_names(&self) -> Vec<&str> {
        self.channels.keys().map(std::string::String::as_str).collect()
    }

    /// Get the number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::channel::Channel;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock channel that counts sends.
    struct MockChannel {
        name: String,
        send_count: Arc<AtomicUsize>,
    }

    impl MockChannel {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                send_count: Arc::new(AtomicUsize::new(0)),
            }
        }

    }

    #[async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &str {
            &self.name
        }

        async fn send(&self, _message: ChannelMessage) -> AgentorResult<()> {
            self.send_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn test_message() -> ChannelMessage {
        ChannelMessage {
            channel_id: "ch-1".to_string(),
            sender_id: "user-1".to_string(),
            content: "Hello".to_string(),
            session_id: None,
        }
    }

    #[test]
    fn test_add_and_count() {
        let mut mgr = ChannelManager::new();
        assert_eq!(mgr.channel_count(), 0);
        mgr.add_channel(Box::new(MockChannel::new("test")));
        assert_eq!(mgr.channel_count(), 1);
    }

    #[test]
    fn test_get_channel() {
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new("slack")));
        assert!(mgr.get("slack").is_some());
        assert!(mgr.get("discord").is_none());
    }

    #[tokio::test]
    async fn test_send_to() {
        let mock = MockChannel::new("test");
        let count = mock.send_count.clone();
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(mock));

        mgr.send_to("test", test_message()).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_send_to_unknown_channel() {
        let mgr = ChannelManager::new();
        let result = mgr.send_to("nonexistent", test_message()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_broadcast() {
        let m1 = MockChannel::new("ch1");
        let c1 = m1.send_count.clone();
        let m2 = MockChannel::new("ch2");
        let c2 = m2.send_count.clone();

        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(m1));
        mgr.add_channel(Box::new(m2));

        let errors = mgr.broadcast(test_message()).await;
        assert!(errors.is_empty());
        assert_eq!(c1.load(Ordering::SeqCst), 1);
        assert_eq!(c2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_channel_names() {
        let mut mgr = ChannelManager::new();
        mgr.add_channel(Box::new(MockChannel::new("slack")));
        mgr.add_channel(Box::new(MockChannel::new("discord")));
        let names = mgr.channel_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"slack"));
        assert!(names.contains(&"discord"));
    }
}
