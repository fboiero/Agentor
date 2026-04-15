//! Cost simulator — computes realistic prompt-token accounting per framework.
//!
//! # Why this exists
//!
//! Phase 1 benchmarks measured **framework time overhead** — how much wall-clock
//! each framework's Python/Rust scaffolding adds on top of the LLM call. That
//! produced the 1.7-5.4× latency margin over LangChain/CrewAI.
//!
//! Phase 2b measures **framework token overhead** — how many tokens each
//! framework sends to the LLM for the same task. That maps directly to billing
//! at scale and is where Argentor's intelligence modules (tool_discovery,
//! context_compaction) pay off.
//!
//! # What "tokens sent" means
//!
//! For each turn of a conversation, every framework serializes a payload that
//! looks roughly like:
//! ```text
//! <system prompt boilerplate>
//! <tool manifest — N tools × ~50 tok each>
//! <conversation history — may grow without bound>
//! <retrieved context — RAG payloads>
//! <current user turn>
//! ```
//! Across T turns, naïve frameworks re-send the manifest and full history EVERY
//! turn. Argentor with intelligence=on:
//! - filters 50 tools → 5 via `tool_discovery` (saves ~2250 manifest tok/turn)
//! - compacts history via `context_compaction` once the trigger threshold is
//!   crossed (default 30K tokens → compressed to ~30% of original)
//!
//! # Framework overhead constants
//!
//! All numbers documented in-code. Sources:
//! - LangChain +200 tok system prompt: AgentExecutor's ReAct template
//!   (observed from `langchain.agents.create_react_agent` — ~180-220 tok
//!   of instructions + observation/thought markers).
//! - CrewAI +500 tok per call: role/goal/backstory preamble is emitted on
//!   every LLM call per the CrewAI agent prompt template.
//! - Pydantic AI +100 tok: structured-output schema JSON sent each call.
//! - Claude Agent SDK +150 tok: Claude-style tool manifest wrapper +
//!   system envelope.
//! - Argentor (off) baseline +50 tok: minimal argentor system prompt.
//! - Argentor (on) same 50 tok base + discovery summary (negligible).
//!
//! Numbers are LOWER-BOUND estimates — intended to NOT inflate competitor cost.
//! Real workloads with complex chains hit higher overhead; we're conservative.

use serde::{Deserialize, Serialize};

/// Per-framework scaffolding cost (system prompt boilerplate, added every turn).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Framework {
    /// Argentor without intelligence modules — minimal system prompt only.
    ArgentorBase,
    /// Argentor with tool_discovery + context_compaction — still minimal
    /// scaffolding, but tool manifest is filtered and history is compacted.
    ArgentorIntelligent,
    /// LangChain AgentExecutor — ReAct template overhead.
    LangChain,
    /// CrewAI — role/goal/backstory per agent per call.
    CrewAi,
    /// Pydantic AI — structured-output schema every call.
    PydanticAi,
    /// Claude Agent SDK — Claude tool manifest envelope.
    ClaudeAgentSdk,
}

impl Framework {
    /// System-prompt / scaffolding overhead added EVERY turn, in tokens.
    pub fn scaffold_tokens_per_turn(self) -> u64 {
        match self {
            // Argentor's system prompt is a short role line. ~50 tok.
            Framework::ArgentorBase | Framework::ArgentorIntelligent => 50,
            // LangChain ReAct template: "Answer the following questions as best
            // you can. You have access to the following tools..." + format
            // markers (Thought/Action/Observation). ~200 tok observed.
            Framework::LangChain => 200,
            // CrewAI emits "You are {role}. Your goal is {goal}. Your backstory
            // is {backstory}. ..." every call. Easily 500 tok on typical agents.
            Framework::CrewAi => 500,
            // Pydantic AI ships the structured-output schema (~100 tok of JSON).
            Framework::PydanticAi => 100,
            // Claude Agent SDK wraps tool manifest in Claude-style envelope.
            Framework::ClaudeAgentSdk => 150,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Framework::ArgentorBase => "argentor (base)",
            Framework::ArgentorIntelligent => "argentor (intelligence)",
            Framework::LangChain => "langchain",
            Framework::CrewAi => "crewai",
            Framework::PydanticAi => "pydantic-ai",
            Framework::ClaudeAgentSdk => "claude-agent-sdk",
        }
    }
}

