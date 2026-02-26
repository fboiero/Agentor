//! Core types and error definitions for the Agentor framework.
//!
//! This crate provides the foundational types shared across all Agentor crates,
//! including error handling, message representations, and tool call abstractions.
//!
//! # Main types
//!
//! - [`AgentorError`] — Unified error enum for all Agentor subsystems.
//! - [`AgentorResult`] — Convenience alias for `Result<T, AgentorError>`.
//! - [`Role`] — Message role (user, assistant, system, tool).
//! - [`Message`] — A single message within a conversation session.
//! - [`ToolCall`] — Represents an LLM-initiated tool invocation request.
//! - [`ToolResult`] — The result returned after executing a tool call.

/// Approval types for human-in-the-loop workflows.
pub mod approval;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// --- Error types ---

/// Top-level error type for the Agentor framework.
///
/// Each variant corresponds to a subsystem that can produce errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentorError {
    /// An error originating from the agent execution loop.
    #[error("Agent error: {0}")]
    Agent(String),

    /// An error from an outbound HTTP request (e.g. LLM API call).
    #[error("HTTP error: {0}")]
    Http(String),

    /// An error related to session persistence or lookup.
    #[error("Session error: {0}")]
    Session(String),

    /// An error in configuration parsing or validation.
    #[error("Config error: {0}")]
    Config(String),

    /// An error raised by a skill during invocation.
    #[error("Skill error: {0}")]
    Skill(String),

    /// An error from a communication channel (e.g. WebSocket, CLI).
    #[error("Channel error: {0}")]
    Channel(String),

    /// An error from the API gateway layer.
    #[error("Gateway error: {0}")]
    Gateway(String),

    /// A security-related error (permissions, TLS, rate limiting).
    #[error("Security error: {0}")]
    Security(String),

    /// A JSON serialization or deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A standard I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// An error from the multi-agent orchestrator.
    #[error("Orchestrator error: {0}")]
    Orchestrator(String),
}

/// A convenience `Result` alias using [`AgentorError`].
pub type AgentorResult<T> = Result<T, AgentorError>;

// --- Message types ---

/// The role of the participant that authored a [`Message`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// --- Tool types ---

/// A request from the LLM to invoke a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier assigned by the LLM for this tool call.
    pub id: String,
    /// Name of the tool to invoke.
    pub name: String,
    /// JSON arguments to pass to the tool.
    pub arguments: serde_json::Value,
}

/// The result returned after executing a [`ToolCall`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// The ID of the [`ToolCall`] this result corresponds to.
    pub call_id: String,
    /// The textual output produced by the tool.
    pub content: String,
    /// Whether the tool execution ended in an error.
    pub is_error: bool,
}

impl ToolResult {
    /// Creates a successful tool result.
    pub fn success(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: false,
        }
    }

    /// Creates an error tool result.
    pub fn error(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: content.into(),
            is_error: true,
        }
    }
}
