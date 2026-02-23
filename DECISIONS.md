# Agentor — Decision & Prompt Log

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