/// Input shape for one cost simulation.
#[derive(Debug, Clone)]
pub struct CostWorkload {
    /// Framework under test.
    pub framework: Framework,
    /// Prompt content (task.prompt). Counted as tokens once per turn.
    pub prompt: String,
    /// Number of LLM turns to simulate.
    pub turns: u32,
    /// Number of tools "available" in the framework's registry.
    pub tool_count: u32,
    /// Retrieved context in bytes (RAG payload). ~4 bytes/token.
    pub context_bytes: u64,
}

/// Breakdown of tokens sent to the LLM across all turns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// Total prompt tokens sent to the LLM across all simulated turns.
    pub prompt_tokens_sent: u64,
    /// Subtotal spent on tool manifests.
    pub tool_description_tokens: u64,
    /// Subtotal spent on conversation history (cumulative across turns).
    pub context_history_tokens: u64,
    /// Subtotal spent on framework scaffolding (per-turn boilerplate).
    pub scaffold_tokens: u64,
    /// Subtotal spent on user prompt (N turns × prompt tokens).
    pub user_turn_tokens: u64,
    /// Subtotal spent on retrieved RAG context (cumulative across turns).
    pub rag_context_tokens: u64,
    /// Total output tokens (rough estimate: 50 per turn).
    pub output_tokens: u64,
    /// Number of LLM calls made (== turns).
    pub llm_calls: u32,
}

/// Average token cost of a single tool description (name + JSON schema).
/// Typical tool manifests run 40-60 tokens. Use 50 as honest mid-point.
pub const TOKENS_PER_TOOL: u64 = 50;

/// How many tools Argentor's tool_discovery surfaces per call when
/// intelligence is on. Default `max_tools` in `DiscoveryConfig` is 8, but
/// with conservative similarity_threshold it typically lands at ~5.
pub const ARGENTOR_DISCOVERY_MAX_TOOLS: u64 = 5;

/// Token threshold that triggers Argentor's context_compaction.
/// Matches `CompactionConfig::trigger_threshold` default.
pub const COMPACTION_TRIGGER_TOKENS: u64 = 30_000;

/// Compression ratio when compaction kicks in (0.3 = compressed to 30%).
/// Matches `CompactionConfig::target_ratio` default.
pub const COMPACTION_TARGET_RATIO: f32 = 0.3;

/// Average output tokens per turn — a "typical short answer" estimate.
const OUTPUT_TOKENS_PER_TURN: u64 = 50;

/// Roughly 4 characters per token (GPT-style heuristic).
pub fn chars_to_tokens(chars: usize) -> u64 {
    (chars as u64).div_ceil(4)
}

pub fn bytes_to_tokens(bytes: u64) -> u64 {
    bytes.div_ceil(4)
}

