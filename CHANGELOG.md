# Changelog

All notable changes to Argentor are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

---

## [Unreleased]

### Added

#### Phase 1 — LLM Provider Expansion (5 → 14 providers)
- 9 new LLM backends: Gemini (full Google API with tool calling), Ollama, Mistral, XAi, AzureOpenAI, Cerebras, Together, DeepSeek, VLlm
- `GeminiBackend` with chat + streaming + tool calling support
- Azure auth handling (api-key header)
- 29 integration tests with wiremock

#### Phase 2 — Docker + Kubernetes Deployment
- Dockerfile with security hardening (strip, non-root, HEALTHCHECK)
- `docker-compose.yml` with resource limits, read-only fs, cap_drop ALL
- Helm chart at `deploy/helm/argentor/` (7 templates: Deployment, Service, Ingress, HPA, PVC, ServiceAccount, helpers)

#### Phase 3 — Skill Registry Security
- `SkillManifest` with metadata validation
- `SkillVetter` — 5-check pipeline (checksum, size, signature, static analysis, capability audit)
- `SkillIndex` for searchable skill catalogs
- Ed25519 signing with constant-time checksum comparison
- 15 tests

#### Phase 4 — Agent Identity & Session System
- `AgentPersonality` with name, role, instructions, style, constraints, expertise
- `CommunicationStyle` and `ThinkingLevel` enums
- Session commands and context compaction
- TOML-based personality loading
- 27 tests

#### Phase 5 — Enterprise Security Hardening
- `RbacPolicy` with Admin, Operator, Viewer, Custom roles
- `PolicyBinding` for per-role permission assignment
- `AuditFilter` with structured audit log querying
- `EncryptedStore` (AES-256-GCM) with PBKDF2-HMAC-SHA256 key derivation
- Tamper detection for encrypted payloads
- 40 security tests

#### Phase 6 — Benchmarks & Performance Proof
- criterion.rs benchmarks for 3 crates (core, security, skills)
- Performance baselines for Message creation, PermissionSet operations, SkillRegistry lookups

#### Phase 7 — Built-in Skills Expansion
- **GitSkill**: libgit2-based git operations (status, diff, log, add, commit, branch)
- **CodeAnalysisSkill**: Language-aware code analysis (function extraction, complexity, dependencies)
- **TestRunnerSkill**: Multi-language test runner with result parsing (Rust, Python, Node, Go)
- **FileArtifactBackend**: File-system persistent artifact storage with path traversal protection
- 33 new tests

#### Phase 8 — Multi-Agent Clusters
- **MessageBus**: Inter-agent A2A communication (send, receive, peek, subscribe, broadcast) — 12 tests
- **Replanner**: Dynamic re-planning with 6 recovery strategies (Retry, Reassign, Decompose, Skip, Abort, Escalate) — 15 tests
- **BudgetTracker**: Per-agent token budget tracking with cost estimation and warning thresholds — 12 tests
- **Collaboration Patterns**: 6 multi-agent patterns (Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm) with builder API — 22 tests

#### Phase 9 — REST API & Gateway
- **REST API**: 10 endpoints under `/api/v1/` (sessions CRUD, skills, agent chat, connections, metrics) — 9 tests
- **Channel Bridge**: ChannelManager-to-MessageRouter bridge with session affinity — 7 tests
- **Parallel Tool Execution**: `execute_parallel()` and `execute_with_timeout()` in SkillRegistry — 6 tests
- **Webhook Integration**: Inbound/outbound webhooks with HMAC-SHA256 validation and session strategy

#### Phase 10 — Observability & MCP Server
- **AgentMetricsCollector**: Prometheus text exposition format, per-agent/tool counters — 14 tests
- **Token Counter**: Per-provider estimation with cost calculation (`ModelPricing`, `UsageTracker`) — 12 tests
- **MCP Server Mode**: Expose skills as MCP tools via JSON-RPC 2.0 stdio protocol — 15 tests
- **Progressive Tool Disclosure**: Tool groups filter skills per agent role (~98% token reduction)

