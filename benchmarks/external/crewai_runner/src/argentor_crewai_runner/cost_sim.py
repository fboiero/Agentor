"""Cost simulator — Python mirror of benchmarks/src/cost_sim.rs.

Deterministically computes how many prompt tokens each framework would send to
the LLM for a given Cost-track workload. Keeping this logic in sync with the
Rust module is what makes cross-framework comparisons apples-to-apples: all
runners use identical accounting for scaffolding, tool manifests, history
growth, and RAG payload handling.

Framework overhead constants (documented, LOWER-BOUND):
- LangChain: +200 tok system prompt (AgentExecutor ReAct template)
- CrewAI: +500 tok per call (role/goal/backstory preamble)
- Pydantic AI: +100 tok (structured output schema)
- Claude Agent SDK: +150 tok (Claude-style tool manifest envelope)
- Argentor base: +50 tok minimal system prompt
- Argentor intelligent: same base + tool_discovery filter + context_compaction

Numbers are LOWER-BOUND — real workloads with complex chains hit higher.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

# Typical per-tool manifest cost (name + JSON schema). 40-60 tok observed,
# use 50 as honest mid-point.
TOKENS_PER_TOOL = 50

# Argentor tool_discovery surfaces at most N tools per call (intelligent path).
ARGENTOR_DISCOVERY_MAX_TOOLS = 5

# Context compaction trigger (tokens) and target ratio.
COMPACTION_TRIGGER_TOKENS = 30_000
COMPACTION_TARGET_RATIO = 0.3

# Output tokens per turn (rough estimate for a "typical short answer").
OUTPUT_TOKENS_PER_TURN = 50


FRAMEWORK_SCAFFOLD_TOKENS = {
    "langchain": 200,
    "crewai": 500,
    "pydantic-ai": 100,
    "claude-agent-sdk": 150,
    "argentor-base": 50,
    "argentor-intelligent": 50,
}


Framework = Literal[
    "langchain", "crewai", "pydantic-ai", "claude-agent-sdk",
    "argentor-base", "argentor-intelligent",
]


@dataclass
class CostBreakdown:
    prompt_tokens_sent: int
    tool_description_tokens: int
    context_history_tokens: int
    scaffold_tokens: int
    user_turn_tokens: int
    rag_context_tokens: int
    output_tokens: int
    llm_calls: int


def chars_to_tokens(chars: int) -> int:
    return (chars + 3) // 4


def bytes_to_tokens(byt: int) -> int:
    return (byt + 3) // 4


def _compact_history_sum(pair_tok: int, turns: int) -> int:
    """Model Argentor context compaction summed across all turns.

    At each turn we emit the running history tokens; then append the turn's
    (user+assistant) pair. If running history crosses the trigger, compress
    to the target ratio. Returns the cumulative history tokens SENT across
    all turns.
    """
    if turns <= 1:
        return 0
    running = 0
    total = 0
    for _ in range(turns):
        total += running
        running += pair_tok
        if running > COMPACTION_TRIGGER_TOKENS:
            running = int(running * COMPACTION_TARGET_RATIO)
    return total


def simulate(
    framework: Framework,
    prompt: str,
    turns: int,
    tool_count: int,
    context_bytes: int,
) -> CostBreakdown:
    """Compute a breakdown of prompt tokens sent across all turns.

    Naïve frameworks (langchain, crewai, pydantic-ai, claude-agent-sdk,
    argentor-base) ship full tool manifest and full history every turn.
    argentor-intelligent filters tools (top-5) and compacts history past
    the trigger threshold.
    """
    prompt_tok = chars_to_tokens(len(prompt))
    rag_tok = bytes_to_tokens(context_bytes)
    scaffold_per_turn = FRAMEWORK_SCAFFOLD_TOKENS[framework]

    if framework == "argentor-intelligent":
        tools_per_turn = min(tool_count, ARGENTOR_DISCOVERY_MAX_TOOLS) * TOKENS_PER_TOOL
    else:
        tools_per_turn = tool_count * TOKENS_PER_TOOL
    tool_description_tokens = tools_per_turn * turns

    pair_tok = prompt_tok + OUTPUT_TOKENS_PER_TURN
    # Naïve: sum_{t=0..T-1} t * pair = pair * T*(T-1)/2
    naive_history_sum = pair_tok * (turns * (turns - 1)) // 2

    if framework == "argentor-intelligent":
        context_history_tokens = _compact_history_sum(pair_tok, turns)
    else:
        context_history_tokens = naive_history_sum

    if framework == "argentor-intelligent" and rag_tok > COMPACTION_TRIGGER_TOKENS:
        rag_context_tokens = int(rag_tok * COMPACTION_TARGET_RATIO) * turns
    else:
        rag_context_tokens = rag_tok * turns

    scaffold_tokens = scaffold_per_turn * turns
    user_turn_tokens = prompt_tok * turns

    prompt_tokens_sent = (
        scaffold_tokens
        + tool_description_tokens
        + context_history_tokens
        + rag_context_tokens
        + user_turn_tokens
    )

    return CostBreakdown(
        prompt_tokens_sent=prompt_tokens_sent,
        tool_description_tokens=tool_description_tokens,
        context_history_tokens=context_history_tokens,
        scaffold_tokens=scaffold_tokens,
        user_turn_tokens=user_turn_tokens,
        rag_context_tokens=rag_context_tokens,
        output_tokens=OUTPUT_TOKENS_PER_TURN * turns,
        llm_calls=turns,
    )
