# Tutorial 7: Agent Intelligence

> Extended thinking. Self-critique. Context compaction. Dynamic tool discovery. State checkpointing. Learning. Six features, one `.with_intelligence()` call.

A vanilla agent reacts to prompts. An **intelligent** agent plans before acting, reviews its own work, compresses context when it runs long, picks tools semantically, remembers prior runs, and can be rolled back to any checkpoint.

Argentor packs all of these behind opt-in builder methods on `AgentRunner`.

---

## Prerequisites

- Completed [Tutorial 1](./01-first-agent.md)
- Understanding of basic `AgentRunner` usage
- An API key — these features cost tokens

---

## 1. The One-Liner: `.with_intelligence()`

Turn everything on with default settings:

```rust
use argentor_agent::AgentRunner;

let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_intelligence();
```

Equivalent to chaining:

```rust
.with_default_thinking()
.with_default_critique()
.with_default_compaction()
.with_default_tool_discovery()
.with_default_checkpoint()
.with_default_learning()
```

That is it. The rest of this tutorial explains what each does, when to tune it, and when to turn it off.

---

## 2. Extended Thinking

Multi-pass reasoning before the agent takes any action. Think of it as scratch paper.

```rust
use argentor_agent::thinking::{ThinkingConfig, ThinkingDepth};

let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_thinking(ThinkingConfig {
        depth: ThinkingDepth::Deep,       // Quick | Standard | Deep | Exhaustive
        decompose_subtasks: true,
        confidence_threshold: 0.7,
        max_thinking_tokens: 4096,
    });
```

Depth levels:

| Depth | Passes | Token cost | Use for |
|-------|--------|-----------|---------|
| `Quick` | 1 pass | low | simple questions |
| `Standard` | 2-3 passes | medium | typical tasks (default) |
| `Deep` | 4-6 passes | high | architecture decisions, complex coding |
| `Exhaustive` | 8+ passes | very high | research, multi-faceted analysis |

After a run, inspect the reasoning:

```rust
if let Some(engine) = runner.thinking() {
    let last = engine.last_result().unwrap();
    println!("Confidence: {:.2}", last.confidence);
    println!("Subtasks: {:?}", last.decomposed_subtasks);
    println!("Recommended tools: {:?}", last.recommended_tools);
    if let Some(plan) = &last.plan {
        println!("Plan:\n{plan}");
    }
}
```

---

## 3. Self-Critique Loop (Reflexion)

After generating a response, the agent reviews it across 6 dimensions and revises if needed. Based on the Reflexion pattern.

```rust
use argentor_agent::critique::{CritiqueConfig, CritiqueDimension};

let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_critique(CritiqueConfig {
        dimensions: vec![
            CritiqueDimension::Correctness,
            CritiqueDimension::Completeness,
            CritiqueDimension::Clarity,
            CritiqueDimension::Safety,
            CritiqueDimension::Coherence,
            CritiqueDimension::Efficiency,
        ],
        min_acceptable_score: 0.75,
        max_revisions: 3,
    });
```

A critique report looks like:

```rust
CritiqueResult {
    dimensions: {
        Correctness: 0.92,
        Completeness: 0.65,   // below threshold
        Clarity: 0.88,
        Safety: 1.0,
        Coherence: 0.81,
        Efficiency: 0.75,
    },
    overall_score: 0.84,
    revised: true,
    revision_count: 1,
}
```

When a dimension scores below `min_acceptable_score`, the agent revises the response and critiques again. Caps at `max_revisions` to bound cost.

---

## 4. Automatic Context Compaction

Conversations grow. At some point you hit the model's context window. Compaction summarizes older turns while preserving recent ones.

```rust
use argentor_agent::compaction::{CompactionConfig, CompactionStrategy};

let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_compaction(CompactionConfig {
        trigger_tokens: 30_000,              // compact when context >= 30K tokens
        target_tokens: 15_000,               // aim for 15K after compaction
        strategy: CompactionStrategy::Hybrid, // Keyword | Summary | Sliding | Hybrid
        preserve_last_n_turns: 4,
        preserve_system_prompt: true,
    });
```

