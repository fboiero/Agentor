# Argentor — Technical Report

> Secure Multi-Agent AI Framework in Rust
> Version 0.1.0 | April 2026

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Overview](#2-architecture-overview)
3. [Crate Map](#3-crate-map)
4. [The Agentic Loop](#4-the-agentic-loop)
5. [Multi-Agent Orchestration](#5-multi-agent-orchestration)
6. [Security Architecture](#6-security-architecture)
7. [WASM Plugin System](#7-wasm-plugin-system)
8. [Protocol Support (MCP + A2A)](#8-protocol-support-mcp--a2a)
9. [Built-in Skills (40+)](#9-built-in-skills-40)
10. [Gateway & REST API](#10-gateway--rest-api)
11. [Observability Stack](#11-observability-stack)
12. [Compliance Modules](#12-compliance-modules)
13. [Data & Memory Layer](#13-data--memory-layer)
14. [SDK & Language Bridges](#14-sdk--language-bridges)
15. [Deployment](#15-deployment)
16. [Project Metrics](#16-project-metrics)

---

## 1. Executive Summary

Argentor is a production-grade multi-agent AI orchestration framework written in Rust. It provides:

- **15 crates** with clean dependency layering (zero circular deps)
- **14 LLM providers** (Claude, OpenAI, Gemini, Ollama, Mistral, xAI, Azure, Cerebras, Together, DeepSeek, vLLM, OpenRouter, Claude Code, Failover)
- **40+ built-in skills** covering data processing, web search, crypto, security scanning, file ops, code analysis
- **WASM-sandboxed plugins** with Ed25519 signing and capability-based permissions
- **MCP + A2A protocol** support for agent interoperability
- **Guardrails pipeline** integrated into the agentic loop (PII, injection, toxicity)
- **Enterprise compliance** modules (GDPR, ISO 27001, ISO 42001, DPGA)
- **3,953 tests**, zero failures, zero clippy warnings

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ARGENTOR FRAMEWORK                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────┐  ┌───────────┐  ┌────────────┐  ┌──────────────────┐  │
│  │   CLI   │  │  Gateway   │  │ Dashboard  │  │   SSO / Auth     │  │
│  │  (REPL) │  │ (REST/WS)  │  │   (HTML)   │  │ (OIDC/SAML/Key) │  │
│  └────┬────┘  └─────┬──────┘  └─────┬──────┘  └───────┬──────────┘  │
│       │             │               │                  │             │
│       └─────────────┼───────────────┼──────────────────┘             │
│                     │               │                                │
│  ┌──────────────────▼───────────────▼──────────────────────────┐    │
│  │                    AGENT RUNNER                              │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐  │    │
│  │  │Guardrails│ │  Cache   │ │ Circuit  │ │ Debug Recorder│  │    │
│  │  │ Pipeline │ │  (LRU)   │ │ Breaker  │ │  (Traces)     │  │    │
│  │  └──────────┘ └──────────┘ └──────────┘ └───────────────┘  │    │
│  │                                                             │    │
│  │  ┌──────────────────────────────────────────────────────┐   │    │
│  │  │              LLM BACKEND (14 Providers)              │   │    │
│  │  │  Claude │ OpenAI │ Gemini │ Ollama │ Mistral │ ...   │   │    │
│  │  └──────────────────────────────────────────────────────┘   │    │
│  └─────────────────────────┬───────────────────────────────────┘    │
│                            │                                        │
│  ┌─────────────────────────▼───────────────────────────────────┐    │
│  │                   SKILL REGISTRY                            │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐   │    │
│  │  │ Built-in │ │   WASM   │ │ Markdown │ │  MCP Skills  │   │    │
│  │  │  (40+)   │ │ Plugins  │ │  Skills  │ │  (Remote)    │   │    │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────────┘   │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │ Orchestrator │  │   Memory     │  │      Compliance          │  │
│  │ (Multi-Agent)│  │ (RAG+Vector) │  │ (GDPR/ISO/DPGA)         │  │
│  │ 6 Patterns   │  │ BM25+Embed   │  │ Report Gen (MD/HTML/JSON)│  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │   Security   │  │   Sessions   │  │     Protocols            │  │
│  │ RBAC+Crypto  │  │ SQLite/File  │  │  MCP + A2A + JSON-RPC   │  │
│  │ Audit+TLS    │  │ Transcripts  │  │  Credential Vault       │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Crate Map

```
                          ┌──────────────┐
                          │ argentor-cli │  (binary, REPL, demos)
                          └──────┬───────┘
                                 │
                 ┌───────────────┼───────────────┐
                 │               │               │
          ┌──────▼──────┐ ┌─────▼──────┐  ┌─────▼──────────┐
          │   gateway   │ │orchestrator│  │    a2a         │
          │ REST/WS/SSO │ │ multi-agent│  │ Agent-to-Agent │
          └──────┬──────┘ └─────┬──────┘  └────────────────┘
                 │              │
          ┌──────┼──────────────┤
          │      │              │
   ┌──────▼──┐ ┌─▼──────────┐ ┌▼───────────┐
   │  agent  │ │  builtins  │ │ compliance │
   │ runner  │ │  40+ skills│ │ GDPR/ISO   │
   └────┬────┘ └──────┬─────┘ └────────────┘
        │             │
   ┌────┼─────────────┤
   │    │             │
┌──▼──┐ ┌▼────────┐ ┌─▼──────┐ ┌──────────┐
│ mcp │ │  skills │ │ memory │ │ channels │
│proxy│ │registry │ │RAG+vec │ │ Slack/WS │
└──┬──┘ └────┬────┘ └────┬───┘ └──────────┘
   │         │           │
   └─────────┼───────────┘
             │
   ┌─────────▼─────────┐
   │  session  │  security  │  core  │
   │  storage  │  RBAC/TLS  │  types │
   └───────────┴────────────┴────────┘
```

### Crate Descriptions

| # | Crate | LOC | Tests | Purpose |
|---|-------|-----|-------|---------|
| 1 | `argentor-core` | ~8K | 63 | Types (Message, ToolCall, ToolResult), event bus, correlation, telemetry |
| 2 | `argentor-security` | ~12K | 526 | Capabilities, RBAC, AES-256 encryption, audit log, SLA, rate limiting |
| 3 | `argentor-session` | ~5K | 84 | Session lifecycle, SQLite backend, transcripts, usage/persona stores |
| 4 | `argentor-skills` | ~8K | 170 | Skill trait, WASM runtime (wasmtime), registry, marketplace, vetting |
| 5 | `argentor-agent` | ~18K | 900 | AgentRunner, 14 LLM backends, guardrails, ReAct, evaluator, cache |
| 6 | `argentor-channels` | ~3K | 18 | Channel trait, Slack adapter, WebChat, manager |
| 7 | `argentor-gateway` | ~20K | 550 | Axum HTTP/WS, REST API, SSO, observability, marketplace API |
| 8 | `argentor-builtins` | ~25K | 900 | 40+ built-in skills (all categories) |
| 9 | `argentor-memory` | ~8K | 140 | Vector store, RAG pipeline, BM25, embeddings (3 providers) |
| 10 | `argentor-mcp` | ~6K | 126 | MCP client/server, credential vault, token pool, proxy orchestrator |
| 11 | `argentor-orchestrator` | ~15K | 356 | Multi-agent engine, 6 patterns, workflows, deployment, health |
| 12 | `argentor-compliance` | ~7K | 280 | GDPR, ISO 27001, ISO 42001, DPGA, report generator |
| 13 | `argentor-a2a` | ~4K | 125 | Google A2A protocol, agent cards, streaming SSE |
| 14 | `argentor-cli` | ~5K | — | Binary, REPL, 8 demos, config |
| 15 | `argentor-python` | ~1K | — | PyO3 bridge (9 classes, build with maturin) |
| | **Total** | **~140K** | **3,953** | |

---

## 4. The Agentic Loop

The core execution engine (`AgentRunner::run()`) implements a multi-turn agentic loop with 8 stages:

```
 User Input
     │
     ▼
 ┌────────────────────┐
 │  1. INPUT GUARD    │──▶ Block if: PII, injection, toxicity
 │     (GuardrailEngine)    Sanitize if: email → [EMAIL]
 └────────┬───────────┘
          │
          ▼
 ┌────────────────────┐
 │  2. CIRCUIT CHECK  │──▶ If provider OPEN → error (don't waste tokens)
 │  (CircuitBreaker)  │    States: Closed → Open → HalfOpen
 └────────┬───────────┘
          │
          ▼
 ┌────────────────────┐
 │  3. CACHE CHECK    │──▶ LRU cache with TTL
 │  (ResponseCache)   │    Key = hash(provider + messages)
 └────────┬───────────┘    HIT → return immediately
          │ MISS
          ▼
 ┌────────────────────┐
 │  4. LLM CALL       │──▶ 14 providers via LlmBackend trait
 │  (chat/stream)     │    Failover: automatic retry on next provider
 └────────┬───────────┘
          │
          ▼
 ┌────────────────────┐
 │  5. OUTPUT GUARD   │──▶ Validate LLM response
 │  (GuardrailEngine) │    Auto-redact PII in output
 └────────┬───────────┘    Block if policy violated
          │
          ▼
 ┌────────────────────┐
 │  6. RESPONSE TYPE  │
 │     Done?          │──▶ YES → cache + return final text
 │     Text?          │──▶ Continue to next turn
 │     ToolUse?       │──▶ Execute tools (step 7)
 └────────┬───────────┘
          │ ToolUse
          ▼
 ┌────────────────────┐
 │  7. TOOL EXECUTE   │──▶ Permission check (capabilities)
 │  (SkillRegistry)   │    Argument validation
 └────────┬───────────┘    Execute via WASM/native/MCP
          │
          ▼
 ┌────────────────────┐
 │  8. RESULT GUARD   │──▶ Sanitize tool output (prevent leaks)
 │  (GuardrailEngine) │    Backfill to context
 └────────┬───────────┘
          │
          └──▶ Loop back to step 1 (next turn)
```

**Key design decisions:**
- Guardrails run at 3 points: pre-LLM input, post-LLM output, post-tool result
- Cache operates BEFORE guardrails on output (cached responses already passed)
- Circuit breaker prevents wasting tokens on failing providers
- All stages are opt-in via builder pattern (`with_guardrails()`, `with_cache()`, etc.)

---

## 5. Multi-Agent Orchestration

The `Orchestrator` decomposes complex tasks into sub-tasks and distributes them across specialized worker agents.

```
                    ┌──────────────────┐
                    │   ORCHESTRATOR   │
                    │  (Coordinator)   │
                    │                  │
                    │  EventBus ──────▶│ emits task.started/completed/failed
                    │  ErrorAggregator │ fingerprints failures
                    │  BudgetTracker   │ per-agent token limits
                    └────────┬─────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
       ┌──────▼──────┐ ┌────▼─────┐ ┌──────▼──────┐
       │  Worker A   │ │ Worker B │ │  Worker C   │
       │ (Researcher)│ │ (Coder)  │ │ (Reviewer)  │
       │             │ │          │ │             │
       │ Skills:     │ │ Skills:  │ │ Skills:     │
       │  web_search │ │  shell   │ │  code_anal  │
       │  summarizer │ │  file_rw │ │  review_eng │
       └─────────────┘ └──────────┘ └─────────────┘
```

### 6 Collaboration Patterns

```
1. PIPELINE           2. MAP-REDUCE          3. DEBATE
   A → B → C             ┌─ M1 ─┐              Proponent
                          │  M2  │──▶ Reduce       │
                          └─ M3 ─┘              Opponent
                                                   │
                                                 Judge

4. ENSEMBLE           5. SUPERVISOR          6. SWARM
   ┌─ A1 ─┐             Supervisor            A ◀──▶ B
   │  A2  │──▶ Vote      │   │   │            │      │
   └─ A3 ─┘             W1  W2  W3            └──▶ C ◀┘
                         (review policy)       (iterate until
                                                consensus)
```

| Pattern | Best For | Example |
|---------|----------|---------|
| Pipeline | Sequential workflows | Research → Write → Review |
| MapReduce | Parallelizable tasks | Analyze 100 files, merge results |
| Debate | Complex decisions | Security assessment with adversarial review |
| Ensemble | High-accuracy needs | Multiple agents vote on classification |
| Supervisor | Quality assurance | Manager reviews all worker outputs |
| Swarm | Creative/exploratory | Brainstorming with convergence |

---

## 6. Security Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    SECURITY LAYERS                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Layer 1: AUTHENTICATION                                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ API Key  │ │  JWT/    │ │  OIDC    │ │    SAML      │  │
│  │  Auth    │ │  OAuth2  │ │  Login   │ │  Enterprise  │  │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────┘  │
│                                                             │
│  Layer 2: AUTHORIZATION                                     │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  RBAC (Role-Based Access Control)                    │  │
│  │  Roles: Admin, Developer, Analyst, Viewer            │  │
│  │  Policies: allow/deny per resource per action        │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Layer 3: CAPABILITY-BASED PERMISSIONS                      │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Each skill declares required capabilities:          │  │
│  │  FileRead{paths}, FileWrite{paths}, ShellExec,       │  │
│  │  NetworkAccess{hosts}, DatabaseQuery, HumanApproval  │  │
│  │  PermissionSet grants subset → principle of least    │  │
│  │  privilege                                           │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Layer 4: WASM SANDBOXING                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Plugins execute in wasmtime with:                   │  │
│  │  - Linear memory isolation (no host access)          │  │
│  │  - Fuel limits (infinite loop prevention)            │  │
│  │  - Ed25519 signature verification                    │  │
│  │  - SHA-256 checksum validation                       │  │
│  │  - Static analysis (suspicious import scanning)      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Layer 5: GUARDRAILS                                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Pre-LLM: PII detection, prompt injection, toxicity  │  │
│  │  Post-LLM: output validation, PII auto-redaction     │  │
│  │  Post-Tool: data leakage prevention in results       │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  Layer 6: ENCRYPTION & AUDIT                                │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  AES-256-GCM encrypted credential vault              │  │
│  │  PBKDF2-HMAC-SHA256 key derivation                   │  │
│  │  Append-only audit log (every action recorded)       │  │
│  │  SIEM export: Splunk, Elasticsearch, CEF, Syslog     │  │
│  │  TLS/mTLS for transport security                     │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. WASM Plugin System

```
  Developer                   Argentor Runtime
  ─────────                   ────────────────

  ┌──────────┐    sign
  │ skill.rs │──────────▶ ┌──────────────────────┐
  │ (source) │            │   SkillManifest      │
  └────┬─────┘            │  - name, version     │
       │ compile          │  - SHA-256 checksum   │
       ▼                  │  - Ed25519 signature  │
  ┌──────────┐            │  - capabilities[]     │
  │skill.wasm│            │  - tags, license      │
  └────┬─────┘            └──────────┬───────────┘
       │                             │
       │         ┌───────────────────▼──────────────────┐
       └────────▶│          SKILL VETTER (5 checks)     │
                 │  1. SHA-256 checksum validation       │
                 │  2. Binary size limit (10MB max)      │
                 │  3. Ed25519 signature verification    │
                 │  4. Capability analysis (blocklist)   │
                 │  5. WASM static analysis              │
                 │     (magic number + import scanning)  │
                 └───────────────────┬──────────────────┘
                                     │ PASS
                                     ▼
                 ┌───────────────────────────────────────┐
                 │          WASMTIME RUNTIME              │
                 │  - Linear memory isolation             │
                 │  - Fuel limits (CPU budget)            │
                 │  - WASI stdio only                     │
                 │  - No filesystem/network access        │
                 │  - spawn_blocking() for async safety   │
                 └───────────────────────────────────────┘
```

---

## 8. Protocol Support (MCP + A2A)

### Model Context Protocol (MCP)

```
  ┌─────────────┐     JSON-RPC 2.0      ┌──────────────┐
  │  Argentor    │◀─────────────────────▶│  MCP Server  │
  │  MCP Client  │     over stdio        │  (External)  │
  │              │                        │              │
  │  methods:    │                        │  Exposes:    │
  │  tools/list  │                        │  - tools     │
  │  tools/call  │                        │  - resources │
  └──────────────┘                        └──────────────┘

  ┌─────────────┐     JSON-RPC 2.0      ┌──────────────┐
  │  External   │◀─────────────────────▶│  Argentor    │
  │  MCP Client │     over HTTP          │  MCP Server  │
  │  (Claude,   │                        │              │
  │   Cursor)   │                        │  Exposes all │
  │              │                        │  registered  │
  └──────────────┘                        │  skills as   │
                                          │  MCP tools   │
                                          └──────────────┘
```

**MCP Infrastructure:**
- `McpProxy`: multiplexes calls to multiple MCP servers
- `CredentialVault`: AES-256 encrypted API key storage with rotation
- `TokenPool`: per-provider rate limiting with tier priority
- `ProxyOrchestrator`: routing (Fixed/RoundRobin/LeastLoaded/PatternBased), circuit breaker, failover

### Agent-to-Agent Protocol (A2A)

```
  ┌─────────────┐                    ┌──────────────┐
  │  Agent A    │  tasks/send        │   Agent B    │
  │  (Argentor) │───────────────────▶│  (Any A2A)   │
  │             │                    │              │
  │  Endpoints: │  tasks/get         │  /.well-known│
  │  /a2a       │◀───────────────────│  /agent.json │
  │  /a2a/stream│  SSE streaming     │              │
  └─────────────┘                    └──────────────┘
```

**A2A Features:**
- Agent discovery via `/.well-known/agent.json`
- Task lifecycle: submitted → working → completed/failed/canceled
- SSE streaming for real-time task updates
- Authentication: Bearer, API Key, OAuth2

---

## 9. Built-in Skills (40+)

```
┌──────────────────────────────────────────────────────────────┐
│                    SKILL REGISTRY                            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  DATA & TEXT              │  CRYPTO & ENCODING               │
│  ┌──────────────────┐    │  ┌──────────────────┐            │
│  │ calculator       │    │  │ hash (SHA/HMAC)  │            │
│  │ text_transform   │    │  │ encode_decode    │            │
│  │ json_query       │    │  │ uuid_generator   │            │
│  │ regex            │    │  └──────────────────┘            │
│  │ data_validator   │    │                                   │
│  │ datetime         │    │  FILE & SYSTEM                    │
│  └──────────────────┘    │  ┌──────────────────┐            │
│                          │  │ file_read        │            │
│  WEB & SEARCH            │  │ file_write       │            │
│  ┌──────────────────┐    │  │ shell            │            │
│  │ web_search       │    │  │ http_fetch       │            │
│  │   (DDG/Tavily/   │    │  └──────────────────┘            │
│  │    Brave/SearXNG) │    │                                   │
│  │ web_scraper      │    │  SECURITY & AI                    │
│  │ rss_reader       │    │  ┌──────────────────┐            │
│  │ dns_lookup       │    │  │ prompt_guard     │            │
│  └──────────────────┘    │  │ secret_scanner   │            │
│                          │  │ diff             │            │
│  CODE & DEV              │  │ summarizer       │            │
│  ┌──────────────────┐    │  └──────────────────┘            │
│  │ git              │    │                                   │
│  │ code_analysis    │    │  ORCHESTRATION                    │
│  │ test_runner      │    │  ┌──────────────────┐            │
│  │ sdk_generator    │    │  │ agent_delegate   │            │
│  │ api_scaffold     │    │  │ task_status      │            │
│  │ iac_generator    │    │  │ human_approval   │            │
│  └──────────────────┘    │  │ artifact_store   │            │
│                          │  └──────────────────┘            │
│  MEMORY                  │                                   │
│  ┌──────────────────┐    │  BROWSER & DOCKER (optional)      │
│  │ memory_store     │    │  ┌──────────────────┐            │
│  │ memory_search    │    │  │ browser_auto     │            │
│  └──────────────────┘    │  │ docker_sandbox   │            │
│                          │  └──────────────────┘            │
└──────────────────────────┴───────────────────────────────────┘
```

### Tool Groups (Progressive Disclosure)

| Group | Skills | Use Case |
|-------|--------|----------|
| `minimal` | echo, time, help, calculator, datetime | Safe for any context |
| `coding` | file_read/write, shell, memory, regex, json_query, diff | Development tasks |
| `web` | http_fetch, browser, web_search, web_scraper, rss_reader, dns_lookup | Web tasks |
| `data` | calculator, text_transform, json_query, regex, data_validator, encode_decode, hash, uuid, diff, summarizer | Data processing |
| `security` | prompt_guard, secret_scanner, hash, data_validator, encode_decode | Security scanning |
| `development` | git, code_analysis, test_runner, +coding tools | Full dev workflow |
| `orchestration` | agent_delegate, task_status, human_approval, artifact_store | Multi-agent |
| `full` | All registered skills | Unrestricted |

---

## 10. Gateway & REST API

```
┌──────────────────────────────────────────────────────────────┐
│                    GATEWAY SERVER (Axum)                      │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  MIDDLEWARE STACK (applied in order)                          │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │ 1. Request Tracing (X-Trace-Id, spans)                  │ │
│  │ 2. SSO Auth (Bearer token / cookie validation)          │ │
│  │ 3. Per-Key Rate Limiting (sliding window, 429+headers)  │ │
│  │ 4. Auth (API key / JWT validation)                      │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                              │
│  ROUTES                                                      │
│  ┌──────────────────────────────────────────┐                │
│  │ Health                                   │                │
│  │  GET  /health         (basic)            │                │
│  │  GET  /health/live    (liveness probe)   │                │
│  │  GET  /health/ready   (readiness probe)  │                │
│  ├──────────────────────────────────────────┤                │
│  │ Auth                                     │                │
│  │  GET  /auth/login     (SSO redirect)     │                │
│  │  GET  /auth/callback  (SSO callback)     │                │
│  │  GET  /auth/logout    (revoke session)   │                │
│  │  GET  /auth/me        (current user)     │                │
│  │  POST /auth/api-key   (key→session)      │                │
│  ├──────────────────────────────────────────┤                │
│  │ Agent API (/api/v1/)                     │                │
│  │  POST /agent/chat                        │                │
│  │  POST /agent/run-task                    │                │
│  │  POST /agent/run-task-stream   (SSE)     │                │
│  │  POST /agent/batch                       │                │
│  │  POST /agent/evaluate                    │                │
│  │  GET  /agent/status                      │                │
│  ├──────────────────────────────────────────┤                │
│  │ Sessions                                 │                │
│  │  GET    /sessions                        │                │
│  │  GET    /sessions/:id                    │                │
│  │  DELETE /sessions/:id                    │                │
│  ├──────────────────────────────────────────┤                │
│  │ Skills                                   │                │
│  │  GET  /skills                            │                │
│  │  GET  /skills/:name                      │                │
│  ├──────────────────────────────────────────┤                │
│  │ Marketplace (/api/v1/marketplace/)       │                │
│  │  GET  /search                            │                │
│  │  GET  /featured                          │                │
│  │  GET  /categories                        │                │
│  │  GET  /skills/:name                      │                │
│  │  GET  /popular                           │                │
│  │  GET  /recent                            │                │
│  │  POST /install/:name                     │                │
│  │  DEL  /install/:name                     │                │
│  │  GET  /installed                         │                │
│  │  GET  /stats                             │                │
│  ├──────────────────────────────────────────┤                │
│  │ Observability                            │                │
│  │  GET  /metrics        (Prometheus)       │                │
│  │  GET  /openapi.json   (OpenAPI 3.0)      │                │
│  │  GET  /dashboard      (HTML SPA)         │                │
│  ├──────────────────────────────────────────┤                │
│  │ WebSocket                                │                │
│  │  WS   /ws             (bidirectional)    │                │
│  ├──────────────────────────────────────────┤                │
│  │ A2A Protocol                             │                │
│  │  GET  /.well-known/agent.json            │                │
│  │  POST /a2a            (JSON-RPC)         │                │
│  │  POST /a2a/stream     (SSE)              │                │
│  └──────────────────────────────────────────┘                │
└──────────────────────────────────────────────────────────────┘
```

---

## 11. Observability Stack

```
  ┌─────────────────────────────────────────────────────────┐
  │                 OBSERVABILITY                            │
  ├─────────────────────────────────────────────────────────┤
  │                                                         │
  │  TRACING (OpenTelemetry)                                │
  │  ┌───────────────────────────────────────────────────┐  │
  │  │ #[tracing::instrument] on:                        │  │
  │  │  - AgentRunner::run() (session_id, max_turns)     │  │
  │  │  - AgentRunner::execute_tool() (tool_name, id)    │  │
  │  │  - AgentRunner::run_streaming() (streaming=true)  │  │
  │  │  - LLM calls (provider, turn, session_id)         │  │
  │  │                                                   │  │
  │  │ Request middleware adds:                           │  │
  │  │  - X-Trace-Id response header                     │  │
  │  │  - X-Span-Id response header                      │  │
  │  │  - Method/path/status/duration spans              │  │
  │  │                                                   │  │
  │  │ Export: OTLP gRPC (Jaeger, Tempo, Datadog)        │  │
  │  └───────────────────────────────────────────────────┘  │
  │                                                         │
  │  METRICS (Prometheus)                                    │
  │  ┌───────────────────────────────────────────────────┐  │
  │  │ GET /metrics returns:                             │  │
  │  │  - http_requests_total{method,path,status}        │  │
  │  │  - http_request_duration_seconds{method,path}     │  │
  │  │  - active_connections                             │  │
  │  │  - llm_calls_total{provider}                      │  │
  │  │  - tool_executions_total{skill}                   │  │
  │  │  - tokens_used_total{provider,direction}          │  │
  │  └───────────────────────────────────────────────────┘  │
  │                                                         │
  │  AUDIT LOG                                              │
  │  ┌───────────────────────────────────────────────────┐  │
  │  │ Every action recorded: session_id, action, skill, │  │
  │  │ outcome (Success/Denied/Error), details (JSON)    │  │
  │  │                                                   │  │
  │  │ Export formats:                                   │  │
  │  │  Splunk | Elasticsearch | CEF | JSON-LD | Syslog  │  │
  │  └───────────────────────────────────────────────────┘  │
  │                                                         │
  │  DEBUG RECORDER                                          │
  │  ┌───────────────────────────────────────────────────┐  │
  │  │ Step-by-step trace: Input → LlmCall → ToolCall   │  │
  │  │ → ToolResult → LlmResponse → Output              │  │
  │  │ With timing, token counts, metadata               │  │
  │  └───────────────────────────────────────────────────┘  │
  └─────────────────────────────────────────────────────────┘
```

---

## 12. Compliance Modules

```
┌──────────────────────────────────────────────────────────────┐
│                    COMPLIANCE ENGINE                          │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────────────┐   ┌────────────────┐                     │
│  │     GDPR       │   │  ISO 27001     │                     │
│  │                │   │                │                     │
│  │ ConsentStore   │   │ AccessControl  │                     │
│  │ - track consent│   │ - login events │                     │
│  │ - check expiry │   │ - denial rate  │                     │
│  │                │   │                │                     │
│  │ DSR Processing │   │ Incidents      │                     │
│  │ - Access       │   │ - severity     │                     │
│  │ - Erasure      │   │ - resolution   │                     │
│  │ - Portability  │   │ - lifecycle    │                     │
│  │ - 30-day SLA   │   │                │                     │
│  └────────────────┘   └────────────────┘                     │
│                                                              │
│  ┌────────────────┐   ┌────────────────┐                     │
│  │  ISO 42001     │   │     DPGA       │                     │
│  │  (AI Systems)  │   │                │                     │
│  │                │   │ Digital Public  │                     │
│  │ AI Inventory   │   │ Goods Alliance │                     │
│  │ - risk level   │   │ - 9 indicators │                     │
│  │ - model info   │   │ - open source  │                     │
│  │                │   │ - data privacy │                     │
│  │ Bias Checks    │   │                │                     │
│  │ - pass/warn/   │   │                │                     │
│  │   fail         │   │                │                     │
│  │                │   │                │                     │
│  │ Transparency   │   │                │                     │
│  │ - decision log │   │                │                     │
│  └────────────────┘   └────────────────┘                     │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │              REPORT GENERATOR                        │    │
│  │                                                      │    │
│  │  Inputs: GDPR + ISO 27001 + ISO 42001 data          │    │
│  │                                                      │    │
│  │  Outputs:                                            │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐           │    │
│  │  │ Markdown │  │   JSON   │  │   HTML   │           │    │
│  │  │ (report) │  │  (API)   │  │(dashboard)│           │    │
│  │  └──────────┘  └──────────┘  └──────────┘           │    │
│  │                                                      │    │
│  │  Executive Summary:                                  │    │
│  │  - Overall status (Compliant/Partial/NonCompliant)   │    │
│  │  - Per-framework score (0-100%)                      │    │
│  │  - Critical findings list                            │    │
│  │  - Recommendations                                   │    │
│  │  - Next review date                                  │    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

---

## 13. Data & Memory Layer

```
┌──────────────────────────────────────────────────────────────┐
│                    MEMORY & DATA                             │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  RAG PIPELINE                                                │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ Ingest → Chunk → Embed → Store → Query → Format     │    │
│  │                                                      │    │
│  │ Chunking: FixedSize | Paragraph | Sentence | Semantic│    │
│  │ Embedding: Local(TF-IDF) | OpenAI | Cohere | Voyage │    │
│  │ Search: Vector similarity + BM25 hybrid              │    │
│  │ Query expansion: rule-based synonym/related terms    │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                              │
│  VECTOR STORES                                               │
│  ┌──────────────┐  ┌──────────────┐                          │
│  │ InMemory     │  │ FileVector   │  (trait-based,            │
│  │ VectorStore  │  │ Store        │   pluggable)              │
│  └──────────────┘  └──────────────┘                          │
│                                                              │
│  SESSION STORAGE                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ FileSession  │  │ SQLite       │  │ JSON Index   │       │
│  │ Store        │  │ Backend      │  │ Store        │       │
│  │              │  │ (WAL mode)   │  │              │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│                                                              │
│  CONVERSATION MEMORY                                         │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ Cross-session per-customer context:                  │    │
│  │ - CustomerProfile (topics, sentiment, history)       │    │
│  │ - ConversationSummarizer (token-budgeted)            │    │
│  │ - System prompt injection for context                │    │
│  └──────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

---

## 14. SDK & Language Bridges

### Python SDK (`pip install argentor-sdk`)

```python
from argentor import ArgentorClient

client = ArgentorClient(base_url="http://localhost:8080", api_key="key")

# Run agent task
result = client.run_task(role="researcher", context="Analyze market trends")

# Stream responses
for event in client.run_task_stream(role="writer", context="Write a report"):
    print(event.text, end="")

# Execute skills directly
result = client.execute_skill("calculator", {"operation": "evaluate", "expression": "2^10"})

# Guardrails check
health = client.health_ready()
```

### TypeScript SDK (`npm install @argentor/sdk`)

```typescript
import { ArgentorClient } from '@argentor/sdk';

const client = new ArgentorClient({ baseUrl: 'http://localhost:8080', apiKey: 'key' });

const result = await client.runTask({ role: 'analyst', context: 'Summarize Q4 results' });

for await (const event of client.runTaskStream({ role: 'writer', context: 'Draft email' })) {
    process.stdout.write(event.text);
}
```

### PyO3 Bridge (`maturin develop`)

```python
import argentor

# Direct Rust execution from Python (no HTTP overhead)
registry = argentor.SkillRegistry()
result = registry.execute("calculator", '{"operation": "evaluate", "expression": "sqrt(144)"}')
print(result.content)  # "12"

guard = argentor.GuardrailEngine()
check = guard.check_input("My SSN is 123-45-6789")
print(check.passed)          # False
print(check.sanitized_text)  # "My SSN is [SSN]"
```

---

## 15. Deployment

### Docker Production

```yaml
# docker-compose.production.yml
services:
  argentor:
    build: .
    ports: ["8080:8080"]
    deploy:
      resources:
        limits: { memory: 512M, cpus: '2' }
    read_only: true
    cap_drop: [ALL]
    security_opt: [no-new-privileges:true]
    healthcheck:
      test: ["CMD", "wget", "--spider", "http://localhost:8080/health/live"]

  prometheus:   # optional (--profile monitoring)
    image: prom/prometheus:latest
    ports: ["9090:9090"]

  grafana:      # optional (--profile monitoring)
    image: grafana/grafana:latest
    ports: ["3000:3000"]
```

### Helm Chart (Kubernetes)

```
deploy/helm/argentor/
  Chart.yaml
  values.yaml
  templates/
    deployment.yaml    (runAsNonRoot, readOnlyRootFilesystem, seccomp)
    service.yaml
    ingress.yaml
    hpa.yaml           (autoscaling 1→10 replicas)
    pvc.yaml
```

---

## 16. Project Metrics

| Metric | Value |
|--------|-------|
| **Language** | Rust (core) + WASM (plugins) |
| **Crates** | 15 (14 workspace + 1 PyO3) |
| **Lines of Code** | ~140,000 |
| **Tests** | 3,953 passing, 0 failures |
| **Clippy** | 0 warnings with `-D warnings` |
| **LLM Providers** | 14 |
| **Built-in Skills** | 40+ |
| **REST Endpoints** | 50+ |
| **Compliance Frameworks** | 4 (GDPR, ISO 27001, ISO 42001, DPGA) |
| **Collaboration Patterns** | 6 (Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm) |
| **Feature Flags** | 7 (telemetry, sqlite, http-embeddings, registry, browser, docker, client) |
| **License** | AGPL-3.0-only |
| **MSRV** | 1.80 |
| **SDKs** | Python + TypeScript + PyO3 |

### Release Channels

| Channel | Package | Command |
|---------|---------|---------|
| crates.io | 13 crates | `cargo add argentor-core` |
| GitHub Releases | Binaries (Linux x86/arm, macOS x86/arm) | Download from releases |
| PyPI | `argentor-sdk` | `pip install argentor-sdk` |
| npm | `@argentor/sdk` | `npm install @argentor/sdk` |
| GHCR | `ghcr.io/fboiero/argentor` | `docker pull ghcr.io/fboiero/argentor` |
| GitHub Pages | Landing + demos | https://fboiero.github.io/Agentor |

### Recent Additions (Phases 55-58)

| Phase | Features |
|-------|----------|
| **55** | Agent Eval & Benchmark suite (5 suites, 45 cases), Workflow DSL (TOML-based, no Rust needed), Knowledge Graph memory (entity-relationship) |
| **56** | SSE Streaming chat (`POST /api/v1/chat/stream`), Cost Optimization Engine (5 strategies), Conversation Trees (Git-like branching) |
| **57** | Tool Builder (3-line definitions), Hook System (Pre/Post with deny/modify), Permission Modes (6 modes incl. PlanOnly), In-Process MCP Server, Universal `query()` API (14 providers) |
| **58** | NDJSON Protocol + headless mode, Context Assembly (auto git + ARGENTOR.md), Agent SDK wrappers (Python + TypeScript) |

---

*Generated for Argentor v0.1.0 — April 2026*
*Repository: https://github.com/fboiero/Argentor*
