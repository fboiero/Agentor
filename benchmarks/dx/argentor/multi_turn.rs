//! Argentor — agent with multi-turn conversation history.
//!
//! Net LOC: 28.
//! Argentor's AgentRunner maintains session state internally;
//! the caller just calls `run_turn` repeatedly on the same agent instance.

use argentor_agent::{AgentRunner, ModelConfig};
use argentor_session::InMemorySessionStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ModelConfig {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 512,
        ..Default::default()
    };
    let store = InMemorySessionStore::new();
    let mut agent = AgentRunner::with_session_store(config, store)?;

    let system = "You are a helpful coding assistant.";

    let r1 = agent.run_turn(system, "What is a closure in Rust?").await?;
    println!("Turn 1: {}", r1.content);

    let r2 = agent.run_turn(system, "Can you give me a code example?").await?;
    println!("Turn 2: {}", r2.content);

    let r3 = agent.run_turn(system, "How does that differ from Python closures?").await?;
    println!("Turn 3: {}", r3.content);

    Ok(())
}

// --- LOC count (net, no blanks/comments) ---
// imports: 2
// main fn block: 16
// TOTAL: 18 net LOC
