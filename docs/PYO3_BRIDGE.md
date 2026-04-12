# PyO3 Bridge — Dynamic Python & LangChain Interop

The `argentor-python` crate is a PyO3/maturin bridge that lets an Argentor
agent (Rust) dynamically load and invoke **arbitrary Python callables** --
including LangChain tools -- as first-class skills.

This document explains when to use the bridge, how it compares with the MCP
alternative, how to set it up, and the relevant trade-offs.

---

## 1. When to use the PyO3 bridge vs MCP

Argentor supports two mechanisms for calling out to non-Rust code:

| Dimension | PyO3 bridge (this crate) | MCP (`argentor-mcp`) |
|---|---|---|
| Transport | In-process, FFI | JSON-RPC 2.0 over stdio / WebSocket |
| Process model | Single process (Python embedded in Rust) | Separate process per server |
| Language | Python only | Language-agnostic |
| Latency | Sub-millisecond per call | Milliseconds (IPC) |
| Isolation | None (shared address space) | Strong (OS process boundary) |
| Failure blast radius | Crashes take down the Argentor process | Confined to the MCP server |
| Concurrency | Bound by the GIL | True parallelism across servers |
| Deployment | Requires Python at runtime | Self-contained binaries |
| Best for | Tight integrations with the LangChain / LlamaIndex ecosystem; research prototypes | Production deployments; untrusted or third-party tools |

**Rule of thumb:**

- Prototyping, research, LangChain interop → PyO3 bridge.
- Production, untrusted tools, high throughput → MCP.

If you're unsure, start with MCP. Migrate to PyO3 only when the call-rate or
data-volume warrants the loss of isolation.

---

## 2. Setup

### 2.1 Install build prerequisites

```bash
# 1. Rust toolchain (stable, edition 2021+).
rustup update stable

# 2. Python 3.9+ with headers. On macOS via Homebrew:
brew install python@3.12

# 3. maturin (Python packaging tool for Rust extensions).
pip install maturin
```

### 2.2 Build the extension module

The `argentor-python` crate is **intentionally excluded from the cargo
workspace** because PyO3 cdylibs interfere with `cargo test --workspace`.
Build it in-place:

```bash
cd crates/argentor-python
maturin develop          # builds in debug, installs into the current venv
# or
maturin build --release  # builds a wheel in target/wheels/
```

After `maturin develop` finishes, `import argentor` works in the active venv.

### 2.3 Verify

```python
import argentor
print(argentor.version())           # "0.1.0"
print(argentor.available_skills())  # [...]
```

---

## 3. Loading custom Python functions as Argentor skills

The dynamic loader is exposed as two types in `argentor_python::dynamic_load`:

- `PythonToolConfig` — declarative description of what to import and call.
- `PythonToolSkill` — the wrapper that resolves the callable on demand.

Minimal Rust usage:

```rust
use argentor_python::{PythonToolConfig, PythonToolSkill};
use serde_json::json;

let cfg = PythonToolConfig {
    module: "math".into(),
    callable: "sqrt".into(),
    name: "sqrt".into(),
    description: "Square-root of a non-negative number".into(),
    parameters_schema: json!({
        "type": "object",
        "properties": { "x": { "type": "number" } },
        "required": ["x"],
    }),
};

let skill = PythonToolSkill::new(cfg);
skill.validate()?;                 // verifies importability + callable
let out = skill.call(&json!([16.0]))?;
assert_eq!(out, "4.0");
```

### 3.1 Argument conventions

JSON argument shape maps to Python call shape:

| JSON value | Python call |
|---|---|
| `{"a": 1, "b": 2}` | `f(a=1, b=2)` (kwargs) |
| `[1, 2, 3]` | `f(1, 2, 3)` (positional) |
| `42` / `"x"` / `true` | `f(42)` (single positional) |

Return values are coerced with `str(...)` to keep the skill interface
uniform.

### 3.2 Discovery

```rust
use argentor_python::discover_python_tools;

// Scan a whole module. Pass a prefix to filter.
let tools = discover_python_tools("my_custom_module", Some("tool_"))?;
for t in tools {
    println!("{} -> {}", t.name, t.qualified_name());
}
```

Names starting with `_` are skipped. Only public callables are reported.

---

## 4. Loading LangChain tools

Install LangChain in the same virtualenv used for `maturin develop`:

