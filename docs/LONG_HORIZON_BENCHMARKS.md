# Long-Horizon Agent Benchmarks (Phase 4 Track 6)

Argentor's claim for this track:

> Multi-step, stateful, tool-chaining tasks expose framework weaknesses that
> short-turn benchmarks hide. Frameworks that balloon context or lose state
> fail here where simple Q&A benchmarks would not notice.

This document covers methodology, results, where Argentor wins, and — honestly
— where it doesn't.

---

## Why long-horizon benchmarks matter

Short-turn benchmarks (Phase 1, Task track) measure framework overhead at a
single LLM call. Long-horizon tasks expose four orthogonal failure modes:

| Failure mode | Symptom | Short-turn benchmark catches? |
|---|---|---|
| Context bloat | Tokens grow quadratically; hits context limit or billing cap | No |
| Goal drift | Agent pivots off-task after turn 3 | No |
| Memory loss | Agent forgets turn-2 facts at turn 9 | No |
| Over-use of turns | Agent takes 12 turns for a 5-turn task | Partially |

The three task families in this benchmark target one or more of these failure
modes each.

---

## Task families

### Code repair (`lh_repair_*`) — 5 tasks

**What it measures**: can the agent iteratively read files, reason about a bug,
apply a fix, and verify with tests — across 5-6 turns — without losing context?

- `lh_repair_01_null_deref` — null-pointer dereference, 5 turns, 3 tools
- `lh_repair_02_off_by_one` — off-by-one in pagination loop, 5 turns, 3 tools
- `lh_repair_03_race_condition` — unsynchronized read in cache, 6 turns, 3 tools
- `lh_repair_04_type_mismatch` — API serialization type error, 5 turns, 3 tools
- `lh_repair_05_missing_error_handling` — bare except silences failures, 5 turns, 3 tools

**Key metric**: memory recall of 4-5 checkpoints (root-cause → fix → test) +
turns used.

### Multi-step research (`lh_research_*`) — 5 tasks

**What it measures**: does the agent follow a 6-8 step research chain without
drifting — and does it make the minimum number of tool calls the task requires?

- `lh_research_01_security_cve` — CVE impact assessment, 7 turns, 6 tools
- `lh_research_02_library_comparison` — Rust HTTP library selection, 6 turns, 5 tools
- `lh_research_03_performance_profiling` — N+1 query diagnosis, 7 turns, 6 tools
- `lh_research_04_api_integration` — Stripe payment API integration plan, 6 turns, 6 tools
- `lh_research_05_architecture_review` — microservice split risk assessment, 8 turns, 8 tools

**Key metric**: goal_drift_score (lower is better), min_tool_calls adherence,
memory recall of 6-7 checkpoints.

### Stateful conversation (`lh_state_*`) — 5 tasks

**What it measures**: can the agent remember facts stated in turn 2 at turn 9?
Does it answer correctly without asking the user to repeat themselves?

- `lh_state_01_user_preferences` — 10-turn editor preferences (dark mode, indent, timezone)
- `lh_state_02_project_context` — 12-turn Rust stack constraints (stable-only, Tokio, Axum)
- `lh_state_03_incremental_data` — 10-turn incremental sales data accumulation
- `lh_state_04_cross_domain_facts` — 11-turn security + deployment policy retention
- `lh_state_05_constraint_tracking` — 10-turn constraint graph with conflict detection

**Key metric**: memory_recall_rate across 6-9 checkpoints; the test relies on
the agent not asking users to repeat stated facts.

---

## Methodology

### Simulation model

All runners use the same deterministic simulation as the Phase 2b Cost track,
extended for long-horizon:

```text
Per-turn tokens = scaffold + tool_manifest + history + prompt_turn

Where:
  scaffold      = framework boilerplate (50 tok Argentor, 200 LangChain, etc.)
  tool_manifest = tool_count × 50 (full manifest); Argentor filters to top-5
  history       = cumulative prior turns; grows linearly/quadratically
  prompt_turn   = prompt.len() / 4 (current user turn)
```

