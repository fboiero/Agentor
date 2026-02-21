//! Regression tests for agentor-agent: ContextWindow, ModelConfig, LlmProvider, AgentRunner.

use agentor_agent::{AgentRunner, ContextWindow, LlmProvider, ModelConfig, StreamEvent};
use agentor_core::{Message, Role};
use agentor_security::{AuditLog, PermissionSet};
use agentor_session::Session;
use agentor_skills::SkillRegistry;
use std::sync::Arc;

// --- ModelConfig & LlmProvider ---

#[test]
fn test_llm_provider_claude_serialization() {
    let provider = LlmProvider::Claude;
    let json = serde_json::to_string(&provider).unwrap();
    assert_eq!(json, "\"claude\"");

    let deserialized: LlmProvider = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, LlmProvider::Claude));
}

#[test]
fn test_llm_provider_openai_serialization() {
    let provider = LlmProvider::OpenAi;
    let json = serde_json::to_string(&provider).unwrap();
    assert_eq!(json, "\"openai\"");

    let deserialized: LlmProvider = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, LlmProvider::OpenAi));
}

#[test]
fn test_llm_provider_openrouter_serialization() {
    let provider = LlmProvider::OpenRouter;
    let json = serde_json::to_string(&provider).unwrap();
    assert_eq!(json, "\"openrouter\"");

    let deserialized: LlmProvider = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, LlmProvider::OpenRouter));
}

#[test]
fn test_model_config_full_serialization() {
    let config = ModelConfig {
        provider: LlmProvider::OpenRouter,
        model_id: "anthropic/claude-sonnet-4".to_string(),
        api_key: "sk-test-123".to_string(),
        api_base_url: None,
        temperature: 0.5,
        max_tokens: 2048,
        max_turns: 10,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();

    assert!(matches!(deserialized.provider, LlmProvider::OpenRouter));
    assert_eq!(deserialized.model_id, "anthropic/claude-sonnet-4");
    assert_eq!(deserialized.temperature, 0.5);
    assert_eq!(deserialized.max_tokens, 2048);
    assert_eq!(deserialized.max_turns, 10);
}

#[test]
fn test_model_config_base_url_defaults() {
    let claude_config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "claude-sonnet-4-20250514".to_string(),
        api_key: "key".to_string(),
        api_base_url: None,
        temperature: 0.7,
        max_tokens: 4096,
        max_turns: 20,
    };
    assert_eq!(claude_config.base_url(), "https://api.anthropic.com");

    let openai_config = ModelConfig {
        provider: LlmProvider::OpenAi,
        model_id: "gpt-4".to_string(),
        api_key: "key".to_string(),
        api_base_url: None,
        temperature: 0.7,
        max_tokens: 4096,
        max_turns: 20,
    };
    assert_eq!(openai_config.base_url(), "https://api.openai.com");

    let openrouter_config = ModelConfig {
        provider: LlmProvider::OpenRouter,
        model_id: "anthropic/claude-sonnet-4".to_string(),
        api_key: "key".to_string(),
        api_base_url: None,
        temperature: 0.7,
        max_tokens: 4096,
        max_turns: 20,
    };
    assert_eq!(openrouter_config.base_url(), "https://openrouter.ai/api");
}

#[test]
fn test_model_config_base_url_custom_override() {
    let config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "test".to_string(),
        api_key: "key".to_string(),
        api_base_url: Some("http://localhost:8080".to_string()),
        temperature: 0.7,
        max_tokens: 4096,
        max_turns: 20,
    };
    assert_eq!(config.base_url(), "http://localhost:8080");
}

#[test]
fn test_model_config_deserialization_with_defaults() {
    let toml_str = r#"
        provider = "claude"
        model_id = "test-model"
        api_key = "test-key"
    "#;

    let config: ModelConfig = toml::from_str(toml_str).unwrap();
    assert!(matches!(config.provider, LlmProvider::Claude));
    assert_eq!(config.temperature, 0.7); // default
    assert_eq!(config.max_tokens, 4096); // default
    assert_eq!(config.max_turns, 20); // default
    assert!(config.api_base_url.is_none());
}

// --- ContextWindow ---

#[test]
fn test_context_window_basic() {
    let mut ctx = ContextWindow::new(10);
    assert_eq!(ctx.messages().len(), 0);
    assert!(ctx.system_prompt().is_none());

    let sid = uuid::Uuid::new_v4();
    ctx.push(Message::user("hello", sid));
    assert_eq!(ctx.messages().len(), 1);
    assert_eq!(ctx.messages()[0].content, "hello");
}

#[test]
fn test_context_window_system_prompt() {
    let mut ctx = ContextWindow::new(10);
    ctx.set_system_prompt("You are helpful.");
    assert_eq!(ctx.system_prompt(), Some("You are helpful."));
}

#[test]
fn test_context_window_truncation() {
    let mut ctx = ContextWindow::new(3);
    let sid = uuid::Uuid::new_v4();

    ctx.push(Message::user("msg1", sid));
    ctx.push(Message::user("msg2", sid));
    ctx.push(Message::user("msg3", sid));
    assert_eq!(ctx.messages().len(), 3);

    // Pushing a 4th message should truncate the oldest
    ctx.push(Message::user("msg4", sid));
    assert_eq!(ctx.messages().len(), 3);
    assert_eq!(ctx.messages()[0].content, "msg2");
    assert_eq!(ctx.messages()[2].content, "msg4");
}

