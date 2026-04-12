# Tutorial 8: MCP (Model Context Protocol) Integration

> Connect to external MCP servers as a client. Expose your skills to other agents as an MCP server. Orchestrate multiple MCP backends through a proxy with credential management.

The **Model Context Protocol** (MCP), open-sourced by Anthropic, is becoming the "HTTP of AI tools". Instead of each framework inventing its own plugin system, MCP servers speak a standard JSON-RPC 2.0 dialect that any compliant client can consume.

Argentor is fully MCP-compliant in three modes:

1. **Client** — connect to remote MCP servers (filesystem, GitHub, Slack, Postgres, etc.)
2. **Server** — expose your Argentor skills to other agents
3. **Proxy** — central control plane mediating many MCP backends

---

## Prerequisites

- Completed [Tutorial 1](./01-first-agent.md) and [Tutorial 5](./05-custom-skills.md)
- Node.js installed (for testing with `@modelcontextprotocol/server-*` packages)
- Basic understanding of JSON-RPC

---

## 1. What is MCP?

An MCP server speaks JSON-RPC 2.0 over one of:

- **stdio** — subprocess with `stdin`/`stdout` (most common)
- **HTTP + SSE** — remote server with streaming
- **WebSocket** — full-duplex remote

Every server exposes:

- **`tools/list`** — enumerate callable tools
- **`tools/call`** — invoke a tool with arguments
- **`resources/list`** / **`resources/read`** — read resources (files, DB rows, etc.)
- **`prompts/list`** / **`prompts/get`** — reusable prompt templates

Reference: [modelcontextprotocol.io](https://modelcontextprotocol.io)

---

## 2. MCP as Client — Connect to External Servers

Add `argentor-mcp` to your project:

```toml
argentor-mcp = { git = "https://github.com/fboiero/Agentor", branch = "master" }
```

### Connecting to public MCP servers

The public MCP ecosystem has **5,800+ servers** covering filesystems, databases, cloud APIs, developer tools, and SaaS products. See [docs/MCP_REGISTRY.md](../MCP_REGISTRY.md) for a curated catalog of the top 100 across 10 categories, or the full integration flow in [docs/MCP_INTEGRATION_GUIDE.md](../MCP_INTEGRATION_GUIDE.md).

Three quick examples:

**Filesystem** — read, write, and search files in a directory:

```toml
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

**GitHub** — issues, PRs, repos, Actions, code search:

```toml
[mcp.servers.github]
command = "docker"
args = ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"]
env = { GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}" }
```

**Postgres** — read-only queries with schema introspection:

```toml
[mcp.servers.postgres]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres", "${POSTGRES_URL}"]
```

With these three entries added to `argentor.toml`, your agent gets filesystem, GitHub, and Postgres access with no additional Rust code.

### Connect programmatically

Connect to the reference filesystem server:

```rust
use argentor_mcp::McpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spawn the MCP server as a subprocess and perform the handshake.
    let (client, tools) = McpClient::connect(
        "npx",
        &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        &[], // no env vars
    ).await?;

    println!("Connected — {} tools discovered:", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name, tool.description.as_deref().unwrap_or("-"));
    }

    // Call a tool
    let result = client.call_tool("read_file", serde_json::json!({
        "path": "/tmp/hello.txt"
    })).await?;

    println!("\nResult: {result}");
    Ok(())
}
```

Expected output:

```
Connected — 6 tools discovered:
  - read_file: Read the complete contents of a file
  - write_file: Create a new file or overwrite an existing one
  - list_directory: List directory entries
  - create_directory: ...
  - move_file: ...
  - search_files: ...

Result: {"content": [{"type": "text", "text": "Hello MCP!"}], "isError": false}
```

---

## 3. Register MCP Tools as Argentor Skills

Use `McpSkill` to wrap an MCP tool so it looks like any other skill to the agent:

```rust
use argentor_mcp::{McpClient, McpSkill};
use argentor_skills::SkillRegistry;
use std::sync::Arc;

let (client, tools) = McpClient::connect(
    "npx",
    &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    &[],
).await?;
let client = Arc::new(client);

let mut registry = SkillRegistry::new();
for tool in tools {
    registry.register(Arc::new(McpSkill::new(
        client.clone(),
        tool,
    )));
}

