pub mod claude;
pub mod claude_code;
pub mod openai;

use crate::llm::LlmResponse;
use crate::stream::StreamEvent;
use agentor_core::{AgentorResult, Message};
use agentor_skills::SkillDescriptor;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Trait for LLM provider backends.
///
/// Each provider (Claude, OpenAI, Groq, ClaudeCode, etc.) implements this trait
/// to handle API communication. This replaces the if/else chain in LlmClient.
///
/// To add a new provider:
/// 1. Create a new module in `backends/`
/// 2. Implement `LlmBackend` for your struct
/// 3. Add the variant to `LlmProvider` enum in `config.rs`
/// 4. Wire it up in `LlmClient::new()` in `llm.rs`
#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Non-streaming chat completion.
    async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse>;

    /// Streaming chat completion.
    ///
    /// Returns a receiver for stream events and a join handle that resolves
    /// to the final aggregated response.
    async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        JoinHandle<AgentorResult<LlmResponse>>,
    )>;
}
