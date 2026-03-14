use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// The role of the participant that authored a [`Message`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// A human end-user.
    User,
    /// The AI assistant.
    Assistant,
    /// A system-level instruction or prompt.
    System,
    /// Output produced by a tool invocation.
    Tool,
}

/// A single message exchanged within a conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier for this message.
    pub id: Uuid,
    /// The role of the message author.
    pub role: Role,
    /// The textual content of the message.
    pub content: String,
    /// The session this message belongs to.
    pub session_id: Uuid,
    /// Optional channel identifier for routing (e.g. WebSocket connection ID).
    pub channel_id: Option<String>,
    /// UTC timestamp of when the message was created.
    pub timestamp: DateTime<Utc>,
    /// Arbitrary key-value metadata attached to the message.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Creates a new message with the given role, content, and session ID.
    pub fn new(role: Role, content: impl Into<String>, session_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            session_id,
            channel_id: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Creates a new message with [`Role::User`].
    pub fn user(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::User, content, session_id)
    }

    /// Creates a new message with [`Role::Assistant`].
    pub fn assistant(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::Assistant, content, session_id)
    }

    /// Creates a new message with [`Role::System`].
    pub fn system(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::System, content, session_id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let session_id = Uuid::new_v4();
        let msg = Message::user("Hello", session_id);
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.session_id, session_id);
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("test", Uuid::new_v4());
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "test");
        assert_eq!(deserialized.role, Role::User);
    }
}
