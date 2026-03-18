#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use argentor_channels::{Channel, ChannelManager, ChannelMessage, WebChatChannel};
use argentor_core::{ArgentorError, ArgentorResult};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// A mock channel that records send count and can optionally fail.
struct MockChannel {
    channel_name: String,
    send_count: Arc<AtomicUsize>,
    fail_sends: bool,
}

impl MockChannel {
    fn new(name: &str) -> Self {
        Self {
            channel_name: name.to_string(),
            send_count: Arc::new(AtomicUsize::new(0)),
            fail_sends: false,
        }
    }

    fn failing(name: &str) -> Self {
        Self {
            channel_name: name.to_string(),
            send_count: Arc::new(AtomicUsize::new(0)),
            fail_sends: true,
        }
    }

    fn count(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.send_count)
    }
}

#[async_trait]
impl Channel for MockChannel {
    fn name(&self) -> &str {
        &self.channel_name
    }

    async fn send(&self, _message: ChannelMessage) -> ArgentorResult<()> {
        if self.fail_sends {
            return Err(ArgentorError::Channel("mock send failure".to_string()));
        }
        self.send_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn test_message() -> ChannelMessage {
    ChannelMessage {
        channel_id: "ch-test".to_string(),
        sender_id: "user-1".to_string(),
        content: "Hello from integration test".to_string(),
        session_id: None,
    }
}

fn test_message_with(channel_id: &str, sender_id: &str, content: &str) -> ChannelMessage {
    ChannelMessage {
        channel_id: channel_id.to_string(),
        sender_id: sender_id.to_string(),
        content: content.to_string(),
        session_id: None,
    }
}

// ---------------------------------------------------------------------------
// 1. WebChatChannel implements Channel trait correctly
// ---------------------------------------------------------------------------

#[tokio::test]
async fn webchat_channel_implements_channel_trait() {
    let channel = WebChatChannel::new(16);

    assert_eq!(channel.name(), "webchat");

    // Subscribe before sending so the broadcast channel has a receiver
    let mut rx = channel.subscribe();

    let msg = test_message_with("webchat", "user-1", "Hello webchat");
    channel.send(msg.clone()).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.content, "Hello webchat");
    assert_eq!(received.sender_id, "user-1");
    assert_eq!(received.channel_id, "webchat");
}

// ---------------------------------------------------------------------------
// 2. ChannelManager can register channels
// ---------------------------------------------------------------------------

#[test]
fn channel_manager_can_register_channels() {
    let mut mgr = ChannelManager::new();
    assert_eq!(mgr.channel_count(), 0);

    mgr.add_channel(Box::new(MockChannel::new("slack")));
    assert_eq!(mgr.channel_count(), 1);

    mgr.add_channel(Box::new(MockChannel::new("discord")));
    assert_eq!(mgr.channel_count(), 2);

    // Verify each channel is accessible by name
    assert!(mgr.get("slack").is_some());
    assert!(mgr.get("discord").is_some());
}

// ---------------------------------------------------------------------------
// 3. ChannelManager send_to sends to the right channel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_manager_send_to_correct_channel() {
    let mock_slack = MockChannel::new("slack");
    let slack_count = mock_slack.count();
    let mock_discord = MockChannel::new("discord");
    let discord_count = mock_discord.count();

    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(mock_slack));
    mgr.add_channel(Box::new(mock_discord));

    // Send only to slack
    mgr.send_to("slack", test_message()).await.unwrap();

    assert_eq!(slack_count.load(Ordering::SeqCst), 1);
    assert_eq!(discord_count.load(Ordering::SeqCst), 0);

    // Send only to discord
    mgr.send_to("discord", test_message()).await.unwrap();

    assert_eq!(slack_count.load(Ordering::SeqCst), 1);
    assert_eq!(discord_count.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// 4. ChannelManager broadcast sends to all channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_manager_broadcast_sends_to_all() {
    let mock1 = MockChannel::new("ch1");
    let c1 = mock1.count();
    let mock2 = MockChannel::new("ch2");
    let c2 = mock2.count();
    let mock3 = MockChannel::new("ch3");
    let c3 = mock3.count();

    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(mock1));
    mgr.add_channel(Box::new(mock2));
    mgr.add_channel(Box::new(mock3));

    let errors = mgr.broadcast(test_message()).await;
    assert!(errors.is_empty());
    assert_eq!(c1.load(Ordering::SeqCst), 1);
    assert_eq!(c2.load(Ordering::SeqCst), 1);
    assert_eq!(c3.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------------------
// 5. ChannelManager channel_names returns all registered
// ---------------------------------------------------------------------------

#[test]
fn channel_manager_list_channels_returns_all_registered() {
    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(MockChannel::new("slack")));
    mgr.add_channel(Box::new(MockChannel::new("discord")));
    mgr.add_channel(Box::new(MockChannel::new("telegram")));

    let names = mgr.channel_names();
    assert_eq!(names.len(), 3);
    assert!(names.contains(&"slack"));
    assert!(names.contains(&"discord"));
    assert!(names.contains(&"telegram"));
}

