use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: String,
    pub session_id: Uuid,
    pub channel_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
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

    pub fn user(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::User, content, session_id)
    }

    pub fn assistant(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::Assistant, content, session_id)
    }

    pub fn system(content: impl Into<String>, session_id: Uuid) -> Self {
        Self::new(Role::System, content, session_id)
    }
}

#[cfg(test)]
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
