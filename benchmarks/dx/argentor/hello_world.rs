//! Argentor — minimal "Hello World" agent.
//!
//! Time-to-first-agent: 15 net LOC (excluding blank lines / comments).
//! Boilerplate categories: ModelConfig (1), AgentRunner::new (1), run (1).
//!
//! Compile: add argentor-agent to Cargo.toml, then `cargo run`.
//! NOTE: Requires ANTHROPIC_API_KEY in environment.

use argentor_agent::{AgentRunner, ModelConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ModelConfig {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 256,
        ..Default::default()
    };

    let mut agent = AgentRunner::new(config)?;

    let response = agent
        .run("You are a helpful assistant.", "Hello, what can you do?")
        .await?;

    println!("{}", response.content);
    Ok(())
}

// --- LOC count (net, no blanks/comments) ---
// use argentor_agent::{AgentRunner, ModelConfig};          1
// async fn main() -> anyhow::Result<()> {                  1
//     let config = ModelConfig {                            1
//         model: "claude-sonnet-4".to_string(),             1
//         max_tokens: 256,                                  1
//         ..Default::default()                              1
//     };                                                    1
//     let mut agent = AgentRunner::new(config)?;            1
//     let response = agent                                  1
//         .run("You are a helpful assistant.", "Hello...")  1
//         .await?;                                          1
//     println!("{}", response.content);                     1
//     Ok(())                                                1
// }                                                         1
// TOTAL: 14 net LOC
