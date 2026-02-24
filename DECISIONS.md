# Agentor — Decision & Prompt Log

---

## 2026-02-24 — Hardening + Documentation Sprint

**Asked**: Execute 4-wave plan: clippy config, unwrap elimination, documentation, integration tests.

**Decided**:
- Wave 1: Strict clippy lints at workspace level, propagated to all 13 crates. CI uses `-D warnings -A missing-docs` (deny all warnings except missing_docs, which stays as warn for progressive improvement).
- Wave 2: All production unwraps replaced with `?` or `unwrap_or_default()`. Infallible operations (signal handlers, reqwest client, Default impls) get `#[allow]` with safety comments. All test code (inline + standalone files) gets blanket `#[allow]`.
- Wave 3: Crate-level `//!` docs + `///` module docs on all lib.rs. Core types fully documented. README updated with current stats. CI excludes missing_docs from deny to avoid blocking on ~500 remaining field/method docs.
- Wave 4: 52 new integration tests across 5 new test files (builtins, memory, mcp, core, compliance). Total: 483 tests.

**Alternatives considered**:
- Could have documented ALL ~500 public items (too expensive, diminishing returns for internal fields)
- Could have used `deny` for missing_docs (would block CI; chose progressive approach instead)
- Could have used `Result` for Default impls instead of `#[allow(expect_used)]` (would break the trait contract)

**Result**: 483 tests passing, 0 clippy errors (with CI flags), 0 unwraps in production code.

---

## 2026-02-22 — Codebase Improvement Sprint

### Decision 1: MCP Proxy Wiring Strategy
- **Timestamp**: 2026-02-22
- **Asked**: Wire MCP proxy (dead code) into actual orchestrator execution
- **Decision**: Added optional proxy to AgentRunner via builder pattern (`with_proxy()`), routing tool calls through `execute_tool()` dispatch. Orchestrator mode uses proxy; gateway serve mode does not.
- **Alternatives**: (1) Make proxy mandatory — rejected, breaks gateway. (2) Proxy at SkillRegistry level — rejected, too invasive.
- **Notes**: `Option<(Arc<McpProxy>, String)>` — String is agent_id for metrics attribution.

### Decision 2: Progressive Tool Disclosure via Tool Groups
- **Timestamp**: 2026-02-22
- **Asked**: Wire progressive tool disclosure — profiles reference non-existent skills, ToolDiscovery unused
- **Decision**: Added `tool_group: Option<String>` to AgentProfile. Engine prefers tool_group, falls back to allowed_skills. Token savings logged via `ToolDiscovery::estimate_token_savings()`.
- **Alternatives**: (1) `filter_by_allowed()` directly — redundant. (2) Remove `allowed_skills` — rejected for backward compat. (3) Context-based filtering — deferred.
- **Notes**: Workers use "minimal" group. Even without tool calls, disclosure saves tokens from schema serialization.

### Decision 3: LlmBackend Trait Abstraction
- **Timestamp**: 2026-02-22 (earlier session)
- **Asked**: Refactor 930-line llm.rs with provider if/else chains
- **Decision**: `LlmBackend` trait + `Box<dyn LlmBackend>` dispatch. 3 backend files under `backends/`. `from_backend()` for custom providers.
- **Alternatives**: Enum dispatch — rejected, less extensible for third-party providers.
- **Notes**: llm.rs: ~930 → ~80 lines.

### Decision 4: Markdown Skills System
- **Timestamp**: 2026-02-22 (earlier session)
- **Asked**: Add markdown-based skills with YAML frontmatter
- **Decision**: MarkdownSkill implementing Skill trait. Supports callable tools and prompt injections. Hot-reload via MarkdownSkillLoader.
- **Alternatives**: TOML frontmatter — chose YAML (markdown standard).
- **Notes**: 13 tests. `prompt_injection: true` flag for system prompt additions.

### Decision 5: CLAUDE.md Replacement
- **Timestamp**: 2026-02-22
- **Asked**: Replace CLAUDE.md with working style / context management / decision logging instructions
- **Decision**: Replaced entirely per user request. Created CONTEXT.md and DECISIONS.md as companion files.
- **Notes**: Previous content preserved in memory files.

### Decision 6: Profile Cleanup
- **Timestamp**: 2026-02-22
- **Asked**: Profiles referenced non-existent skills (artifact_store, agent_delegate, task_status)
- **Decision**: Removed `artifact_store` from worker profiles. Kept orchestrator references as aspirational. Shifted primary filtering to tool groups.
- **Notes**: `filter_to_new()` silently ignores non-existent skills.

---

## 2026-02-22 — Session 2: HITL & E2E Testing

