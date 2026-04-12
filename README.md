# Argentor

**Secure multi-agent AI framework in Rust with WASM sandboxed plugins, MCP integration, and compliance modules.**

[![CI](https://github.com/fboiero/Argentor/actions/workflows/ci.yml/badge.svg)](https://github.com/fboiero/Argentor/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0--only-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/Tests-4498%20passing-brightgreen.svg)]()
[![LOC](https://img.shields.io/badge/LOC-187K%2B-informational.svg)]()
[![Crates](https://img.shields.io/badge/Crates-15-informational.svg)]()
[![PyPI](https://img.shields.io/badge/PyPI-argentor--sdk-blue.svg)](https://pypi.org/project/argentor-sdk/)
[![npm](https://img.shields.io/badge/npm-%40argentor%2Fsdk-red.svg)](https://www.npmjs.com/package/@argentor/sdk)

---

## Demo

> An Argentor agent running an automated 8-step DevOps pipeline — real tool execution, no API keys, no mocks.

<p align="center">
  <img src="docs/demo_pipeline.gif" alt="Argentor Pipeline Demo" width="700">
</p>

<details>
<summary><b>Run it yourself</b></summary>

```bash
cargo run -p argentor-cli --example demo_pipeline
```

The agent executes 8 real tools: `shell` (git stats, LOC, annotations, security scan), `file_read` (Cargo.toml), `memory_store` (vector embeddings), `memory_search` (cosine similarity), and `file_write` (Markdown report). All with permission checks and audit logging.

</details>

---

## What is Argentor?

Argentor is an autonomous AI agent framework designed for **security**, **compliance**, and **multi-agent orchestration**. Built from scratch in Rust, it addresses critical vulnerabilities found in existing frameworks (RCE, sandbox escape, SSRF, path traversal) while providing a complete multi-agent system following [Anthropic's recommended patterns](https://docs.anthropic.com/en/docs/build-with-claude/agentic-systems).

Unlike frameworks that bolt on security as an afterthought, Argentor makes it foundational: every skill runs in a WASM sandbox with capability-based permissions, every tool call is audit-logged, and every agent operates within a strict permission boundary. The result is a framework you can trust with production workloads where security and regulatory compliance matter.

Argentor also provides a complete platform for building multi-agent systems — from code generation pipelines to DevOps automation — with built-in observability, token budget tracking, and support for 14 LLM providers. Whether you are deploying a single agent or orchestrating a team of specialized workers, Argentor handles the complexity while keeping everything auditable and secure.

---

## Key Features

### Core Runtime
- **Rust 1.75+** with strict clippy lints (`unwrap_used`, `expect_used`, etc.) — zero errors, warnings in CI
- **Multi-provider LLM support** — Claude, OpenAI, Gemini, OpenRouter, Groq, Ollama, Mistral, XAi, Azure OpenAI, Cerebras, Together, DeepSeek, and more (14 providers)
- **Automatic failover** across LLM backends with `RetryPolicy` (exponential backoff, error classification)
- **Circuit breaker** per LLM provider (Closed→Open→HalfOpen state machine) — integrated into AgentRunner
- **LLM response cache** — In-memory LRU with TTL expiration, hit/miss metrics, token savings tracking
- **Streaming responses** (SSE) with `StreamEvent` types
- **Config hot-reload** via file watcher (`notify` crate, 500ms debounce)
- **Token estimation** per provider with cost calculation
- **Batch processor** for grouping multiple LLM requests with priority queuing

### Security (Key Differentiator)
- **WASM sandboxed plugins** via wasmtime + WASI
- **Capability-based permissions** (FileRead, FileWrite, ShellExec, NetworkAccess, etc.)
- **SSRF prevention** — blocks localhost, link-local, and private network ranges
- **Path traversal protection** — canonicalization + blocklist
- **Shell injection blocking** — detects `rm -rf`, fork bombs, and other dangerous patterns
- **Input sanitizer** — strips control characters, prevents log poisoning
- **Rate limiting** — token bucket algorithm
- **TLS/mTLS support**
- **JWT authentication** (HMAC-SHA256) with API key hashing
- **OAuth2 provider configuration** (GitHub, Google, custom)
- **SSO/SAML authentication** for enterprise identity providers
- **Encrypted credential store** (AES-256-GCM with PBKDF2 key derivation)
- **RBAC policy engine** — Admin, Operator, Viewer, Custom roles with per-role permissions
- **Audit logging** — append-only JSONL with structured querying and statistics

### Multi-Agent Orchestration
- **Orchestrator-Workers pattern** (Anthropic recommended)
- **10 agent roles** — Orchestrator, Spec, Coder, Tester, Reviewer, Architect, SecurityAuditor, DevOps, DocumentWriter, Custom
- **TaskQueue with DAG resolution** — topological sort with cycle detection
- **Inter-agent messaging** — MessageBus with send, receive, and broadcast
- **Dynamic replanning** with 6 recovery strategies: Retry, Reassign, Decompose, Skip, Abort, Escalate
- **Token budget tracking** per agent with cost estimation
- **AgentMonitor** with real-time metrics (turns, tool calls, tokens, errors)
- **Progressive tool disclosure** — ~98% token reduction
- **6 collaboration patterns** — Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm
- **Sub-agent spawning** with configurable depth limits

### Code Generation and DevOps Skills
- **API Scaffold Generator** — generates complete projects (Rust/Axum, Python/FastAPI, Node/Express) with routes, models, Dockerfile, and tests
- **IaC Generator** — Docker multi-stage builds, docker-compose, Helm charts, Terraform (AWS/GCP), GitHub Actions CI/CD
- **Code Analysis skill** — language-aware AST analysis
- **Test Runner skill** — multi-language test execution with result parsing
- **Git operations skill** — libgit2-based, no shell commands

### Universal Skill Toolkit
- **50+ built-in skills** — calculator, CSV/YAML/JSON tools, regex, UUID/hash generators, crypto, web search, security scanning, template engine, JWT tools, color converter, semver tools, cron parser, and more
- **Multi-provider web search** — DuckDuckGo, Tavily, Brave, SearXNG with unified interface
- **Plugin marketplace** — skill publishing, discovery, dependency resolution, and version management

### Guardrails Pipeline
- **PII detection** — credit card (Luhn), SSN, email, phone number detection with redaction
- **Prompt injection blocking** — 23+ pattern signatures
- **Toxicity filter** — content policy enforcement on input and output
- **Guardrails integrated into agent pipeline** — pre/post execution filtering

### Gateway and API
- **HTTP/WebSocket gateway** (axum-based)
- **REST API** — 40+ endpoints (10 core + 17 control plane + 13 proxy management)
- **Control Plane API** — 17 endpoints for deployment management, agent registry, and health monitoring
- **Web Dashboard** — dark-themed SPA at `/dashboard` with deployment management, agent catalog, health monitoring
- **OpenAPI 3.0 spec** — auto-generated at `/openapi.json`
- **Prometheus-compatible `/metrics` endpoint** for observability
- **OpenTelemetry observability** — OTLP export, distributed tracing with `#[tracing::instrument]`
- **Per-API-key rate limiting** with tenant-aware enforcement (Free/Pro/Enterprise tiers)
- **Rate limit headers** — X-RateLimit-*, IETF draft RateLimit, Retry-After
- **Webhook support** — inbound/outbound with HMAC-SHA256 validation
- **Channel bridge** — Slack, Discord, Telegram, and Webchat adapters
- **WebSocket-based human approval channel**

### A2A Protocol (Agent-to-Agent)
- **Google A2A interop** — JSON-RPC 2.0 over HTTP
- **Agent discovery** via `/.well-known/agent.json` (AgentCard)
- **A2AServer** with `TaskHandler` trait for custom task processing
- **A2AClient** for communicating with remote A2A-compliant agents
- **Full task lifecycle** — send, get, cancel, list tasks

### Memory and Search
- **Vector memory** with local embeddings (bag-of-words FNV, 256 dimensions)
- **Hybrid search** — BM25 + embedding similarity with Reciprocal Rank Fusion
- **Query expansion** with synonym groups
- **JSONL persistence** for vector stores
- **File-based and database-backed session stores**

### Compliance
- **GDPR** — consent tracking, right to erasure (Art. 17), data portability (Art. 20)
- **ISO 27001** — access control logging, incident response, risk assessment
- **ISO 42001** — AI system inventory, bias monitoring, transparency logging, HITL
- **DPGA** — all 9 indicators assessed
- **Compliance report generation** — Markdown, JSON, and HTML output formats
- **Multi-region data routing** — configurable data residency with region-aware request routing

### MCP Integration
- **MCP Client** (JSON-RPC 2.0 over stdio)
- **MCP Server mode** — expose skills as MCP tools
- **MCP Proxy** — centralized control plane with logging, metrics, and rate limiting
- **Proxy Orchestrator** — multi-proxy coordination with routing rules, circuit breaker, and failover
- **Credential Vault** — centralized API token management with rotation, quotas, and provider grouping
- **Token Pool** — per-provider token pool with sliding-window rate limiting and tier priority
- **Tool discovery** and auto-reconnect

### Production Hardening
- **Graceful shutdown** — 4-phase ordered shutdown (PreDrain→Drain→Cleanup→Final) with timeout enforcement
- **Distributed correlation** — W3C traceparent propagation, span hierarchy, baggage across agents
- **Error aggregation** — Fingerprinting, deduplication, severity escalation, trend analysis
- **Alert engine** — 8 condition types, cooldown suppression, batch evaluation, acknowledge workflow
- **SLA tracker** — Uptime %, response time compliance, incident lifecycle, compliance reports
- **Multi-format metrics export** — JSON, CSV, OpenMetrics (Prometheus), InfluxDB Line Protocol
- **Event bus** — Pub/sub for decoupled component communication (orchestrator events)
- **Structured output parser** — JSON schema extraction from LLM text with auto-pattern fallback
- **Debug recorder** — Step-by-step reasoning traces for agent debugging

### Code Intelligence
- **CodeGraph** — Regex-based AST analysis for Rust, Python, TypeScript, Go
- **DiffEngine** — Precise diff generation via LCS, unified diff format
- **TestOracle** — Parsing cargo test, pytest, jest, go test with TDD cycle automation
- **CodePlanner** — Implementation planning with DAG ordering and risk assessment
- **ReviewEngine** — 25+ rules across 7 dimensions (security, performance, style, correctness)
- **DevTeam** — Pre-configured teams with 8 workflow templates and quality gates

### SDKs and Language Bridges
- **Python SDK** (`argentor-sdk` on PyPI) — httpx + pydantic, sync + async, 24 models
- **TypeScript SDK** (`@argentor/sdk` on npm) — fetch-based, strict TypeScript, SSE streaming
- **PyO3 Python bridge** (`argentor-python` crate) — native Rust-to-Python bindings for direct embedding

### Agent Intelligence (Key Differentiator)
- **Extended Thinking Mode** — multi-pass reasoning (Quick/Standard/Deep/Exhaustive) with task decomposition and confidence scoring
- **Self-Critique Loop** — Reflexion pattern: agent reviews and revises its own responses across 6 quality dimensions
- **Automatic Context Compaction** — summarizes conversation history when approaching token limits (4 strategies)
- **Dynamic Tool Discovery** — semantic search for relevant tools instead of loading all (TF-IDF + keyword hybrid)
- **Agent Handoffs** — sequential control transfer between specialized agents (OpenAI Agents SDK pattern)
- **State Checkpointing** — save/restore complete agent state for time-travel debugging (LangGraph pattern)
- **Trace Visualization** — JSON, Mermaid gantt charts, and flame graph output for execution debugging
- **Dynamic Tool Generation** — agents create new tools at runtime from declarative specs
- **Process Reward Scoring** — per-step reasoning quality scoring across 7 categories
- **Learning Feedback Loop** — tool selector that improves over time with execution outcome data

### Agent Evaluation & Benchmarks
- **Agent Eval & Benchmark suite** — 5 benchmark suites, 45 test cases for measuring agent quality
- **Workflow DSL** — TOML-based workflow definitions, no Rust code needed
- **Knowledge Graph memory** — entity-relationship graph for structured agent memory

### Streaming & Cost Management
- **SSE Streaming chat** — `POST /api/v1/chat/stream` for real-time token-by-token responses
- **Cost Optimization Engine** — 5 strategies for minimizing LLM spend while maintaining quality
- **Conversation Trees** — Git-like branching for conversation history (branch, merge, diff)

### Developer Tooling
- **Tool Builder** — 3-line tool definitions for rapid skill creation
- **Hook System** — Pre/Post execution hooks with deny/modify capabilities
- **Permission Modes** — 6 modes including PlanOnly for safe agent execution
- **In-Process MCP Server** — run MCP server in-process without stdio overhead

### Protocol & Integration
- **Universal `query()` API** — single API covering all 14 LLM providers
- **NDJSON Protocol** — newline-delimited JSON for structured agent communication
- **Headless mode** — run agents without interactive terminal (CI/CD, automation)
- **Context Assembly** — auto-assembles git context + ARGENTOR.md project files
- **Agent SDK wrappers** — Python and TypeScript SDK wrappers for agent orchestration

### Additional Capabilities
- **Docker sandbox** for untrusted code execution
- **Browser automation** — navigate, screenshot, extract_text, fill_form, click
- **Cron-like task scheduling** for recurring agent jobs
- **Artifact storage** — in-memory and file-system backends
- **Human-in-the-loop approval** — auto-approve, callback, stdin, and WebSocket channels
- **Session transcripts** — append-only JSONL
- **Markdown-based skill definitions**
- **Agent personality system** — name, role, instructions, style, constraints, expertise, thinking level
- **Skill vetting pipeline** — checksum verification, size limits, ed25519 signature validation, WASM static analysis
- **CLI REPL** — Interactive agent debugging shell with 12 commands

---

## Architecture

```
                         ┌──────────────────┐
                         │   Orchestrator   │
                         │   (Opus model)   │
                         └────────┬─────────┘
                                  │
          ┌───────────┬───────────┼───────────┬───────────┐
          │           │           │           │           │
    ┌─────▼─────┐┌────▼─────┐┌───▼────┐┌─────▼────┐┌────▼─────┐
    │   Spec    ││  Coder   ││ Tester ││ Reviewer ││ Architect│
    │  Worker   ││  Worker  ││ Worker ││  Worker  ││  Worker  │
    └─────┬─────┘└────┬─────┘└───┬────┘└─────┬────┘└────┬─────┘
          │           │          │            │          │
          └───────────┴──────────┼────────────┴──────────┘
                                 │
                    ┌────────────▼────────────┐
                    │       MCP Proxy         │  <-- Centralized control plane
                    │     (argentor-mcp)      │
                    └────────────┬────────────┘
                                 │
          ┌──────────────────────┼──────────────────────┐
          │                      │                      │
    ┌─────▼──────┐    ┌──────────▼──────────┐    ┌──────▼──────┐
    │   Skills   │    │  External MCP       │    │   Audit     │
    │   (WASM)   │    │  Servers + Tools    │    │   Log       │
    └────────────┘    └─────────────────────┘    └─────────────┘
          │                                            │
    ┌─────▼──────┐                              ┌──────▼──────┐
    │ Capability │                              │ Compliance  │
    │   Check    │                              │  Modules    │
    └────────────┘                              └─────────────┘
```

### Data Flow

1. **Ingest** — Requests arrive via REST API, WebSocket, or channel adapters (Slack, Discord, Telegram)
2. **Route** — The gateway authenticates, rate-limits, and routes to the appropriate agent or orchestrator
3. **Plan** — The orchestrator decomposes the task into a DAG of subtasks with dependency resolution
4. **Execute** — Specialized workers execute subtasks in parallel (respecting dependencies), each with isolated context
5. **Proxy** — All tool calls pass through the MCP Proxy for permission validation, logging, and progressive disclosure
6. **Synthesize** — The orchestrator collects artifacts, validates consistency, and produces the final output
7. **Audit** — Every action is logged to the append-only audit trail for compliance

---

## Crates

| Crate | Description |
|-------|-------------|
| `argentor-core` | Core types, errors, correlation context, event bus, error aggregator, metrics export |
| `argentor-security` | Capabilities, RBAC, rate limiting, audit, TLS/mTLS, JWT, encrypted store, alerts, SLA tracking |
| `argentor-session` | Session management, `FileSessionStore`, persistence |
| `argentor-skills` | Skill trait, `SkillRegistry`, WASM sandbox runtime, vetting pipeline, ed25519 signing |
| `argentor-agent` | Agent runner, 14 LLM backends, failover, streaming, circuit breaker, cache, code intelligence |
| `argentor-channels` | Multi-platform channel adapters (Slack, Discord, Telegram, Webchat) |
| `argentor-gateway` | HTTP/WebSocket gateway with auth, webhooks, Prometheus metrics, control plane, dashboard, OpenAPI |
| `argentor-builtins` | Built-in skills: shell, file I/O, HTTP, memory, browser, Docker, code generation |
| `argentor-memory` | Vector memory, hybrid search (BM25 + embeddings), query expansion |
| `argentor-mcp` | MCP client/server/proxy, proxy orchestrator, credential vault, token pool |
| `argentor-orchestrator` | Multi-agent engine, TaskQueue with DAG, AgentMonitor, DeploymentManager, HealthChecker |
| `argentor-compliance` | GDPR, ISO 27001, ISO 42001, DPGA compliance modules |
| `argentor-a2a` | Google A2A protocol: AgentCard, A2AServer, A2AClient, JSON-RPC 2.0 interop |
| `argentor-python` | PyO3 Python bridge — native Rust bindings for embedding Argentor in Python applications |
| `argentor-cli` | CLI binary (`serve`, `deploy`, `agents`, `health`, `skill list`) with config hot-reload |

---

## Quick Start

### Prerequisites

- Rust 1.75+ (`rustup update stable`)
- An API key from Claude, OpenAI, Gemini, or another supported provider

### Build

```bash
git clone https://github.com/fboiero/Argentor.git
cd Argentor
cargo build --workspace
```

### Configure

Copy and edit the configuration file:

```bash
cp argentor.toml my-config.toml
# Edit my-config.toml with your API key and preferences
```

Example minimal configuration:

```toml
[model]
provider = "claude"
model_id = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"
temperature = 0.7
max_tokens = 4096
max_turns = 20

[server]
host = "0.0.0.0"
port = 3000
```

### Run

```bash
# Start the gateway server
cargo run --bin argentor -- serve

# List available skills
cargo run --bin argentor -- skill list

# Generate compliance report
cargo run --bin argentor -- compliance report
```

### Client SDKs

```bash
# Python
pip install argentor-client

# TypeScript
npm install @argentor/client
```

```python
from argentor_client import ArgentorClient

client = ArgentorClient(base_url="http://localhost:3000", tenant_id="my-tenant")
result = client.run_task("sales_qualifier", "Lead: Acme Corp, LATAM, CFO, Score 75")
print(result["response"])
```

```typescript
import { ArgentorClient } from '@argentor/client';

const client = new ArgentorClient({ baseUrl: 'http://localhost:3000', tenantId: 'my-tenant' });
const result = await client.runTask('support_responder', 'Customer needs help with withdrawal');
console.log(result.response);
```

### Test

```bash
cargo test --workspace           # Run all 3953 tests
cargo clippy --workspace         # 0 warnings (strict lints)
cargo fmt --all -- --check       # Check formatting
```

---

## Security Model

Argentor uses defense-in-depth with capability-based security:

| Threat | Defense |
|--------|---------|
| RCE via gateway | Origin validation + mTLS |
| Sandbox escape | WASM isolation (wasmtime + WASI) |
| SSRF | NetworkAccess capability with allowlist, blocks private/link-local ranges |
| Path traversal | FileRead/FileWrite scoped to directories, canonicalization + blocklist |
| Auth bypass | JWT authentication (HMAC-SHA256) + API key middleware |
| Credential theft | Encrypted credential store (AES-256-GCM, PBKDF2 key derivation) |
| Privilege escalation | RBAC policy engine (Admin/Operator/Viewer/Custom roles) |
| Log poisoning | Sanitizer strips control characters |
| Supply chain (plugins) | WASM isolation + capability audit + ed25519 signature verification |
| Shell injection | Command sanitizer blocks `rm -rf`, fork bombs, dangerous patterns |
| Brute force | Token bucket rate limiting per agent/endpoint |
| Man-in-the-middle | TLS/mTLS support |

### Capabilities

Each skill declares the capabilities it needs. The permission system validates before execution:

```toml
[[skills]]
name = "file_reader"
type = "wasm"
path = "skills/file-reader.wasm"
[skills.capabilities]
file_read = ["/tmp", "/home/user/docs"]
```

### Authentication and Authorization

```toml
# JWT authentication
[auth]
jwt_secret = "${JWT_SECRET}"
algorithm = "HS256"

# OAuth2 providers
[[auth.oauth2]]
provider = "github"
client_id = "${GITHUB_CLIENT_ID}"
client_secret = "${GITHUB_CLIENT_SECRET}"

# RBAC roles
[[rbac.roles]]
name = "operator"
permissions = ["execute_skills", "read_sessions"]
allowed_skills = ["shell", "file_read", "file_write"]
rate_limit = { requests_per_minute = 100 }
```

---

## Multi-Agent Orchestration

The orchestrator follows Anthropic's recommended **Orchestrator-Workers** pattern:

1. **Plan** -- Orchestrator decomposes the task into subtasks with a dependency graph (DAG)
2. **Execute** -- Specialized workers execute in parallel (respecting dependencies), each with an isolated context window
3. **Synthesize** -- Orchestrator collects artifacts, validates consistency, produces final output

### Agent Roles

| Role | Responsibility |
|------|---------------|
| **Orchestrator** | Decomposes tasks, delegates to workers, synthesizes results |
| **Spec** | Analyzes requirements, generates specifications |
| **Coder** | Generates secure, idiomatic code |
| **Tester** | Writes and validates tests |
| **Reviewer** | Reviews code for security and compliance |
| **Architect** | Designs system architecture and makes structural decisions |
| **SecurityAuditor** | Performs security analysis and vulnerability assessment |
| **DevOps** | Handles deployment, infrastructure, and CI/CD |
| **DocumentWriter** | Generates documentation and reports |
| **Custom** | User-defined role with custom instructions |

### Collaboration Patterns

| Pattern | Description |
|---------|-------------|
| **Pipeline** | Sequential processing through a chain of agents |
| **MapReduce** | Parallel execution with result aggregation |
| **Debate** | Multiple agents argue positions, best response wins |
| **Ensemble** | Multiple agents produce outputs, results are merged |
| **Supervisor** | A supervisor agent monitors and corrects worker agents |
| **Swarm** | Autonomous agents self-organize around tasks |

### Recovery Strategies

When a subtask fails, the orchestrator can apply dynamic replanning:

- **Retry** -- Re-execute the failed task
- **Reassign** -- Assign to a different worker
- **Decompose** -- Break the task into smaller subtasks
- **Skip** -- Skip the task and continue
- **Abort** -- Stop the entire pipeline
- **Escalate** -- Escalate to human review

### Token Budget Tracking

Each agent operates within a configurable token budget. The orchestrator tracks cumulative token usage and cost estimation across all workers, enabling cost-aware task allocation.

### Human-in-the-Loop

High-risk operations require human approval:

```rust
TaskStatus::NeedsHumanReview  // Pauses execution until approved
```

Approval channels: auto-approve (testing), stdin (CLI), WebSocket (gateway), callback (custom).

---

## Code Generation and DevOps Skills

Argentor includes built-in skills for code generation and infrastructure automation.

### API Scaffold Generator

Generate complete project scaffolds from a specification:

```toml
[[skills]]
name = "api_scaffold"
type = "builtin"
```

Supported targets:

| Framework | What Gets Generated |
|-----------|-------------------|
| **Rust / Axum** | Routes, models, handlers, Cargo.toml, Dockerfile, tests |
| **Python / FastAPI** | Routes, models, schemas, requirements.txt, Dockerfile, tests |
| **Node / Express** | Routes, models, middleware, package.json, Dockerfile, tests |

### IaC Generator

Generate infrastructure-as-code artifacts:

| Output | Details |
|--------|---------|
| **Docker** | Multi-stage builds with security hardening |
| **docker-compose** | Service definitions with resource limits, read-only fs |
| **Helm charts** | Full chart with templates (Deployment, Service, Ingress, HPA, PVC) |
| **Terraform** | AWS and GCP provider configurations |
| **GitHub Actions** | CI/CD workflows (check, test, clippy, fmt) |

### Additional Development Skills

- **Code Analysis** -- language-aware AST analysis for code understanding
- **Test Runner** -- multi-language test execution with structured result parsing
- **Git Operations** -- repository operations via libgit2 (no shell commands, no injection risk)

---

## Observability

### Prometheus Metrics

Argentor exposes a `/metrics` endpoint compatible with Prometheus:

```
GET /metrics
```

Available metrics include request counts, latency histograms, active connections, and agent-level statistics.

### Token Tracking

Per-agent and per-session token usage tracking with cost estimation by provider:

- Input/output token counts per turn
- Cumulative cost per agent and per orchestration run
- Budget enforcement with configurable limits

### Audit Logging

Append-only JSONL audit logs with structured querying:

```rust
let results = query_audit_log(&log, AuditFilter {
    action: Some("tool_call".into()),
    agent_id: Some("coder-01".into()),
    from: Some(start_time),
    ..Default::default()
});
```

---

## Compliance

### GDPR
- Consent tracking with `ConsentStore`
- Right to erasure (Art. 17)
- Data portability (Art. 20)
- Purpose limitation

### ISO 27001
- Access control logging
- Incident response tracking
- Risk assessment records

### ISO 42001 (AI Management)
- AI system inventory
- Bias monitoring
- Transparency logging
- Human oversight for high-risk decisions

### DPGA (Digital Public Goods Alliance)

Argentor targets all 9 DPGA indicators:

1. **Open Source** -- AGPL-3.0-only
2. **SDG Relevance** -- SDG 9 (Innovation), SDG 16 (Institutions)
3. **Open Data** -- MCP interoperability with open datasets
4. **Privacy** -- GDPR compliance module
5. **Documentation** -- Comprehensive docs (EN/ES)
6. **Open Standards** -- MCP (AAIF/Linux Foundation), WASM, WIT
7. **Ownership** -- Clear governance
8. **Do No Harm** -- ISO 42001, HITL, bias monitoring
9. **Interoperability** -- MCP + A2A protocol support

---

## MCP Integration

Argentor implements [Model Context Protocol](https://modelcontextprotocol.io/) for tool integration.

### Client Mode

Connect to external MCP servers:

```toml
[[mcp_servers]]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

### Server Mode

Expose Argentor skills as MCP tools for other agents and clients to consume:

```toml
[mcp_server]
enabled = true
transport = "stdio"
```

### Proxy Mode

The MCP Proxy acts as a centralized control plane:

- Logging of every tool invocation
- Permission validation against capability policies
- Rate limiting per agent
- Progressive tool disclosure (~98% token reduction)
- Auto-reconnect with health checks
- Tool discovery across multiple backends

---

## Docker

### Build and Run

```bash
docker build -t argentor .
docker run -p 3000:3000 argentor serve
```

### Docker Compose

```bash
docker-compose -f docker-compose.production.yml up -d
```

The included `docker-compose.production.yml` provides security hardening:
- Resource limits (memory and CPU)
- Read-only filesystem
- Dropped capabilities (`cap_drop: ALL`)
- Non-root user

### Helm Chart

Deploy to Kubernetes:

```bash
helm install argentor deploy/helm/argentor/
```

The Helm chart includes templates for Deployment, Service, Ingress, HPA, PVC, and ServiceAccount.

---

## What's Next?

- **[10 Step-by-Step Tutorials](docs/tutorials/)** — From empty directory to production-ready multi-agent systems. Covers first agent, skills, orchestration, RAG, custom skills, guardrails, agent intelligence, MCP, deployment, and observability.
- [Getting Started Guide](docs/GETTING_STARTED.md) — 5-minute quick start (CLI, SDKs, Docker).
- [Deployment Guide](docs/DEPLOYMENT.md) — Production deployment (Docker, Kubernetes, Helm, multi-region).
- [Technical Report](docs/TECHNICAL_REPORT.md) — Architecture deep dive.
- [Comparison](docs/COMPARISON.md) — How Argentor compares to LangChain, CrewAI, AutoGen.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## License

This project is licensed under the **GNU Affero General Public License v3.0** -- see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- [Anthropic](https://anthropic.com) -- Claude models and MCP protocol
- [wasmtime](https://wasmtime.dev) -- WebAssembly runtime
- [Axum](https://github.com/tokio-rs/axum) -- Web framework
- [DPGA](https://digitalpublicgoods.net) -- Digital Public Goods Alliance
- [wiremock](https://github.com/LukeMathWalker/wiremock-rs) -- HTTP mocking for integration tests
- [criterion](https://github.com/bheisler/criterion.rs) -- Benchmarking framework
