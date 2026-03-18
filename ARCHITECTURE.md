# Argentor — Arquitectura y Decisiones

## Resumen

Argentor es un framework de agentes AI autónomos y seguros, escrito en Rust. Inspirado en OpenClaw pero reescrito desde cero para resolver vulnerabilidades críticas de seguridad (RCE, sandbox escape, SSRF, path traversal).

**Licencia**: AGPL-3.0-only

---

## Paso a Paso de Construcción

### Fase 1: Fundación (8 crates core)

1. **argentor-core** — Tipos base: `Message`, `ToolCall`, `ToolResult`, `ArgentorError`
2. **argentor-security** — `Capability` enum, `PermissionSet`, `Sanitizer`, `AuditLog`, `RateLimiter`, TLS/mTLS
3. **argentor-session** — `Session` con historial de mensajes, `FileSessionStore` (JSON)
4. **argentor-skills** — Trait `Skill`, `SkillRegistry` con verificación de permisos, `WasmSkillRuntime` (wasmtime + WASI)
5. **argentor-agent** — `AgentRunner` con agentic loop (Prompt → LLM → ToolCall → Execute → Backfill → Repeat)
6. **argentor-channels** — Trait `Channel` para adaptadores de mensajería
7. **argentor-gateway** — axum WebSocket gateway con `ConnectionManager`, `MessageRouter`, middleware auth/rate-limit
8. **argentor-cli** — Binary con clap: `serve`, `skill list`

### Fase 2: Features Avanzados (3 crates)

9. **argentor-memory** — Vector memory: `VectorStore` trait, `InMemoryVectorStore`, `FileVectorStore` (JSONL), `LocalEmbedding` (bag-of-words FNV 256 dims), `HybridSearcher` (BM25 + embedding), `QueryExpander`
10. **argentor-mcp** — MCP Client: JSON-RPC 2.0 sobre stdio, `McpSkill`, `McpProxy` (control plane centralizado), `McpServerManager` (auto-reconnect, health checks), `ToolDiscovery`
11. **argentor-builtins** — Skills built-in: shell, file_read, file_write, http_fetch, browser, memory_store, memory_search, human_approval, artifact_store, agent_delegate, task_status, docker_sandbox, browser_automation

### Fase 3: Multi-Agent + Compliance (2 crates)

12. **argentor-orchestrator** — Sistema multi-agente:
    - `AgentRole`: Orchestrator, Spec, Coder, Tester, Reviewer
    - `AgentProfile`: config por rol (model, system_prompt, allowed_skills, max_turns)
    - `TaskQueue`: cola con topological sort y detección de ciclos
    - `Orchestrator`: motor plan → execute → synthesize
    - `AgentMonitor`: métricas en tiempo real (turns, tool_calls, tokens, errors)

13. **argentor-compliance** — Módulos de compliance:
    - GDPR: `ConsentStore`, `DataSubjectRequest`, erasure, portability
    - ISO 27001: `AccessControlEvent`, `SecurityIncident`, risk assessment
    - ISO 42001: `AiSystemRecord`, `BiasCheck`, `TransparencyLog`, HITL
    - DPGA: 9 indicadores, `assess_argentor_dpga()`

---

## Arquitectura Multi-Agente

Siguiendo las recomendaciones de Anthropic (2025-2026):

```
                    ┌─────────────────┐
                    │   Orchestrator  │
                    │   (Opus model)  │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
        │   Spec    │ │   Coder   │ │  Tester   │
        │  Worker   │ │  Worker   │ │  Worker   │
        └─────┬─────┘ └─────┬─────┘ └─────┬─────┘
              │              │              │
              └──────────────┼──────────────┘
                             │
                    ┌────────▼────────┐
                    │   MCP Proxy     │  ← Control plane centralizado
                    │  (argentor-mcp)  │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────▼─────┐ ┌─────▼─────┐ ┌─────▼─────┐
        │   Skills  │ │  External │ │  Audit    │
        │   (WASM)  │ │MCP Servers│ │   Log     │
        └───────────┘ └───────────┘ └───────────┘
```

