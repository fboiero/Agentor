# Agentor

**Secure multi-agent AI framework in Rust with WASM sandboxed plugins, MCP integration, and compliance modules.**

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/Tests-483-green.svg)]()

---

## What is Agentor?

Agentor is an autonomous AI agent framework designed for **security**, **compliance**, and **multi-agent orchestration**. Built from scratch in Rust, it addresses critical vulnerabilities found in existing frameworks (RCE, sandbox escape, SSRF, path traversal) while providing a complete multi-agent system following [Anthropic's recommended patterns](https://docs.anthropic.com/en/docs/build-with-claude/agentic-systems).

### Key Features

- **WASM Sandboxed Plugins** — Skills run in WebAssembly (wasmtime + WASI) with capability-based permissions
- **Multi-Agent Orchestration** — Orchestrator-Workers pattern with specialized agents (Spec, Coder, Tester, Reviewer)
- **MCP Centralized Proxy** — All tool calls routed through a central control plane with logging, metrics, and progressive tool disclosure
- **Human-in-the-Loop (HITL)** — Mandatory approval for high-risk operations
- **Compliance Built-in** — GDPR, ISO 27001, ISO 42001, DPGA modules
- **Capability-based Security** — Fine-grained permissions per skill (FileRead, NetworkAccess, etc.)
- **Vector Memory** — Local embedding-based memory with JSONL persistence
- **Multi-Provider LLM** — Claude, OpenAI, OpenRouter (200+ models)
- **Failover** — Automatic LLM backend failover across providers
- **Transcripts** — Full conversation transcript capture and replay
- **Hybrid Search** — BM25 + embedding similarity for semantic memory retrieval
- **Webhooks** — Inbound/outbound webhook support in the gateway
- **Docker Sandbox** — Run untrusted code in isolated Docker containers
- **Browser Automation** — Headless browser skill for web scraping and interaction
- **Scheduler** — Cron-like task scheduling for recurring agent jobs
- **Query Expansion** — Automatic query rewriting for improved search recall
- **Config Hot-Reload** — Live configuration updates without server restart
- **Markdown Skills** — Define skills declaratively in Markdown files

---

## Architecture

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
                    │   MCP Proxy     │  ← Centralized control plane
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

### Crates

| Crate | Description |
|-------|-------------|
| `agentor-core` | Core types, errors, and message definitions |
| `agentor-security` | Capabilities, permissions, rate limiting, audit, TLS |
| `agentor-session` | Session management and persistence |
| `agentor-skills` | Skill system with WASM sandbox, plugins, and registry |
| `agentor-agent` | Agent runner, LLM backends, failover, streaming |
| `agentor-channels` | Multi-platform communication channels |
| `agentor-gateway` | HTTP/WebSocket gateway with auth and webhooks |
| `agentor-builtins` | Built-in skills (shell, file I/O, HTTP, memory, browser, Docker) |
| `agentor-memory` | Semantic memory with hybrid search and query expansion |
| `agentor-mcp` | Model Context Protocol client, proxy, and discovery |
| `agentor-orchestrator` | Multi-agent orchestration, scheduling, monitoring |
| `agentor-compliance` | GDPR, ISO 27001, ISO 42001, DPGA compliance |
| `agentor-cli` | CLI binary (serve, skill list) |

---

## Quick Start

### Prerequisites

- Rust 1.75+ (`rustup update stable`)
- An API key from Claude, OpenAI, or OpenRouter

### Build

```bash
git clone https://github.com/fboiero/Agentor.git
cd Agentor
cargo build --workspace
```

### Configure

Copy and edit the configuration file:

```bash
cp agentor.toml my-config.toml
# Edit my-config.toml with your API key and preferences
```

Example minimal configuration:

```toml
[model]
provider = "claude"
model_id = "claude-sonnet-4-20250514"
api_key = "${ANTHROPIC_API_KEY}"
temperature = 0.7
max_tokens = 4096
max_turns = 20

[server]
host = "0.0.0.0"
port = 3000
```

### Run

```bash
# Start the gateway server
cargo run --bin agentor -- serve

# List available skills
cargo run --bin agentor -- skill list

# Generate compliance report
cargo run --bin agentor -- compliance report
```

### Test

```bash
cargo test --workspace           # Run all 483 tests
cargo clippy --workspace         # 0 warnings
cargo fmt --all -- --check       # Check formatting
```

---

## Security Model

Agentor uses defense-in-depth with capability-based security:

| Threat | Defense |
|--------|---------|
| RCE via gateway | Origin validation + mTLS |
| Sandbox escape | WASM isolation (wasmtime) |
| SSRF | NetworkAccess capability with allowlist |
| Path traversal | FileRead/FileWrite scoped to directories |
| Auth bypass | API key middleware |
| Log poisoning | Sanitizer strips control characters |
| Supply chain (plugins) | WASM isolation + capability audit |

### Capabilities

Each skill declares the capabilities it needs. The permission system validates before execution:

```toml
[[skills]]
name = "file_reader"
type = "wasm"
path = "skills/file-reader.wasm"
[skills.capabilities]
file_read = ["/tmp", "/home/user/docs"]
```

---

## Multi-Agent Orchestration

The orchestrator follows Anthropic's recommended **Orchestrator-Workers** pattern:

1. **Plan** — Orchestrator decomposes the task into subtasks with a dependency graph
2. **Execute** — Specialized workers execute in parallel (respecting dependencies)
3. **Synthesize** — Orchestrator collects artifacts, validates consistency, produces final output

### Agent Roles

- **Orchestrator** — Decomposes tasks, delegates, synthesizes results
- **Spec** — Analyzes requirements, generates specifications
- **Coder** — Generates secure, idiomatic Rust code
- **Tester** — Writes and validates tests
- **Reviewer** — Reviews code for security and compliance

### Human-in-the-Loop

High-risk operations require human approval:

```rust
TaskStatus::NeedsHumanReview  // Pauses execution until approved
```

---

## Compliance

### GDPR
- Consent tracking with `ConsentStore`
- Right to erasure (Art. 17)
- Data portability (Art. 20)
- Purpose limitation

### ISO 27001
- Access control logging
- Incident response tracking
- Risk assessment records

### ISO 42001 (AI Management)
- AI system inventory
- Bias monitoring
- Transparency logging
- Human oversight for high-risk decisions

### DPGA (Digital Public Goods Alliance)

Agentor targets all 9 DPGA indicators:

1. **Open Source** — AGPL-3.0-only
2. **SDG Relevance** — SDG 9 (Innovation), SDG 16 (Institutions)
3. **Open Data** — MCP interoperability with open datasets
4. **Privacy** — GDPR compliance module
5. **Documentation** — Comprehensive docs (EN/ES)
6. **Open Standards** — MCP (AAIF/Linux Foundation), WASM, WIT
7. **Ownership** — Clear governance
8. **Do No Harm** — ISO 42001, HITL, bias monitoring
9. **Interoperability** — MCP + A2A protocol support

---

## MCP Integration

Agentor implements [Model Context Protocol](https://modelcontextprotocol.io/) for tool integration:

```toml
[[mcp_servers]]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

The MCP Proxy centralizes all tool calls with:
- Logging of every invocation
- Permission validation
- Rate limiting per agent
- Progressive tool disclosure (~98% token reduction)

---

## Docker

```bash
docker build -t agentor .
docker run -p 3000:3000 agentor serve
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## License

This project is licensed under the **GNU Affero General Public License v3.0** — see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- [Anthropic](https://anthropic.com) — Claude models and MCP protocol
- [wasmtime](https://wasmtime.dev) — WebAssembly runtime
- [Axum](https://github.com/tokio-rs/axum) — Web framework
- [DPGA](https://digitalpublicgoods.net) — Digital Public Goods Alliance
