# Agentor — Claude Code Instructions

## Project
Agentor is a secure, multi-agent AI framework in Rust with WASM sandboxed plugins.
License: AGPL-3.0-only. Repo: github.com/fboiero/Agentor

## Language
Communicate in Spanish with the user.

## Conventions
- Rust 2021 edition, workspace with 13 crates
- `cargo fmt --all` before committing
- `cargo clippy --workspace` must show 0 warnings
- `cargo test --workspace` must pass all tests (239+)
- No `unwrap()` in production code paths
- Use `AgentorResult<T>` for all fallible operations
- Capability-based permissions: every skill declares required capabilities
- All tool calls are audit-logged via `AuditLog`

## Architecture
- Agentic loop: Prompt → LLM → ToolCall → Execute Skill → Backfill → Repeat
- Multi-agent: Orchestrator-Workers pattern (Spec, Coder, Tester, Reviewer)
- MCP as centralized proxy for tool calls
- HITL (human-in-the-loop) for high-risk operations
- WASM sandbox for plugins (wasmtime + WASI)

## Compliance
- GDPR: consent tracking, data erasure, portability
- ISO 27001: access control, incident response, audit
- ISO 42001: AI inventory, bias monitoring, transparency
- DPGA: 9 indicators (open source, SDGs, privacy, docs, standards, ownership, do-no-harm, interop)

## Crate Map
| Crate | Purpose |
|-------|---------|
| agentor-core | Types, errors, Message, ToolCall, ToolResult |
| agentor-security | Capability, PermissionSet, RateLimiter, AuditLog, TLS |
| agentor-session | Session, FileSessionStore |
| agentor-skills | Skill trait, SkillRegistry, WasmSkillRuntime |
| agentor-agent | AgentRunner, ModelConfig, agentic loop, streaming |
| agentor-channels | Channel trait |
| agentor-gateway | axum WebSocket gateway, middleware |
| agentor-builtins | echo, time, help, memory_store, memory_search |
| agentor-memory | VectorStore, FileVectorStore, LocalEmbedding |
| agentor-mcp | McpClient (JSON-RPC 2.0 stdio), McpSkill |
| agentor-orchestrator | Multi-agent engine, TaskQueue, AgentMonitor |
| agentor-compliance | GDPR, ISO 27001, ISO 42001, DPGA |
| agentor-cli | CLI binary (serve, skill list) |