Strategies:

| Strategy | How it works |
|----------|--------------|
| `Keyword` | Drop low-relevance turns, keep keyword-dense ones |
| `Summary` | LLM-summarize old turns into a single system message |
| `Sliding` | Keep last N turns, drop the rest |
| `Hybrid` | Summarize old + keep last N (default, best quality) |

The agent will never fail with "context too large" — compaction kicks in automatically.

---

## 5. Dynamic Tool Discovery

With 50+ skills registered, sending them all to the LLM wastes tokens and confuses tool selection. Dynamic discovery uses TF-IDF + keyword hybrid search to pick the N most relevant tools per turn.

```rust
use argentor_agent::tool_discovery::{DiscoveryConfig, DiscoveryStrategy};

let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_tool_discovery(DiscoveryConfig {
        max_tools_per_turn: 8,
        strategy: DiscoveryStrategy::Hybrid, // Tfidf | Keyword | Hybrid
        min_relevance: 0.15,
        always_include: vec!["calculator".into()],
    });
```

Token savings (real measurement on a typical registry of 50 skills):

```
Without discovery: ~8_400 tokens per LLM call (tool catalog)
With discovery:    ~1_100 tokens per LLM call (~86% reduction)
```

Inspect what was selected:

```rust
if let Some(discovery) = runner.tool_discovery() {
    let last = discovery.last_result().unwrap();
    println!("Selected {}/{} tools ({:.0}% of available)",
        last.selected_tools.len(),
        last.total_available,
        (last.selected_tools.len() as f64 / last.total_available as f64) * 100.0,
    );
    println!("Token savings: ~{}", last.token_savings);
}
```

---

## 6. State Checkpointing (Time-Travel Debugging)

Save agent state at any turn, restore later, branch alternate timelines. Based on LangGraph's checkpointing.

```rust
use argentor_agent::checkpoint::CheckpointConfig;

let mut runner = AgentRunner::new(config, skills, permissions, audit)
    .with_checkpoint(CheckpointConfig {
        auto_checkpoint_every_n_turns: 5,
        max_checkpoints: 20,
        persist_to_disk: Some("./checkpoints".into()),
    });

// Run
let mut session = Session::new();
runner.run(&mut session, "Research X, then summarize.").await?;

// Create a checkpoint manually
if let Some(mgr) = runner.checkpoint_manager_mut() {
    let id = mgr.checkpoint(&session, "after-research").await?;
    println!("Saved checkpoint {id}");

    // ... later ...
    let restored_session = mgr.restore(id).await?;
    // Continue from this point
}
```

Use cases:

- **Rollback** — undo a bad turn
- **Branch** — try different follow-ups from the same base state
- **Replay** — reproduce exact agent behavior for debugging
- **A/B testing** — compare two model versions from the same starting state

---

## 7. Learning Feedback Loop

Track which tools work well for which kinds of tasks. Over time, the learning engine biases tool selection toward proven winners.

```rust
use argentor_agent::learning::LearningConfig;

let mut runner = AgentRunner::new(config, skills, permissions, audit)
    .with_learning(LearningConfig {
        persist_to_disk: Some("./learning-data.jsonl".into()),
        min_samples_for_recommendation: 10,
        success_weight: 1.0,
        latency_weight: 0.3,
    });

// Record outcomes after a run
if let Some(engine) = runner.learning_mut() {
    engine.record_feedback(
        "customer_lookup",
        true,   // success
        245,    // latency_ms
        None,   // error_kind
    );
}

// Query recommendations
if let Some(engine) = runner.learning() {
    let recs = engine.top_tools_for_task("customer support", 5);
    for rec in recs {
        println!("{} (score={:.2}, samples={})",
            rec.tool_name, rec.score, rec.sample_count);
    }
}
```

A weekly report shows trends:

```rust
if let Some(engine) = runner.learning() {
    let report = engine.generate_report(std::time::Duration::from_days(7));
    println!("{}", report.markdown_summary());
}
```

---

## 8. Combining with Debug Recorder

All intelligence features emit steps into the debug recorder. Enable to capture a full trace:

