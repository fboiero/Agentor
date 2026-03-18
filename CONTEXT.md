# Argentor — Session Context
> Last updated: 2026-03-18 (session 23)

## Current Goal
Production-grade autonomous AI agent framework — full-stack platform with A2A interop, web dashboard, and OpenTelemetry.

## What's Completed

### Phase 1 — LLM Provider Expansion (5 → 14 providers)
- 9 new providers: Gemini, Ollama, Mistral, XAi, AzureOpenAi, Cerebras, Together, DeepSeek, VLlm
- `GeminiBackend` — full Google Gemini API backend (chat + streaming + tool calling)
- Azure auth handling (api-key header)
- 29 integration tests with wiremock

### Phase 2 — Docker + K8s Deployment
- Improved Dockerfile with security hardening (strip, non-root, HEALTHCHECK)
- `docker-compose.yml` with resource limits, read-only fs, cap_drop ALL
- Helm chart at `deploy/helm/argentor/` (7 templates)

### Phase 3 — Skill Registry Seguro
- `SkillManifest`, `SkillVetter` (5-check pipeline), `SkillIndex`
- Ed25519 signing, constant-time checksum comparison
- 15 tests

### Phase 4 — Agent Identity + Session System
- `AgentPersonality`, `CommunicationStyle`, `ThinkingLevel`
- Session commands, context compaction, TOML loading
- 27 tests

### Phase 5 — Enterprise Security Hardening
- `RbacPolicy`, `PolicyBinding`, `AuditFilter`, `EncryptedStore` (AES-256-GCM)
- PBKDF2-HMAC-SHA256 key derivation, tamper detection
- 40 security tests

### Phase 6 — Benchmarks + Performance Proof
- criterion.rs benchmarks for 3 crates (core, security, skills)

### Phase 7 — Built-in Skills Expansion
- GitSkill (libgit2-based), CodeAnalysisSkill, TestRunnerSkill
- FileArtifactBackend for persistent artifact storage
- 33 new tests

### Phase 8 — Multi-Agent Clusters
- MessageBus A2A (send/receive/broadcast) — 12 tests
- Replanner with 6 recovery strategies — 15 tests
- BudgetTracker with per-agent token budgets — 12 tests
- 6 collaboration patterns (Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm) — 22 tests

### Phase 9 — REST API & Gateway
- 10 REST API endpoints under /api/v1/ — 9 tests
- Channel bridge — 7 tests
- Parallel tool execution in SkillRegistry — 6 tests
- Webhook integration with session strategy

### Phase 10 — Observability & MCP Server
- AgentMetricsCollector with Prometheus export — 14 tests
- Token counter per provider with cost estimation — 12 tests
- MCP Server mode (expose skills as MCP tools) — 15 tests
- Progressive tool disclosure with tool groups

### Phase 11 — Deployment Infrastructure & Code Generation
- API Scaffold Generator (Rust/Axum, Python/FastAPI, Node/Express) — 19 tests
- IaC Generator (Docker, Helm, Terraform AWS/GCP, GitHub Actions) — 14 tests
- DatabaseSessionStore with metadata queries — 14 tests
- JWT/OAuth2 authentication (HMAC-SHA256, API key hashing) — 25 tests
- Prometheus /metrics endpoint integrated into gateway
- Documentation and website update

### Phase 12 — Orchestrator as Deployment Platform
- DeploymentManager (deploy/undeploy/scale/restart, heartbeats, health checks, auto-restart) — 24 tests
- AgentRegistry (register/search/update/delete, catalog import/export, 9 default definitions) — 20 tests
- HealthChecker (liveness/readiness/heartbeat probes, Healthy→Degraded→Unhealthy→Dead transitions, auto-recovery) — 23 tests
- Control Plane REST API (17 endpoints under /api/v1/control-plane/) — 17 tests
- Clippy hardening: fixed redundant closures, uninlined format args, expect_used across workspace