// ---------------------------------------------------------------------------
// 6. Channel receive returns messages in order
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_receive_returns_messages_in_order() {
    let channel = WebChatChannel::new(16);
    let mut rx = channel.subscribe();

    let messages = vec!["first", "second", "third"];
    for content in &messages {
        channel
            .send(test_message_with("webchat", "user-1", content))
            .await
            .unwrap();
    }

    for expected_content in &messages {
        let received = rx.recv().await.unwrap();
        assert_eq!(received.content, *expected_content);
    }
}

// ---------------------------------------------------------------------------
// 7. Empty manager has no channels
// ---------------------------------------------------------------------------

#[test]
fn empty_manager_has_no_channels() {
    let mgr = ChannelManager::new();
    assert_eq!(mgr.channel_count(), 0);
    assert!(mgr.channel_names().is_empty());
    assert!(mgr.get("anything").is_none());
}

// ---------------------------------------------------------------------------
// 8. Send to unknown channel returns error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_to_unknown_channel_returns_error() {
    let mgr = ChannelManager::new();
    let result = mgr.send_to("nonexistent", test_message()).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("nonexistent"),
        "Error message should mention the missing channel name, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// 9. Multiple channels can be registered
// ---------------------------------------------------------------------------

#[test]
fn multiple_channels_can_be_registered() {
    let mut mgr = ChannelManager::new();
    let channel_names = ["alpha", "beta", "gamma", "delta", "epsilon"];

    for name in &channel_names {
        mgr.add_channel(Box::new(MockChannel::new(name)));
    }

    assert_eq!(mgr.channel_count(), 5);

    for name in &channel_names {
        let ch = mgr.get(name);
        assert!(ch.is_some(), "Channel '{name}' should be registered");
        assert_eq!(ch.unwrap().name(), *name);
    }
}

// ---------------------------------------------------------------------------
// 10. Channel send and receive roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_send_and_receive_roundtrip() {
    let channel = WebChatChannel::new(16);
    let mut rx = channel.subscribe();

    let original = ChannelMessage {
        channel_id: "webchat".to_string(),
        sender_id: "user-42".to_string(),
        content: "roundtrip payload".to_string(),
        session_id: Some(uuid::Uuid::new_v4()),
    };

    channel.send(original.clone()).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.channel_id, original.channel_id);
    assert_eq!(received.sender_id, original.sender_id);
    assert_eq!(received.content, original.content);
    assert_eq!(received.session_id, original.session_id);
}

