# Tutorial 5: Creating Custom Skills

> Write your own skill with `ToolBuilder` in 10 lines, or go full `Skill` trait for maximum control. Plus: sandboxed WASM plugins.

Argentor's 50+ built-in skills cover common use cases, but every project needs custom tools eventually — internal APIs, proprietary databases, domain-specific calculators, workflow-specific transformations.

Argentor gives you three levels of skill creation:

1. **`ToolBuilder`** — fluent builder, perfect for simple tools.
2. **`Skill` trait** — implement it yourself for full control (custom JSON schemas, async I/O, nested calls).
3. **WASM plugin** — compile to `.wasm` and load dynamically for sandboxed, untrusted code.

---

## Prerequisites

- Completed [Tutorial 1](./01-first-agent.md) and [Tutorial 2](./02-using-skills.md)
- Familiarity with async Rust (`async fn`, `Arc`, `Box<dyn Trait>`)

---

## 1. Quick Skill with `ToolBuilder`

The fluent builder from `argentor_skills::ToolBuilder` covers 80% of use cases:

```rust
use argentor_skills::ToolBuilder;
use argentor_skills::SkillRegistry;
use std::sync::Arc;

let greet_tool = ToolBuilder::new("greet")
    .description("Greet a user by name with an optional custom greeting")
    .param("name", "string", "The user's name", true)
    .param("greeting", "string", "Custom greeting (defaults to 'Hello')", false)
    .handler(|args| {
        let name = args["name"].as_str().unwrap_or("World");
        let greeting = args["greeting"].as_str().unwrap_or("Hello");
        Ok(format!("{greeting}, {name}!"))
    })
    .build();

let mut registry = SkillRegistry::new();
registry.register(greet_tool);
```

That is it. The LLM now sees `greet` as an available tool with a proper JSON schema.

### Async handler

For I/O-bound tools:

```rust
use argentor_skills::ToolBuilder;

let weather_tool = ToolBuilder::new("weather")
    .description("Get current weather for a city")
    .param("city", "string", "City name", true)
    .async_handler(|args| async move {
        let city = args["city"].as_str().ok_or_else(|| {
            argentor_core::ArgentorError::Skill("missing city".into())
        })?;

        let url = format!("https://wttr.in/{city}?format=3");
        let resp = reqwest::get(&url).await
            .map_err(|e| argentor_core::ArgentorError::Skill(e.to_string()))?
            .text().await
            .map_err(|e| argentor_core::ArgentorError::Skill(e.to_string()))?;

        Ok(resp)
    })
    .build();
```

### Declaring capabilities

If your tool needs permission-gated I/O, declare it:

```rust
use argentor_security::Capability;

let shell_wrapper = ToolBuilder::new("run_safe_script")
    .description("Runs a pre-approved internal script")
    .param("args", "array", "Arguments to pass", false)
    .capability(Capability::ShellExec { allowed_commands: vec!["/opt/scripts/run.sh".into()] })
    .handler(|_args| {
        let output = std::process::Command::new("/opt/scripts/run.sh")
            .output()
            .map_err(|e| argentor_core::ArgentorError::Skill(e.to_string()))?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    })
    .build();
```

The runner will refuse to execute this tool unless the `PermissionSet` was granted `Capability::ShellExec { allowed_commands: [...] }` with a matching path.

---

## 2. Full `Skill` Trait Implementation

When you need custom state, nested calls, or non-JSON parameter validation, implement the trait directly:

```rust
use argentor_skills::skill::{Skill, SkillDescriptor};
use argentor_core::{ArgentorError, ArgentorResult, ToolCall, ToolResult};
use argentor_security::Capability;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

/// A skill that queries an internal customer database.
pub struct CustomerLookupSkill {
    db_pool: Arc<sqlx::PgPool>,
}

impl CustomerLookupSkill {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { db_pool: pool }
    }
}

#[async_trait]
impl Skill for CustomerLookupSkill {
    fn descriptor(&self) -> SkillDescriptor {
        SkillDescriptor {
            name: "customer_lookup".into(),
            description: "Look up a customer by ID or email. Returns name, tier, and account balance.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "customer_id": { "type": "string", "description": "UUID" },
                    "email": { "type": "string", "format": "email" }
                },
                "oneOf": [
                    { "required": ["customer_id"] },
                    { "required": ["email"] }
                ]
            }),
            capabilities: vec![Capability::DatabaseQuery],
        }
    }

    async fn execute(&self, call: ToolCall) -> ArgentorResult<ToolResult> {
        let args = call.arguments;

        let row = if let Some(id) = args.get("customer_id").and_then(|v| v.as_str()) {
            sqlx::query!("SELECT name, tier, balance FROM customers WHERE id = $1", id)
                .fetch_optional(&*self.db_pool)
                .await
                .map_err(|e| ArgentorError::Skill(e.to_string()))?
        } else if let Some(email) = args.get("email").and_then(|v| v.as_str()) {
            sqlx::query!("SELECT name, tier, balance FROM customers WHERE email = $1", email)
                .fetch_optional(&*self.db_pool)
                .await
                .map_err(|e| ArgentorError::Skill(e.to_string()))?
        } else {
            return Ok(ToolResult {
                call_id: call.id,
                content: json!({"error": "either customer_id or email is required"}).to_string(),
                is_error: true,
            });
        };

        let content = match row {
            Some(r) => json!({
                "name": r.name,
                "tier": r.tier,
                "balance": r.balance,
            }).to_string(),
            None => json!({"error": "customer not found"}).to_string(),
        };

        Ok(ToolResult {
            call_id: call.id,
            content,
            is_error: false,
        })
    }
}
```

