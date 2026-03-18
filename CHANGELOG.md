# Changelog

All notable changes to Argentor are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

### Added

#### Phase 15 — Integration & Production Wiring
- **McpServerManager integration**: CredentialVault and TokenPool wired via builder methods, credentials resolved before MCP server connections — 8 tests
- **ProxyOrchestrator in Orchestrator**: Intelligent proxy routing per worker role in multi-agent pipelines, metrics reporting — 5 tests
- **Proxy Management API**: 13 REST endpoints under `/api/v1/proxy-management/` for credentials, tokens, and orchestrator management with automatic secret redaction — 12 tests
- **PersistentStore**: Atomic JSON file persistence for control plane state (deployments, agents, health), credential and token pool snapshots — 17 tests
- **E2E Proxy Orchestration Demo**: `demo_proxy_orchestration.rs` showcasing full credential → pool → routing → circuit breaker pipeline

#### Phase 14 — MCP Proxy Orchestration Hub
- **CredentialVault**: Centralized API token storage with usage tracking, expiry detection, provider grouping, rotation, and quota enforcement — 21 tests
- **ProxyOrchestrator**: Multi-proxy coordination with routing rules (Fixed/RoundRobin/LeastLoaded/PatternBased), circuit breaker (open/half-open/closed), failover, aggregated metrics — 24 tests
- **TokenPool**: Per-provider token pool with sliding-window rate limiting, daily quotas, tier priority (Production/Development/Free/Backup), weighted selection — 27 tests

#### Phase 12 — Orchestrator as Deployment Platform
- **DeploymentManager**: Deploy/undeploy/scale/restart agents with replicas, heartbeats, auto-restart — 24 tests
- **AgentRegistry**: Thread-safe registration with name uniqueness, catalog import/export, 9 default role definitions — 20 tests
- **HealthChecker**: Liveness/readiness/heartbeat probes, state machine transitions (Healthy→Degraded→Unhealthy→Dead), auto-recovery — 23 tests
- **Control Plane REST API**: 17 endpoints under `/api/v1/control-plane/` for managing deployments, agents, and health — 17 tests

#### Phase 13 — Full-Stack Platform
- **Gateway Wiring**: `GatewayServer::build_full()` mounts control-plane and REST API routers, backward compatible
- **A2A Protocol Crate** (`argentor-a2a`): Google Agent-to-Agent interop via JSON-RPC 2.0, AgentCard discovery, TaskHandler trait, A2AClient/A2AServer — 30+ tests
- **Web Dashboard**: Dark-themed SPA at `/dashboard` with deployment management, agent catalog, health monitoring, and metrics — embedded HTML, no build tooling
- **OpenTelemetry**: `TelemetryConfig` behind `telemetry` feature flag, OTLP export, `#[tracing::instrument]` on key paths
- **CLI Subcommands**: `deploy`, `agents`, `health` commands for managing the deployment platform via HTTP API
- **E2E Deployment Demo**: `demo_deployment.rs` showcasing full lifecycle (registry → deploy → heartbeats → scaling → health → cleanup)

#### Phase 7 — Built-in Skills Expansion
- **GitSkill**: libgit2-based git operations (status, diff, log, add, commit, branch)
- **CodeAnalysisSkill**: Language-aware code analysis (function extraction, complexity, dependency listing)
- **TestRunnerSkill**: Multi-language test runner with result parsing (Rust, Python, Node, Go)
- **FileArtifactBackend**: File-system persistent artifact storage with path traversal protection

#### Phase 8 — Multi-Agent Clusters
- **MessageBus**: Inter-agent communication (send, receive, peek, subscribe, broadcast) — 12 tests
- **Replanner**: Dynamic re-planning with 6 recovery strategies (Retry, Reassign, Decompose, Skip, Abort, Escalate) — 15 tests
- **BudgetTracker**: Per-agent token budget tracking with cost estimation and warning thresholds — 12 tests
- **Collaboration Patterns**: 6 multi-agent patterns (Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm) with builder API — 22 tests

#### Phase 9 — REST API & Gateway
- **REST API**: 10 endpoints under `/api/v1/` (sessions CRUD, skills list/detail, agent chat, connections, metrics) — 9 tests
- **Channel Bridge**: ChannelManager-to-MessageRouter bridge with session affinity — 7 tests
- **Parallel Tool Execution**: `execute_parallel()` and `execute_with_timeout()` in SkillRegistry — 6 tests

#### Phase 10 — Observability & MCP Server
- **Prometheus Metrics**: `AgentMetricsCollector` with text exposition format export, endpoint `/metrics` — 14 tests
- **Token Counter**: Per-provider estimation with cost calculation (`ModelPricing`, `UsageTracker`) — 12 tests
- **MCP Server Mode**: Expose skills as MCP tools via JSON-RPC 2.0 stdio protocol — 15 tests
- **Progressive Tool Disclosure**: Tool groups filter skills per agent role (~98% token reduction)