#### Phase 11 — Deployment Infrastructure & Code Generation
- **API Scaffold Generator**: Generate complete projects (Rust/Axum, Python/FastAPI, Node/Express) from JSON specs — 19 tests
- **IaC Generator**: Docker, docker-compose, Helm charts, Terraform AWS/GCP, GitHub Actions — 14 tests
- **DatabaseSessionStore**: Database-backed sessions with metadata queries and expiration cleanup — 14 tests
- **JWT/OAuth2 Authentication**: HMAC-SHA256 JWT, API key hashing, OAuth2 provider config, axum middleware — 25 tests
- **Prometheus Gateway Integration**: `/metrics` endpoint on gateway, `/api/v1/metrics/prometheus` on REST API

#### Phase 12 — Orchestrator as Deployment Platform
- **DeploymentManager**: Deploy/undeploy/scale/restart agents with replicas, heartbeats, auto-restart — 24 tests
- **AgentRegistry**: Thread-safe registration with name uniqueness, catalog import/export, 9 default role definitions — 20 tests
- **HealthChecker**: Liveness/readiness/heartbeat probes, state machine (Healthy→Degraded→Unhealthy→Dead), auto-recovery — 23 tests
- **Control Plane REST API**: 17 endpoints under `/api/v1/control-plane/` for managing deployments, agents, and health — 17 tests
- Clippy hardening: fixed redundant closures, uninlined format args, expect_used across workspace

#### Phase 13 — Full-Stack Platform
- **Gateway Wiring**: `GatewayServer::build_full()` mounts control-plane and REST API routers via axum `.merge()`
- **A2A Protocol Crate** (`argentor-a2a`): Google Agent-to-Agent interop via JSON-RPC 2.0, AgentCard, A2AServer/A2AClient, TaskHandler trait — 30+ tests
- **Web Dashboard**: Dark-themed SPA at `/dashboard` with deployment management, agent catalog, health monitoring — embedded HTML via `include_str!`
- **OpenTelemetry**: `TelemetryConfig` behind `telemetry` feature flag, OTLP export, `#[tracing::instrument]` on key paths
- **CLI Subcommands**: `deploy` (create/list/status/scale/stop/delete), `agents` (list/search), `health` (summary/unhealthy/status)
- **E2E Deployment Demo**: `demo_deployment.rs` — full lifecycle from registry to cleanup

#### Phase 14 — MCP Proxy Orchestration Hub
- **CredentialVault**: Centralized API token storage with usage tracking, expiry detection, provider grouping, rotation, quota enforcement — 21 tests
- **ProxyOrchestrator**: Multi-proxy coordination with routing rules (Fixed/RoundRobin/LeastLoaded/PatternBased), circuit breaker, failover, aggregated metrics — 24 tests
- **TokenPool**: Per-provider token pool with sliding-window rate limiting, daily quotas, tier priority (Production/Development/Free/Backup) — 27 tests

#### Phase 15 — Integration & Production Wiring
- **McpServerManager integration**: CredentialVault and TokenPool wired via builder methods, credentials resolved before connections — 8 tests
- **ProxyOrchestrator in Orchestrator**: Intelligent proxy routing per worker role, metrics reporting — 5 tests
- **Proxy Management API**: 13 REST endpoints under `/api/v1/proxy-management/` for credentials, tokens, orchestrator management with automatic secret redaction — 12 tests
- **PersistentStore**: Atomic JSON file persistence (tmp+rename) for control plane, credential, and token pool snapshots — 17 tests
- **E2E Proxy Demo**: `demo_proxy_orchestration.rs` with 6 phases (vault → pool → orchestrator → routing → circuit breaker → metrics)