Register it:

```rust
let pool = Arc::new(sqlx::PgPool::connect(&db_url).await?);
registry.register(Arc::new(CustomerLookupSkill::new(pool)));
```

---

## 3. Capabilities Cheat Sheet

All the capabilities you can declare live in `argentor_security::Capability`:

| Capability | Purpose |
|------------|---------|
| `FileRead { allowed_paths }` | Read files under specific paths |
| `FileWrite { allowed_paths }` | Write files under specific paths |
| `ShellExec { allowed_commands }` | Run shell commands |
| `NetworkAccess { allowed_hosts }` | Make HTTP/TCP requests |
| `DatabaseQuery` | Query a database |
| `CryptoOp` | Perform cryptographic operations |
| `ProcessSpawn` | Spawn subprocesses |

An empty `allowed_*` vector means "allow all, subject to defense-in-depth". For production, always use explicit allowlists.

---

## 4. Register and Run

```rust
use argentor_agent::{AgentRunner, LlmProvider, ModelConfig};
use argentor_security::{AuditLog, Capability, PermissionSet};
use argentor_session::Session;
use std::path::PathBuf;
use std::sync::Arc;

let mut registry = SkillRegistry::new();
registry.register(greet_tool);
registry.register(weather_tool);
registry.register(Arc::new(CustomerLookupSkill::new(pool)));

let mut permissions = PermissionSet::new();
permissions.grant(Capability::NetworkAccess { allowed_hosts: vec!["wttr.in".into()] });
permissions.grant(Capability::DatabaseQuery);

let config = ModelConfig {
    provider: LlmProvider::Claude,
    model_id: "claude-sonnet-4-20250514".into(),
    api_key: std::env::var("ANTHROPIC_API_KEY")?,
    api_base_url: None,
    temperature: 0.3,
    max_tokens: 2048,
    max_turns: 5,
    fallback_models: vec![],
    retry_policy: None,
};

let runner = AgentRunner::new(
    config,
    Arc::new(registry),
    permissions,
    Arc::new(AuditLog::new(PathBuf::from("./audit"))),
);

let mut session = Session::new();
let response = runner.run(
    &mut session,
    "Greet Alice, then check the weather in Buenos Aires, and look up customer with email alice@example.com.",
).await?;

println!("{response}");
```

---

## 5. WASM Plugins (Sandboxed)

For code you do not fully trust — third-party plugins, community contributions, multi-tenant platforms — compile to WebAssembly and load at runtime. The WASM plugin runs inside wasmtime with WASI, isolated from the host.

### 5.1 Author a plugin

In a separate crate (`my-plugin`):

```toml
# Cargo.toml
[package]
name = "my-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust
// src/lib.rs
#[no_mangle]
pub extern "C" fn execute(args_ptr: *const u8, args_len: usize) -> *const u8 {
    let args_slice = unsafe { std::slice::from_raw_parts(args_ptr, args_len) };
    let args_str = std::str::from_utf8(args_slice).unwrap_or("{}");
    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();

    let x = args["x"].as_f64().unwrap_or(0.0);
    let y = args["y"].as_f64().unwrap_or(0.0);

    let result = serde_json::json!({ "sum": x + y });
    let s = result.to_string();
    // In real code: use a proper host-side memory allocator + length protocol.
    let bytes = s.into_bytes();
    Box::leak(bytes.into_boxed_slice()).as_ptr()
}
```

Compile it:

```bash
cargo build --target wasm32-wasip1 --release
# Output: target/wasm32-wasip1/release/my_plugin.wasm
```

### 5.2 Load the plugin in Argentor