### Flujo de Ejecución

1. **Plan**: Orchestrator analiza input, descompone en subtareas con grafo de dependencias
2. **Execute**: Workers ejecutan en paralelo (respetando dependencias), context window aislado por worker
3. **Synthesize**: Orchestrator recolecta artefactos, valida consistencia, produce resultado final

### Human-in-the-Loop (HITL)

- `TaskStatus::NeedsHumanReview` pausa la ejecución
- Obligatorio para: cambios de seguridad, deployments, eliminación de datos, acciones irreversibles
- Skill `human_approval` solicita aprobación antes de ejecutar

---

## Seguridad vs OpenClaw

| Vulnerabilidad OpenClaw | Defensa Argentor |
|---|---|
| RCE via gateway URL | Origin validation + mTLS |
| Docker sandbox escape | WASM sandbox (wasmtime) |
| SSRF | NetworkAccess capability con allowlist |
| Path traversal | FileRead/FileWrite scoped |
| Auth bypass | API key middleware |
| Log poisoning | Sanitizer strips control chars |
| Supply chain (skills) | WASM isolation + capability + audit |

---

## Compliance

### GDPR
- Consent tracking con `ConsentStore`
- Right to erasure (Art. 17) — `DataSubjectRequest::Erasure`
- Data portability (Art. 20) — export en formato machine-readable
- Purpose limitation — consent por propósito

### ISO 27001
- Access control logging (`AccessControlEvent`)
- Incident response tracking (`SecurityIncident`)
- Risk assessment records
- Audit trail comprehensivo (AuditLog)

### ISO 42001
- AI system inventory (`AiSystemRecord`)
- Bias monitoring (`BiasCheck`)
- Transparency logging (`TransparencyLog`)
- Human oversight para decisiones de alto riesgo

### DPGA (9 indicadores)
1. Open Source — AGPL-3.0-only, repo público
2. SDG Relevance — SDG 9 (Innovation), SDG 16 (Institutions)
3. Open Data — MCP interoperability
4. Privacy — GDPR module
5. Documentation — Docs bilingüe (EN/ES)
6. Open Standards — MCP (AAIF/Linux Foundation), WASM, WIT
7. Ownership — Governance claro
8. Do No Harm — ISO 42001, HITL, bias monitoring
9. Interoperability — MCP + A2A protocols

---

## Features Avanzados (Session 4-5)

### Model Failover
- `FailoverBackend` wrapping múltiples LLM backends con retry automático
- `RetryPolicy` con exponential backoff (configurable base/max)
- Clasificación de errores: 429/5xx → retry, 400 → skip al siguiente backend

### Session Transcripts
- `TranscriptEvent` (5 variantes), `TranscriptEntry`, `TranscriptStore` trait
- `FileTranscriptStore` — JSONL append-only, un archivo por sesión

### Hybrid Search (BM25 + Vector)
- `Bm25Index` con inverted index y scoring BM25 (k1=1.2, b=0.75)
- `HybridSearcher` con Reciprocal Rank Fusion (rrf_k=60)
- `alpha: f32` para balance (0.0=pure BM25, 1.0=pure vector)
- `QueryExpander` trait + `RuleBasedExpander` con 10 grupos de sinónimos

### Webhooks
- `WebhookConfig`, `SessionStrategy`, validación HMAC-SHA256 (constant-time)
- Template rendering con `{{payload}}`

### Plugin System
- `Plugin` trait con lifecycle hooks (on_load, on_unload, on_event)
- `PluginManifest`, `PluginEvent` (6 variantes), `PluginRegistry`