#### Phase 11 — Deployment Infrastructure & Code Generation
- **API Scaffold Generator**: Generate complete projects (Rust/Axum, Python/FastAPI, Node/Express) from JSON specs — 19 tests
- **IaC Generator**: Generate Docker, docker-compose, Helm charts, Terraform AWS/GCP, GitHub Actions — 14 tests
- **DatabaseSessionStore**: Database-backed sessions with metadata queries and expiration cleanup — 14 tests
- **JWT/OAuth2 Authentication**: HMAC-SHA256 JWT, API key hashing, OAuth2 provider config, axum middleware — 25 tests
- **Prometheus Gateway Integration**: `/metrics` endpoint on gateway, `/api/v1/metrics/prometheus` on REST API

#### Documentation
- **Clippy hardening**: Strict workspace-level lints (unwrap_used, expect_used, uninlined_format_args, etc.)
- **Crate documentation**: `//!` crate-level docs and `///` module docs for all 13 crates
- **Integration tests**: 461 new tests (total: 944, was 483)
- **CHANGELOG.md**: This file
- **ARCHITECTURE.md**: Updated with all 11 phases
- **Website**: Updated landing page with full feature showcase

### Changed
- **CI**: `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs`
- **README**: Complete rewrite with current stats (944 tests, 54K LOC, 13 crates)
- **GatewayServer**: `build_with_middleware()` now accepts optional `AgentMetricsCollector`
- **RestApiState**: Added optional `AgentMetricsCollector` for observability data
- **MetricsResponse**: Expanded with tool_calls, tokens, active_agents, security_events fields

### Fixed
- Eliminated all `.unwrap()` / `.expect()` in production code (~30 files)
- Replaced with `?`, `map_err()`, `unwrap_or_default()`, or `#[allow]` with safety comments
- Fixed complex type warning in failover.rs (SleepFn type alias)
- Auto-fixed 176 uninlined format args and 12 redundant closures

---

## [0.1.0] - 2025-02-23

### Added

#### Core Framework (8 crates)
- **argentor-core**: Base types (`Message`, `ToolCall`, `ToolResult`, `ArgentorError`), role enum, approval types
- **argentor-security**: Capability-based permissions, `PermissionSet`, `RateLimiter` (token bucket), `AuditLog` (append-only), `Sanitizer`, TLS/mTLS config
- **argentor-session**: `Session` lifecycle, `FileSessionStore` (JSON persistence), `FileTranscriptStore` (JSONL)
- **argentor-skills**: `Skill` trait, `SkillRegistry`, `WasmSkillRuntime` (wasmtime + WASI sandbox), `SkillLoader`, `MarkdownSkill`, `Plugin` system
- **argentor-agent**: `AgentRunner` with agentic loop, `ModelConfig`, `LlmProvider` (Claude, OpenAI, OpenRouter), `ContextWindow`, `StreamEvent`, `FailoverBackend` with exponential backoff
- **argentor-channels**: `Channel` trait, `ChannelManager`, WebChat, Slack, Discord, Telegram adapters
- **argentor-gateway**: Axum HTTP/WebSocket gateway, `AuthConfig` (API key), `ConnectionManager`, `MessageRouter`, rate limiting middleware, `WebhookConfig` (HMAC-SHA256), `WsApprovalChannel`
- **argentor-cli**: CLI binary with `serve`, `skill list`, `compliance report`, `orchestrate` subcommands, config hot-reload via `ConfigWatcher`

#### Advanced Features (3 crates)
- **argentor-memory**: `VectorStore` trait, `FileVectorStore` (JSONL), `InMemoryVectorStore`, `LocalEmbedding` (TF-IDF), `HybridSearcher` (BM25 + embedding + RRF), `Bm25Index`, `QueryExpander`
- **argentor-mcp**: `McpClient` (JSON-RPC 2.0 over stdio), `McpSkill`, `McpProxy` (centralized control plane with logging/metrics/rate limiting), `McpServerManager` (auto-reconnect, health checks), `ToolDiscovery`
- **argentor-builtins**: 13 built-in skills — shell, file_read, file_write, http_fetch, browser, memory_store, memory_search, human_approval, artifact_store, agent_delegate, task_status, docker_sandbox, browser_automation

#### Multi-Agent + Compliance (2 crates)
- **argentor-orchestrator**: `Orchestrator` (plan/execute/synthesize), `TaskQueue` (topological sort), `AgentMonitor` (real-time metrics), `Scheduler` (cron), `SubAgentSpawner`, `AgentProfile` per role
- **argentor-compliance**: GDPR (`ConsentStore`, data subject rights), ISO 27001 (access control, incident response), ISO 42001 (AI inventory, bias monitoring, transparency), DPGA (9 indicators), `ComplianceReport` generation, `JsonReportStore`, `ComplianceHookChain`

#### Infrastructure
- Dockerfile (multi-stage build, non-root user)
- GitHub Actions CI (check, test, clippy, fmt)
- Example config (`argentor.toml`)
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
