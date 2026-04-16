//! Bug 3: Malformed prompt template — unclosed {{variable}} placeholder.
//!
//! Expected error quality:
//! Argentor validates prompt templates at compile time (type system) or
//! at agent construction. A raw string with {{unclosed passes through silently —
//! Argentor does NOT validate free-form system prompts (by design).
//! Result: the malformed template is sent as-is to the LLM. No error.
//! Score: file/line — 0 (no error raised)
//!        names problem — 0 (silent pass-through)
//!        suggests fix — 0 (no feedback)
//! Total diagnostic score: 0/30 → 0.0/10
//!
//! NOTE: This is a genuine weakness. Structured prompt libraries (e.g.,
//! LangChain's PromptTemplate) catch this at construction time. Argentor
//! takes raw strings and trusts the developer to validate them.

use argentor_agent::{AgentRunner, ModelConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ModelConfig {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 256,
        ..Default::default()
    };
    let mut agent = AgentRunner::new(config)?;

    // BUG: unclosed template variable — {{name} missing closing brace
    // Argentor will send this string verbatim to the LLM without error.
    let malformed_system = "You are an assistant for {{name}. Answer their question.";
    let _r = agent.run(malformed_system, "Hello").await?;
    Ok(())
}
