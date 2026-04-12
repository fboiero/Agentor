# MCP Integration Guide

> The complete playbook for using [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) with Argentor. By the end of this document you will be connecting to third-party servers, exposing your own skills, routing traffic through a central proxy, and managing credentials for dozens of tools at once.

This guide is for engineers integrating real workloads. If you are new to MCP, start with [Tutorial 8: MCP Integration](./tutorials/08-mcp-integration.md) for the hands-on walkthrough.

---

## 1. What is MCP?

The **Model Context Protocol** is an open specification published by Anthropic in November 2024. It defines a standard JSON-RPC 2.0 dialect that AI agents can use to discover and invoke tools, read resources, and fetch prompt templates — independent of vendor, framework, or language.

Three transports are supported:

| Transport | Typical use |
|-----------|-------------|
| **stdio** | A subprocess speaks MCP on its `stdin` / `stdout`. This is the default for CLI-style integrations. |
| **HTTP + SSE** | A long-lived HTTP connection where the server streams responses over Server-Sent Events. |
| **WebSocket** | Full-duplex, bidirectional, low-latency. Used for remote MCP servers and live event streams. |

Each MCP server exposes up to three primitive categories:

- **Tools** — callable RPCs with a JSON Schema (`tools/list`, `tools/call`)
- **Resources** — read-only URIs such as files, database rows, or API responses (`resources/list`, `resources/read`)
- **Prompts** — reusable prompt templates parameterized by arguments (`prompts/list`, `prompts/get`)

