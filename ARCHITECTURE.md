# Agentor — Arquitectura y Decisiones

## Resumen

Agentor es un framework de agentes AI autónomos y seguros, escrito en Rust. Inspirado en OpenClaw pero reescrito desde cero para resolver vulnerabilidades críticas de seguridad (RCE, sandbox escape, SSRF, path traversal).

**Licencia**: AGPL-3.0-only

---

## Paso a Paso de Construcción

### Fase 1: Fundación (8 crates core)

1. **agentor-core** — Tipos base: `Message`, `ToolCall`, `ToolResult`, `AgentorError`
2. **agentor-security** — `Capability` enum, `PermissionSet`, `Sanitizer`, `AuditLog`, `RateLimiter`, TLS/mTLS
3. **agentor-session** — `Session` con historial de mensajes, `FileSessionStore` (JSON)
4. **agentor-skills** — Trait `Skill`, `SkillRegistry` con verificación de permisos, `WasmSkillRuntime` (wasmtime + WASI)
5. **agentor-agent** — `AgentRunner` con agentic loop (Prompt → LLM → ToolCall → Execute → Backfill → Repeat)
6. **agentor-channels** — Trait `Channel` para adaptadores de mensajería
7. **agentor-gateway** — axum WebSocket gateway con `ConnectionManager`, `MessageRouter`, middleware auth/rate-limit
8. **agentor-cli** — Binary con clap: `serve`, `skill list`

### Fase 2: Features Avanzados (3 crates)

9. **agentor-memory** — Vector memory: `VectorStore` trait, `InMemoryVectorStore`, `FileVectorStore` (JSONL), `LocalEmbedding` (bag-of-words FNV 256 dims)
10. **agentor-mcp** — MCP Client: JSON-RPC 2.0 sobre stdio, `McpSkill` convierte MCP tools a skills automáticamente
11. **agentor-builtins** — Skills built-in: echo, time, help, memory_store, memory_search

### Fase 3: Multi-Agent + Compliance (2 crates)

12. **agentor-orchestrator** — Sistema multi-agente:
    - `AgentRole`: Orchestrator, Spec, Coder, Tester, Reviewer
    - `AgentProfile`: config por rol (model, system_prompt, allowed_skills, max_turns)
    - `TaskQueue`: cola con topological sort y detección de ciclos
    - `Orchestrator`: motor plan → execute → synthesize
    - `AgentMonitor`: métricas en tiempo real (turns, tool_calls, tokens, errors)

13. **agentor-compliance** — Módulos de compliance:
    - GDPR: `ConsentStore`, `DataSubjectRequest`, erasure, portability
    - ISO 27001: `AccessControlEvent`, `SecurityIncident`, risk assessment
    - ISO 42001: `AiSystemRecord`, `BiasCheck`, `TransparencyLog`, HITL
    - DPGA: 9 indicadores, `assess_agentor_dpga()`

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
                    │  (agentor-mcp)  │
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

| Vulnerabilidad OpenClaw | Defensa Agentor |
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

## Verificación

```bash
cargo build --workspace          # Compila 13 crates
cargo test --workspace           # 239+ tests
cargo clippy --workspace         # 0 warnings
cargo fmt --all -- --check       # 0 diffs
```
