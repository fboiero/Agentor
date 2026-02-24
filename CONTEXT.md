# Agentor — Session Context
> Last updated: 2026-02-23 (session 4)

## Current Goal
11 OpenClaw parity features implemented. Framework at feature parity for coding-agent capabilities.

## Task Tracker
| Task | Description | Status |
|------|-------------|--------|
| #45 | LlmBackend trait abstraction | **COMPLETED** |
| #46 | Markdown skills system | **COMPLETED** |
| #47 | Tool groups in SkillRegistry | **COMPLETED** |
| #48 | Wire MCP Proxy into orchestrator | **COMPLETED** |
| #50 | Wire progressive tool disclosure | **COMPLETED** |
| #49 | Implement HITL with human_approval skill | **COMPLETED** |
| #51 | Add E2E orchestration integration test | **COMPLETED** |
| #52 | CLI Approval Channel (stdin) | **COMPLETED** |
| #53 | Orchestration Builtins (3 skills) | **COMPLETED** |
| #54 | WebSocket Approval Channel | **COMPLETED** |
| #55 | Channels Completion (Socket Mode, Gateway, Manager) | **COMPLETED** |
| #56 | MCP Server Manager (auto-reconnect, health) | **COMPLETED** |
| #57 | Compliance Integration (hooks + persistence) | **COMPLETED** |
| #58 | Model Failover | **COMPLETED** |
| #59 | Session Transcripts | **COMPLETED** |
| #60 | Hybrid Search BM25+Vector | **COMPLETED** |
| #61 | Webhooks | **COMPLETED** |
| #62 | Plugin System | **COMPLETED** |
| #63 | Docker Sandbox | **COMPLETED** |
| #64 | Sub-agent Spawning | **COMPLETED** |
| #65 | Config Hot-Reload | **COMPLETED** |
| #66 | Cron/Scheduler | **COMPLETED** |
| #67 | Query Expansion | **COMPLETED** |
| #68 | Browser Automation | **COMPLETED** |

## What's Completed in Session 4

### Wave A — Fundamentals

**#58 — Model Failover**
- `FailoverBackend` in `agentor-agent/src/failover.rs` wrapping `Vec<Box<dyn LlmBackend>>`
- `RetryPolicy { max_retries, backoff_base_ms, backoff_max_ms }` with exponential backoff
- `is_retryable()` error classification (429, 5xx, timeout → retry; 400 → skip)
- `fallback_models: Vec<ModelConfig>` + `retry_policy: Option<RetryPolicy>` in ModelConfig
- LlmClient auto-wraps in FailoverBackend when fallback_models non-empty
- 7 tests

**#59 — Session Transcripts**
- `TranscriptEvent` enum (5 variants), `TranscriptEntry`, `TranscriptStore` trait
- `FileTranscriptStore` — JSONL append-only, one file per session
- 5 tests

**#60 — Hybrid Search BM25+Vector**
- `Bm25Index` with inverted index, BM25 scoring (k1=1.2, b=0.75)
- `HybridSearcher` combining VectorStore + Bm25Index with RRF fusion (rrf_k=60)
- `alpha: f32` for balance (0.0=pure BM25, 1.0=pure vector, default 0.5)
- 11 tests

### Wave B — Extensibility

**#61 — Webhooks**
- `WebhookConfig`, `SessionStrategy`, HMAC-SHA256 validation (constant-time)
- `webhook_handler` for axum, template rendering with `{{payload}}`
- 10 tests

**#62 — Plugin System**
- `Plugin` trait with lifecycle hooks (on_load, on_unload, on_event)
- `PluginManifest`, `PluginEvent` (6 variants), `PluginRegistry`
- 6 tests

### Wave C — Infrastructure

**#63 — Docker Sandbox**
- `DockerSandbox` + `DockerShellSkill` behind `docker` feature flag (bollard)
- `DockerSandboxConfig` with memory/CPU limits, timeout, network toggle
- `sanitize_command()` for injection prevention
- 8 tests

**#64 — Sub-agent Spawning**
- `SubAgentSpawner` with max_depth (3) and max_children_per_task (5) limits
- `SpawnRequest`, `children_of()`, integrated into orchestrator Task type
- 6 tests

**#65 — Config Hot-Reload**
- `ConfigWatcher` using `notify::RecommendedWatcher` with 500ms debounce
- `ReloadableConfig` — security, skills, mcp_servers, tool_groups, webhooks
- 4 tests

### Wave D — Automation

**#66 — Cron/Scheduler**
- `Scheduler` with cron expression parsing, next fire time calculation
- `ScheduledJob { name, cron_expression, task_description, enabled }`
- Background task loop with sleep-until-next-fire
- 5 tests

**#67 — Query Expansion**
- `QueryExpander` trait + `RuleBasedExpander` with 10 synonym groups
- `deduplicate_results()` for multi-query dedup
- 5 tests

### Wave E — Browser

**#68 — Browser Automation**
- `BrowserAutomation` + `BrowserAutomationSkill` behind `browser` feature (fantoccini)
- Actions: navigate, screenshot, extract_text, fill_form, click
- `BrowserConfig { webdriver_url, headless, timeout_secs, screenshot_dir }`
- ~5 tests

## Build Health
- `cargo build --workspace` — OK
- `cargo test --workspace` — **431 tests passing** (was 342 before session 4)
- `cargo clippy --workspace` — **0 warnings**

## Git State
- **Branch**: master, all changes LOCAL and UNCOMMITTED
- Session 2 commit: `617e808` (tasks #45-#51)
- Session 3 commit: `f19e66c` (tasks #52-#57)
- Session 4: tasks #58-#68 not yet committed

## Key File Paths (new in session 4)
| File | Role |
|------|------|
| `crates/agentor-agent/src/failover.rs` | Model failover with exponential backoff |
| `crates/agentor-session/src/transcript.rs` | JSONL session transcripts |
| `crates/agentor-memory/src/bm25.rs` | BM25 inverted index |
| `crates/agentor-memory/src/hybrid.rs` | Hybrid search (BM25 + Vector + RRF) |
| `crates/agentor-memory/src/query_expansion.rs` | Query expansion with synonyms |
| `crates/agentor-gateway/src/webhook.rs` | Webhook handler with HMAC validation |
| `crates/agentor-skills/src/plugin.rs` | Plugin system (trait + registry) |
| `crates/agentor-builtins/src/docker_sandbox.rs` | Docker sandbox skill |
| `crates/agentor-builtins/src/browser_automation.rs` | Browser automation skill |
| `crates/agentor-orchestrator/src/spawner.rs` | Sub-agent spawning |
| `crates/agentor-orchestrator/src/scheduler.rs` | Cron/scheduler |
| `crates/agentor-cli/src/config_watcher.rs` | Config hot-reload watcher |

## New Dependencies (session 4)
| Dep | Crate | Feature Flag |
|-----|-------|-------------|
| `bollard` | agentor-builtins | `docker` |
| `fantoccini` | agentor-builtins | `browser` |
| `notify` | agentor-cli | — |
| `cron` | agentor-orchestrator | — |
