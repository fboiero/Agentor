# Agentor — Session Context
> Last updated: 2026-02-22 (session 2)

## Current Goal
Improve Agentor multi-agent framework by closing critical gaps identified in codebase audit.

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

## What's Completed (all uncommitted)

### Task #45 — LlmBackend Trait
- Created `LlmBackend` trait in `backends/mod.rs`
- 3 implementations: `ClaudeBackend`, `OpenAiBackend`, `ClaudeCodeBackend`
- `llm.rs` reduced from ~930 to ~80 lines, dispatches via `Box<dyn LlmBackend>`
- `from_backend()` constructor for custom providers

### Task #46 — Markdown Skills
- `markdown_skill.rs` with MarkdownSkill, MarkdownSkillLoader, hot-reload
- YAML frontmatter for metadata, supports callable tools and prompt injections
- 3 example skills in `skills/markdown/`
- 13 tests

### Task #47 — Tool Groups
- `ToolGroup` struct in `registry.rs` with 5 default groups (minimal, coding, web, orchestration, full)
- `filter_by_group()`, `skills_in_group()`, `register_group()` methods
- Configurable via `agentor.toml`
- 7 tests

### Task #48 — MCP Proxy Wiring
- `proxy: OptionalProxy` field + `with_proxy()` builder on `AgentRunner`
- `execute_tool()` dispatch: proxy if configured, direct otherwise
- Orchestrator mode routes through proxy; gateway serve mode does not

### Task #50 — Progressive Tool Disclosure
- `tool_group: Option<String>` on `AgentProfile`
- All profiles use tool groups: orchestrator → "orchestration", workers → "minimal"
- Engine prefers tool_group, falls back to allowed_skills
- `ToolDiscovery::estimate_token_savings()` logged per worker

### Task #49 — HITL (human_approval skill)
- `HumanApprovalSkill` in `agentor-builtins/src/human_approval.rs`
- `ApprovalChannel` trait with `AutoApproveChannel` and `CallbackApprovalChannel`
- Schema: task_id, description, risk_level (low/medium/high/critical), context
- Registered in all `register_builtins*` functions (auto-approve by default)
- Engine `detect_review_flags()` detects NEEDS_HUMAN_REVIEW, CRITICAL_SECURITY_ISSUE, etc.
- Tasks flagged → `TaskStatus::NeedsHumanReview` + monitor `WaitingForApproval`
- `OrchestratorResult.needs_review_tasks` field added
- `TaskQueue.is_done()` treats NeedsHumanReview as terminal state
- 7 tests in human_approval.rs

### Task #51 — E2E Orchestration Integration Test
- `tests/e2e_orchestration.rs` with `MockBackend` implementing `LlmBackend`
- Mock returns deterministic responses per role, verifies context flow via asserts
- `AgentRunner::from_backend()` — inject custom backend for testing
- `Orchestrator::with_backend_factory()` — role-based backend factory
- 7 E2E tests: happy path, HITL flagging, proxy metrics, monitor tracking, progressive disclosure, progress callback, queue state

## Future Gaps (not yet tasked)
- Compliance modules are hardcoded placeholders
- Channels crate is a stub (trait only)
- Missing builtins: `artifact_store`, `agent_delegate`, `task_status`
- No external MCP server integration
- CLI approval channel (stdin-based for interactive HITL)
- WebSocket approval channel for dashboard-based HITL

## Git State
- **Branch**: master, all changes LOCAL and UNCOMMITTED
- **Modified** (17+): runner.rs, engine.rs, profiles.rs, types.rs, registry.rs, llm.rs, lib.rs files, Cargo.toml files, config.rs, agentor.toml, CLAUDE.md, task_queue.rs, regression.rs
- **New**: backends/ (4 files), markdown_skill.rs, skills/markdown/ (3 files), human_approval.rs, e2e_orchestration.rs, CONTEXT.md, DECISIONS.md

## Build Health
- `cargo build --workspace` — OK
- `cargo test --workspace` — **287 tests passing**
- `cargo clippy --workspace` — **0 warnings**

## Key File Paths
| File | Role |
|------|------|
| `crates/agentor-agent/src/runner.rs` | AgentRunner + proxy + from_backend |
| `crates/agentor-agent/src/backends/` | LlmBackend trait + implementations |
| `crates/agentor-agent/src/llm.rs` | LlmClient dispatcher |
| `crates/agentor-orchestrator/src/engine.rs` | Orchestrator + HITL + backend_factory |
| `crates/agentor-orchestrator/src/profiles.rs` | Worker profiles + tool_group |
| `crates/agentor-orchestrator/src/types.rs` | AgentProfile, Task, Artifact |
| `crates/agentor-orchestrator/src/task_queue.rs` | TaskQueue + needs_review_count |
| `crates/agentor-orchestrator/tests/e2e_orchestration.rs` | E2E tests with MockBackend |
| `crates/agentor-builtins/src/human_approval.rs` | HITL approval skill |
| `crates/agentor-skills/src/registry.rs` | SkillRegistry + ToolGroup |
| `crates/agentor-skills/src/markdown_skill.rs` | Markdown skills |
| `crates/agentor-mcp/src/proxy.rs` | McpProxy |
| `crates/agentor-mcp/src/discovery.rs` | ToolDiscovery |
| `crates/agentor-cli/src/main.rs` | CLI commands |
