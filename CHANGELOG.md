# Changelog

All notable changes to Agentor are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

### Added
- **Clippy hardening**: Strict workspace-level lints (unwrap_used, expect_used, uninlined_format_args, redundant_closure_for_method_calls, etc.)
- **Crate documentation**: `//!` crate-level docs and `///` module docs for all 13 crates
- **Core type docs**: Full `///` documentation on AgentorError, Message, Role, ToolCall, ToolResult and all fields/variants
- **Integration tests**: 52 new tests across 5 files (builtins, memory, mcp, core, compliance)
- **CHANGELOG.md**: This file

### Changed
- **CI**: `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs`
- **README**: Updated test count to 483, added new features list, Docker section, updated crate table

### Fixed
- Eliminated all `.unwrap()` / `.expect()` in production code (~30 files)
- Replaced with `?`, `map_err()`, `unwrap_or_default()`, or `#[allow]` with safety comments
- Fixed complex type warning in failover.rs (SleepFn type alias)
- Auto-fixed 176 uninlined format args and 12 redundant closures

---

## [0.1.0] - 2025-02-23

### Added

#### Core Framework (8 crates)
- **agentor-core**: Base types (`Message`, `ToolCall`, `ToolResult`, `AgentorError`), role enum, approval types
- **agentor-security**: Capability-based permissions, `PermissionSet`, `RateLimiter` (token bucket), `AuditLog` (append-only), `Sanitizer`, TLS/mTLS config
- **agentor-session**: `Session` lifecycle, `FileSessionStore` (JSON persistence), `FileTranscriptStore` (JSONL)
- **agentor-skills**: `Skill` trait, `SkillRegistry`, `WasmSkillRuntime` (wasmtime + WASI sandbox), `SkillLoader`, `MarkdownSkill`, `Plugin` system
- **agentor-agent**: `AgentRunner` with agentic loop, `ModelConfig`, `LlmProvider` (Claude, OpenAI, OpenRouter), `ContextWindow`, `StreamEvent`, `FailoverBackend` with exponential backoff
- **agentor-channels**: `Channel` trait, `ChannelManager`, WebChat, Slack, Discord, Telegram adapters
- **agentor-gateway**: Axum HTTP/WebSocket gateway, `AuthConfig` (API key), `ConnectionManager`, `MessageRouter`, rate limiting middleware, `WebhookConfig` (HMAC-SHA256), `WsApprovalChannel`
- **agentor-cli**: CLI binary with `serve`, `skill list`, `compliance report`, `orchestrate` subcommands, config hot-reload via `ConfigWatcher`

#### Advanced Features (3 crates)
- **agentor-memory**: `VectorStore` trait, `FileVectorStore` (JSONL), `InMemoryVectorStore`, `LocalEmbedding` (TF-IDF), `HybridSearcher` (BM25 + embedding + RRF), `Bm25Index`, `QueryExpander`
- **agentor-mcp**: `McpClient` (JSON-RPC 2.0 over stdio), `McpSkill`, `McpProxy` (centralized control plane with logging/metrics/rate limiting), `McpServerManager` (auto-reconnect, health checks), `ToolDiscovery`
- **agentor-builtins**: 13 built-in skills — shell, file_read, file_write, http_fetch, browser, memory_store, memory_search, human_approval, artifact_store, agent_delegate, task_status, docker_sandbox, browser_automation

#### Multi-Agent + Compliance (2 crates)
- **agentor-orchestrator**: `Orchestrator` (plan/execute/synthesize), `TaskQueue` (topological sort), `AgentMonitor` (real-time metrics), `Scheduler` (cron), `SubAgentSpawner`, `AgentProfile` per role
- **agentor-compliance**: GDPR (`ConsentStore`, data subject rights), ISO 27001 (access control, incident response), ISO 42001 (AI inventory, bias monitoring, transparency), DPGA (9 indicators), `ComplianceReport` generation, `JsonReportStore`, `ComplianceHookChain`

#### Infrastructure
- Dockerfile (multi-stage build, non-root user)
- GitHub Actions CI (check, test, clippy, fmt)
- Example config (`agentor.toml`)
- WASM echo-skill example
- Markdown skill templates (rust_conventions, security_review, test_guidelines)

### Security
- WASM sandbox isolation for all skill plugins
- Capability-based permission model (FileRead, FileWrite, NetworkAccess, ShellExec)
- SSRF prevention with network allowlists
- Path traversal prevention with directory scoping
- Input sanitization (control character stripping)
- API key authentication middleware
- Rate limiting (token bucket)
- HMAC-SHA256 webhook validation
- Human-in-the-loop for high-risk operations