Argentor with `intelligence=on` applies:
1. `tool_discovery`: filters N tools → top 5 per-turn
2. `context_compaction`: compresses running history to 30% once it exceeds 30K
   tokens (default `CompactionConfig` threshold)

### Framework memory behaviour (documented)

| Framework | Memory mechanism | Compaction | Notes |
|-----------|-----------------|------------|-------|
| Argentor (intelligence=on) | Session state + compaction | ✓ | Compacts at 30K tok trigger |
| LangChain | `ConversationBufferMemory` (default) | ✗ | Full history every turn; grows quadratically |
| CrewAI | `CrewMemory` (short-term + long-term + entity) | ✗ | Recall is good; but role/goal preamble adds 500 tok/turn |
| PydanticAI | None (application-managed) | ✗ | Framework does not provide memory; caller must thread history |
| Claude Agent SDK | System-provided session (messages array) | ✗ | Full conversation history sent each turn; 200K context window makes exhaustion rare |

### Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `turns_used` | Actual LLM turns consumed | count |
| `tokens_accumulated` | Cumulative prompt tokens across all turns | tokens |
| `tokens_at_turn_10` | Token count normalized to 10 turns (extrapolated if < 10, actual if ≥ 10) | tokens |
| `memory_recall_rate` | Fraction of `memory_checkpoints` present in final output | 0.0–1.0 |
| `goal_drift_score` | 10 × (1 − recall_rate) — 0 = on-task, 10 = off-task | 0–10 |
| `compaction_savings_pct` | Tokens saved vs naive quadratic baseline | % |

### Threats to validity

1. **Mock LLM**: the simulation uses canned per-checkpoint outputs; a real LLM
   might recall checkpoints that are not keyword-matched, or might miss them
   even with full history. The recall metric is a proxy, not ground truth.

2. **Goal drift heuristic**: we measure drift by checking whether checkpoint
   keywords appear in the output. This is coarse — a sophisticated agent could
   discuss irrelevant topics while still echoing checkpoint keywords.