// Now the agent sees 6 new tools: read_file, write_file, list_directory, etc.
```

The agent treats MCP tools identically to native skills. Capabilities, audit logs, guardrails all still apply.

---

## 4. MCP as Server — Expose Your Skills

Run Argentor itself as an MCP server so other agents (Claude Desktop, Cursor, custom agents) can invoke your skills:

```bash
cargo run -p argentor-cli -- mcp serve
```

By default this exposes every skill in the default registry over stdio. Configure via `argentor.toml`:

```toml
[mcp_server]
enabled = true
transport = "stdio"           # or "http", "websocket"
bind = "0.0.0.0:8090"         # for http / websocket
exposed_skills = ["calculator", "file_read", "web_search"]  # empty = all
require_auth = true
api_key = "${ARGENTOR_MCP_KEY}"
```

Test with the official MCP inspector:

```bash
npx @modelcontextprotocol/inspector cargo run -p argentor-cli -- mcp serve
```

You will see the Argentor skills listed in the inspector UI, callable interactively.

### From another Argentor instance

```rust
let (client, _) = McpClient::connect(
    "cargo",
    &["run", "-p", "argentor-cli", "--", "mcp", "serve"],
    &[("ARGENTOR_MCP_KEY", "your-shared-secret")],
).await?;

let result = client.call_tool("calculator",
    serde_json::json!({ "expression": "2 + 2" })).await?;
```

---

## 5. MCP Proxy — Central Control Plane

When you have many MCP backends (filesystem, GitHub, Slack, internal DB, custom tools), a **proxy** gives you:

- Unified endpoint for all agents
- Centralized logging of every tool call
- Per-agent rate limiting
- Progressive tool disclosure (send only relevant tools — ~98% token reduction)
- Auto-reconnect on backend failures
- Circuit breakers per backend

```rust
use argentor_mcp::McpProxy;
use argentor_skills::SkillRegistry;
use argentor_security::PermissionSet;
use std::sync::Arc;

let mut registry = SkillRegistry::new();
// ... register your skills

let permissions = PermissionSet::new();
let proxy = Arc::new(McpProxy::new(Arc::new(registry), permissions));

// Route an AgentRunner's tool calls through the proxy
let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_proxy(proxy.clone(), "my-agent-id");

// Get proxy metrics as JSON
let stats = proxy.to_json().await;
println!("{}", serde_json::to_string_pretty(&stats)?);
```

Output:

```json
{
  "total_calls": 147,
  "successful_calls": 142,
  "denied_calls": 3,
  "failed_calls": 2,
  "avg_latency_ms": 187.4,
  "calls_by_skill": {
    "file_read": 48,
    "web_search": 31,
    "calculator": 23,
    ...
  },
  "rate_limited_agents": []
}
```

---

## 6. Credential Vault

MCP API tokens, database passwords, and third-party keys pile up fast. The `CredentialVault` centralizes storage with AES-256-GCM encryption, per-credential quotas, and automatic rotation:

```rust
use argentor_mcp::credential_vault::{CredentialPolicy, CredentialVault};

let vault = CredentialVault::new();

// Add with default policy
vault.add("openai-primary", "openai", "api_key", "sk-...", CredentialPolicy::default())?;
vault.add("openai-backup", "openai", "api_key", "sk-...", CredentialPolicy::default())?;

// Add with rate limits + daily cap
vault.add("tavily-prod", "tavily", "api_key", "tvly-...",
    CredentialPolicy {
        max_calls_per_minute: Some(60),
        max_calls_per_day: Some(5000),
        expires_at: None,
        rotate_after: Some(std::time::Duration::from_secs(30 * 86400)),
    },
)?;

// Resolve best available credential for a provider
let cred = vault.resolve("openai")?;
let api_key = cred.value;  // decrypted on access
```

Persist to disk (encrypted):

```rust
vault.save_to_file("./vault.enc", "master-password").await?;
// ...later...
let vault = CredentialVault::load_from_file("./vault.enc", "master-password").await?;
```

---

## 7. Token Pool

For providers that multiplex across many keys (load balancing, tier separation), use `TokenPool`:

```rust
use argentor_mcp::token_pool::{TokenPool, TokenTier};

let pool = TokenPool::new();
pool.add_token("openai", "sk-premium1", TokenTier::Premium, 100).await?;
pool.add_token("openai", "sk-premium2", TokenTier::Premium, 100).await?;
pool.add_token("openai", "sk-standard", TokenTier::Standard, 20).await?;

// Acquire a token (sliding window, tier priority)
let token = pool.acquire("openai", TokenTier::Premium).await?;
// ... use the token ...
pool.release("openai", &token).await;
```

Exhausted premium tokens fall through to standard automatically.

---

## 8. Multi-Proxy Orchestration

Run multiple MCP proxies (different teams, different regions, different security domains) and let the orchestrator route across them:

```rust
use argentor_mcp::proxy_orchestrator::{ProxyOrchestrator, RoutingRule, RoutingStrategy};