### Decision 7: HITL via ApprovalChannel Trait
- **Timestamp**: 2026-02-22
- **Asked**: Implement human-in-the-loop approval for high-risk operations
- **Decision**: `ApprovalChannel` trait with `request_approval(ApprovalRequest) -> ApprovalDecision`. Two implementations: `AutoApproveChannel` (default/testing) and `CallbackApprovalChannel` (custom async closures). The skill blocks until the channel returns a decision, so the agent loop pauses naturally.
- **Alternatives**: (1) Parse review text only — fragile, no real approval flow. (2) Separate HITL service with message queue — too complex for now. (3) Engine-level approval hooks — rejected, skill-level is more composable.
- **Notes**: Engine also detects review flags (NEEDS_HUMAN_REVIEW markers) in Reviewer output to auto-flag tasks. `TaskQueue.is_done()` updated to treat NeedsHumanReview as terminal.

### Decision 8: Backend Factory for Testable Orchestrator
- **Timestamp**: 2026-02-22
- **Asked**: E2E orchestration test needs mock LLM backends
- **Decision**: Added `BackendFactory = Arc<dyn Fn(&AgentRole) -> Box<dyn LlmBackend>>` to Orchestrator. `AgentRunner::from_backend()` allows bypassing `ModelConfig`-based construction. Factory is role-aware so mocks can return different responses per agent.
- **Alternatives**: (1) Mock HTTP server — more realistic but slower and fragile. (2) Test only at task_queue level — misses context flow verification.
- **Notes**: MockBackend asserts context flow (e.g., Coder receives SPECIFICATION, Reviewer receives CODE+TESTS). 7 E2E tests covering happy path, HITL, proxy, monitor, disclosure, progress callbacks, queue state.

---

## 2026-02-23 — Session 3: Closing All 6 Gaps

### Decision 9: ApprovalChannel Types Moved to agentor-core
- **Timestamp**: 2026-02-23
- **Asked**: Both agentor-builtins and agentor-gateway need ApprovalChannel — where to put shared types?
- **Decision**: Moved `ApprovalChannel`, `ApprovalRequest`, `ApprovalDecision`, `RiskLevel` to `agentor-core::approval`. Re-exported from builtins for backward compat.
- **Alternatives**: (1) Gateway depends on builtins — pulls in memory crate transitively. (2) New `agentor-hitl` crate — overkill.
- **Notes**: Added `async-trait` to core's dependencies for the trait definition.

### Decision 10: TaskQueueHandle Trait to Avoid Circular Dependencies
- **Timestamp**: 2026-02-23
- **Asked**: Orchestration builtins (agent_delegate, task_status) need access to TaskQueue, but builtins can't depend on orchestrator
- **Decision**: Created `TaskQueueHandle` trait in builtins with `add_task()`, `get_task_info()`, `list_tasks()`, `task_summary()`. Orchestrator implements the trait for its TaskQueue. Injected via `Arc<dyn TaskQueueHandle>`.
- **Alternatives**: (1) Move TaskQueue to core — too much orchestrator logic in core. (2) builtins depends on orchestrator — verified no cycle exists, but trait is cleaner.
- **Notes**: `TaskInfo` and `TaskSummary` are self-contained structs in builtins, not tied to orchestrator types.

### Decision 11: MCP Server Manager with Exponential Backoff
- **Timestamp**: 2026-02-23
- **Asked**: Replace ad-hoc MCP connection loop in CLI with proper management
- **Decision**: `McpServerManager` with `connect_all()`, `health_check()`, `reconnect_with_backoff()`, `start_health_loop()`. Backoff: 1s→2s→4s...max 60s, 5 retries. Health checks via `list_tools()` probe.
- **Alternatives**: (1) Simple retry loop — no backoff, could hammer failing servers. (2) Circuit breaker pattern — overkill for MCP stdio processes.
- **Notes**: CLI `Serve` starts health loop; `Orchestrate` does not (short-lived).

### Decision 12: Compliance Hook Chain Pattern
- **Timestamp**: 2026-02-23
- **Asked**: Wire compliance modules into orchestrator runtime
- **Decision**: `ComplianceHook` trait with `on_event(ComplianceEvent)`. `ComplianceHookChain` as composite dispatcher. Two hooks: `Iso27001Hook` (maps to access control events) and `Iso42001Hook` (maps to AI transparency logs). Orchestrator emits TaskStarted/TaskCompleted events. Builder: `with_compliance(hooks)`.
- **Alternatives**: (1) Direct module calls in engine — tightly coupled. (2) Event bus/pubsub — too complex for current needs.
- **Notes**: `JsonReportStore` persists reports as `{framework}_{timestamp}.json`. CLI `Compliance Report` now saves to disk.

