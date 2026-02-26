use crate::backends::claude::ClaudeBackend;
use crate::backends::claude_code::ClaudeCodeBackend;
use crate::backends::openai::OpenAiBackend;
use crate::backends::LlmBackend;
use crate::config::{LlmProvider, ModelConfig};
use crate::failover::FailoverBackend;
use crate::stream::StreamEvent;
use agentor_core::{AgentorResult, Message, ToolCall};
use agentor_skills::SkillDescriptor;
use tokio::sync::mpsc;

/// Response from the LLM — either text content or a tool call request.
#[derive(Debug)]
pub enum LlmResponse {
    Text(String),
    ToolUse {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    Done(String),
}

/// LLM client that dispatches to the correct provider backend.
///
/// Uses the `LlmBackend` trait to abstract away provider-specific API differences.
/// To add a new provider: implement `LlmBackend` in `backends/` and wire it here.
pub struct LlmClient {
    backend: Box<dyn LlmBackend>,
}

impl LlmClient {
    pub fn new(config: ModelConfig) -> Self {
        let fallback_models = config.fallback_models.clone();
        let retry_policy = config.retry_policy.clone();

        let make_backend = |cfg: ModelConfig| -> Box<dyn LlmBackend> {
            match cfg.provider {
                LlmProvider::Claude => Box::new(ClaudeBackend::new(cfg)),
                LlmProvider::OpenAi | LlmProvider::OpenRouter | LlmProvider::Groq => {
                    Box::new(OpenAiBackend::new(cfg))
                }
                LlmProvider::ClaudeCode => Box::new(ClaudeCodeBackend::new(cfg)),
            }
        };

        let backend: Box<dyn LlmBackend> = if fallback_models.is_empty() {
            make_backend(config)
        } else {
            let policy = retry_policy.unwrap_or_default();
            let mut backends: Vec<Box<dyn LlmBackend>> = Vec::new();
            backends.push(make_backend(config));
            for fallback in fallback_models {
                backends.push(make_backend(fallback));
            }
            Box::new(FailoverBackend::new(backends, policy))
        };

        Self { backend }
    }

    /// Create from a pre-built backend (for custom/external providers).
    pub fn from_backend(backend: Box<dyn LlmBackend>) -> Self {
        Self { backend }
    }

    /// Non-streaming chat completion.
    pub async fn chat(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<LlmResponse> {
        self.backend.chat(system_prompt, messages, tools).await
    }

    /// Streaming chat completion.
    ///
    /// Returns an `mpsc::Receiver<StreamEvent>` that yields events as the LLM
    /// generates its response, plus the final aggregated `LlmResponse` when done.
    pub async fn chat_stream(
        &self,
        system_prompt: Option<&str>,
        messages: &[Message],
        tools: &[SkillDescriptor],
    ) -> AgentorResult<(
        mpsc::Receiver<StreamEvent>,
        tokio::task::JoinHandle<AgentorResult<LlmResponse>>,
    )> {
        self.backend
            .chat_stream(system_prompt, messages, tools)
            .await
    }
}
