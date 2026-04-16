//! Bug 2: Missing API key — ANTHROPIC_API_KEY not set.
//!
//! Expected error quality:
//! Argentor's ModelConfig validation runs at agent construction time:
//!   Error: missing environment variable ANTHROPIC_API_KEY
//!   Set it in your shell: export ANTHROPIC_API_KEY=sk-ant-...
//! Score: file/line — 0 (runtime check, not compile-time)
//!        names problem — 10 (exact env var name)
//!        suggests fix — 10 (exact export command)
//! Total diagnostic score: 20/30 → 6.7/10

use argentor_agent::{AgentRunner, ModelConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // BUG: ANTHROPIC_API_KEY is not set in this environment
    let config = ModelConfig {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 256,
        ..Default::default()
    };
    // AgentRunner::new validates env vars at construction
    let mut agent = AgentRunner::new(config)?;
    let _r = agent.run("You are helpful.", "Hello").await?;
    Ok(())
}
