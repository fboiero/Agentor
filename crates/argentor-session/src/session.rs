use argentor_core::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A conversation session that groups messages, tracks active skills,
/// and carries arbitrary metadata.
///
/// Every session is assigned a unique [`Uuid`] at creation time.
/// Messages are appended through [`Session::add_message`], which also
/// updates the `updated_at` timestamp automatically.
///
/// # Examples
///
/// ```
/// use argentor_session::Session;
/// use argentor_core::{Message, Role};
/// use uuid::Uuid;
///
/// let mut session = Session::new();
/// let msg = Message::user("Hello", session.id);
/// session.add_message(msg);
/// assert_eq!(session.message_count(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for this session (UUID v4).
    pub id: Uuid,
    /// Ordered list of messages exchanged in this session.
    pub messages: Vec<Message>,
    /// Names of skills currently available to the agent in this session.
    pub active_skills: Vec<String>,
    /// UTC timestamp of when the session was created.
    pub created_at: DateTime<Utc>,
    /// UTC timestamp of the last modification (e.g., message added).
    pub updated_at: DateTime<Utc>,
    /// Arbitrary key-value metadata attached to the session.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Session {
    /// Create a new session with a fresh UUID and empty message history.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            messages: Vec::new(),
            active_skills: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    /// Append a message to the session and update `updated_at`.
    pub fn add_message(&mut self, message: Message) {
        self.updated_at = Utc::now();
        self.messages.push(message);
    }

    /// Return the number of messages in the session.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