### Docker Sandbox
- `DockerSandbox` + `DockerShellSkill` (feature flag `docker`, bollard)
- `DockerSandboxConfig` con límites de memoria/CPU, timeout, network toggle
- `sanitize_command()` para prevención de inyección

### Sub-agent Spawning
- `SubAgentSpawner` con max_depth (3) y max_children_per_task (5)
- `SpawnRequest`, integrado en orchestrator Task type

### Config Hot-Reload
- `ConfigWatcher` usando `notify::RecommendedWatcher` con debounce 500ms
- `ReloadableConfig`: security, skills, mcp_servers, tool_groups, webhooks

### Scheduler
- `Scheduler` con parsing de expresiones cron y cálculo de next fire time
- `ScheduledJob { name, cron_expression, task_description, enabled }`

### Browser Automation
- `BrowserAutomation` + `BrowserAutomationSkill` (feature flag `browser`, fantoccini)
- Acciones: navigate, screenshot, extract_text, fill_form, click

### Code Hardening (Session 5)
- Clippy estricto a nivel workspace (unwrap_used, expect_used, uninlined_format_args, etc.)
- 0 unwraps en código de producción
- `//!` crate docs + `///` module/item docs en todos los crates
- 52 tests de integración nuevos (builtins, memory, mcp, core, compliance)

---

## Fase 4: Multi-Agent Clusters (Session 6-7)

### MessageBus A2A
- `MessageBus` con send/receive/peek/subscribe/broadcast
- `AgentMessage` con sender, recipient (unicast/broadcast), `MessageType` (TaskAssignment, StatusUpdate, DataShare, Approval, Custom)
- 12 tests

### Replanner
- `Replanner` con análisis de contexto de fallo y selección automática de estrategia
- 6 `RecoveryStrategy`: Retry, Reassign, Decompose, Skip, Abort, Escalate
- `ReplanHistory` para auditoría completa
- Reasignación heurística por rol (Coder→Architect, Tester→Coder, etc.)
- 15 tests

### Budget Tracker
- `BudgetTracker` con `TokenBudget` por agente y `AgentUsage` tracking
- `BudgetStatus`: WithinBudget, Warning (>80%), Exceeded
- Estimación de costos por provider y presupuestos default por rol
- 12 tests

### Persistent Artifacts
- `FileArtifactBackend` implementando `ArtifactBackend` trait
- Layout: `base_dir/artifacts/{key}/content.dat` + `metadata.json`
- Protección contra path traversal, async I/O
- 11 tests

### Collaboration Patterns
- 6 patrones: Pipeline, MapReduce, Debate, Ensemble, Supervisor, Swarm
- Builder API con `PatternConfigBuilder` + validación
- Estimación de costo por patrón
- 22 tests

---

## Fase 5: REST API & Gateway (Session 8-9)

### REST API
- 10 endpoints bajo `/api/v1/`: sessions CRUD, skills list/detail, agent chat, connections, metrics
- `RestApiState` con MessageRouter, ConnectionManager, SessionStore, SkillRegistry
- `ApiError` con HTTP status codes apropiados
- 9 tests

### Channel Bridge
- `ChannelBridge` conectando ChannelManager al MessageRouter
- Afinidad de sesión por canal + sender
- 7 tests

### Parallel Tool Execution
- `execute_parallel()` en SkillRegistry — ejecución concurrente de tool calls
- `execute_with_timeout()` — wrapper con timeout
- 6 tests

---

## Fase 6: Observability & Security Avanzada (Session 10)

### Prometheus Metrics
- `AgentMetricsCollector` con export en formato texto Prometheus
- Métricas: tool_calls, errors, tokens, latency, active_agents, security_events, compliance_checks
- Endpoint `/metrics` en el gateway (scrape-ready para Prometheus/Grafana)
- Endpoint `/api/v1/metrics/prometheus` en REST API
- 14 tests

