#![allow(clippy::unwrap_used, clippy::expect_used)]

use agentor_core::*;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// 1. Message serialization roundtrip
// ---------------------------------------------------------------------------

#[test]
fn message_serialization_roundtrip() {
    let session_id = Uuid::new_v4();
    let mut msg = Message::user("Hello, Agentor!", session_id);
    msg.metadata.insert(
        "source".to_string(),
        serde_json::Value::String("test".to_string()),
    );

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, msg.id);
    assert_eq!(deserialized.role, Role::User);
    assert_eq!(deserialized.content, "Hello, Agentor!");
    assert_eq!(deserialized.session_id, session_id);
    assert_eq!(deserialized.timestamp, msg.timestamp);
    assert_eq!(
        deserialized.metadata.get("source"),
        Some(&serde_json::Value::String("test".to_string()))
    );
}

// ---------------------------------------------------------------------------
// 2. ToolCall -> ToolResult flow (success and error variants)
// ---------------------------------------------------------------------------

#[test]
fn tool_call_to_tool_result_flow() {
    let tool_call = ToolCall {
        id: "call_abc123".to_string(),
        name: "web_search".to_string(),
        arguments: serde_json::json!({"query": "Rust async"}),
    };

    // Success path
    let success_result = ToolResult::success(&tool_call.id, "Found 42 results");
    assert_eq!(success_result.call_id, tool_call.id);
    assert_eq!(success_result.content, "Found 42 results");
    assert!(!success_result.is_error);

    // Error path
    let error_result = ToolResult::error(&tool_call.id, "Network timeout");
    assert_eq!(error_result.call_id, tool_call.id);
    assert_eq!(error_result.content, "Network timeout");
    assert!(error_result.is_error);

    // Verify ToolCall serialization roundtrip
    let json = serde_json::to_string(&tool_call).unwrap();
    let deserialized: ToolCall = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, "call_abc123");
    assert_eq!(deserialized.name, "web_search");
    assert_eq!(deserialized.arguments, serde_json::json!({"query": "Rust async"}));
}

// ---------------------------------------------------------------------------
// 3. Error Display and From impls
// ---------------------------------------------------------------------------

#[test]
fn error_display_and_from_impls() {
    // String-based variants display correctly
    let agent_err = AgentorError::Agent("loop crashed".to_string());
    assert_eq!(agent_err.to_string(), "Agent error: loop crashed");

    let http_err = AgentorError::Http("connection refused".to_string());
    assert_eq!(http_err.to_string(), "HTTP error: connection refused");

    let session_err = AgentorError::Session("not found".to_string());
    assert_eq!(session_err.to_string(), "Session error: not found");

    let config_err = AgentorError::Config("missing key".to_string());
    assert_eq!(config_err.to_string(), "Config error: missing key");

    let skill_err = AgentorError::Skill("timeout".to_string());
    assert_eq!(skill_err.to_string(), "Skill error: timeout");

    let channel_err = AgentorError::Channel("closed".to_string());
    assert_eq!(channel_err.to_string(), "Channel error: closed");

    let gateway_err = AgentorError::Gateway("502 bad gateway".to_string());
    assert_eq!(gateway_err.to_string(), "Gateway error: 502 bad gateway");

    let security_err = AgentorError::Security("unauthorized".to_string());
    assert_eq!(security_err.to_string(), "Security error: unauthorized");

    let orchestrator_err = AgentorError::Orchestrator("deadlock".to_string());
    assert_eq!(orchestrator_err.to_string(), "Orchestrator error: deadlock");

    // From<serde_json::Error> conversion
    let bad_json = serde_json::from_str::<serde_json::Value>("not json");
    let serde_err = bad_json.unwrap_err();
    let agentor_err: AgentorError = serde_err.into();
    assert!(agentor_err.to_string().starts_with("JSON error:"));

    // From<std::io::Error> conversion
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let agentor_err: AgentorError = io_err.into();
    assert!(agentor_err.to_string().starts_with("IO error:"));
}

// ---------------------------------------------------------------------------
// 4. Message factory methods set correct roles
// ---------------------------------------------------------------------------

#[test]
fn message_factory_methods() {
    let session_id = Uuid::new_v4();

    let user_msg = Message::user("question", session_id);
    assert_eq!(user_msg.role, Role::User);
    assert_eq!(user_msg.content, "question");
    assert_eq!(user_msg.session_id, session_id);
    assert!(user_msg.metadata.is_empty());

    let assistant_msg = Message::assistant("answer", session_id);
    assert_eq!(assistant_msg.role, Role::Assistant);
    assert_eq!(assistant_msg.content, "answer");
    assert_eq!(assistant_msg.session_id, session_id);

    let system_msg = Message::system("you are helpful", session_id);
    assert_eq!(system_msg.role, Role::System);
    assert_eq!(system_msg.content, "you are helpful");
    assert_eq!(system_msg.session_id, session_id);

    // Each factory should produce a unique message ID
    let msg_a = Message::user("a", session_id);
    let msg_b = Message::user("b", session_id);
    assert_ne!(msg_a.id, msg_b.id);
}

// ---------------------------------------------------------------------------
// 5. ToolResult success/error factories set is_error correctly
// ---------------------------------------------------------------------------

#[test]
fn tool_result_success_error_factories() {
    let success = ToolResult::success("id_1", "all good");
    assert!(!success.is_error);
    assert_eq!(success.call_id, "id_1");
    assert_eq!(success.content, "all good");

    let error = ToolResult::error("id_2", "something broke");
    assert!(error.is_error);
    assert_eq!(error.call_id, "id_2");
    assert_eq!(error.content, "something broke");

    // Roundtrip: success variant preserves is_error = false
    let json = serde_json::to_string(&success).unwrap();
    let deser: ToolResult = serde_json::from_str(&json).unwrap();
    assert!(!deser.is_error);

    // Roundtrip: error variant preserves is_error = true
    let json = serde_json::to_string(&error).unwrap();
    let deser: ToolResult = serde_json::from_str(&json).unwrap();
    assert!(deser.is_error);
}

// ---------------------------------------------------------------------------
// 6. Role serialization/deserialization to/from strings
// ---------------------------------------------------------------------------

#[test]
fn role_serialization() {
    // Serialize: serde(rename_all = "lowercase") means variants become lowercase strings
    let user_json = serde_json::to_string(&Role::User).unwrap();
    assert_eq!(user_json, "\"user\"");

    let assistant_json = serde_json::to_string(&Role::Assistant).unwrap();
    assert_eq!(assistant_json, "\"assistant\"");

    let system_json = serde_json::to_string(&Role::System).unwrap();
    assert_eq!(system_json, "\"system\"");

    let tool_json = serde_json::to_string(&Role::Tool).unwrap();
    assert_eq!(tool_json, "\"tool\"");

    // Deserialize back
    let user: Role = serde_json::from_str("\"user\"").unwrap();
    assert_eq!(user, Role::User);

    let assistant: Role = serde_json::from_str("\"assistant\"").unwrap();
    assert_eq!(assistant, Role::Assistant);

    let system: Role = serde_json::from_str("\"system\"").unwrap();
    assert_eq!(system, Role::System);

    let tool: Role = serde_json::from_str("\"tool\"").unwrap();
    assert_eq!(tool, Role::Tool);

    // Invalid role string should fail
    let bad: Result<Role, _> = serde_json::from_str("\"unknown\"");
    assert!(bad.is_err());
}
