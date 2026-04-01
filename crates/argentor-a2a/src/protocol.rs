//! A2A protocol types following the Google Agent-to-Agent specification.
//!
//! All types use `#[serde(rename_all = "camelCase")]` to match the A2A wire format.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Agent Card
// ---------------------------------------------------------------------------

/// Metadata describing an A2A-compliant agent's identity, capabilities, and skills.
///
/// Agent cards are served at `/.well-known/agent.json` and allow other agents
/// to discover what this agent can do before sending tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    /// Human-readable name of the agent.
    pub name: String,
    /// Short description of what the agent does.
    pub description: String,
    /// The base URL where this agent is reachable.
    pub url: String,
    /// Version string for this agent (e.g. "1.0.0").
    pub version: String,
    /// Information about the agent's provider/organization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// Protocol-level capabilities this agent supports.
    pub capabilities: AgentCapabilities,
    /// List of skills this agent can perform.
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
    /// Default content types accepted as input.
    #[serde(default)]
    pub default_input_modes: Vec<String>,
    /// Default content types produced as output.
    #[serde(default)]
    pub default_output_modes: Vec<String>,
    /// Authentication requirements for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationInfo>,
}

/// Information about the organization or individual providing the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProvider {
    /// Name of the provider organization.
    pub organization: String,
    /// Optional URL for the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Protocol-level capabilities advertised by an A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    /// Whether the agent supports streaming responses via SSE.
    #[serde(default)]
    pub streaming: bool,
    /// Whether the agent supports push notifications for task updates.
    #[serde(default)]
    pub push_notifications: bool,
    /// Whether the agent retains and exposes task state transition history.
    #[serde(default)]
    pub state_transition_history: bool,
}

/// A skill advertised by an A2A agent.
///
/// Skills describe specific capabilities the agent can perform, allowing
/// callers to select the right agent for a given task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    /// Unique identifier for this skill.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description of what the skill does.
    pub description: String,
    /// Searchable tags for discovery.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Example inputs or use-case descriptions.
    #[serde(default)]
    pub examples: Vec<String>,
}

/// Authentication requirements for communicating with an A2A agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationInfo {
    /// Supported authentication schemes.
    #[serde(default)]
    pub schemes: Vec<AuthScheme>,
}

/// A single authentication scheme supported by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthScheme {
    /// The scheme identifier (e.g. "Bearer", "ApiKey", "OAuth2").
    pub scheme: String,
    /// Optional service URL for token exchange or key validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Task types
// ---------------------------------------------------------------------------

/// A task represents a unit of work within the A2A protocol.
///
/// Tasks follow a lifecycle: `Submitted` -> `Working` -> `Completed`/`Failed`/`Canceled`.
/// The `InputRequired` state allows the agent to request additional information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A2ATask {
    /// Unique task identifier.
    pub id: String,
    /// Session identifier for grouping related tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Current status of the task.
    pub status: TaskStatus,
    /// Messages exchanged within this task.
    #[serde(default)]
    pub messages: Vec<TaskMessage>,
    /// Artifacts produced by this task.
    #[serde(default)]
    pub artifacts: Vec<TaskArtifact>,
    /// Arbitrary key-value metadata attached to the task.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    /// History of status transitions (if `state_transition_history` is enabled).
    #[serde(default)]
    pub history: Vec<TaskStatusEvent>,
    /// Timestamp when the task was created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Timestamp when the task was last updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/// Current lifecycle status of an A2A task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    /// The task has been submitted but not yet started.
    Submitted,
    /// The agent is actively working on the task.
    Working,
    /// The agent needs additional input from the caller.
    InputRequired,
    /// The task completed successfully.
    Completed,
    /// The task failed.
    Failed,
    /// The task was canceled by the caller.
    Canceled,
}

/// A recorded status transition event for task history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusEvent {
    /// The status the task transitioned to.
    pub status: TaskStatus,
    /// When the transition occurred.
    pub timestamp: DateTime<Utc>,
    /// Optional human-readable description of the transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// A message exchanged within an A2A task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMessage {
    /// The role of the message sender.
    pub role: MessageRole,
    /// Content parts that make up this message.
    pub parts: Vec<MessagePart>,
    /// Arbitrary key-value metadata attached to the message.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// The role of a message sender within the A2A protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageRole {
    /// A message from the calling user or agent.
    User,
    /// A message from the receiving agent.
    Agent,
}

