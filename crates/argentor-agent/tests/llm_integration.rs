#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests that call real LLM APIs.
//!
//! **These tests are `#[ignore]` by default** and will NOT run during normal
//! `cargo test`.  To run them you need the appropriate API keys:
//!
//! ```sh
//! cargo test -p argentor-agent --test llm_integration -- --ignored
//! ```
//!
//! Required environment variables (one per provider):
//!
//! - `ANTHROPIC_API_KEY`  — for Claude tests
//! - `OPENAI_API_KEY`     — for OpenAI tests
//! - `GOOGLE_API_KEY`     — for Gemini tests

use argentor_agent::backends::claude::ClaudeBackend;
use argentor_agent::backends::gemini::GeminiBackend;
use argentor_agent::backends::openai::OpenAiBackend;
use argentor_agent::backends::LlmBackend;
use argentor_agent::config::{LlmProvider, ModelConfig};
use argentor_agent::llm::LlmResponse;
use argentor_agent::AgentRunner;
use argentor_core::Message;
use argentor_security::{AuditLog, PermissionSet};
use argentor_skills::{SkillDescriptor, SkillRegistry};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn user_message(content: &str) -> Message {
    Message::new(argentor_core::Role::User, content, Uuid::new_v4())
}

fn make_config(provider: LlmProvider, model_id: &str, api_key: String) -> ModelConfig {
    ModelConfig {
        provider,
        model_id: model_id.into(),
        api_key,
        api_base_url: None,
        temperature: 0.0, // deterministic
        max_tokens: 256,
        max_turns: 5,
        fallback_models: vec![],
        retry_policy: None,
    }
}

fn simple_calculator_tool() -> SkillDescriptor {
    SkillDescriptor {
        name: "calculator".to_string(),
        description: "Evaluate a simple arithmetic expression and return the result.".to_string(),
        parameters_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "The arithmetic expression to evaluate, e.g. '2 + 2'"
                }
            },
            "required": ["expression"]
        }),
        required_capabilities: vec![],
    }
}

// ---------------------------------------------------------------------------
// Claude (Anthropic) tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn test_claude_real_chat() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");
    let config = make_config(LlmProvider::Claude, "claude-haiku-4-5-20251001", api_key);
    let backend = ClaudeBackend::new(config);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        backend.chat(
            Some("You are a helpful assistant. Be concise."),
            &[user_message("Say hello in one sentence.")],
            &[],
        ),
    )
    .await
    .expect("request timed out after 30s")
    .expect("Claude API call failed");

    match result {
        LlmResponse::Done(text) | LlmResponse::Text(text) => {
            assert!(!text.is_empty(), "Response should not be empty");
            // Sanity: the model should produce some recognizable text
            let lower = text.to_lowercase();
            assert!(
                lower.contains("hello") || lower.contains("hi") || lower.contains("hey"),
                "Expected a greeting, got: {text}"
            );
        }
        other => panic!("Expected text response, got: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY"]
async fn test_openai_real_chat() {
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");
    let config = make_config(LlmProvider::OpenAi, "gpt-4o-mini", api_key);
    let backend = OpenAiBackend::new(config);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        backend.chat(
            Some("You are a helpful assistant. Be concise."),
            &[user_message("Say hello in one sentence.")],
            &[],
        ),
    )
    .await
    .expect("request timed out after 30s")
    .expect("OpenAI API call failed");

    match result {
        LlmResponse::Done(text) | LlmResponse::Text(text) => {
            assert!(!text.is_empty(), "Response should not be empty");
        }
        other => panic!("Expected text response, got: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires GOOGLE_API_KEY"]
async fn test_gemini_real_chat() {
    let api_key = std::env::var("GOOGLE_API_KEY").expect("GOOGLE_API_KEY required");
    let config = make_config(LlmProvider::Gemini, "gemini-2.0-flash", api_key);
    let backend = GeminiBackend::new(config);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        backend.chat(
            Some("You are a helpful assistant. Be concise."),
            &[user_message("Say hello in one sentence.")],
            &[],
        ),
    )
    .await
    .expect("request timed out after 30s")
    .expect("Gemini API call failed");

    match result {
        LlmResponse::Done(text) | LlmResponse::Text(text) => {
            assert!(!text.is_empty(), "Response should not be empty");
        }
        other => panic!("Expected text response, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Claude tool calling
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn test_claude_tool_calling() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");
    let config = make_config(LlmProvider::Claude, "claude-haiku-4-5-20251001", api_key);
    let backend = ClaudeBackend::new(config);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        backend.chat(
            Some("You have a calculator tool. Use it to answer math questions."),
            &[user_message("What is 2 + 2? Use the calculator tool.")],
            &[simple_calculator_tool()],
        ),
    )
    .await
    .expect("request timed out after 30s")
    .expect("Claude API call failed");

    match result {
        LlmResponse::ToolUse { tool_calls, .. } => {
            assert!(!tool_calls.is_empty(), "Expected at least one tool call");
            assert_eq!(
                tool_calls[0].name, "calculator",
                "Expected calculator tool call, got: {}",
                tool_calls[0].name
            );
            // The arguments should contain the expression
            let args = &tool_calls[0].arguments;
            assert!(
                args.get("expression").is_some(),
                "Expected 'expression' argument in tool call, got: {args}"
            );
        }
        other => panic!("Expected ToolUse response, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Full AgentRunner end-to-end with real Claude backend
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn test_agent_runner_real_e2e() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");
    let config = make_config(LlmProvider::Claude, "claude-haiku-4-5-20251001", api_key);

    // Set up a skill registry with builtins (echo, time, etc.)
    let mut registry = SkillRegistry::new();
    argentor_builtins::register_builtins(&mut registry);
    let skills = Arc::new(registry);

    let permissions = PermissionSet::new();
    let audit = Arc::new(AuditLog::new(PathBuf::from("/tmp/argentor-test-audit")));

    let agent = AgentRunner::new(config, skills, permissions, audit).with_system_prompt(
        "You are a helpful assistant. Answer concisely. \
             If asked a math question, just respond with the answer directly.",
    );

    let mut session = argentor_session::Session::new();

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        agent.run(&mut session, "What is 2 + 2? Reply with just the number."),
    )
    .await
    .expect("request timed out after 30s")
    .expect("AgentRunner::run failed");

    assert!(!response.is_empty(), "Response should not be empty");
    assert!(
        response.contains('4'),
        "Expected response to contain '4', got: {response}"
    );
}