```rust
let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_debug_recorder("task-abc-123")
    .with_intelligence();

// After running:
let trace = runner.debug_recorder().finalize();
let json = serde_json::to_string_pretty(&trace)?;
std::fs::write("./trace.json", json)?;
```

The trace includes:

- Every thinking pass (with confidence, subtasks, plan)
- Every critique result (with dimension scores)
- Every compaction decision (tokens before / after)
- Every tool discovery (selected tools, token savings)
- Every tool call with timing
- Every checkpoint event

See [Tutorial 10: Observability](./10-observability.md) for visualization tools (TraceViz, Mermaid gantt, flame graph).

---

## 9. Cost-Aware Model Routing

Routing simple tasks to cheap models and complex ones to premium models is easy with `ModelRouter`:

```rust
use argentor_agent::{ModelRouter, ModelOption, ModelTier, RoutingStrategy, TaskComplexity};

let router = ModelRouter::new(RoutingStrategy::CostAware)
    .with_option(ModelOption {
        tier: ModelTier::Economy,
        provider: LlmProvider::Groq,
        model_id: "llama-3.3-70b-versatile".into(),
        cost_per_1k_input_tokens: 0.0001,
        cost_per_1k_output_tokens: 0.0002,
    })
    .with_option(ModelOption {
        tier: ModelTier::Premium,
        provider: LlmProvider::Claude,
        model_id: "claude-sonnet-4-20250514".into(),
        cost_per_1k_input_tokens: 0.003,
        cost_per_1k_output_tokens: 0.015,
    });

let decision = router.route("What is 2+2?", TaskComplexity::Trivial);
// → Economy tier (Groq llama)

let decision = router.route(
    "Design a distributed cache with consistency guarantees",
    TaskComplexity::Complex,
);
// → Premium tier (Claude Sonnet)
```

Wire the router into your agent by selecting the returned model before building `ModelConfig`.

---

## 10. Agent Handoffs

Pass control between specialized agents mid-conversation (OpenAI Agents SDK pattern):

```rust
// See argentor_orchestrator::handoff for the full API
```

Combined with a MessageBus ([Tutorial 3](./03-multi-agent-orchestration.md)), handoffs enable complex workflows like:

```
Triage → (simple) → FastAgent → reply
       → (complex) → ExpertAgent → reply
       → (regulated) → ComplianceAgent → SecurityReview → reply
```

---

## Common Issues

**Token usage explodes after `with_intelligence()`**
Extended thinking + critique can 3-4× your token bill. Dial back to `Quick`/`Standard` thinking, and set `max_revisions: 1` on critique. Or enable `tool_discovery` which typically saves more than thinking costs.

**Critique always fails on `Safety`**
Check the critique engine's prompt — it may flag anything discussing topics near its training-set boundaries. Drop `Safety` from the dimensions for internal use, keep it for customer-facing.

**Compaction drops important turns**
Increase `preserve_last_n_turns` or switch to `CompactionStrategy::Summary` which preserves semantic content at the cost of a summarization LLM call.

**Dynamic tool discovery misses an obvious tool**
Add it to `always_include`. Or lower `min_relevance` (the default 0.15 can be strict).

**Checkpoint restore produces different output**
LLMs are non-deterministic at temperature > 0. Set `temperature: 0.0` for reproducible replays.

**Learning engine never makes recommendations**
Default `min_samples_for_recommendation` is 10. For early stages, drop it to 3-5.

---

## What You Built

- An agent that thinks before acting
- An agent that critiques and revises its own responses
- An agent that compacts context automatically (no more context overflow)
- An agent that picks tools semantically (massive token savings)
- An agent you can checkpoint and rewind
- An agent that learns from outcomes

All behind one `.with_intelligence()` call, or tuned individually when you need precision.

---

## Next Steps

- **[Tutorial 3: Multi-Agent Orchestration](./03-multi-agent-orchestration.md)** — combine intelligent single agents into intelligent teams.
- **[Tutorial 10: Observability](./10-observability.md)** — visualize all this decision-making.
- **[Tutorial 9: Production Deployment](./09-deployment.md)** — ship it.