```bash
pip install langchain langchain-community
# optional provider extras
pip install langchain-openai duckduckgo-search
```

Then, from Rust:

```rust
use argentor_python::{LangChainAdapter, LangChainCategory};

// Check availability.
if LangChainAdapter::is_available() {
    println!("langchain {}", LangChainAdapter::version().unwrap_or_default());
}

// Load a tool by class name -- tries langchain.tools, langchain_community.tools,
// and langchain_core.tools in order.
let skill = LangChainAdapter::load_tool("DuckDuckGoSearchRun")?;

// Or use a category hint for a faster lookup.
let emb = LangChainAdapter::load_tool_in_category(
    "OpenAIEmbeddings",
    LangChainCategory::Embeddings,
)?;

// List the canonical tool names Argentor knows about.
let all = LangChainAdapter::list_available_tools()?;
```

The adapter encodes the LangChain module layout as of early 2026. If LangChain
reshuffles classes again, override the search path explicitly via
`PythonToolConfig::module`.

---

## 5. Performance considerations

### 5.1 The GIL

Every PyO3 call acquires the **Global Interpreter Lock**. This means:

- Two Argentor worker agents that both invoke Python tools will **serialize**
  on the GIL, even on a multi-core machine.
- Long-running Python tools block all other Python calls in the same Argentor
  process.
- If your workload is embarrassingly parallel, prefer MCP (one server per
  worker) or use Python's `multiprocessing` inside the tool.

### 5.2 Marshalling overhead

Every call pays the cost of JSON → Python dict → call → `str()` → UTF-8
decode. This is typically sub-millisecond but shows up in hot loops.

Mitigations:

- Batch multiple items into a single call.
- Cache parsed JSON schemas in `PythonToolConfig` (already done).
- Keep the interpreter warm across calls (PyO3 does this automatically).

### 5.3 Cold-start

The first `Python::with_gil` pays for interpreter initialization (~tens of
ms). Subsequent calls are cheap. Don't measure cold-start as if it were
steady-state latency.

---

## 6. Limitations

1. **Python must be installed in the deployment environment.** This is fine
   for dev and many production setups but rules out fully-static Rust
   binaries.
2. **Not suitable for high-throughput.** A busy Argentor agent calling a PyO3
   tool thousands of times per second will be GIL-bound.
3. **No sandboxing.** A misbehaving Python tool can `os.system("rm -rf /")`
   or exhaust memory. Use MCP + OS sandboxing for untrusted code.
4. **Python errors bubble up as opaque strings.** The adapter coerces
   exceptions into `Result<_, String>`; structured error types require more
   work in a future revision.
5. **LangChain compatibility is best-effort.** LangChain reorganizes its
   modules frequently; `LangChainAdapter::load_tool` probes three canonical
   paths but cannot fix upstream renames.
6. **`argentor-python` is excluded from `cargo test --workspace`.** Run
   `cargo test` inside the crate directory instead.

---

## 7. Security checklist

Before deploying a PyO3-loaded tool in production:

- [ ] Audit the Python source of every loaded callable.
- [ ] Pin the Python version and package versions (`requirements.txt` +
      lockfile).
- [ ] Run the Argentor process with the narrowest possible OS permissions.
- [ ] Prefer MCP for any third-party tool.
- [ ] Wrap the Argentor process in a container with restricted syscalls
      (seccomp) where feasible.
- [ ] Enable the Argentor guardrail engine for all LLM I/O that involves
      PyO3-loaded tools.

---

## 8. File map

| Path | Purpose |
|---|---|
| `crates/argentor-python/src/lib.rs` | Root bindings + existing skills |
| `crates/argentor-python/src/dynamic_load.rs` | `PythonToolConfig`, `PythonToolSkill`, discovery |
| `crates/argentor-python/src/langchain_compat.rs` | `LangChainAdapter`, `LangChainCategory` |
| `crates/argentor-python/examples/load_langchain_tool.py` | Python-side usage example |
| `docs/PYO3_BRIDGE.md` | This document |

---

## 9. Further reading

- PyO3 user guide: <https://pyo3.rs/>
- Maturin: <https://www.maturin.rs/>
- LangChain tools concept: <https://python.langchain.com/docs/concepts/tools/>
- Argentor MCP client: `crates/argentor-mcp/`