// ---------------------------------------------------------------------------
// 11. ChannelManager broadcast collects errors from failing channels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_manager_broadcast_collects_errors() {
    let good = MockChannel::new("good-channel");
    let good_count = good.count();
    let bad = MockChannel::failing("bad-channel");

    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(good));
    mgr.add_channel(Box::new(bad));

    let errors = mgr.broadcast(test_message()).await;

    // One channel should fail, one should succeed
    assert_eq!(errors.len(), 1);
    assert_eq!(good_count.load(Ordering::SeqCst), 1);

    // The error should mention the failure
    let err_msg = errors[0].to_string();
    assert!(
        err_msg.contains("mock send failure"),
        "Error message should contain the mock failure text, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// 12. WebChatChannel connection lifecycle (subscribe, send, drop)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn webchat_channel_connection_lifecycle() {
    let channel = WebChatChannel::new(8);

    // Phase 1: subscribe and verify send works
    let mut rx1 = channel.subscribe();
    channel
        .send(test_message_with("webchat", "u1", "msg1"))
        .await
        .unwrap();
    let received = rx1.recv().await.unwrap();
    assert_eq!(received.content, "msg1");

    // Phase 2: a second subscriber joins mid-stream
    let mut rx2 = channel.subscribe();
    channel
        .send(test_message_with("webchat", "u2", "msg2"))
        .await
        .unwrap();

    // Both subscribers should receive the second message
    let r1 = rx1.recv().await.unwrap();
    let r2 = rx2.recv().await.unwrap();
    assert_eq!(r1.content, "msg2");
    assert_eq!(r2.content, "msg2");

    // Phase 3: drop first subscriber, channel should still work for second
    drop(rx1);
    channel
        .send(test_message_with("webchat", "u3", "msg3"))
        .await
        .unwrap();
    let r2 = rx2.recv().await.unwrap();
    assert_eq!(r2.content, "msg3");
}

// ---------------------------------------------------------------------------
// 13. WebChatChannel send fails when no subscribers (broadcast drops)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn webchat_send_fails_without_subscribers() {
    let channel = WebChatChannel::new(8);

    // No subscribers: tokio broadcast::send returns Err when there are no receivers
    let result = channel.send(test_message()).await;
    assert!(
        result.is_err(),
        "Send should fail when there are no active subscribers"
    );
}

// ---------------------------------------------------------------------------
// 14. ChannelManager default constructor works
// ---------------------------------------------------------------------------

#[test]
fn channel_manager_default_constructor() {
    let mgr = ChannelManager::default();
    assert_eq!(mgr.channel_count(), 0);
    assert!(mgr.channel_names().is_empty());
}

// ---------------------------------------------------------------------------
// 15. Registering a channel with a duplicate name replaces the old one
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registering_duplicate_name_replaces_channel() {
    let first = MockChannel::new("slack");
    let first_count = first.count();
    let second = MockChannel::new("slack");
    let second_count = second.count();

    let mut mgr = ChannelManager::new();
    mgr.add_channel(Box::new(first));
    mgr.add_channel(Box::new(second));

    // Only one channel should be registered under "slack"
    assert_eq!(mgr.channel_count(), 1);

    mgr.send_to("slack", test_message()).await.unwrap();

    // The second (replacement) channel should receive the message
    assert_eq!(second_count.load(Ordering::SeqCst), 1);
    assert_eq!(first_count.load(Ordering::SeqCst), 0);
}

// ---------------------------------------------------------------------------
// 16. ChannelMessage serialization roundtrip
// ---------------------------------------------------------------------------

#[test]
fn channel_message_serialization_roundtrip() {
    let session_id = uuid::Uuid::new_v4();
    let original = ChannelMessage {
        channel_id: "slack".to_string(),
        sender_id: "user-99".to_string(),
        content: "serializable content".to_string(),
        session_id: Some(session_id),
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: ChannelMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.channel_id, original.channel_id);
    assert_eq!(deserialized.sender_id, original.sender_id);
    assert_eq!(deserialized.content, original.content);
    assert_eq!(deserialized.session_id, Some(session_id));
}