#[test]
fn test_context_window_estimated_tokens() {
    let mut ctx = ContextWindow::new(100);
    let sid = uuid::Uuid::new_v4();

    ctx.set_system_prompt("system"); // 6 chars -> ~1 token
    ctx.push(Message::user("hello world", sid)); // 11 chars -> ~2 tokens

    let tokens = ctx.estimated_tokens();
    assert!(tokens > 0);
    // "system" (6/4=1) + "hello world" (11/4=2) = 3
    assert_eq!(tokens, 3);
}

#[test]
fn test_context_window_clear() {
    let mut ctx = ContextWindow::new(10);
    let sid = uuid::Uuid::new_v4();

    ctx.push(Message::user("test", sid));
    assert_eq!(ctx.messages().len(), 1);

    ctx.clear();
    assert_eq!(ctx.messages().len(), 0);
}

// --- StreamEvent serialization ---

#[test]
fn test_stream_event_text_delta_serialization() {
    let event = StreamEvent::TextDelta {
        text: "Hello".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"text_delta\""));
    assert!(json.contains("\"text\":\"Hello\""));

    let deserialized: StreamEvent = serde_json::from_str(&json).unwrap();
    if let StreamEvent::TextDelta { text } = deserialized {
        assert_eq!(text, "Hello");
    } else {
        panic!("Expected TextDelta");
    }
}

#[test]
fn test_stream_event_tool_call_start() {
    let event = StreamEvent::ToolCallStart {
        id: "call_1".to_string(),
        name: "shell".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"tool_call_start\""));

    let deserialized: StreamEvent = serde_json::from_str(&json).unwrap();
    if let StreamEvent::ToolCallStart { id, name } = deserialized {
        assert_eq!(id, "call_1");
        assert_eq!(name, "shell");
    } else {
        panic!("Expected ToolCallStart");
    }
}

#[test]
fn test_stream_event_done() {
    let event = StreamEvent::Done;
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: StreamEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, StreamEvent::Done));
}

#[test]
fn test_stream_event_error() {
    let event = StreamEvent::Error {
        message: "timeout".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: StreamEvent = serde_json::from_str(&json).unwrap();
    if let StreamEvent::Error { message } = deserialized {
        assert_eq!(message, "timeout");
    } else {
        panic!("Expected Error");
    }
}

// --- AgentRunner construction ---

#[tokio::test]
async fn test_agent_runner_construction() {
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(AuditLog::new(tmp.path().join("audit")));
    let skills = Arc::new(SkillRegistry::new());
    let permissions = PermissionSet::new();

    let config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "test-model".to_string(),
        api_key: "test-key".to_string(),
        api_base_url: Some("http://127.0.0.1:1".to_string()),
        temperature: 0.7,
        max_tokens: 100,
        max_turns: 3,
    };

    // Just verify construction doesn't panic
    let _agent = AgentRunner::new(config, skills, permissions, audit);
}

#[tokio::test]
async fn test_agent_runner_with_builtins() {
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(AuditLog::new(tmp.path().join("audit")));
    let mut registry = SkillRegistry::new();
    agentor_builtins::register_builtins(&mut registry);

    // Verify all 5 builtins registered (shell, file_read, file_write, http_fetch, browser)
    assert_eq!(registry.skill_count(), 5);

    let skills = Arc::new(registry);
    let permissions = PermissionSet::new();

    let config = ModelConfig {
        provider: LlmProvider::OpenRouter,
        model_id: "test".to_string(),
        api_key: "key".to_string(),
        api_base_url: Some("http://127.0.0.1:1".to_string()),
        temperature: 0.7,
        max_tokens: 100,
        max_turns: 3,
    };

    let _agent = AgentRunner::new(config, skills, permissions, audit);
}

#[tokio::test]
async fn test_agent_runner_run_fails_with_bad_url() {
    let tmp = tempfile::tempdir().unwrap();
    let audit = Arc::new(AuditLog::new(tmp.path().join("audit")));
    let skills = Arc::new(SkillRegistry::new());
    let permissions = PermissionSet::new();

    let config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "test-model".to_string(),
        api_key: "test-key".to_string(),
        api_base_url: Some("http://127.0.0.1:1".to_string()),
        temperature: 0.7,
        max_tokens: 100,
        max_turns: 3,
    };

    let agent = AgentRunner::new(config, skills, permissions, audit);
    let mut session = Session::new();

    // Should fail because the LLM endpoint is unreachable
    let result = agent.run(&mut session, "hello").await;
    assert!(result.is_err());

    // Session should still have the user message
    assert!(session.message_count() >= 1);
    assert_eq!(session.messages[0].role, Role::User);
    assert_eq!(session.messages[0].content, "hello");
}

// --- Message construction regression ---

#[test]
fn test_message_all_roles() {
    let sid = uuid::Uuid::new_v4();

    let user = Message::user("hi", sid);
    assert_eq!(user.role, Role::User);

    let assistant = Message::assistant("hello", sid);
    assert_eq!(assistant.role, Role::Assistant);

    let system = Message::system("prompt", sid);
    assert_eq!(system.role, Role::System);

    let tool = Message::new(Role::Tool, "result", sid);
    assert_eq!(tool.role, Role::Tool);
}

#[test]
fn test_message_metadata() {
    let sid = uuid::Uuid::new_v4();
    let mut msg = Message::user("test", sid);
    msg.metadata
        .insert("key".to_string(), serde_json::json!("value"));

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.metadata.get("key").unwrap(), "value");
}