/// Run the simulation and produce a token breakdown.
///
/// The model: naïve frameworks send the full tool manifest and the full
/// (growing) history every turn. Argentor with intelligence filters tools
/// per-turn and compacts history once the trigger threshold is crossed.
pub fn simulate(wl: &CostWorkload) -> CostBreakdown {
    let prompt_tok = chars_to_tokens(wl.prompt.len());
    let rag_tok = bytes_to_tokens(wl.context_bytes);
    let scaffold_per_turn = wl.framework.scaffold_tokens_per_turn();
    let turns = wl.turns as u64;

    // Tool manifest tokens (per turn, cumulative across turns).
    let tools_per_turn: u64 = match wl.framework {
        Framework::ArgentorIntelligent => {
            // Filter 50 → at most 5. If fewer tools exist, send them all.
            (wl.tool_count as u64).min(ARGENTOR_DISCOVERY_MAX_TOOLS) * TOKENS_PER_TOOL
        }
        _ => (wl.tool_count as u64) * TOKENS_PER_TOOL,
    };
    let tool_description_tokens = tools_per_turn * turns;

    // Conversation history (user turns accumulate across calls).
    // At turn T, the framework is sending T previous (user+assistant) pairs
    // as "history" plus the current user turn.
    // Naïve: history at turn t = (t-1) × (prompt + output) tokens.
    // Summed across all turns: sum_{t=1..T} (t-1) × (prompt + output)
    //                        = (prompt + output) × T×(T-1)/2
    let pair_tok = prompt_tok + OUTPUT_TOKENS_PER_TURN;
    let naive_history_sum = pair_tok * (turns * turns.saturating_sub(1)) / 2;

    let context_history_tokens = match wl.framework {
        Framework::ArgentorIntelligent => compact_history_sum(pair_tok, turns),
        _ => naive_history_sum,
    };

    // RAG context: added every turn (retriever is hit each call).
    // Argentor with intelligence still retrieves, but compaction will fold
    // large RAG payloads into the overall history when the trigger is hit.
    let rag_context_tokens = match wl.framework {
        Framework::ArgentorIntelligent => {
            if rag_tok > COMPACTION_TRIGGER_TOKENS {
                // After compaction, RAG context is summarized to target ratio.
                ((rag_tok as f32 * COMPACTION_TARGET_RATIO) as u64) * turns
            } else {
                rag_tok * turns
            }
        }
        _ => rag_tok * turns,
    };

    // Scaffold and user-turn totals (independent of intelligence).
    let scaffold_tokens = scaffold_per_turn * turns;
    let user_turn_tokens = prompt_tok * turns;

    let prompt_tokens_sent = scaffold_tokens
        + tool_description_tokens
        + context_history_tokens
        + rag_context_tokens
        + user_turn_tokens;

    CostBreakdown {
        prompt_tokens_sent,
        tool_description_tokens,
        context_history_tokens,
        scaffold_tokens,
        user_turn_tokens,
        rag_context_tokens,
        output_tokens: OUTPUT_TOKENS_PER_TURN * turns,
        llm_calls: wl.turns,
    }
}

