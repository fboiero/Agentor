# Tutorial 3: Multi-Agent Orchestration

> Build a team of specialized agents that plan, code, test, and review — using Anthropic's Orchestrator-Workers pattern.

One generalist agent hits context limits, mixes concerns, and is hard to audit. A **team** of focused workers, coordinated by an orchestrator, scales much better. Argentor ships `argentor-orchestrator` for exactly this.

---

## Prerequisites

- Completed [Tutorial 1](./01-first-agent.md) and [Tutorial 2](./02-using-skills.md)
- An API key (Claude or OpenAI) — the orchestrator consumes more tokens than a single agent
- Basic understanding of DAGs (directed acyclic graphs)

---

## 1. The Orchestrator-Workers Pattern

The pattern (recommended by [Anthropic](https://docs.anthropic.com/en/docs/build-with-claude/agentic-systems)) decomposes work into three phases:

```
┌──────────────────┐
│   Orchestrator   │  Phase 1: Plan
│  decomposes task │  → builds DAG of subtasks
└────────┬─────────┘
         │
┌────────┼─────────────────────────────┐
│        │                             │  Phase 2: Execute
│  ┌─────▼────┐ ┌──────────┐ ┌───────▼─────┐
│  │   Spec   │ │  Coder   │ │   Tester    │  workers run in parallel
│  │  Worker  │ │  Worker  │ │   Worker    │  respecting dependencies
│  └──────────┘ └──────────┘ └─────────────┘
│        │          │              │          
└────────┼──────────┴──────────────┘
         │
┌────────▼─────────┐
│   Orchestrator   │  Phase 3: Synthesize
│ collects results │  → merges artifacts into final output
└──────────────────┘
```

Each worker runs in its own `AgentRunner` with an isolated context window and a focused system prompt. The orchestrator stitches artifacts together.

---

## 2. Agent Roles

Argentor defines 10 built-in roles in `argentor_orchestrator::AgentRole`:

| Role | Responsibility |
|------|----------------|
| `Orchestrator` | Decompose → dispatch → synthesize |
| `Spec` | Turn user requirements into an explicit specification |
| `Architect` | System design, interface boundaries |
| `Coder` | Write code that implements the spec |
| `Tester` | Write and run tests |
| `Reviewer` | Code review (correctness, style, security) |
| `SecurityAuditor` | Vulnerability analysis |
| `DevOps` | Dockerfiles, CI, deployment |
| `DocumentWriter` | README, API docs, runbooks |
| `Custom(String)` | Your own named role |

Each role has a default profile via `argentor_orchestrator::profiles::default_profiles(&base_config)`.

---

## 3. A Minimal Development Team

Here is the smallest-possible multi-agent pipeline:

```rust
use argentor_agent::{LlmProvider, ModelConfig};
use argentor_builtins::register_builtins;
use argentor_orchestrator::Orchestrator;
use argentor_security::{AuditLog, Capability, PermissionSet};
use argentor_skills::SkillRegistry;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    // 1. Base config — workers inherit defaults from here.
    let base_config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "claude-sonnet-4-20250514".into(),
        api_key: std::env::var("ANTHROPIC_API_KEY")?,
        api_base_url: None,
        temperature: 0.5,
        max_tokens: 4096,
        max_turns: 10,
        fallback_models: vec![],
        retry_policy: None,
    };

    // 2. Shared skill registry.
    let mut registry = SkillRegistry::new();
    register_builtins(&mut registry);
    let skills = Arc::new(registry);

    // 3. Permissions: workers need shell for tests, file I/O for code.
    let mut permissions = PermissionSet::new();
    permissions.grant(Capability::FileRead { allowed_paths: vec![] });
    permissions.grant(Capability::FileWrite { allowed_paths: vec!["/tmp".into()] });
    permissions.grant(Capability::ShellExec { allowed_commands: vec![] });

    // 4. Audit log — shared across all workers for end-to-end traceability.
    let audit = Arc::new(AuditLog::new(PathBuf::from("./audit-logs")));

    // 5. Build orchestrator with DEFAULT profiles
    //    (spec, architect, coder, tester, reviewer all pre-configured).
    let orchestrator = Orchestrator::new(&base_config, skills, permissions, audit)
        .with_output_dir(PathBuf::from("./team-output"));

    // 6. Run
    let result = orchestrator
        .run("Build a Rust function that validates an email address with a regex and returns an enum (Valid/Invalid(reason)). Include unit tests.")
        .await?;

    println!("\n=== Orchestrator Result ===");
    println!("Artifacts produced: {}", result.artifacts.len());
    for artifact in &result.artifacts {
        println!(" - [{:?}] {} ({} bytes)",
            artifact.kind, artifact.name, artifact.content.len());
    }
    println!("\nFinal summary:\n{}", result.summary);

    Ok(())
}
```

### What happens when you run it

```
 INFO argentor_orchestrator::engine: Orchestrator: starting pipeline
 INFO argentor_orchestrator::engine: Orchestrator Phase 1: Planning
 INFO argentor_orchestrator::engine: Orchestrator: plan complete subtask_count=4
 INFO argentor_orchestrator::engine: Phase 2: Executing 4 subtasks
 INFO worker{role=Spec}: writing specification
 INFO worker{role=Coder}: implementing spec
 INFO worker{role=Tester}: generating tests
 INFO worker{role=Reviewer}: reviewing code
 INFO argentor_orchestrator::engine: Phase 3: Synthesizing

=== Orchestrator Result ===
Artifacts produced: 4
 - [Spec] requirements.md (812 bytes)
 - [Code] email_validator.rs (1_203 bytes)
 - [Test] email_validator_tests.rs (1_456 bytes)
 - [Review] review.md (624 bytes)
```

`./team-output/` will contain the real files on disk.

---

## 4. Custom Profiles (Your Own Team)

If the defaults are too heavy or you want a specialized team, build profiles explicitly:

```rust
use argentor_orchestrator::types::{AgentProfile, AgentRole};

let spec_profile = AgentProfile {
    role: AgentRole::Spec,
    model: base_config.clone(),
    system_prompt: "You are a lean spec writer. Output only bullet-point \
                    requirements and acceptance criteria — no prose.".into(),
    allowed_skills: vec!["file_write".into()],
    tool_group: None,
    max_turns: 3,
    permissions: PermissionSet::default(),
};

let coder_profile = AgentProfile {
    role: AgentRole::Coder,
    model: base_config.clone(),
    system_prompt: "Write idiomatic Rust. Prefer `?` over `unwrap`. \
                    All public items get doc comments.".into(),
    allowed_skills: vec!["file_write".into(), "file_read".into(), "code_analysis".into()],
    tool_group: None,
    max_turns: 8,
    permissions: PermissionSet::default(),
};

let tester_profile = AgentProfile {
    role: AgentRole::Tester,
    model: base_config.clone(),
    system_prompt: "Write cargo tests. Run `cargo test`. Report failures \
                    with actionable diagnostics.".into(),
    allowed_skills: vec!["file_write".into(), "file_read".into(), "shell".into(), "test_runner".into()],
    tool_group: None,
    max_turns: 6,
    permissions: PermissionSet::default(),
};

let orchestrator = Orchestrator::with_profiles(
    vec![spec_profile, coder_profile, tester_profile],
    skills,
    permissions,
    audit,
);
```

---

## 5. Inter-Agent Communication: `MessageBus`

Agents can send messages to each other instead of round-tripping through artifacts. `MessageBus` lives in `argentor_orchestrator::message_bus`:

```rust
use argentor_orchestrator::message_bus::{
    AgentMessage, BroadcastTarget, MessageBus, MessageType,
};
use argentor_orchestrator::types::AgentRole;

let bus = MessageBus::new();

// Orchestrator asks Coder to implement a module.
let msg = AgentMessage::new(
    AgentRole::Orchestrator,
    BroadcastTarget::Direct(AgentRole::Coder),
    "Implement the auth module as described in spec.md".to_string(),
    MessageType::Query,
);
bus.send(msg).await;

// Coder polls its inbox.
let messages = bus.receive(&AgentRole::Coder).await;
for m in &messages {
    println!("Coder got from {:?}: {}", m.from, m.content);
}
```

Broadcast to everyone:

```rust
let notice = AgentMessage::new(
    AgentRole::Orchestrator,
    BroadcastTarget::Broadcast,
    "Spec updated — please reload".to_string(),
    MessageType::StatusUpdate,
);
bus.send(notice).await;
```

---

## 6. Progress Callbacks

Wire a callback to stream progress to stdout or a UI:

```rust
let orchestrator = Orchestrator::new(&base_config, skills, permissions, audit)
    .with_progress(|role, msg| {
        println!("[{role:?}] {msg}");
    });
```

You will see lines like `[Coder] generating module scaffold`, `[Tester] 12 passed, 0 failed`.

---

## 7. Collaboration Patterns

Beyond the default sequential DAG, Argentor supports six patterns in `argentor_orchestrator::patterns`:

| Pattern | Use case |
|---------|----------|
| **Pipeline** | Sequential chain: A → B → C |
| **MapReduce** | Fan out to N workers, aggregate results |
| **Debate** | Multiple agents argue; best answer wins |
| **Ensemble** | All agents answer, results merged/voted |
| **Supervisor** | A supervisor watches workers and corrects them |
| **Swarm** | Workers self-organize via shared goal + bus |

Example — `MapReduce` to summarize 10 PDFs in parallel:

```rust
use argentor_orchestrator::patterns::{MapReducePattern, MapReduceConfig};

let map_config = MapReduceConfig {
    mapper_role: AgentRole::Custom("Summarizer".into()),
    reducer_role: AgentRole::Custom("Aggregator".into()),
    parallelism: 5,
};

let pattern = MapReducePattern::new(map_config);
let result = pattern.run(&orchestrator, pdf_paths).await?;
```

---

## 8. Dynamic Replanning

When a subtask fails, the orchestrator can replan with six recovery strategies:

- `Retry` — run the same task again
- `Reassign` — move the task to a different worker role
- `Decompose` — break the task into smaller subtasks
- `Skip` — drop it and continue
- `Abort` — halt the pipeline
- `Escalate` — pause for human approval

Configure via `argentor_orchestrator::replanner::ReplannerConfig`.

---

## 9. Token Budget Tracking

Every worker's LLM calls are metered. Query the monitor:

```rust
let monitor = orchestrator.monitor();
let stats = monitor.all_stats().await;
for (role, s) in stats {
    println!("{role:?}: {} turns, {} tokens, ${:.4}",
        s.turns, s.total_tokens, s.estimated_cost_usd);
}
```

Typical output after a 4-worker run:

```
Orchestrator: 2 turns, 3_412 tokens, $0.0051
Spec:         3 turns, 5_118 tokens, $0.0077
Coder:        6 turns, 12_849 tokens, $0.0193
Tester:       4 turns, 7_962 tokens, $0.0119
Reviewer:     2 turns, 4_301 tokens, $0.0064
```

---

## Common Issues

**"Dependency cycle detected in task graph"**
Your custom profiles or planner produced a cycle. Inspect `orchestrator.queue().read().await` and dump the DAG.

**All workers produce the same generic answer**
Your system prompts are too similar. Give each role a *distinct* persona and set of constraints.

**Token explosion**
The orchestrator duplicates context across workers. Turn on **progressive tool disclosure** via the MCP proxy (which is already wired by default) and set `max_turns` per role conservatively (`3`-`6` is usually plenty).

**One worker crashes the whole pipeline**
Configure the replanner. By default the orchestrator retries once; switch to `Decompose` or `Reassign` for production.

**Artifacts are not written to disk**
You forgot `.with_output_dir(...)`. Without it, artifacts only live in memory on the `OrchestratorResult`.

---

## What You Built

- A working Orchestrator-Workers pipeline producing code + tests + review
- Custom role profiles tailored to your team's style
- Inter-agent messaging via `MessageBus`
- Per-agent token accounting via `AgentMonitor`

---

## Next Steps

- **[Tutorial 4: Building a RAG Pipeline](./04-rag-pipeline.md)** — give workers a shared knowledge base.
- **[Tutorial 8: MCP Integration](./08-mcp-integration.md)** — let external tools talk to your agents over the MCP protocol.
- **[Tutorial 9: Production Deployment](./09-deployment.md)** — containerize and deploy your team.