#### Phase 16 — Full Router Wiring & Integration Tests
- **`build_complete()`**: Mounts ALL routers (dashboard, control plane, REST API, proxy management) + backward compat via `build_full()`
- **Gateway E2E router test**: Validates /health, /dashboard, /metrics and all /api/v1/* endpoints — 11 tests
- **Channel integration tests**: Channel trait, ChannelManager (send, broadcast, error handling), WebChatChannel lifecycle — 16 tests
- **Approval + persistence tests**: WsApprovalChannel + PersistentStore+ControlPlaneState roundtrip — 14+ tests

#### Phase 17 — A2A Gateway Integration & Streaming
- **A2A router in gateway**: `build_complete()` accepts `a2a: Option<Arc<A2AServerState>>`, mounts `/.well-known/agent.json` and `/a2a`
- **Streaming A2A (SSE)**: `TaskStreamEvent`, `StreamingTaskHandler` trait, `POST /a2a/stream` with Server-Sent Events — 3 tests
- **CLI `a2a` subcommand**: 5 subcommands (discover, send, status, cancel, list) via A2AClient
- **Gateway A2A integration tests**: EchoHandler + 4 tests for agent card, tasks/send, JSON-RPC dispatch
- **Module wiring fixes**: Restored missing modules in argentor-mcp, argentor-orchestrator, argentor-gateway

#### Phase 18 — Intelligent Agent Core (5 Modules)
- **ReAct Engine** (`react.rs`): Structured Think→Act→Observe→Reflect cycle, configurable max steps, reflection interval, confidence threshold — 14 tests
- **Smart Tool Selector** (`tool_selector.rs`): TF-IDF keyword similarity + historical success rate tracking, adaptive selection — 17 tests
- **Self-Evaluation Engine** (`evaluator.rs`): Heuristic scoring on 4 dimensions (relevance, consistency, completeness, clarity), Accept/Refine/Reject actions — 22 tests
- **Cost-Aware Model Router** (`model_router.rs`): Multi-tier LLM selection (Fast/Balanced/Powerful), task complexity estimation, budget tracking — 17 tests
- **Adaptive Memory** (`adaptive_memory.rs`): Cross-session memory with importance decay, keyword recall, auto-extraction of facts and error resolutions — 22 tests

#### Phase 19 — Code Intelligence Vertical (6 Modules)
- **CodeGraph** (`code_graph.rs`): Regex-based AST for Rust/Python/TypeScript/Go, symbol table, dependency graph, call graph, impact analysis — 23 tests
- **DiffEngine** (`diff_engine.rs`): LCS-based diff generation, unified diff format, multi-file `DiffPlan`, token estimation — 22 tests
- **TestOracle** (`test_oracle.rs`): Parsing cargo test/pytest/jest/go test, error classification (11 types), fix strategy, TDD cycle state machine — 24 tests
- **CodePlanner** (`code_planner.rs`): Implementation planning with DAG ordering (Kahn's algorithm), 8 agent roles, risk assessment, parallelizable steps — 24 tests
- **ReviewEngine** (`review_engine.rs`): 25+ rules across 7 dimensions (Security/Performance/Style/Correctness/ErrorHandling/Documentation/TestCoverage), weighted scoring, verdict system — 29 tests
- **DevTeam** (`dev_team.rs`): Pre-configured teams (FullStack/Minimal/Security) with 8 workflow templates, quality gates, handoff protocols — 23 tests

#### Phase 20 — Production Hardening & Runtime Intelligence (6 Modules)
- **CorrelationContext** (`correlation.rs`): W3C traceparent propagation, span hierarchy (parent/child), baggage items, `TraceCollector` with capacity limits — 24 tests
- **ErrorAggregator** (`error_aggregator.rs`): FNV-1a fingerprinting with message normalization (numbers→`<N>`), deduplication, severity escalation, trend analysis with time buckets — 24 tests
- **ResponseCache** (`response_cache.rs`): Custom in-memory LRU cache with TTL expiration, hit/miss statistics, token savings tracking, eviction metrics — 21 tests
- **StructuredOutputParser** (`structured_output.rs`): JSON schema-based extraction from LLM text (markdown code blocks, raw JSON, key-value pairs, lists), auto-pattern fallback, field validation with defaults — 24 tests
- **ShutdownManager** (`graceful_shutdown.rs`): 4-phase ordered shutdown (PreDrain→Drain→Cleanup→Final), hook registration, timeout enforcement, per-hook timing report — 16 tests
- **CLI REPL** (`repl.rs`): Interactive agent debugging shell with 12 commands (help, skills, sessions, config, metrics, health, set, get, history, clear, version, exit) — 27 tests

#### Phase 21 — Advanced Observability & Monitoring (5 Modules)
- **AlertEngine** (`alert_engine.rs`): 8 alert condition types (GT/LT/GTE/LTE/EQ/OutsideRange/InsideRange/RateExceeds), severity levels (Info→Warning→Critical→Emergency), cooldown suppression, batch evaluation, acknowledge workflow — 24 tests
- **SlaTracker** (`sla_tracker.rs`): SLA compliance tracking with uptime percentage, response time monitoring, incident lifecycle (start→close on recovery), compliance report generation across all SLAs — 22 tests
- **CircuitBreaker** (`circuit_breaker.rs`): Per-provider Closed→Open→HalfOpen state machine, configurable failure/success thresholds, recovery timeout, `CircuitBreakerRegistry` for multi-provider management — 22 tests
- **MetricsExporter** (`metrics_export.rs`): Multi-format export — JSON, CSV, OpenMetrics (Prometheus text), InfluxDB Line Protocol. Counter/Gauge/Histogram types, label support — 20 tests
- **RateLimitHeaders** (`rate_limit_headers.rs`): X-RateLimit-Limit/Remaining/Reset + IETF draft RateLimit/RateLimit-Policy headers, Retry-After, utilization tracking, round-trip parsing — 14 tests

#### Phase 22 — Developer Experience & Ecosystem (4 Modules)
- **OpenApiGenerator** (`openapi.rs`): OpenAPI 3.0.3 spec generation with endpoint definitions, parameters (path/query/header), responses, tags, security schemes (Bearer JWT, API Key). Argentor default API spec with 7+ endpoints — 20 tests
- **EventBus** (`event_bus.rs`): In-process pub/sub event system with topic-based routing, subscriber management, event history with capacity limits, per-topic statistics — 21 tests
- **DebugRecorder** (`debug_recorder.rs`): Step-by-step reasoning trace capture with 11 step types (Input, Thinking, Decision, ToolCall, ToolResult, LlmCall, LlmResponse, CacheHit, Error, Output, Custom), token accumulation, metadata, trace summary. Disabled mode for production — 20 tests
- **BatchProcessor** (`batch_processor.rs`): Batch request queuing with priority sorting, configurable batch size/concurrency, continue-on-error mode, per-batch statistics and success rate — 20 tests

#### Phase 23 — Integration Sprint (Wiring into Core Execution Paths)
- **AgentRunner integration**: ResponseCache (check cache before LLM call, store on final response), CircuitBreaker (check provider health before call, record success/failure), DebugRecorder (record every step: Input, LlmCall, LlmResponse, CacheHit, ToolCall, ToolResult, Error, Output). Builder methods: `with_cache()`, `with_circuit_breaker()`, `with_debug_recorder()`. Accessors: `cache_stats()`, `circuit_breakers()`, `debug_recorder()`
- **Gateway Server integration**: Added `/openapi.json` endpoint serving auto-generated OpenAPI 3.0.3 spec
- **Orchestrator integration**: EventBus emitting `orchestrator.task.started`, `orchestrator.task.completed`, `orchestrator.task.failed` events with structured JSON payloads (task_id, role, duration_ms, error). ErrorAggregator collecting worker failures with LlmProvider category and role/task_id correlation. Accessors: `event_bus()`, `error_aggregator()`
- **LlmBackend trait**: Added `provider_name()` method with default `"unknown"`. Implemented for all 5 backends: `claude`, `openai`, `gemini`, `claude-code`, `failover`

### Changed
- **README.md**: Updated badges (1833 tests, 96K+ LOC), added Production Hardening and Code Intelligence sections, updated crate descriptions
- **GatewayServer**: `build_complete()` now mounts `/openapi.json` route
- **Orchestrator**: Constructor initializes EventBus and ErrorAggregator, WorkerContext carries both for parallel tasks
- **AgentRunner**: Constructor initializes disabled DebugRecorder and default CircuitBreakerRegistry
- **LlmBackend trait**: Added `provider_name()` method (non-breaking, has default implementation)

### Fixed
- ResponseCache miss counter not incrementing on key-not-found (early return before `self.misses += 1`)
- ErrorAggregator fingerprint normalization causing false grouping of `format!("error {i}")` patterns in tests (numbers normalized to `<N>`)
- AlertEngine borrow checker conflict in `evaluate()` — split into two-pass (collect matching rules, then mutate state)
- Incremental build cache corruption (resolved with `cargo clean`)

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