### Token Counter
- Estimación heurística por provider (Claude 4.5 chars/token, OpenAI 4 chars/token, Gemini 4 chars/token)
- `ModelPricing` con precios default por modelo
- `UsageTracker` para acumulación de uso y costo
- 12 tests

### MCP Server Mode
- `McpServer` exponiendo skills como tools MCP via JSON-RPC 2.0 stdio
- Handlers: initialize, tools/list, tools/call, ping
- 15 tests

### Progressive Tool Disclosure
- Tool groups que filtran skills por rol de agente
- ~98% reducción de tokens en prompts

---

## Fase 7: Infraestructura de Despliegue & Code Generation (Session 11)

### API Scaffold Generator
- Skill `api_scaffold` que genera proyectos completos desde especificaciones JSON
- 3 frameworks: Rust/Axum, Python/FastAPI, Node/Express
- Genera: manifest (Cargo.toml/requirements.txt/package.json), entry point con server setup, route handlers, modelos DB, Dockerfile, README
- Soporte para SQLite y PostgreSQL
- Validación de permisos FileWrite en output_dir
- 19 tests

### IaC Generator
- Skill `iac_generator` para generación de Infrastructure-as-Code
- 6 targets:
  - **Docker**: Dockerfile multi-stage (builder + runtime), non-root user, health check
  - **docker-compose**: app + postgres + redis, volumes, networks
  - **Helm**: Chart.yaml, values.yaml, 7 templates (deployment, service, ingress, hpa, configmap, secrets, _helpers.tpl)
  - **Terraform AWS**: ECS Fargate, ALB, RDS, VPC, security groups, IAM, CloudWatch
  - **Terraform GCP**: Cloud Run, Cloud SQL, VPC, private services access
  - **GitHub Actions**: CI (check, test, clippy, fmt) + Deploy (build, push, deploy, smoke test)
- 14 tests

### Database Abstraction
- `DatabaseSessionStore` implementando `SessionStore` trait
- `DatabaseConfig` con variantes Sqlite y Postgres (API diseñada para drop-in de sqlx)
- Implementación actual: JSON files con directory layout database-like + `Arc<RwLock<>>` index
- Métodos adicionales: `query_by_metadata()`, `cleanup_expired()`, `count()`
- 14 tests

### JWT/OAuth2 Authentication
- `AuthService` con JWT HMAC-SHA256 (implementación manual, sin dependencia jsonwebtoken)
- `AuthConfig` con `AuthMode`: None, ApiKey, Jwt, OAuth2, Combined
- `ApiKeyConfig` con SHA-256 hash (nunca plaintext), permisos, rate limit, expiración
- `OAuth2ProviderConfig` para GitHub, Google, custom
- `JwtClaims` con sub, exp, iat, iss, aud, permissions, agent_roles
- Middleware axum: extrae Bearer/X-Api-Key, valida, inyecta `AuthenticatedUser`
- Comparación constant-time contra timing attacks
- API keys prefijadas con `agtr_` y dominio separado SHA-256
- 25 tests

---

## Verificación

```bash
cargo build --workspace          # Compila 13 crates
cargo test --workspace           # 944 tests
cargo clippy --workspace         # 0 warnings (strict lints enabled)
cargo fmt --all -- --check       # 0 diffs
```

---

## Estadísticas del Proyecto

| Métrica | Valor |
|---------|-------|
| Crates | 13 |
| Líneas de código | ~54,000 |
| Tests | 944 |
| Tests fallidos | 0 |
| Clippy warnings | 0 |
| Archivos .rs | 140+ |
| Skills built-in | 13 (shell, file_read, file_write, http_fetch, browser, git, code_analysis, test_runner, memory_store, memory_search, human_approval, api_scaffold, iac_generator) |
| Roles de agente | 10 |
| Patrones de colaboración | 6 |
| Providers LLM | 12+ |
| Módulos de compliance | 4 (GDPR, ISO 27001, ISO 42001, DPGA) |
| Licencia | AGPL-3.0-only |