```rust
use argentor_skills::{SkillConfig, SkillLoader, WasmSkillRuntime};

let runtime = WasmSkillRuntime::new()?;
let plugin_config = SkillConfig {
    name: "wasm_adder".into(),
    path: "./plugins/my_plugin.wasm".into(),
    description: "Adds two numbers (WASM plugin)".into(),
    capabilities: vec![],
};
let loader = SkillLoader::new(runtime);
let skill = loader.load(&plugin_config).await?;
registry.register(skill);
```

### 5.3 Vet the plugin first

For untrusted sources, use `SkillVetter`:

```rust
use argentor_skills::{SkillManifest, SkillVetter};

let vetter = SkillVetter::new()
    .with_max_size(5 * 1024 * 1024)    // 5 MB cap
    .with_signature_verification()
    .with_static_analysis();

let manifest = SkillManifest::from_file("./plugins/my_plugin.wasm.manifest.json")?;
let result = vetter.verify("./plugins/my_plugin.wasm", &manifest).await?;

if !result.approved {
    eprintln!("Plugin rejected: {:?}", result.issues);
    return Ok(());
}
```

`VettingResult` reports signature validity, binary size, flagged WASM imports, and any capability mismatches.

---

## 6. Markdown Skills (Prompt-Only)

Sometimes a "skill" is really just a reusable prompt. Argentor's `MarkdownSkill` lets you author prompts in `.md` files with YAML frontmatter:

```markdown
---
name: rust_reviewer
description: Review Rust code for idiomaticity, safety, and clippy lints.
parameters:
  code:
    type: string
    description: The Rust source to review.
    required: true
---

You are a senior Rust reviewer. Review the following code:

```rust
{{code}}
```

Report:
1. Any use of `unwrap()`, `expect()`, `panic!` outside tests.
2. Missing doc comments on public items.
3. Opportunities to simplify with `?`, `map`, or `filter_map`.
```

Load it:

```rust
use argentor_skills::MarkdownSkillLoader;

let loader = MarkdownSkillLoader::new("./skills/markdown");
let skills = loader.load_all().await?;
for skill in skills.skills {
    registry.register(skill);
}
```

The LLM sees `rust_reviewer` as a callable tool, and when invoked the prompt template is rendered with `{{code}}` substituted — perfect for composing agent workflows without Rust code.

---

## 7. Publish to the Skill Marketplace

Argentor includes a simple marketplace (`argentor_skills::marketplace`) for discovering and sharing skills:

```rust
use argentor_skills::marketplace::{MarketplaceClient, MarketplaceSearch};

let client = MarketplaceClient::new("https://marketplace.argentor.dev");

// Search
let results = client.search(MarketplaceSearch {
    query: Some("postgres".into()),
    ..Default::default()
}).await?;

// Install
for entry in results.entries {
    client.install(&entry.id, &mut registry).await?;
}
```

To publish your own:

```bash
# Sign your WASM artifact with ed25519
argentor marketplace sign ./my_plugin.wasm --key ./mykey.pem

# Publish
argentor marketplace publish ./my_plugin.wasm --manifest manifest.json
```

---

## Common Issues

**"Skill execution denied: capability not granted"**
You registered the skill but forgot to grant the matching capability in `PermissionSet`. Check the `capabilities` vec on the descriptor.

**LLM never calls your tool**
The description is too vague. Describe *when* to use it, not just *what* it does. Good: `"Look up a customer record by ID or email. Use this for customer service inquiries or billing questions."`

**`ToolBuilder` panics at build time**
You forgot to call `.handler()` or `.async_handler()`. The builder requires at least one.

**WASM plugin crashes host**
It cannot — wasmtime sandboxes the plugin process. But a buggy plugin can return malformed JSON; always validate with `serde_json::from_str` in your host glue.

**Tool arguments fail JSON schema validation**
The LLM sometimes produces slightly off-spec JSON. Loosen your schema (`additionalProperties: true`) or parse defensively in your handler.

**Handler panics → agent crashes**
Return `Err(ArgentorError::Skill(...))` instead of panicking. The runner captures errors and passes them back to the LLM, which will often recover.

---

## What You Built

- A one-liner custom tool via `ToolBuilder`
- A full-featured skill hitting a real database
- A sandboxed WASM plugin loaded dynamically
- A prompt-only Markdown skill

---

## Next Steps

- **[Tutorial 6: Guardrails & Security](./06-guardrails-security.md)** — add input validation on your custom skill inputs.
- **[Tutorial 8: MCP Integration](./08-mcp-integration.md)** — expose your custom skills as MCP tools for other agents to call.
- **[Tutorial 10: Observability](./10-observability.md)** — instrument custom skills with tracing.