Authoritative reference: [modelcontextprotocol.io](https://modelcontextprotocol.io).

---

## 2. Why MCP Matters

At the time of writing (April 2026), the public MCP ecosystem has crossed **5,800 servers** covering filesystems, databases, cloud APIs, communication tools, developer platforms, and SaaS products. Major vendors (GitHub, Stripe, Cloudflare, HashiCorp, Grafana, Microsoft, AWS, Anthropic) maintain first-party MCP servers.

For Argentor this is a force multiplier. Instead of writing one-off Rust crates for every API, we hook into MCP once and every compliant server becomes a usable tool — with no loss of security, because the calls still pass through Argentor's capability system, guardrails, and audit log.

Operational benefits:

- **Zero code** to add a new integration (only config)
- **Standardized tool schemas** — the agent sees them in a uniform shape
- **Cross-framework compatibility** — the same server works with Claude Desktop, Cursor, LangChain, CrewAI, OpenAI Agents SDK, and Argentor
- **Per-call observability** — every tool invocation is logged, metered, and rate-limited by the MCP proxy

See [MCP_REGISTRY.md](./MCP_REGISTRY.md) for the top 100 servers across 10 categories.

---

## 3. Argentor's MCP Support

The `argentor-mcp` crate provides three operating modes:

### Client mode
Argentor connects outward to external MCP servers. Each server's tools are wrapped as Argentor [skills](../crates/argentor-skills) and become callable by any agent. This is what you use to consume the 5,800+ public servers.

### Server mode
Argentor itself speaks MCP. Other agents (Claude Desktop, Cursor, arbitrary LLM clients) can connect to Argentor and see your registered skills as MCP tools. This is useful for sharing internal tools with your team's LLM workstations, or for exposing a curated toolset across a company's agent fleet.

### Proxy mode
A single MCP endpoint that multiplexes to many backend servers. The proxy centralizes logging, rate limiting, permission validation, progressive tool disclosure, and circuit breakers. This is the production topology for fleets of agents sharing a common toolset.

Source modules:

| Module | Responsibility | Path |
|--------|----------------|------|
| `client` | JSON-RPC 2.0 client, subprocess lifecycle, handshake | `crates/argentor-mcp/src/client.rs` |
| `protocol` | JSON-RPC envelopes, MCP type definitions | `crates/argentor-mcp/src/protocol.rs` |
| `skill` | Adapter exposing an MCP tool as an Argentor `Skill` | `crates/argentor-mcp/src/skill.rs` |
| `server` | Serve Argentor skills as MCP over stdio / HTTP / WS | `crates/argentor-mcp/src/server.rs` |
| `proxy` | Multi-backend multiplexer with metrics | `crates/argentor-mcp/src/proxy.rs` |
| `proxy_orchestrator` | Route across multiple proxies, circuit breakers | `crates/argentor-mcp/src/proxy_orchestrator.rs` |
| `credential_vault` | Encrypted credential store (AES-256-GCM) | `crates/argentor-mcp/src/credential_vault.rs` |
| `token_pool` | Per-provider token pools with sliding-window limits | `crates/argentor-mcp/src/token_pool.rs` |
| `manager` | Health, reconnect, lifecycle for N servers | `crates/argentor-mcp/src/manager.rs` |
| `discovery` | Enumerate tools/resources/prompts on connect | `crates/argentor-mcp/src/discovery.rs` |
| `in_process` | Define MCP servers inside the same process (no subprocess) | `crates/argentor-mcp/src/in_process.rs` |

---

## 4. Configuring an MCP Server in `argentor.toml`

Argentor reads MCP server definitions from the `[mcp.servers.<alias>]` tables in your config file. Each entry describes how to spawn the server and what environment it needs.

### Minimal configuration

```toml
# argentor.toml

[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/var/data"]
```

### With environment variables

```toml
[mcp.servers.github]
command = "docker"
args = ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }
```

Variables of the form `${NAME}` are resolved from the process environment at spawn time. Missing variables cause startup to fail with a descriptive error.

### Full options

```toml
[mcp.servers.postgres]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres", "${POSTGRES_URL}"]
env = { NODE_ENV = "production" }
enabled = true                     # set false to keep the entry but not spawn
auto_restart = true                # respawn on crash
restart_max_attempts = 5           # give up after this many consecutive failures
health_check_interval_secs = 30    # ping interval (tools/list as heartbeat)
connection_timeout_secs = 10       # handshake timeout
tool_allow_list = ["query"]        # only expose these tools (empty = all)
tool_deny_list = ["execute_sql"]   # never expose these even if allow_list is empty
```

### Multiple servers

You can configure as many servers as you need. Argentor spawns them in parallel at boot:

```toml
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/var/data"]

[mcp.servers.github]
command = "docker"
args = ["run", "-i", "--rm", "ghcr.io/github/github-mcp-server"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }

[mcp.servers.slack]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-slack"]
env = { SLACK_BOT_TOKEN = "${SLACK_BOT_TOKEN}", SLACK_TEAM_ID = "${SLACK_TEAM}" }
```

---

## 5. Using MCP Tools from an Agent

Once configured, MCP tools behave exactly like native Argentor skills. The agent does not know (or need to know) where the tool's implementation lives.

### Via code (full control)

```rust
use argentor_agent::{AgentRunner, LlmProvider, ModelConfig};
use argentor_mcp::{McpClient, McpSkill};
use argentor_security::{AuditLog, Capability, PermissionSet};
use argentor_session::Session;
use argentor_skills::SkillRegistry;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Spawn filesystem MCP and discover tools.
    let (fs_client, fs_tools) = McpClient::connect(
        "npx",
        &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        &[],
    ).await?;
    let fs_client = Arc::new(fs_client);

    // 2. Spawn GitHub MCP.
    let gh_token = std::env::var("GITHUB_TOKEN")?;
    let (gh_client, gh_tools) = McpClient::connect(
        "docker",
        &["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN",
          "ghcr.io/github/github-mcp-server"],
        &[("GITHUB_PERSONAL_ACCESS_TOKEN", &gh_token)],
    ).await?;
    let gh_client = Arc::new(gh_client);

    // 3. Register every MCP tool as a skill.
    let mut registry = SkillRegistry::new();
    for tool in fs_tools {
        registry.register(Arc::new(McpSkill::new(fs_client.clone(), tool)));
    }
    for tool in gh_tools {
        registry.register(Arc::new(McpSkill::new(gh_client.clone(), tool)));
    }

    // 4. Build permissions and audit log.
    let mut permissions = PermissionSet::new();
    permissions.grant(Capability::FileRead { allowed_paths: vec!["/tmp".into()] });
    permissions.grant(Capability::NetworkAccess { allowed_hosts: vec!["api.github.com".into()] });

    let audit = Arc::new(AuditLog::new(PathBuf::from("./audit")));

    // 5. Configure the LLM.
    let config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "claude-sonnet-4-20250514".into(),
        api_key: std::env::var("ANTHROPIC_API_KEY")?,
        api_base_url: None,
        temperature: 0.3,
        max_tokens: 4096,
        max_turns: 10,
        fallback_models: vec![],
        retry_policy: None,
    };

    // 6. Run the agent.
    let runner = AgentRunner::new(config, Arc::new(registry), permissions, audit);
    let mut session = Session::new();
    let response = runner.run(
        &mut session,
        "List the 5 latest issues in fboiero/Agentor and save a summary to /tmp/issues.md"
    ).await?;

    println!("{response}");
    Ok(())
}
```

### Via the CLI (zero code)

```bash
cargo run -p argentor-cli -- serve --config argentor.toml
```

Any agent invoked through the CLI automatically sees every configured MCP server's tools.

---

## 6. Argentor as MCP Server (Exposing Skills)

Turn Argentor into an MCP server so other agents can call its skills.

```toml
[mcp_server]
enabled = true
transport = "stdio"                              # or "http", "websocket"
bind = "0.0.0.0:8090"                            # http / ws only
exposed_skills = []                              # empty = expose all
require_auth = true
api_key = "${ARGENTOR_MCP_KEY}"
```

Run it:

```bash
cargo run -p argentor-cli -- mcp serve
```

Verify with the official inspector UI:

```bash
npx @modelcontextprotocol/inspector cargo run -p argentor-cli -- mcp serve
```

The inspector connects, lists every skill, and lets you call them interactively.

### Consuming Argentor from another Argentor

```rust
use argentor_mcp::McpClient;

let (client, tools) = McpClient::connect(
    "cargo",
    &["run", "-p", "argentor-cli", "--", "mcp", "serve"],
    &[("ARGENTOR_MCP_KEY", "your-shared-secret")],
).await?;

println!("Discovered {} skills from remote Argentor", tools.len());

let result = client.call_tool(
    "calculator",
    serde_json::json!({ "expression": "47 * 32" }),
).await?;
```

### Consuming from Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS):

```json
{
  "mcpServers": {
    "argentor": {
      "command": "/usr/local/bin/argentor",
      "args": ["mcp", "serve"],
      "env": {
        "ARGENTOR_MCP_KEY": "shared-secret"
      }
    }
  }
}
```

Claude Desktop will discover every Argentor skill and expose them to the conversation.

---

## 7. Credential Vault for MCP API Keys

Managing 30 API keys across 30 MCP servers quickly becomes untenable. `CredentialVault` centralizes all of them in an AES-256-GCM encrypted store with rotation and per-credential quotas.

```rust
use argentor_mcp::credential_vault::{CredentialPolicy, CredentialVault};
use std::time::Duration;

let vault = CredentialVault::new();

// Register credentials with default policy.
vault.add("openai-primary",   "openai",   "api_key", "sk-...",  CredentialPolicy::default())?;
vault.add("openai-backup",    "openai",   "api_key", "sk-...",  CredentialPolicy::default())?;
vault.add("github-admin",     "github",   "token",   "ghp-...", CredentialPolicy::default())?;

// With rate limits and rotation.
vault.add(
    "tavily-prod",
    "tavily",
    "api_key",
    "tvly-...",
    CredentialPolicy {
        max_calls_per_minute: Some(60),
        max_calls_per_day: Some(5_000),
        expires_at: None,
        rotate_after: Some(Duration::from_secs(30 * 86_400)),  // 30 days
    },
)?;

// Resolve the best available credential for a provider.
let cred = vault.resolve("openai")?;
std::env::set_var("OPENAI_API_KEY", &cred.value);
```

Persist to disk (encrypted with a master password):

```rust
vault.save_to_file("./vault.enc", "master-password").await?;
// ...restart later...
let vault = CredentialVault::load_from_file("./vault.enc", "master-password").await?;
```

Or load from environment for CI / container use:

```rust
let vault = CredentialVault::from_env()?;
```

The vault supports:

- Provider grouping (e.g., 3 OpenAI keys, auto-rotated)
- Per-credential rate limits (rpm and rpd)
- Expiration and rotation schedules
- Denial on exhaustion (returns `ArgentorError::RateLimitExceeded`)
- Audit-log integration (every resolve is logged)

---

## 8. Multi-Proxy Orchestration

For organizations running agents across regions, teams, or security domains, a single proxy is not enough. `ProxyOrchestrator` lets you compose many proxies behind a single dispatch surface with routing rules, round-robin, and circuit breakers.

```rust
use argentor_mcp::proxy_orchestrator::{ProxyOrchestrator, RoutingRule, RoutingStrategy};
use argentor_mcp::McpProxy;
use argentor_security::PermissionSet;
use argentor_skills::SkillRegistry;
use std::sync::Arc;

// Build per-domain proxies.
let internal_registry = Arc::new(SkillRegistry::new());
let internal = Arc::new(McpProxy::new(internal_registry, PermissionSet::new()));

let external_registry = Arc::new(SkillRegistry::new());
let external = Arc::new(McpProxy::new(external_registry, PermissionSet::new()));

let eu_registry = Arc::new(SkillRegistry::new());
let eu = Arc::new(McpProxy::new(eu_registry, PermissionSet::new()));

// Compose.
let orchestrator = ProxyOrchestrator::new()
    .with_proxy("internal", internal)
    .with_proxy("external", external)
    .with_proxy("eu-region", eu)
    .with_routing_rule(RoutingRule {
        pattern: "gdpr_*".into(),
        target_proxy: "eu-region".into(),
    })
    .with_routing_rule(RoutingRule {
        pattern: "slack_*".into(),
        target_proxy: "external".into(),
    })
    .with_strategy(RoutingStrategy::RoundRobin);

// Dispatch.
let result = orchestrator
    .call("gdpr_export_user_data", serde_json::json!({ "user_id": "123" }))
    .await?;
```

Routing order:

1. Exact match on `pattern` (glob-style)
2. `RoutingStrategy` fallback (RoundRobin, LeastLoad, FirstHealthy)
3. Circuit breaker isolation — unhealthy proxies are skipped automatically

Metrics are exposed as JSON:

```rust
let snapshot = orchestrator.metrics().await;
println!("{}", serde_json::to_string_pretty(&snapshot)?);
```

---

## 9. Debugging MCP Connections

### Handshake timeout

The server is almost certainly printing non-JSON output to stdout (common with `npx` / `npm install` progress). Fixes:

- Install globally so npm does not print progress: `npm i -g @modelcontextprotocol/server-filesystem`
- Route stderr to a log file rather than suppress it to diagnose faster

### Tool call returns `-32602 Invalid params`

The arguments you sent do not match the tool's declared schema. Inspect:

```rust
for tool in &tools {
    println!("{}: {}", tool.name, serde_json::to_string_pretty(&tool.input_schema)?);
}
```

### Server crashes silently

Enable stderr capture:

```rust
// In Argentor's Command setup:
cmd.stderr(std::process::Stdio::piped());
```

Then read child stderr in a spawned task and forward to `tracing::warn!`.

### MCP Inspector UI

The reference inspector is the fastest way to manually test a server:

```bash
npx @modelcontextprotocol/inspector npx -y @modelcontextprotocol/server-filesystem /tmp
```

Or point it at Argentor:

```bash
npx @modelcontextprotocol/inspector cargo run -p argentor-cli -- mcp serve
```

### Tracing at the wire

Argentor logs every MCP JSON-RPC message at `DEBUG` level. Enable with:

```bash
RUST_LOG=argentor_mcp=debug cargo run ...
```

Each message is tagged with its server alias, id, and method.

### Health checks

`McpServerManager` runs periodic `tools/list` heartbeats against each configured server. If a server stops responding, the manager:

1. Marks it `Unhealthy`
2. Attempts reconnect with exponential backoff
3. Emits a `mcp.server.unhealthy` event to the orchestrator event bus
4. Stops routing calls until it reconnects

Inspect the state:

```rust
for (name, status) in manager.statuses().await {
    println!("{name}: {:?}", status);
}
```

---

## 10. Common Patterns

### RAG via MCP

Use a vector store MCP server (Qdrant, Pinecone, Weaviate) as the retrieval backend for Argentor's native RAG pipeline.

```rust
let (qdrant_client, qdrant_tools) = McpClient::connect(
    "uvx",
    &["mcp-server-qdrant"],
    &[("QDRANT_URL", "http://localhost:6333"), ("COLLECTION_NAME", "docs")],
).await?;

let qdrant = Arc::new(qdrant_client);
// register tools...
```

The agent now has `qdrant_search` and `qdrant_store` alongside every other tool — and Argentor's `RagPipeline` can be pointed at those tools for ingestion and retrieval.

### Tool composition across servers

A single agent turn can chain calls across multiple MCP servers:

1. `github.search_issues` (GitHub MCP) — find issues referencing a bug ID
2. `postgres.query` (Postgres MCP) — lookup customer data for each issue reporter
3. `slack.post_message` (Slack MCP) — notify the on-call engineer

Argentor's agent loop handles the sequencing. Every tool is just a `Skill`; the LLM decides the order.

### Progressive tool disclosure

With 50+ MCP tools active the prompt blows past token limits. Progressive disclosure limits the LLM to a relevant subset per turn:

```rust
use argentor_skills::progressive::{ProgressiveDisclosure, ToolGroup};

let disclosure = ProgressiveDisclosure::new()
    .with_group(ToolGroup::minimal())          // always-on tools
    .with_group(ToolGroup::named("github"))    // when task mentions GitHub
    .with_group(ToolGroup::named("database"))  // when task mentions SQL / DB
    .with_max_per_turn(10);

runner.with_tool_disclosure(disclosure);
```

Measured: ~98% reduction in tool-definition tokens on prompts with 100+ MCP tools active.

### In-process MCP for latency-critical tools

When you cannot afford subprocess overhead (handshake, JSON encoding, pipe I/O), define the server in-process:

```rust
use argentor_mcp::in_process::InProcessMcpServer;

let server = InProcessMcpServer::new()
    .with_tool("fast_op", |args| async move {
        // direct function call — no subprocess
        Ok(serde_json::json!({ "result": 42 }))
    });

let client = server.client();  // no handshake, zero-copy channel
```

This looks like MCP to callers but skips the transport layer entirely.

### Per-tenant MCP isolation

In multi-tenant deployments, each tenant can have its own MCP proxy with tenant-specific credentials:

```rust
let tenant_a = McpProxy::new(registry_a, permissions_a);
let tenant_b = McpProxy::new(registry_b, permissions_b);

let orchestrator = ProxyOrchestrator::new()
    .with_proxy("tenant-a", Arc::new(tenant_a))
    .with_proxy("tenant-b", Arc::new(tenant_b));

// Route by tenant id from the agent's metadata.
```

Combined with Argentor's multi-tenancy layer, this gives you true isolation: tenant A's agents cannot see tenant B's MCP tools or credentials.

---

## Further reading

- **Hands-on walkthrough:** [Tutorial 8: MCP Integration](./tutorials/08-mcp-integration.md)
- **Server catalog:** [MCP_REGISTRY.md](./MCP_REGISTRY.md)
- **Source code:** `crates/argentor-mcp/src/`
- **Spec:** [modelcontextprotocol.io](https://modelcontextprotocol.io)
- **Community servers:** [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers)

---

## TL;DR

1. **Find a server** in [MCP_REGISTRY.md](./MCP_REGISTRY.md) or [awesome-mcp-servers](https://github.com/punkpeye/awesome-mcp-servers).
2. **Paste its snippet** into `argentor.toml` under `[mcp.servers.<alias>]`.
3. **Set the env vars** it needs (or put keys in the Credential Vault).
4. **Run Argentor** — the tools show up automatically in the registry.
5. **Your agent can call them** like any other skill.

That is the entire loop. Everything else in this guide — proxies, orchestrators, vaults, in-process servers — is optional scaling and hardening on top of that core flow.