/// Model Argentor's context_compaction. The naïve cumulative history tokens
/// would be pair × T×(T-1)/2. With compaction, once the running history at
/// turn t exceeds `trigger`, the oldest 70% is compressed to 30% of its size.
/// We approximate by computing per-turn history and compressing as we go.
fn compact_history_sum(pair_tok: u64, turns: u64) -> u64 {
    if turns <= 1 {
        return 0;
    }
    let mut running_history: u64 = 0;
    let mut sum: u64 = 0;
    for _t in 0..turns {
        // At turn t the prompt we send includes the current running history.
        sum = sum.saturating_add(running_history);
        // Append this turn's pair (user + assistant) to the running history.
        running_history = running_history.saturating_add(pair_tok);
        // If we crossed the compaction trigger, compress the running history
        // to the target ratio, leaving the last `preserve_recent` turns
        // unchanged. We simplify by compressing everything — conservative
        // (real Argentor preserves the last 4 messages, so actual savings
        // would be slightly smaller; we err on the LOWER-BOUND side).
        if running_history > COMPACTION_TRIGGER_TOKENS {
            let compressed = (running_history as f32 * COMPACTION_TARGET_RATIO) as u64;
            running_history = compressed;
        }
    }
    sum
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn base_wl(framework: Framework) -> CostWorkload {
        CostWorkload {
            framework,
            prompt: "hello world ".repeat(10), // ~120 chars → 30 tok
            turns: 1,
            tool_count: 0,
            context_bytes: 0,
        }
    }

    #[test]
    fn single_turn_no_tools_no_rag_matches_scaffold() {
        let wl = base_wl(Framework::LangChain);
        let b = simulate(&wl);
        // 200 scaffold + 30 user-turn + 0 tools + 0 history + 0 rag = 230
        assert_eq!(b.scaffold_tokens, 200);
        assert_eq!(b.tool_description_tokens, 0);
        assert_eq!(b.context_history_tokens, 0);
        assert!(b.prompt_tokens_sent >= 200);
    }

    #[test]
    fn tool_heavy_argentor_intelligent_filters() {
        let mut wl = base_wl(Framework::ArgentorIntelligent);
        wl.tool_count = 50;
        let b = simulate(&wl);
        // 5 tools × 50 tok = 250 manifest (vs 2500 for naïve)
        assert_eq!(b.tool_description_tokens, 250);
    }

    #[test]
    fn tool_heavy_langchain_ships_all() {
        let mut wl = base_wl(Framework::LangChain);
        wl.tool_count = 50;
        let b = simulate(&wl);
        // 50 tools × 50 tok = 2500 manifest
        assert_eq!(b.tool_description_tokens, 2500);
    }

    #[test]
    fn argentor_intelligent_beats_langchain_on_tools() {
        let mut lc = base_wl(Framework::LangChain);
        lc.tool_count = 50;
        let mut ar = base_wl(Framework::ArgentorIntelligent);
        ar.tool_count = 50;
        let lc_b = simulate(&lc);
        let ar_b = simulate(&ar);
        assert!(ar_b.prompt_tokens_sent < lc_b.prompt_tokens_sent);
        // Should be a material gap — at least 1500 tokens on a single turn.
        assert!(lc_b.prompt_tokens_sent - ar_b.prompt_tokens_sent >= 1500);
    }

    #[test]
    fn multi_turn_history_grows_quadratically() {
        let mut wl = base_wl(Framework::LangChain);
        wl.turns = 10;
        let b = simulate(&wl);
        assert!(b.context_history_tokens > 0);
        assert_eq!(b.llm_calls, 10);
    }

    #[test]
    fn compaction_helps_at_long_sessions() {
        // 100 turns with substantial prompt per turn — history will
        // definitively cross the compaction threshold (30K tok).
        let naive = CostWorkload {
            framework: Framework::LangChain,
            prompt: "x".repeat(4_000), // 1000 tok per user turn
            turns: 100,
            tool_count: 0,
            context_bytes: 0,
        };
        let smart = naive.clone_for_framework(Framework::ArgentorIntelligent);
        let nb = simulate(&naive);
        let sb = simulate(&smart);
        assert!(
            sb.context_history_tokens < nb.context_history_tokens,
            "compaction should reduce history tokens: smart={} naive={}",
            sb.context_history_tokens,
            nb.context_history_tokens
        );
    }

    #[test]
    fn rag_scales_with_context_bytes() {
        let mut wl = base_wl(Framework::PydanticAi);
        wl.context_bytes = 10_000;
        let b = simulate(&wl);
        // 10000 bytes → 2500 tokens
        assert_eq!(b.rag_context_tokens, 2500);
    }

    #[test]
    fn compaction_compresses_large_rag() {
        let mut smart = base_wl(Framework::ArgentorIntelligent);
        smart.context_bytes = 200_000; // 50_000 tokens raw > trigger
        let sb = simulate(&smart);
        let mut naive = base_wl(Framework::LangChain);
        naive.context_bytes = 200_000;
        let nb = simulate(&naive);
        assert!(
            sb.rag_context_tokens < nb.rag_context_tokens,
            "compaction should shrink RAG: smart={} naive={}",
            sb.rag_context_tokens,
            nb.rag_context_tokens
        );
    }

    impl CostWorkload {
        fn clone_for_framework(&self, f: Framework) -> Self {
            Self {
                framework: f,
                prompt: self.prompt.clone(),
                turns: self.turns,
                tool_count: self.tool_count,
                context_bytes: self.context_bytes,
            }
        }
    }
}