### Phase 13 — Full-Stack Platform (5 Features)

#### A) Gateway + CLI Wiring
- `GatewayServer::build_full()` — mounts control_plane_router and api_router via axum `.merge()`
- CLI subcommands: Deploy (create/list/status/scale/stop/delete/summary), Agents (list/search), Health (summary/unhealthy/status)
- CLI uses reqwest to call control plane HTTP API

#### B) A2A Protocol Crate (argentor-a2a)
- Google Agent-to-Agent interop protocol (JSON-RPC 2.0)
- AgentCard, A2ATask, TaskMessage, TaskArtifact, TaskStatus
- A2AServer with TaskHandler trait + JSON-RPC dispatch (tasks/send, tasks/get, tasks/cancel, tasks/list, agent/card)
- A2AClient (behind `client` feature flag) with get_agent_card, send_task, get_task, cancel_task
- AgentCardBuilder for fluent agent card construction
- 30+ tests

#### C) Web Dashboard
- Single HTML SPA (`dashboard.html`) with dark theme, sidebar navigation
- Sections: Overview, Deployments, Agents, Health, Metrics
- Auto-refresh, status badges, create/scale/delete deployment forms
- Served via `include_str!` at GET /dashboard

#### D) OpenTelemetry
- TelemetryConfig behind `telemetry` feature flag in argentor-core
- OTLP export with init_telemetry()/shutdown_telemetry()
- `#[tracing::instrument]` on key paths (runner, engine, router)

#### E) E2E Deployment Demo
- `demo_deployment.rs` — full lifecycle: registry → deploy → heartbeats → scaling → health → cleanup
- ANSI colors, no API keys needed

### Phase 14 — MCP Proxy Orchestration Hub
- **CredentialVault** — Bóveda segura de credenciales API con rotación, cuotas diarias, resolución por proveedor (least-used), agrupación por provider — 21 tests
- **ProxyOrchestrator** — Coordina múltiples McpProxy instances con routing inteligente (Fixed/RoundRobin/LeastLoaded/PatternBased), circuit breaker (open/half-open/closed), failover automático, métricas agregadas — 24 tests
- **TokenPool** — Pool de tokens por proveedor con rate limiting (sliding window 60s), selección inteligente (MostRemaining/RoundRobin/WeightedRandom/TierPriority), cuotas diarias, tiers (Production/Development/Free/Backup) — 27 tests

### Phase 15 — Integration & Production Wiring
- **McpServerManager wiring** — Vault + TokenPool integrados en manager con builder methods (`with_vault`, `with_token_pool`), resolución de credenciales en `connect_all`, credential_source tracking — 8 tests
- **ProxyOrchestrator en engine** — `with_proxy_orchestrator()` builder en Orchestrator, routing por worker role, métricas al final del pipeline — 5 tests
- **Proxy Management API** — 13 endpoints REST bajo `/api/v1/proxy-management/` para credentials CRUD, token pool, orchestrator metrics, redacción automática de secretos — 12 tests
- **Persistent state** — `PersistentStore` con escritura atómica (tmp+rename), save/load de ControlPlaneSnapshot, CredentialSnapshot, TokenPoolSnapshot — 17 tests
- **E2E demo** — `demo_proxy_orchestration.rs` con 6 fases (vault → pool → orchestrator → routing → circuit breaker → metrics)

