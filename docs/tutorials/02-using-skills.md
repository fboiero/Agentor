# Tutorial 2: Using Built-in Skills

> Give your agent tools. Grant it capabilities. Watch it use the calculator, read files, and search the web.

Argentor ships with 50+ built-in skills grouped across data/text processing, file I/O, shell execution, web access, cryptography, and more. In this tutorial we wire up three common skills — `calculator`, `file_read`, and `web_search` — and observe how the agent picks which one to use.

---

## Prerequisites

- Completed [Tutorial 1: First Agent](./01-first-agent.md)
- Working `my-first-agent` project with Argentor dependencies
- `ANTHROPIC_API_KEY` (or any other provider key) available

---

## 1. List the Built-in Skills

The fastest way to see what ships out of the box is the CLI:

```bash
cargo run -p argentor-cli -- skill list
```

You will see entries like:

```
calculator          Precise arithmetic with no floating-point surprises
file_read           Read a UTF-8 file from an allowlisted directory
file_write          Write a UTF-8 file to an allowlisted directory
shell               Execute a shell command (with dangerous-command blocklist)
http_fetch          Fetch a URL with SSRF protection
web_search          Search the web (DuckDuckGo, Tavily, Brave, SearXNG)
memory_store        Store content in the vector store with embeddings
memory_search       Semantic search over stored memories
git                 libgit2-backed repository operations
...
```

Each skill declares:

- A **name** — how the LLM refers to it in tool calls.
- A **description** — shown to the LLM so it knows when to pick this tool.
- A **JSON schema** — defines parameters and their types.
- A **capability list** — what permissions the runner must grant before the skill can run.

---

## 2. Understand Capabilities

Argentor uses **capability-based security**. A skill declares what it needs (e.g., `FileRead`, `ShellExec`), and the runner only executes it if the `PermissionSet` has granted that capability.

```rust
use argentor_security::{Capability, PermissionSet};

let mut permissions = PermissionSet::new();

// Allow file reads from two specific directories
permissions.grant(Capability::FileRead {
    allowed_paths: vec![
        "/tmp".into(),
        std::env::current_dir()?.to_string_lossy().to_string(),
    ],
});

// Allow file writes only to /tmp
permissions.grant(Capability::FileWrite {
    allowed_paths: vec!["/tmp".into()],
});

// Allow network access (needed by web_search / http_fetch)
permissions.grant(Capability::NetworkAccess {
    allowed_hosts: vec![], // empty = any host, subject to SSRF guard
});

// Allow shell execution (with command blocklist handled at the skill layer)
permissions.grant(Capability::ShellExec {
    allowed_commands: vec![], // empty = any non-dangerous command
});
```

> Passing empty `allowed_paths` / `allowed_hosts` means "no path restriction at the capability layer" but the skills themselves still apply defense-in-depth (path canonicalization, SSRF blocklist, dangerous-command detection).

---

## 3. Example: Calculator

The calculator skill needs zero capabilities — it is pure math.

```rust
use argentor_agent::{AgentRunner, LlmProvider, ModelConfig};
use argentor_builtins::register_builtins;
use argentor_security::{AuditLog, PermissionSet};
use argentor_session::Session;
use argentor_skills::SkillRegistry;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let mut registry = SkillRegistry::new();
    register_builtins(&mut registry);

    let config = ModelConfig {
        provider: LlmProvider::Claude,
        model_id: "claude-sonnet-4-20250514".into(),
        api_key: std::env::var("ANTHROPIC_API_KEY")?,
        api_base_url: None,
        temperature: 0.3,
        max_tokens: 1024,
        max_turns: 5,
        fallback_models: vec![],
        retry_policy: None,
    };

    // Calculator needs no capabilities.
    let permissions = PermissionSet::new();
    let audit = Arc::new(AuditLog::new(PathBuf::from("./audit-logs")));

    let runner = AgentRunner::new(config, Arc::new(registry), permissions, audit);

    let mut session = Session::new();
    let response = runner
        .run(
            &mut session,
            "A customer ordered 17 items at $23.45 each with a 9.5% tax. \
             Show the subtotal, tax, and total with 2 decimal places.",
        )
        .await?;

    println!("\n{response}\n");
    Ok(())
}
```

### Expected output

```
─── Agent response ───

Subtotal: $398.65
Tax (9.5%): $37.87
Total: $436.52

(Calculation performed via the `calculator` skill to avoid floating-point errors.)
```

The agent noticed a math problem, called the `calculator` skill instead of guessing, and summarized the result.

---

## 4. Example: File Read

Create a sample file:

```bash
echo "The quick brown fox jumps over the lazy dog.
Line two has more content.
Line three is the shortest." > /tmp/sample.txt
```

Add the `FileRead` capability and re-run:

```rust
let mut permissions = PermissionSet::new();
permissions.grant(argentor_security::Capability::FileRead {
    allowed_paths: vec!["/tmp".into()],
});

// Prompt:
let response = runner.run(
    &mut session,
    "Read /tmp/sample.txt and count how many words it contains.",
).await?;
```

### Expected flow

```
 INFO argentor_skills::registry: executing skill name="file_read"
 INFO argentor_agent::runner: tool_result call_id="..." skill="file_read"

─── Agent response ───
/tmp/sample.txt contains 18 words across 3 lines.
```

### What the tool call looks like under the hood

The LLM produced a tool-use message roughly like:

```json
{
  "type": "tool_use",
  "name": "file_read",
  "input": { "path": "/tmp/sample.txt" }
}
```

Argentor validated the path against the `FileRead` allowlist, executed the skill, wrote an audit entry, and handed the result back to the LLM for the final natural-language answer.

