pub mod approval;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// --- Error types ---

#[derive(Debug, thiserror::Error)]
pub enum AgentorError {
    #[error("Agent error: {0}")]
    Agent(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Skill error: {0}")]
    Skill(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Orchestrator error: {0}")]
    Orchestrator(String),
}

pub type AgentorResult<T> = Result<T, AgentorError>;

// --- Message types ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// --- Tool types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: true,
        }
    }
}
