use crate::failover::RetryPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Claude,
    OpenAi,
    OpenRouter,
    /// Groq cloud inference — OpenAI-compatible API, free tier with rate limits.
    Groq,
    /// Use the local `claude` CLI in headless mode (-p --output-format json).
    /// No API key needed — uses the user's existing Claude Code session/subscription.
    ClaudeCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: LlmProvider,
    pub model_id: String,
    pub api_key: String,
    pub api_base_url: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default)]
    pub fallback_models: Vec<ModelConfig>,
    #[serde(default)]
    pub retry_policy: Option<RetryPolicy>,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_max_turns() -> u32 {
    20
}

impl ModelConfig {
    pub fn base_url(&self) -> &str {
        if let Some(url) = &self.api_base_url {
            url
        } else {
            match self.provider {
                LlmProvider::Claude => "https://api.anthropic.com",
                LlmProvider::OpenAi => "https://api.openai.com",
                LlmProvider::OpenRouter => "https://openrouter.ai/api",
                LlmProvider::Groq => "https://api.groq.com/openai",
                LlmProvider::ClaudeCode => "local://claude-cli",
            }
        }
    }
}