/// A single content part within a message or artifact.
///
/// Messages and artifacts are composed of one or more parts, each carrying
/// text, file data, or arbitrary structured data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MessagePart {
    /// A plain-text content part.
    #[serde(rename = "text")]
    Text {
        /// The text content.
        text: String,
    },
    /// A file content part.
    #[serde(rename = "file")]
    File {
        /// The file data.
        file: FileContent,
    },
    /// An arbitrary structured data part.
    #[serde(rename = "data")]
    Data {
        /// The structured data payload.
        data: serde_json::Value,
    },
}

/// File content that can be transmitted inline (bytes) or by reference (URI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// Optional file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// MIME type of the file (e.g. "application/pdf").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Base64-encoded file bytes (inline transfer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    /// URI pointing to the file (reference transfer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

// ---------------------------------------------------------------------------
// Artifact types
// ---------------------------------------------------------------------------

/// An artifact produced by an A2A task.
///
/// Artifacts represent output data — files, documents, structured results —
/// generated during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifact {
    /// Unique identifier for this artifact.
    pub id: String,
    /// Human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Description of the artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Content parts that make up this artifact.
    pub parts: Vec<MessagePart>,
    /// Arbitrary key-value metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// JSON-RPC envelope
// ---------------------------------------------------------------------------

/// JSON-RPC 2.0 request envelope for the A2A protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcEnvelope {
    /// Must be "2.0".
    pub jsonrpc: String,
    /// Request identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// The RPC method name.
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// An A2A JSON-RPC request parsed into a typed variant.
#[derive(Debug, Clone)]
pub enum A2ARequest {
    /// Send a new task or add a message to an existing task.
    SendTask {
        /// The task ID (if continuing an existing task).
        id: Option<String>,
        /// The session ID for task grouping.
        session_id: Option<String>,
        /// The message to send.
        message: TaskMessage,
        /// Optional metadata.
        metadata: HashMap<String, serde_json::Value>,
    },
    /// Retrieve the current state of a task.
    GetTask {
        /// The task ID to retrieve.
        id: String,
    },
    /// Cancel a running task.
    CancelTask {
        /// The task ID to cancel.
        id: String,
    },
    /// Retrieve the agent card.
    GetAgentCard,
    /// List tasks, optionally filtered by session.
    ListTasks {
        /// Optional session ID filter.
        session_id: Option<String>,
    },
}

/// An A2A JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AResponse {
    /// JSON-RPC version, always "2.0".
    pub jsonrpc: String,
    /// The request ID this response corresponds to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    /// The result payload (present on success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// The error payload (present on failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<A2AError>,
}

/// A JSON-RPC error object for A2A responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AError {
    /// Numeric error code (following JSON-RPC conventions).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl A2AResponse {
    /// Create a success response with the given result.
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(A2AError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Create a "method not found" error response.
    pub fn method_not_found(id: Option<serde_json::Value>, method: &str) -> Self {
        Self::error(id, -32601, format!("Method not found: {method}"))
    }

    /// Create an "invalid params" error response.
    pub fn invalid_params(id: Option<serde_json::Value>, detail: &str) -> Self {
        Self::error(id, -32602, format!("Invalid params: {detail}"))
    }

    /// Create an "internal error" response.
    pub fn internal_error(id: Option<serde_json::Value>, detail: &str) -> Self {
        Self::error(id, -32603, format!("Internal error: {detail}"))
    }

    /// Create a "task not found" error response.
    pub fn task_not_found(id: Option<serde_json::Value>, task_id: &str) -> Self {
        Self::error(id, -32001, format!("Task not found: {task_id}"))
    }
}

impl A2ATask {
    /// Create a new task in [`TaskStatus::Submitted`] state with an initial message.
    pub fn new(message: TaskMessage) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: None,
            status: TaskStatus::Submitted,
            messages: vec![message],
            artifacts: vec![],
            metadata: HashMap::new(),
            history: vec![TaskStatusEvent {
                status: TaskStatus::Submitted,
                timestamp: now,
                message: None,
            }],
            created_at: Some(now),
            updated_at: Some(now),
        }
    }

    /// Transition the task to a new status, recording the event in history.
    pub fn transition_to(&mut self, status: TaskStatus, description: Option<String>) {
        let now = Utc::now();
        self.history.push(TaskStatusEvent {
            status: status.clone(),
            timestamp: now,
            message: description,
        });
        self.status = status;
        self.updated_at = Some(now);
    }

    /// Add a message to this task.
    pub fn add_message(&mut self, message: TaskMessage) {
        self.messages.push(message);
        self.updated_at = Some(Utc::now());
    }

    /// Add an artifact to this task.
    pub fn add_artifact(&mut self, artifact: TaskArtifact) {
        self.artifacts.push(artifact);
        self.updated_at = Some(Utc::now());
    }

    /// Check whether the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Canceled
        )
    }
}