3. **No human evaluation**: ground-truth answers (e.g. "the correct CVE
   remediation is X") are not scored against real subject-matter experts. The
   benchmark tests process fidelity (did the agent follow the steps?), not
   answer quality.

4. **Compaction threshold**: in real Argentor, the 30K token compaction trigger
   rarely fires in tasks of 5-12 turns with short prompts. The Phase 2b cost
   track showed this more clearly with the 50-turn task. Long-horizon tasks at
   5-12 turns are below the trigger for most task prompts.

5. **External runners not installed**: cross-framework comparison uses the same
   token accounting model (cost_sim) with framework-specific scaffold constants.
   It is NOT a live run of LangChain/CrewAI/PydanticAI/Claude SDK — the external
   Python runners share the identical simulator formula. This means the token
   comparison reflects the framework's documented overhead model, not a live API
   observation.

---

## Results (Argentor — samples=1)

### Per-task results (Argentor intelligence=on)

| Task | Turns | Tokens | Tok@T10 | Recall | Drift | Checkpoints |
|------|-------|--------|---------|--------|-------|-------------|
| `lh_repair_01_null_deref` | 5 | 2,775 | 5,550 | 100% | 0.0 | 4/4 |
| `lh_repair_02_off_by_one` | 5 | 2,685 | 5,370 | 100% | 0.0 | 4/4 |
| `lh_repair_03_race_condition` | 6 | 3,861 | 6,430 | 100% | 0.0 | 5/5 |
| `lh_repair_04_type_mismatch` | 5 | 2,580 | 5,160 | 100% | 0.0 | 4/4 |
| `lh_repair_05_missing_error_handling` | 5 | 2,640 | 5,280 | 100% | 0.0 | 5/5 |
| `lh_research_01_security_cve` | 7 | 5,362 | 7,660 | 100% | 0.0 | 6/6 |
| `lh_research_02_library_comparison` | 6 | 4,398 | 7,330 | 100% | 0.0 | 5/5 |
| `lh_research_03_performance_profiling` | 7 | 5,138 | 7,340 | 100% | 0.0 | 6/6 |
| `lh_research_04_api_integration` | 6 | 4,377 | 7,290 | 100% | 0.0 | 6/6 |
| `lh_research_05_architecture_review` | 8 | 7,616 | 9,520 | 100% | 0.0 | 7/7 |
| `lh_state_01_user_preferences` | 10 | 8,635 | 8,635 | 100% | 0.0 | 6/6 |
| `lh_state_02_project_context` | 12 | 8,034 | 8,034 | 100% | 0.0 | 7/7 |
| `lh_state_03_incremental_data` | 10 | 5,555 | 5,555 | 100% | 0.0 | 8/8 |
| `lh_state_04_cross_domain_facts` | 11 | 6,864 | 6,864 | 100% | 0.0 | 9/9 |
| `lh_state_05_constraint_tracking` | 10 | 5,390 | 5,390 | 100% | 0.0 | 8/8 |

**Argentor mean Tok@T10: 6,761 | Mean recall: 100% | All 15 tasks succeeded**

### By task family

| Family | Tok@T10 (mean) | Recall |
|--------|---------------|--------|
| Code repair (5 tasks) | 5,558 | 100% |
| Multi-step research (5 tasks) | 7,828 | 100% |
| Stateful conversation (5 tasks) | 6,896 | 100% |

### Cross-framework comparison (tokens at turn 10)

Token counts for competitors are computed using the identical cost_sim model
with their documented scaffold overhead constants. The framework-specific
constants are the same as Phase 2b:

| Framework | Scaffold tok/turn | Source |
|-----------|-----------------|--------|
| Argentor (intelligence=on) | 50 | Minimal system prompt + compaction |
| LangChain | 200 | AgentExecutor ReAct template |
| CrewAI | 500 | role/goal/backstory preamble per call |
| PydanticAI | 100 | Structured-output schema |
| Claude Agent SDK | 150 | Claude tool manifest envelope |

Tok@T10 comparison (mean across all 15 tasks):

| Framework | Tok@T10 (mean) | vs Argentor | Ratio |
|-----------|---------------|-------------|-------|
| **Argentor (intelligence=on)** | **6,761** | — | 1.00× |
| PydanticAI | 7,261 | +500 tok | 1.07× |
| Claude Agent SDK | 7,761 | +1,000 tok | 1.15× |
| LangChain | 8,261 | +1,500 tok | 1.22× |
| CrewAI | 11,261 | +4,500 tok | 1.67× |

Notes on the comparison:
- The difference at T10 is **purely scaffold overhead** (N × scaffold_tok/turn
  where N=10). History and tool-manifest savings are minimal at task lengths
  5-12 turns with short prompts.
- CrewAI's 1.67× premium is the largest gap; it comes entirely from the 500
  tok/turn role/goal/backstory preamble multiplied across 10 turns (4,500 tok).
- Tool discovery (Argentor's tool-filtering) saves additional tokens on research
  tasks (lh_research_*) where `tool_count` is 5-8. At 5 tools there is no gain;
  at 8 tools Argentor ships 5 instead of 8 (saves 150 tok/turn × 7 turns = 1,050
  tok per research task).
- Context compaction does NOT fire in any of the 15 tasks at observed token
  volumes. The 30K trigger would require ~300 turn exchanges with these prompt
  sizes. Compaction savings are therefore 0% in this benchmark.

---

## Where Argentor wins

**Scaffold overhead at scale**: Argentor's 50 tok/turn scaffold vs CrewAI's 500
tok/turn means a 10-turn task costs 4,500 fewer scaffold tokens — before any
intelligence features apply. At 100K tasks/day that is a measurable cost
difference (~$1.35/day at Claude Sonnet 4 pricing, $492/year for the scaffold
difference alone).

**Goal coherence (simulated)**: The simulation gives Argentor a 100% recall
rate because the native runner deterministically emits all checkpoint keywords.
A real LLM evaluation would be needed to verify this under adversarial
conditions (see Track 5, adversarial benchmarks).

**Tool discovery on research tasks**: On tasks with 6-8 tools, Argentor filters
to 5, saving 50-150 tok/turn vs naive frameworks. For the research family this
is ~1,050 additional tokens saved per task.

---

## Where Argentor loses or ties

**Memory depth**: Argentor's compaction is designed for very long sessions
(>30K tokens). For the 5-12 turn tasks in this benchmark, compaction NEVER
fires. LangChain's `ConversationBufferMemory`, PydanticAI's app-managed history,
and the Claude Agent SDK's system-managed session all provide the same in-context
recall at these session lengths. Argentor has no architectural advantage here
over a framework that simply passes the full conversation to each LLM call.

**Context compaction at 10 turns**: The "compaction savings" column shows -89.3%
for Argentor — meaning Argentor actually uses MORE tokens than the naive
quadratic baseline for these short tasks. This happens because the naive baseline
formula (50 scaffold × T + 100 × T(T-1)/2) underestimates the task's actual
prompt-token contribution. Compaction helps only when sessions are very long.

**Recall metric limitation**: The 100% recall rate for all frameworks reflects
the simulation echoing checkpoint keywords in output. A live LLM evaluation
would show real differences — frameworks without session persistence (PydanticAI)
would show lower recall on long-horizon stateful tasks if the application layer
doesn't correctly thread history. The simulation cannot capture this failure mode.

**Memory architecture**: CrewAI's `CrewMemory` (short-term + long-term + entity
memory via mem0) is a richer built-in memory system than Argentor's current
session store + compaction. For multi-agent crew workflows, CrewAI may outperform
Argentor on complex state across agents — at the cost of higher token overhead.

**No actual LLM runs**: all results in this track are from the deterministic
cost simulator. Actual LLM quality on these tasks (does the agent find the right
bug? does it recall turn-2 facts?) is not measured. The Phase 1 task-quality
track provides LLM-quality measurements on simpler tasks.

---

## How to reproduce

```bash
cd /path/to/Agentor

# Build the Rust harness
cargo build -p argentor-benchmarks --release

# Run long-horizon track (Argentor only — native runner, no Python required)
./target/release/bench \
  --tasks-dir ./benchmarks/tasks \
  long-horizon --runners argentor --samples 1

# Run all tests
cargo test -p argentor-benchmarks

# Run clippy
cargo clippy -p argentor-benchmarks --no-deps -- -D warnings
```

To run external Python runners (when installed):

```bash
pip install -e benchmarks/external/langchain_runner
pip install -e benchmarks/external/crewai_runner
pip install -e benchmarks/external/pydantic_ai_runner
pip install -e benchmarks/external/claude_agent_sdk_runner

export ARGENTOR_LC_RUNNER=$(which argentor-lc-runner)
export ARGENTOR_CREWAI_RUNNER=$(which argentor-crewai-runner)
export ARGENTOR_PYDANTIC_AI_RUNNER=$(which argentor-pydantic-ai-runner)
export ARGENTOR_CLAUDE_AGENT_SDK_RUNNER=$(which argentor-claude-agent-sdk-runner)

./target/release/bench \
  --tasks-dir ./benchmarks/tasks \
  long-horizon \
  --runners argentor,langchain,crewai,pydantic-ai,claude-agent-sdk \
  --samples 1
```

JSON output: `benchmarks/results/long_horizon_<timestamp>.json`

---

## Implementation

- **Task YAMLs**: `benchmarks/tasks/lh_repair_*/`, `lh_research_*/`, `lh_state_*/`
  (15 tasks, 3 families × 5 tasks each)
- **Metric module**: `benchmarks/src/metrics/long_horizon.rs` — `LongHorizonMetrics`,
  `LongHorizonSummary`, `compute()`, `aggregate()`
- **New `TaskKind` variant**: `LongHorizon` in `benchmarks/src/task.rs`
- **New task fields**: `required_turns`, `min_tool_calls`, `memory_checkpoints`
- **Argentor native runner**: `benchmarks/src/runners/argentor.rs` —
  `run_long_horizon()` method, compaction-aware token simulation
- **External runner updates**: all 4 Python runners updated with `long_horizon`
  task kind handling in `agent.py` and new fields in `models.py`
- **CLI subcommand**: `long-horizon` in `benchmarks/src/main.rs` — mirrors
  `security` and `cost` subcommand structure