---

## 5. Example: Web Search

`web_search` hits DuckDuckGo by default but also supports Tavily, Brave, and SearXNG if you set the right environment variables.

```rust
use argentor_builtins::SearchProvider;
```

The skill picks up any of these env vars automatically:

- `TAVILY_API_KEY` — uses Tavily
- `BRAVE_API_KEY` — uses Brave Search
- `SEARXNG_URL` — uses a self-hosted SearXNG instance

You need `NetworkAccess`:

```rust
permissions.grant(argentor_security::Capability::NetworkAccess {
    allowed_hosts: vec![], // any public host allowed, SSRF blocklist still applies
});
```

Prompt:

```rust
let response = runner.run(
    &mut session,
    "Search the web for 'Rust 1.80 release notes' and summarize the top 3 changes.",
).await?;
```

### Expected output

```
─── Agent response ───
Based on the top web results for Rust 1.80:

1. LazyCell / LazyLock stabilized — replaces lazy_static / OnceCell patterns.
2. Range patterns in match expressions are now exhaustive-checked.
3. #[cfg] boolean literal evaluation fixed edge cases in macro_rules.

Sources: blog.rust-lang.org, official release notes, ...
```

---

## 6. How the Agent Picks a Skill

When the agent runs, Argentor sends the LLM a tool catalog that looks like:

```json
[
  {"name": "calculator", "description": "...", "input_schema": {...}},
  {"name": "file_read",  "description": "...", "input_schema": {...}},
  {"name": "web_search", "description": "...", "input_schema": {...}}
]
```

The LLM reads the user's prompt, the tool descriptions, and emits a `tool_use` message naming the right tool with structured arguments. Argentor:

1. Validates arguments against the JSON schema.
2. Checks `PermissionSet` for the required capabilities.
3. Executes the skill (sandboxed if WASM).
4. Writes an audit entry.
5. Hands the result back to the LLM for the next turn.

### Improving tool selection

With 50+ builtins, every extra tool costs context tokens. Two tools help:

**`.with_default_tool_discovery()`** — semantic search picks only the N most relevant tools per turn:

```rust
let runner = AgentRunner::new(config, skills, permissions, audit)
    .with_default_tool_discovery();
```

**Tool groups** — register a named subset with `SkillRegistry::create_group`:

```rust
let mut registry = SkillRegistry::new();
register_builtins(&mut registry);
registry.create_group("math-only", vec!["calculator".into()]);
```

See [Tutorial 7: Agent Intelligence](./07-agent-intelligence.md) for the full list of tuning knobs.

---

## 7. Skill Catalogue (What's Shipped)

**Data & Text** — calculator, text_transform, json_query, regex_tool, data_validator, datetime_tool, csv_processor, yaml_processor, markdown_renderer, template_engine, diff_tool, summarizer.

**Files & System** — file_read, file_write, file_hasher, shell, git, env_manager.

**Web & Network** — http_fetch, web_search, web_scraper, rss_reader, dns_lookup, ip_tools, browser, browser_automation.

**Crypto & Encoding** — hash_tool, encode_decode, uuid_generator, jwt_tool.

**Security & AI** — prompt_guard, secret_scanner.

**Memory & Knowledge** — memory_store, memory_search, knowledge_graph.

**Code & DevOps** — code_analysis, test_runner, api_scaffold, iac_generator, docker_sandbox, sdk_generator.

**Versioning & Ops** — semver_tool, cron_parser, metrics_collector.

**Orchestration** — artifact_store, agent_delegate, task_status, human_approval.

Register everything in one call:

```rust
use argentor_builtins::{register_builtins_with_memory};
use argentor_memory::{InMemoryVectorStore, LocalEmbedding};

let store = Arc::new(InMemoryVectorStore::new());
let embedder = Arc::new(LocalEmbedding::default());
register_builtins_with_memory(&mut registry, store, embedder);
```

---

## Common Issues

**"Permission denied: FileRead"**
You registered the skill but did not grant the `Capability::FileRead { allowed_paths: ... }` capability. Open the audit log to see the denial reason.

**"SSRF blocked: 127.0.0.1 is a private address"**
The `http_fetch` and `web_search` skills block localhost and link-local ranges by default. Set `NetworkAccess { allowed_hosts: vec!["127.0.0.1".into()] }` to override — but only do this for local testing.

**Agent ignores the skill and hallucinates an answer**
Make the description sharper. If the default description says "Calculator", the LLM may skip it for simple sums. Override via a custom skill or strengthen the system prompt: `"Always use the calculator skill for any arithmetic, no matter how simple."`

**"Skill 'web_search' not found"**
Check that `register_builtins()` was called before you built the registry's `Arc`. Once wrapped in `Arc<SkillRegistry>` it is immutable.

**`tool_call` audit entries show `is_error: true`**
Inspect the `content` field in the audit log — skills return structured JSON with an error message you can grep.

---

## What You Built

- An agent that picks the right built-in skill per prompt (`calculator`, `file_read`, `web_search`)
- A capability-scoped `PermissionSet` matching each skill's needs
- An audit trail showing every tool invocation with timestamps and outcomes

---

## Next Steps

- **[Tutorial 3: Multi-Agent Orchestration](./03-multi-agent-orchestration.md)** — run a team of specialized agents instead of one generalist.
- **[Tutorial 5: Custom Skills](./05-custom-skills.md)** — build your own skill with `ToolBuilder` or by implementing the `Skill` trait.
- **[Tutorial 6: Guardrails & Security](./06-guardrails-security.md)** — lock down skill inputs before executing them.