impl TaskMessage {
    /// Create a user message with a single text part.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            parts: vec![MessagePart::Text { text: text.into() }],
            metadata: HashMap::new(),
        }
    }

    /// Create an agent message with a single text part.
    pub fn agent_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Agent,
            parts: vec![MessagePart::Text { text: text.into() }],
            metadata: HashMap::new(),
        }
    }
}

impl TaskArtifact {
    /// Create a text artifact.
    pub fn text(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: Some(name.into()),
            description: None,
            parts: vec![MessagePart::Text { text: text.into() }],
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC standard error codes
// ---------------------------------------------------------------------------

/// Standard JSON-RPC 2.0 error code: parse error.
pub const JSONRPC_PARSE_ERROR: i64 = -32700;
/// Standard JSON-RPC 2.0 error code: invalid request.
pub const JSONRPC_INVALID_REQUEST: i64 = -32600;
/// Standard JSON-RPC 2.0 error code: method not found.
pub const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
/// Standard JSON-RPC 2.0 error code: invalid params.
pub const JSONRPC_INVALID_PARAMS: i64 = -32602;
/// Standard JSON-RPC 2.0 error code: internal error.
pub const JSONRPC_INTERNAL_ERROR: i64 = -32603;

/// A2A-specific error code: task not found.
pub const A2A_TASK_NOT_FOUND: i64 = -32001;
/// A2A-specific error code: task already in terminal state.
pub const A2A_TASK_TERMINAL: i64 = -32002;

// ---------------------------------------------------------------------------
// Streaming event types
// ---------------------------------------------------------------------------

/// An event emitted during streaming task processing via `tasks/sendSubscribe`.
///
/// These events are sent as Server-Sent Events (SSE) to inform the caller
/// about task progress in real time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskStreamEvent {
    /// A status transition event indicating the task moved to a new state.
    #[serde(rename = "status_update")]
    StatusUpdate {
        /// The task ID this event belongs to.
        task_id: String,
        /// The new status of the task.
        status: TaskStatus,
        /// Optional human-readable description of the status change.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// An artifact produced during task execution.
    #[serde(rename = "artifact")]
    Artifact {
        /// The task ID this event belongs to.
        task_id: String,
        /// The produced artifact.
        artifact: TaskArtifact,
    },
    /// An intermediate message emitted during task execution.
    #[serde(rename = "message")]
    Message {
        /// The task ID this event belongs to.
        task_id: String,
        /// The intermediate message.
        message: TaskMessage,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_serialization() {
        let card = AgentCard {
            name: "TestAgent".to_string(),
            description: "A test agent".to_string(),
            url: "http://localhost:3000".to_string(),
            version: "1.0.0".to_string(),
            provider: Some(AgentProvider {
                organization: "Argentor".to_string(),
                url: Some("https://argentor.dev".to_string()),
            }),
            capabilities: AgentCapabilities {
                streaming: true,
                push_notifications: false,
                state_transition_history: true,
            },
            skills: vec![AgentSkill {
                id: "summarize".to_string(),
                name: "Summarize".to_string(),
                description: "Summarize a document".to_string(),
                tags: vec!["nlp".to_string()],
                examples: vec!["Summarize this article".to_string()],
            }],
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            authentication: None,
        };

        let json = serde_json::to_string_pretty(&card).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "TestAgent");
        assert_eq!(parsed["capabilities"]["streaming"], true);
        assert_eq!(parsed["skills"][0]["id"], "summarize");
        assert!(parsed.get("authentication").is_none());
    }

    #[test]
    fn test_agent_card_deserialization() {
        let json = r#"{
            "name": "RemoteAgent",
            "description": "An external agent",
            "url": "https://remote.example.com",
            "version": "2.0.0",
            "capabilities": {
                "streaming": false,
                "pushNotifications": true,
                "stateTransitionHistory": false
            },
            "skills": [],
            "defaultInputModes": ["application/json"],
            "defaultOutputModes": ["application/json"]
        }"#;

        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.name, "RemoteAgent");
        assert!(card.capabilities.push_notifications);
        assert!(!card.capabilities.streaming);
        assert_eq!(card.default_input_modes, vec!["application/json"]);
    }

