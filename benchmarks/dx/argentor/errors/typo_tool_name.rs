//! Bug 1: Typo in tool name — "get_wether" instead of "get_weather".
//!
//! Expected error quality:
//! Argentor's SkillRegistry::get() returns a typed error:
//!   Error: skill not found: "get_wether"
//!   Available skills: ["get_weather", "echo", "time"]
//! Score: file/line — 0 (panic at runtime, not compile-time)
//!        names problem — 10 (exact name + suggestions list)
//!        suggests fix — 8 (available list implies correct spelling)
//! Total diagnostic score: 18/30 → 6.0/10

use argentor_agent::AgentRunner;
use argentor_skills::SkillRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let registry = SkillRegistry::new();
    // BUG: intentional typo — "get_wether" is not registered
    let _skill = registry.get("get_wether")?;  // <-- typo here
    Ok(())
}