### Phase 16 — Full Router Wiring & Integration Tests
- **build_complete()** — método que monta TODOS los routers (dashboard, control plane, REST API, proxy management) + backward compat vía build_full()
- **Gateway E2E router test** — valida que /health, /dashboard, /metrics y todos los /api/v1/* endpoints responden correctamente — 11 tests
- **Channel integration tests** — 16 tests para Channel trait, ChannelManager (send, broadcast, error handling), WebChatChannel lifecycle
- **Approval + persistence tests** — WsApprovalChannel + PersistentStore+ControlPlaneState roundtrip — 14+ tests

### Phase 17 — A2A Gateway Integration & Streaming
- **A2A router in gateway** — `build_complete()` ahora acepta `a2a: Option<Arc<A2AServerState>>`, monta `/.well-known/agent.json` y `/a2a` endpoints en el gateway
- **Streaming A2A (SSE)** — `TaskStreamEvent` enum, `StreamingTaskHandler` trait, endpoint `POST /a2a/stream` con Server-Sent Events para tasks/sendSubscribe, fallback a single event para non-streaming handlers — 3 tests
- **CLI `a2a` subcommand** — 5 subcomandos: discover (agent card), send (task), status, cancel, list — usa A2AClient para discover/send/status/cancel, reqwest directo para list
- **Gateway A2A integration tests** — EchoHandler + 4 tests validando agent card, tasks/send, agent/card via JSON-RPC, method not found
- **Module wiring fixes** — Restaurados módulos faltantes en argentor-mcp (credential_vault, proxy_orchestrator, token_pool), argentor-orchestrator (deployment, health, registry), argentor-gateway (auth, control_plane, dashboard, persistence, proxy_management)

### Phase 18 — Intelligent Agent Core (5 Modules)
- **ReAct Engine** (`react.rs`) — Structured Think→Act→Observe→Reflect reasoning cycle. `ReActEngine` with `ReActStep`, `ReActAction`, `ReActTrace`, `ReActOutcome`. Configurable max steps, reflection interval, confidence threshold. Parse-based step extraction and trace summarization — 14 tests
- **Smart Tool Selector** (`tool_selector.rs`) — TF-IDF keyword similarity + historical success rate tracking. `ToolSelector` with `SelectionStrategy` (All/KeywordMatch/Relevance/Adaptive). Records success/failure per tool, auto-adapts selection based on usage patterns — 17 tests
- **Self-Evaluation Engine** (`evaluator.rs`) — Heuristic scoring on 4 dimensions: relevance, consistency, completeness, clarity. `ResponseEvaluator` with `QualityScore`, `EvaluationResult`, `EvaluationAction` (Accept/Refine/Reject). Configurable thresholds and max refinement iterations — 22 tests
- **Cost-Aware Model Router** (`model_router.rs`) — Multi-tier LLM selection with `ModelTier` (Fast/Balanced/Powerful), `TaskComplexity` estimation (7 heuristic factors), `RoutingStrategy` (CostOptimized/QualityOptimized/Balanced/Tiered). Budget tracking, Claude preset helper — 17 tests
- **Adaptive Memory** (`adaptive_memory.rs`) — Cross-session memory with `MemoryKind` (Fact/Preference/ToolPattern/Summary/ErrorResolution), keyword-based recall with importance decay over time. Auto-extraction of facts and error resolutions, configurable pruning — 22 tests

### Phase 19 — Code Intelligence Vertical (6 Modules)
- **CodeGraph** (`code_graph.rs`) — Lightweight AST-like code analysis: regex-based parsing for Rust/Python/TypeScript/Go. Symbol table, dependency graph, call graph, impact analysis, relevant context builder. `CodeGraph`, `CodeSymbol`, `ImpactAnalysis`, `CodeContext` — 23 tests
- **DiffEngine** (`diff_engine.rs`) — Precise diff generation via LCS algorithm, application, validation. Unified diff format serialization/parsing. Multi-file `DiffPlan`. Token estimation for LLM budgeting. `DiffEngine`, `FileDiff`, `DiffHunk`, `DiffPlan` — 22 tests
- **TestOracle** (`test_oracle.rs`) — Test output parsing for cargo test, pytest, jest, go test. Error classification (11 types), fix strategy suggestion, TDD cycle state machine (Red→Green→Refactor). `TestOracle`, `FailureAnalysis`, `TddCycle` — 24 tests
- **CodePlanner** (`code_planner.rs`) — Implementation planning: feature, bugfix, refactor, add-tests plans with dependency-ordered steps, role assignment (8 roles), risk assessment, DAG validation (Kahn's algorithm), parallelizable step detection. `CodePlanner`, `ImplementationPlan`, `PlanStep` — 24 tests
- **ReviewEngine** (`review_engine.rs`) — Multi-dimensional code review with 25+ rules across 7 dimensions (Security/Performance/Style/Correctness/ErrorHandling/Documentation/TestCoverage). SEC001-SEC008, PERF001-PERF005, STY001-STY006, ERR001-ERR005, DOC001-DOC003, COR001-COR003. Weighted scoring, verdict system (Approve/RequestChanges/Block), markdown report — 29 tests
- **DevTeam** (`dev_team.rs` in argentor-orchestrator) — Pre-configured development teams (FullStack/Minimal/Security) with 8 workflow templates (ImplementFeature/FixBug/Refactor/AddTests/SecurityAudit/CodeReview/Optimize/WriteDocumentation). Quality gates, role-based model recommendations, system prompts per role, handoff protocols — 23 tests

### Phase 20 — Production Hardening & Runtime Intelligence (6 Modules)
- **CorrelationContext** (`argentor-core/src/correlation.rs`) — Distributed trace context propagation with W3C traceparent format, span hierarchy, baggage propagation, TraceCollector with capacity limits. `CorrelationContext`, `SpanContext`, `ContextPropagator`, `TraceCollector` — 24 tests
- **ErrorAggregator** (`argentor-core/src/error_aggregator.rs`) — Error fingerprinting with message normalization, deduplication, severity tracking, trend analysis with time buckets, top-N ranking. `ErrorAggregator`, `ErrorGroup`, `ErrorFingerprint`, `ErrorTrend` — 24 tests
- **ResponseCache** (`argentor-agent/src/response_cache.rs`) — In-memory LRU cache for LLM responses with TTL expiration, hit/miss statistics, token savings tracking, eviction metrics. `ResponseCache`, `CacheKey`, `CacheStats` — 21 tests
- **StructuredOutputParser** (`argentor-agent/src/structured_output.rs`) — JSON schema-based extraction from LLM text (markdown code blocks, raw JSON, key-value pairs, lists). Auto-pattern fallback, field validation, default values. `StructuredOutputParser`, `OutputSchema`, `ExtractedOutput` — 24 tests
- **ShutdownManager** (`argentor-gateway/src/graceful_shutdown.rs`) — Graceful shutdown with 4 ordered phases (PreDrain→Drain→Cleanup→Final), hook registration, timeout enforcement, shutdown report. `ShutdownManager`, `ShutdownHook`, `ShutdownPhase`, `ShutdownReport` — 16 tests
- **CLI REPL** (`argentor-cli/src/repl.rs`) — Interactive agent debugging shell with 12 commands (help, skills, sessions, config, metrics, health, set, get, history, clear, version, exit). Command parsing, context management, history. `ReplCommand`, `ReplContext`, `ReplOutput` — 27 tests

### Phase 21 — Advanced Observability & Monitoring (6 Modules)
- **AlertEngine** (`argentor-security/src/alert_engine.rs`) — Alert rules with 8 condition types (GT/LT/GTE/LTE/EQ/OutsideRange/InsideRange/RateExceeds), severity levels, cooldown suppression, batch evaluation, acknowledge workflow. `AlertEngine`, `AlertRule`, `AlertCondition`, `Alert` — 24 tests
- **SlaTracker** (`argentor-security/src/sla_tracker.rs`) — SLA compliance tracking with uptime percentage, response time monitoring, incident lifecycle (start→close), compliance report generation. `SlaTracker`, `SlaDefinition`, `SlaStatus`, `Incident` — 22 tests
- **CircuitBreaker** (`argentor-agent/src/circuit_breaker.rs`) — Per-provider circuit breaker state machine (Closed→Open→HalfOpen), configurable failure/success thresholds, recovery timeout, registry for multi-provider management. `CircuitBreaker`, `CircuitBreakerRegistry`, `CircuitConfig` — 22 tests
- **MetricsExporter** (`argentor-core/src/metrics_export.rs`) — Multi-format export: JSON, CSV, OpenMetrics (Prometheus), InfluxDB Line Protocol. Counter/Gauge/Histogram metric types, label support. `MetricsExporter`, `MetricPoint`, `ExportFormat` — 20 tests
- **RateLimitHeaders** (`argentor-gateway/src/rate_limit_headers.rs`) — X-RateLimit-* and IETF draft RateLimit headers, Retry-After, utilization tracking, round-trip parsing. `RateLimitHeaders`, `RateLimitInfo` — 14 tests

### Phase 22 — Developer Experience & Ecosystem (5 Modules)
- **OpenApiGenerator** (`argentor-gateway/src/openapi.rs`) — OpenAPI 3.0.3 spec generation with endpoint definitions, parameters, responses, tags, auth schemes. Argentor default API spec with 7+ endpoints. `OpenApiGenerator`, `ApiEndpoint`, `ApiParameter` — 20 tests
- **EventBus** (`argentor-core/src/event_bus.rs`) — In-process pub/sub event system with topic-based routing, subscriber management, event history, statistics. `EventBus`, `Event`, `SubscriptionId`, `EventBusStats` — 21 tests
- **DebugRecorder** (`argentor-agent/src/debug_recorder.rs`) — Step-by-step reasoning trace capture with 11 step types, token accumulation, metadata, trace summary. Disabled mode for production. `DebugRecorder`, `DebugStep`, `DebugTrace`, `TraceSummary` — 20 tests
- **BatchProcessor** (`argentor-agent/src/batch_processor.rs`) — Batch request queuing with priority sorting, configurable batch size/concurrency, continue-on-error mode, per-batch statistics. `BatchProcessor`, `BatchRequest`, `BatchResult`, `BatchConfig` — 20 tests

### Phase 23 — Integration Sprint (Wiring Modules into Core Paths)
- **AgentRunner integration** — ResponseCache (LRU with TTL before LLM calls), CircuitBreaker (per-provider with auto-registration), DebugRecorder (step-by-step traces for Input/LlmCall/LlmResponse/CacheHit/ToolCall/ToolResult/Error/Output). Builder methods: `with_cache()`, `with_circuit_breaker()`, `with_debug_recorder()`. Accessors: `cache_stats()`, `circuit_breakers()`, `debug_recorder()`.
- **Gateway Server integration** — Added `/openapi.json` endpoint serving auto-generated OpenAPI 3.0.3 spec via `argentor_openapi_spec()`.
- **Orchestrator integration** — EventBus emitting `orchestrator.task.started`, `orchestrator.task.completed`, `orchestrator.task.failed` events with structured JSON payloads. ErrorAggregator collecting worker failures with LlmProvider category and role/task_id tracking. Accessors: `event_bus()`, `error_aggregator()`.
- **LlmBackend trait** — Added `provider_name()` method with default `"unknown"`. Implemented for all 5 backends: claude, openai, gemini, claude-code, failover.

## Build Health
- `cargo test --workspace` — **1833 tests passing**, 0 failures
- `cargo check --workspace` — 0 errors
- `cargo clippy --workspace` — 0 errors
- ~96,000+ LOC across 14 crates

## Key Integration Points (Phase 23)
| Component | Integrated Modules | How |
|-----------|-------------------|-----|
| AgentRunner | ResponseCache, CircuitBreaker, DebugRecorder | Builder pattern, pre/post LLM call hooks |
| GatewayServer | OpenApiGenerator | `/openapi.json` route |
| Orchestrator | EventBus, ErrorAggregator | Task lifecycle events, error fingerprinting |
| LlmBackend trait | CircuitBreaker, ResponseCache | `provider_name()` for keying |