    #[test]
    fn test_task_status_serialization() {
        let statuses = vec![
            (TaskStatus::Submitted, "\"submitted\""),
            (TaskStatus::Working, "\"working\""),
            (TaskStatus::InputRequired, "\"inputRequired\""),
            (TaskStatus::Completed, "\"completed\""),
            (TaskStatus::Failed, "\"failed\""),
            (TaskStatus::Canceled, "\"canceled\""),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, expected);

            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn test_task_creation_and_lifecycle() {
        let msg = TaskMessage::user_text("Hello, agent!");
        let mut task = A2ATask::new(msg);

        assert_eq!(task.status, TaskStatus::Submitted);
        assert_eq!(task.messages.len(), 1);
        assert!(!task.is_terminal());
        assert_eq!(task.history.len(), 1);

        task.transition_to(TaskStatus::Working, Some("Processing request".to_string()));
        assert_eq!(task.status, TaskStatus::Working);
        assert_eq!(task.history.len(), 2);
        assert!(!task.is_terminal());

        task.add_message(TaskMessage::agent_text("Done!"));
        assert_eq!(task.messages.len(), 2);

        task.transition_to(TaskStatus::Completed, None);
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.is_terminal());
        assert_eq!(task.history.len(), 3);
    }

    #[test]
    fn test_task_serialization_roundtrip() {
        let mut task = A2ATask::new(TaskMessage::user_text("Translate this"));
        task.session_id = Some("session-123".to_string());
        task.transition_to(TaskStatus::Working, None);
        task.add_artifact(TaskArtifact::text("translation", "Translated text"));
        task.transition_to(TaskStatus::Completed, None);

        let json = serde_json::to_string(&task).unwrap();
        let parsed: A2ATask = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.session_id, Some("session-123".to_string()));
        assert_eq!(parsed.status, TaskStatus::Completed);
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.artifacts.len(), 1);
        assert_eq!(parsed.history.len(), 3);
    }

    #[test]
    fn test_message_part_text_serialization() {
        let part = MessagePart::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "text");
        assert_eq!(parsed["text"], "Hello");
    }

    #[test]
    fn test_message_part_file_serialization() {
        let part = MessagePart::File {
            file: FileContent {
                name: Some("report.pdf".to_string()),
                mime_type: Some("application/pdf".to_string()),
                bytes: Some("dGVzdA==".to_string()),
                uri: None,
            },
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "file");
        assert_eq!(parsed["file"]["name"], "report.pdf");
        assert_eq!(parsed["file"]["mimeType"], "application/pdf");
    }

    #[test]
    fn test_message_part_data_serialization() {
        let part = MessagePart::Data {
            data: serde_json::json!({"key": "value", "count": 42}),
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "data");
        assert_eq!(parsed["data"]["key"], "value");
        assert_eq!(parsed["data"]["count"], 42);
    }

    #[test]
    fn test_message_part_deserialization() {
        let json = r#"{"type":"text","text":"Hello world"}"#;
        let part: MessagePart = serde_json::from_str(json).unwrap();
        match part {
            MessagePart::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_a2a_response_success() {
        let resp = A2AResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({"status": "ok"}),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_a2a_response_error() {
        let resp = A2AResponse::error(Some(serde_json::json!(2)), -32600, "Invalid request");
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid request");
    }

    #[test]
    fn test_a2a_response_method_not_found() {
        let resp = A2AResponse::method_not_found(Some(serde_json::json!(3)), "unknown/method");
        let err = resp.error.unwrap();
        assert_eq!(err.code, JSONRPC_METHOD_NOT_FOUND);
        assert!(err.message.contains("unknown/method"));
    }

    #[test]
    fn test_a2a_response_task_not_found() {
        let resp = A2AResponse::task_not_found(Some(serde_json::json!(4)), "task-xyz");
        let err = resp.error.unwrap();
        assert_eq!(err.code, A2A_TASK_NOT_FOUND);
        assert!(err.message.contains("task-xyz"));
    }

    #[test]
    fn test_json_rpc_envelope_serialization() {
        let envelope = JsonRpcEnvelope {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "tasks/send".to_string(),
            params: Some(serde_json::json!({"message": {"role": "user", "parts": []}})),
        };
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "tasks/send");
    }

    #[test]
    fn test_json_rpc_envelope_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":42,"method":"tasks/get","params":{"id":"task-1"}}"#;
        let envelope: JsonRpcEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.method, "tasks/get");
        assert_eq!(envelope.id, Some(serde_json::json!(42)));
    }

    #[test]
    fn test_authentication_info_serialization() {
        let auth = AuthenticationInfo {
            schemes: vec![
                AuthScheme {
                    scheme: "Bearer".to_string(),
                    service_url: Some("https://auth.example.com/token".to_string()),
                },
                AuthScheme {
                    scheme: "ApiKey".to_string(),
                    service_url: None,
                },
            ],
        };
        let json = serde_json::to_string(&auth).unwrap();
        let parsed: AuthenticationInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schemes.len(), 2);
        assert_eq!(parsed.schemes[0].scheme, "Bearer");
        assert!(parsed.schemes[1].service_url.is_none());
    }

    #[test]
    fn test_task_message_user_text() {
        let msg = TaskMessage::user_text("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.parts.len(), 1);
        match &msg.parts[0] {
            MessagePart::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_task_message_agent_text() {
        let msg = TaskMessage::agent_text("Response");
        assert_eq!(msg.role, MessageRole::Agent);
        assert_eq!(msg.parts.len(), 1);
    }

    #[test]
    fn test_task_artifact_text() {
        let artifact = TaskArtifact::text("output", "Result data");
        assert_eq!(artifact.name, Some("output".to_string()));
        assert_eq!(artifact.parts.len(), 1);
    }

    #[test]
    fn test_agent_capabilities_default() {
        let caps = AgentCapabilities::default();
        assert!(!caps.streaming);
        assert!(!caps.push_notifications);
        assert!(!caps.state_transition_history);
    }

    #[test]
    fn test_task_stream_event_status_update_serialization() {
        let event = TaskStreamEvent::StatusUpdate {
            task_id: "task-1".to_string(),
            status: TaskStatus::Working,
            message: Some("Processing".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "status_update");
        assert_eq!(parsed["task_id"], "task-1");
        assert_eq!(parsed["status"], "working");
        assert_eq!(parsed["message"], "Processing");
    }

    #[test]
    fn test_task_stream_event_artifact_serialization() {
        let event = TaskStreamEvent::Artifact {
            task_id: "task-2".to_string(),
            artifact: TaskArtifact::text("output", "Result data"),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "artifact");
        assert_eq!(parsed["task_id"], "task-2");
        assert!(parsed["artifact"].is_object());
    }

    #[test]
    fn test_task_stream_event_message_serialization() {
        let event = TaskStreamEvent::Message {
            task_id: "task-3".to_string(),
            message: TaskMessage::agent_text("Intermediate update"),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "message");
        assert_eq!(parsed["task_id"], "task-3");
        assert!(parsed["message"].is_object());
    }

    #[test]
    fn test_task_stream_event_status_update_no_message() {
        let event = TaskStreamEvent::StatusUpdate {
            task_id: "task-4".to_string(),
            status: TaskStatus::Completed,
            message: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "status_update");
        assert_eq!(parsed["status"], "completed");
        assert!(parsed.get("message").is_none());
    }

    #[test]
    fn test_task_stream_event_deserialization() {
        let json =
            r#"{"type":"status_update","task_id":"t-1","status":"working","message":"busy"}"#;
        let event: TaskStreamEvent = serde_json::from_str(json).unwrap();
        match event {
            TaskStreamEvent::StatusUpdate {
                task_id,
                status,
                message,
            } => {
                assert_eq!(task_id, "t-1");
                assert_eq!(status, TaskStatus::Working);
                assert_eq!(message, Some("busy".to_string()));
            }
            _ => panic!("Expected StatusUpdate variant"),
        }
    }

    #[test]
    fn test_task_add_artifact() {
        let mut task = A2ATask::new(TaskMessage::user_text("Generate report"));
        task.add_artifact(TaskArtifact::text("report", "Report content"));
        assert_eq!(task.artifacts.len(), 1);
        assert_eq!(task.artifacts[0].name, Some("report".to_string()));
    }

    #[test]
    fn test_message_role_serialization() {
        let user_json = serde_json::to_string(&MessageRole::User).unwrap();
        assert_eq!(user_json, "\"user\"");

        let agent_json = serde_json::to_string(&MessageRole::Agent).unwrap();
        assert_eq!(agent_json, "\"agent\"");

        let user: MessageRole = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, MessageRole::User);
    }
}
