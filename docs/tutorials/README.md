# Argentor Tutorials

Step-by-step guides for building production-grade AI agents with Argentor. Each tutorial is ~300-500 lines of actionable instructions, real code, and expected output — no hand-waving.

Start at Tutorial 1 and work through them in order, or jump to the topic you need.

---

## The Path

### Foundations (1-2)

| # | Tutorial | What you will build |
|---|----------|---------------------|
| **1** | [**First Agent**](./01-first-agent.md) | A working `AgentRunner` in 5 minutes. Cargo project, `ModelConfig`, `SkillRegistry`, your first LLM conversation. |
| **2** | [**Using Skills**](./02-using-skills.md) | Wire up the calculator, file reader, and web search. Understand capabilities and permissions. |

### Scaling Up (3-5)

| # | Tutorial | What you will build |
|---|----------|---------------------|
| **3** | [**Multi-Agent Orchestration**](./03-multi-agent-orchestration.md) | A team of Spec / Coder / Tester / Reviewer agents using the Orchestrator-Workers pattern. |
| **4** | [**RAG Pipeline**](./04-rag-pipeline.md) | Ingest documents, embed, retrieve with hybrid search, inject into agent prompts. |
| **5** | [**Custom Skills**](./05-custom-skills.md) | Build your own tools with `ToolBuilder`, the full `Skill` trait, or sandboxed WASM plugins. |

### Production Ready (6-10)

| # | Tutorial | What you will build |
|---|----------|---------------------|
| **6** | [**Guardrails & Security**](./06-guardrails-security.md) | PII detection, prompt-injection blocking, output sanitization, custom rules, audit logging. |
| **7** | [**Agent Intelligence**](./07-agent-intelligence.md) | Extended thinking, self-critique, context compaction, dynamic tool discovery, checkpointing, learning. |
| **8** | [**MCP Integration**](./08-mcp-integration.md) | Connect to external MCP servers, expose your skills over MCP, run a multi-backend proxy with credential vault. |
| **9** | [**Production Deployment**](./09-deployment.md) | Docker, Kubernetes, Helm, health probes, Prometheus metrics, graceful shutdown. |
| **10** | [**Observability**](./10-observability.md) | OpenTelemetry traces, Mermaid gantts, correlation propagation, error aggregation, alert engine. |

---

## Prerequisites (for all tutorials)

- **Rust 1.80+** — install via [rustup](https://rustup.rs)
- **An LLM API key** — Anthropic / OpenAI / Gemini / any of 14 supported providers, OR use Ollama for local models
- **Docker** (for Tutorials 8, 9, 10)
- **A text editor** and basic familiarity with `cargo` (new, add, run)

No prior Argentor knowledge required — Tutorial 1 starts from an empty directory.

---

## Recommended Learning Paths

### Path A — "I want to build a single smart agent"

1 → 2 → 5 → 6 → 7

### Path B — "I want to build a team of agents"

1 → 2 → 3 → 5 → 6

### Path C — "I want RAG over my company docs"

1 → 2 → 4 → 6 → 9

### Path D — "I want to integrate with my existing MCP tools"

1 → 2 → 8 → 9 → 10

### Path E — "Just get me to production ASAP"

1 → 6 → 9 → 10

---

## What's Not Covered Here

These tutorials focus on **using** Argentor as a library. For deeper reference material:

- **[GETTING_STARTED.md](../GETTING_STARTED.md)** — the quick-start guide (SDKs, CLI, demos)
- **[DEPLOYMENT.md](../DEPLOYMENT.md)** — multi-region deployment, mTLS, PostgreSQL sessions
- **[TECHNICAL_REPORT.md](../TECHNICAL_REPORT.md)** — architecture deep dive
- **[COMPARISON.md](../COMPARISON.md)** — how Argentor compares to LangChain, CrewAI, AutoGen, etc.
- **[BENCHMARKS.md](../BENCHMARKS.md)** — performance numbers

For API-level reference, run `cargo doc --open --workspace` to browse the generated rustdoc.

---

## Contributing

Found a bug, unclear explanation, or missing tutorial? Open an issue or PR at [github.com/fboiero/Agentor](https://github.com/fboiero/Agentor).

When proposing a new tutorial, follow the format of the existing ten:

- Clear title and one-line description
- Prerequisites section
- Step-by-step instructions with runnable code
- Expected output snippets
- "Common Issues" troubleshooting
- "Next Steps" linking to related tutorials

---

**Now go build something.** Start with [Tutorial 1: First Agent](./01-first-agent.md).