let orchestrator = ProxyOrchestrator::new()
    .with_proxy("internal", internal_proxy.clone())
    .with_proxy("external", external_proxy.clone())
    .with_proxy("eu-region", eu_proxy.clone())
    .with_routing_rule(RoutingRule {
        pattern: "gdpr_*".into(),
        target_proxy: "eu-region".into(),
    })
    .with_routing_rule(RoutingRule {
        pattern: "slack_*".into(),
        target_proxy: "external".into(),
    })
    .with_strategy(RoutingStrategy::RoundRobin);

// Dispatch — orchestrator picks the right proxy
let result = orchestrator.call("gdpr_export_user_data",
    serde_json::json!({"user_id": "123"})).await?;
```

Circuit breakers trip per-proxy; unhealthy backends get isolated automatically.

---

## 9. MCP Resources

Beyond tools, MCP exposes **resources** — read-only data sources:

```rust
let resources = client.list_resources().await?;
for res in &resources {
    println!("  {} ({})", res.uri, res.mime_type.as_deref().unwrap_or("?"));
}

let content = client.read_resource("file:///tmp/report.md").await?;
println!("{content}");
```

Argentor's own server exposes resources for each skill's metadata and any configured knowledge bases.

---

## 10. A Full End-to-End Example

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
    // 1. Connect to MCP filesystem server
    let (fs_client, fs_tools) = McpClient::connect(
        "npx",
        &["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        &[],
    ).await?;
    let fs_client = Arc::new(fs_client);

    // 2. Connect to MCP GitHub server
    let github_token = std::env::var("GITHUB_TOKEN")?;
    let (gh_client, gh_tools) = McpClient::connect(
        "npx",
        &["-y", "@modelcontextprotocol/server-github"],
        &[("GITHUB_PERSONAL_ACCESS_TOKEN", &github_token)],
    ).await?;
    let gh_client = Arc::new(gh_client);

    // 3. Register all as Argentor skills
    let mut registry = SkillRegistry::new();
    for tool in fs_tools {
        registry.register(Arc::new(McpSkill::new(fs_client.clone(), tool)));
    }
    for tool in gh_tools {
        registry.register(Arc::new(McpSkill::new(gh_client.clone(), tool)));
    }

    // 4. Set up agent
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

    let mut permissions = PermissionSet::new();
    permissions.grant(Capability::FileRead { allowed_paths: vec!["/tmp".into()] });
    permissions.grant(Capability::FileWrite { allowed_paths: vec!["/tmp".into()] });
    permissions.grant(Capability::NetworkAccess { allowed_hosts: vec!["api.github.com".into()] });

    let audit = Arc::new(AuditLog::new(PathBuf::from("./audit")));

    let runner = AgentRunner::new(config, Arc::new(registry), permissions, audit);

    // 5. Let the agent use both MCP servers
    let mut session = Session::new();
    let response = runner.run(
        &mut session,
        "Fetch the latest 3 issues from the 'fboiero/Argentor' GitHub repo and write a summary to /tmp/issues.md"
    ).await?;

    println!("\n{response}");
    Ok(())
}
```

---

## Common Issues

**"Failed to spawn MCP server 'npx'"**
Install Node.js. Confirm `npx --version` works in the shell Argentor inherits.

**"MCP server handshake timeout"**
The server is probably printing something to stdout that is not JSON (like `npm install` progress). Suppress with `stderr` redirection or install the package globally first: `npm i -g @modelcontextprotocol/server-filesystem`.

**"Tool call failed: -32602 Invalid params"**
Your arguments do not match the MCP tool's schema. Inspect `tool.input_schema` after `connect()`.

**Credential vault asks for master password every run**
Set `ARGENTOR_VAULT_PASSWORD` env var and use `CredentialVault::from_env()`, or integrate with your secrets manager (AWS Secrets Manager, HashiCorp Vault).

**Proxy metrics show `avg_latency_ms` spiking**
One backend is slow. Inspect `calls_by_skill` — if one skill is hot, move it to its own proxy with higher rate limits, or enable the circuit breaker to trip faster.

**Unable to reconnect after MCP server crash**
Wrap the client construction in a `retry` loop with exponential backoff. The `argentor-mcp::manager::McpManager` does this for you automatically.

---

## What You Built

- An agent using external MCP servers as if they were native skills
- An Argentor instance exposing skills as an MCP server for other agents
- A proxy orchestrating many MCP backends with routing rules and circuit breakers
- An encrypted credential vault with per-token quotas and rotation
- A token pool with tier-based priority

---

## Next Steps

- **[Tutorial 9: Production Deployment](./09-deployment.md)** — deploy the proxy with TLS and rate limiting.
- **[Tutorial 3: Multi-Agent Orchestration](./03-multi-agent-orchestration.md)** — let teams share a common MCP proxy.
- **[Tutorial 10: Observability](./10-observability.md)** — trace MCP calls across the entire request path.