### Decision 13: Channels Socket Mode & Gateway
- **Timestamp**: 2026-02-23
- **Asked**: Complete channels crate stubs with real WebSocket implementations
- **Decision**: Full implementations using `tokio-tungstenite`. Slack: connections.open API → WSS → envelope ACK loop. Discord: REST gateway → WSS → Hello → Identify → heartbeat task → event dispatch.
- **Alternatives**: (1) Use serenity/poise for Discord — heavy deps, less control. (2) HTTP polling — doesn't support real-time events.
- **Notes**: Both forward events to `mpsc` channel. `ChannelManager` provides unified send/broadcast interface.

### Summary — Session 3 Stats
- 6 tasks completed (#52-#57)
- Tests: 287 → 330 (+43 new)
- Clippy warnings: 0
- New files: 10
- Modified files: ~20

---

## 2026-02-23 — Session 4: 11 OpenClaw Parity Features

### Decision 14: Model Failover with FailoverBackend Wrapper
- **Timestamp**: 2026-02-23
- **Asked**: Implement model failover with retry + fallback backends
- **Decision**: `FailoverBackend` wraps `Vec<Box<dyn LlmBackend>>` and implements `LlmBackend`. Iterates backends in order on failure with exponential backoff. `is_retryable()` classifies errors (429/5xx/timeout → retry, 400 → skip). LlmClient auto-wraps when `fallback_models` is non-empty.
- **Alternatives**: (1) Retry at HTTP layer — misses provider-specific logic. (2) Retry in each backend — duplicates logic.
- **Notes**: `RetryPolicy` defaults: max_retries=3, backoff_base=1000ms, backoff_max=30000ms.

### Decision 15: Session Transcripts as JSONL
- **Timestamp**: 2026-02-23
- **Asked**: Implement persistent session transcripts
- **Decision**: `TranscriptStore` trait with `FileTranscriptStore` writing JSONL (one file per session). `TranscriptEvent` enum with 5 variants: UserMessage, AssistantMessage, ToolCallRequest, ToolCallResult, SystemEvent. Append-only, read sorts by turn+timestamp.
- **Alternatives**: (1) SQLite — heavier dep. (2) Single JSON file — re-serialization on each append.
- **Notes**: JSONL chosen for append-only friendliness and easy streaming reads.

### Decision 16: Hybrid Search with RRF Fusion
- **Timestamp**: 2026-02-23
- **Asked**: Implement hybrid BM25+vector search
- **Decision**: Separate `Bm25Index` (in-memory inverted index) and `HybridSearcher` (wraps VectorStore + Bm25Index). Fusion via Reciprocal Rank Fusion (RRF, k=60). `alpha` parameter balances BM25 vs vector (default 0.5).
- **Alternatives**: (1) Weighted score combination — biased by score distributions. (2) External search engine (Tantivy) — too heavy for embedded use.
- **Notes**: BM25 params: k1=1.2, b=0.75. Tokenization: split non-alphanum, lowercase, filter len>1.

### Decision 17: Webhooks with HMAC-SHA256 Validation
- **Timestamp**: 2026-02-23
- **Asked**: Add webhook ingestion endpoints
- **Decision**: `WebhookConfig` with optional secret for HMAC-SHA256 validation (constant-time comparison). Template rendering with `{{payload}}` substitution. `SessionStrategy::New` or `ByHeader(name)` for session routing.
- **Alternatives**: (1) Signature in query param — less standard. (2) JWT validation — overkill for webhooks.
- **Notes**: Route: `POST /webhook/:name`. Handler validates HMAC via `X-Webhook-Signature` header.

### Decision 18: Plugin System with Lifecycle Hooks
- **Timestamp**: 2026-02-23
- **Asked**: Add extensible plugin system
- **Decision**: `Plugin` trait with `manifest()`, `on_load(registry)`, `on_unload()`, `on_event(PluginEvent)`. `PluginRegistry` manages lifecycle. 6 event types: SessionCreated, SessionEnded, ToolCallBefore, ToolCallAfter, MessageReceived, Custom.
- **Alternatives**: (1) WASM-only plugins — already have WasmSkillRuntime for that. (2) Dynamic library loading — platform-specific, unsafe.
- **Notes**: Plugin trait is Rust-native; WASM plugins use the existing WasmSkillRuntime path.

### Decision 19: Docker Sandbox Behind Feature Flag
- **Timestamp**: 2026-02-23
- **Asked**: Implement Docker-based command sandboxing
- **Decision**: `DockerSandbox` and `DockerShellSkill` behind `docker` feature flag using bollard crate. `DockerSandboxConfig` with memory/CPU limits, timeout, network toggle, mount paths. `sanitize_command()` blocks injection via semicolons, pipes, backticks.
- **Alternatives**: (1) Always include bollard — bloats binary for users without Docker. (2) gVisor/Firecracker — not available as Rust crate.
- **Notes**: Container reused per session, cleanup on drop. `ExecResult { exit_code, stdout, stderr }`.

### Decision 20: Sub-agent Spawning with Depth/Children Limits
- **Timestamp**: 2026-02-23
- **Asked**: Allow agents to spawn sub-agents dynamically
- **Decision**: `SubAgentSpawner` with `max_depth=3` and `max_children_per_task=5` safety limits. `SpawnRequest` includes parent_task_id. Task struct extended with `parent_task: Option<Uuid>` and `depth: u32`. Agent delegate skill gets `spawn_subtask` action.
- **Alternatives**: (1) No limits — runaway spawning risk. (2) Global agent count limit — less granular control.
- **Notes**: Spawner creates tasks in the existing TaskQueue; engine picks them up naturally.

### Decision 21: Config Hot-Reload via notify Crate
- **Timestamp**: 2026-02-23
- **Asked**: Support runtime config reloading without restart
- **Decision**: `ConfigWatcher` using `notify::RecommendedWatcher` on agentor.toml. Debounce 500ms. `ReloadableConfig` has optional sections (security, skills, mcp_servers, tool_groups, webhooks). Non-reloadable: model config, server bind, TLS.
- **Alternatives**: (1) Polling — higher latency, wastes CPU. (2) inotify directly — Linux-only.
- **Notes**: `notify` is cross-platform (inotify/kqueue/ReadDirectoryChanges). Background thread with std::mpsc for event coalescing.

### Decision 22: Cron Scheduler with Background Loop
- **Timestamp**: 2026-02-23
- **Asked**: Add scheduled task execution
- **Decision**: `Scheduler` with `ScheduledJob` config (cron expression, task description, enabled flag). `start()` returns `JoinHandle` for background loop. Uses `cron` crate for expression parsing and next-fire-time calculation.
- **Alternatives**: (1) tokio-cron-scheduler — heavier dep with its own runtime. (2) Simple interval — not flexible enough for real scheduling.
- **Notes**: Each job creates a new session and runs independently. Disabled jobs are skipped.

### Decision 23: Query Expansion with Rule-Based Synonyms
- **Timestamp**: 2026-02-23
- **Asked**: Expand search queries for better recall
- **Decision**: `QueryExpander` trait in agentor-memory. `RuleBasedExpander` with 10 synonym groups (e.g., error↔bug↔issue, create↔make↔build). Future: LLM-based expander in builtins (avoids circular dep memory→agent).
- **Alternatives**: (1) Word2Vec/embedding similarity — requires model loading. (2) External thesaurus API — adds latency.
- **Notes**: `deduplicate_results()` merges results from multiple expanded queries by ID.

### Decision 24: Browser Automation Behind Feature Flag
- **Timestamp**: 2026-02-23
- **Asked**: Add WebDriver-based browser automation
- **Decision**: `BrowserAutomation` + `BrowserAutomationSkill` behind `browser` feature flag using fantoccini. Actions: navigate, screenshot, extract_text, fill_form, click. Requires external chromedriver/geckodriver.
- **Alternatives**: (1) Headless Chrome via chrome-devtools-rs — Chrome-only. (2) playwright-rust — no stable crate.
- **Notes**: Lazy connection (on first use). Screenshot saves to configurable dir.

### Decision 25: Parallel Implementation Strategy
- **Timestamp**: 2026-02-23
- **Asked**: How to implement 11 features efficiently
- **Decision**: Launched 10+ background agents in parallel (one per feature). Manual integration pass afterward to resolve concurrent edits to shared files (lib.rs, Cargo.toml). Required fixing 7 lib.rs files and multiple Cargo.toml files post-agent completion.
- **Alternatives**: (1) Sequential implementation — slower but no conflicts. (2) Worktree isolation — git worktrees per feature then merge.
- **Notes**: Concurrent editing of shared files (lib.rs) was the main challenge. Resolved by re-reading and rewriting after all agents completed. Net result: 89 new tests, 13 new files, 4 new deps.

### Summary — Session 4 Stats
- 11 tasks completed (#58-#68)
- Tests: 342 → 431 (+89 new)
- Clippy warnings: 0
- New files: 13
- New dependencies: 4 (bollard, fantoccini, notify, cron)
- Modified files: ~25
