# Argentor Comparison Experiment — Iteration Log

> Each entry is a measured improvement from the baseline in `EXPERIMENT_BASELINE.md`.
> Format: `## Round N — YYYY-MM-DD — <focus area>`

---

## Round 1 — 2026-04-12 — Guardrails Optimization

### Hypothesis
Baseline showed guardrails at 0.541ms per check — the bottleneck. Hypothesis: regex compilation happening on every call instead of cached.

### Investigation
Found in `crates/argentor-agent/src/guardrails.rs:326` that `pii_patterns()` was called by `check_pii()` on every input check, with comment literally claiming "Regex::new is cheap" — incorrect. Each `check_input()` was compiling 4 regexes from scratch.

### Fix
Replaced `fn pii_patterns() -> PiiPatterns` (compiles each call) with a `OnceLock<PiiPatterns>` singleton. Patterns now compile exactly once for the entire process lifetime.

```rust
static PII_PATTERNS: std::sync::OnceLock<PiiPatterns> = std::sync::OnceLock::new();

fn pii_patterns() -> &'static PiiPatterns {
    PII_PATTERNS.get_or_init(|| { ... })
}
```

### Results

| Metric | Baseline | Round 1 | Improvement |
|--------|----------|---------|-------------|
| `input_check_clean` | 0.541ms | **0.003ms** | **180x faster** |
| `input_check_pii_credit_card` | 0.555ms | **0.003ms** | **185x faster** |
| `input_check_prompt_injection` | 0.521ms | **0.003ms** | **174x faster** |
| `input_check_neutral` | 0.547ms | **0.003ms** | **182x faster** |

### Impact at scale
- **Per-turn overhead** went from ~1.5ms (3 guardrail checks) to **~9µs**
- At 1000 turns/sec, saves **1.5 seconds of CPU** every second
- Memory: minor reduction (no per-call allocation of 4 regex structs)

### Tests
All 7 guardrail tests pass. No behavioral change.

### Side note
The original comment "Regex::new is cheap for these patterns" was a documented assumption — false. Lesson: profile, don't assume.

---

## Round 2 — 2026-04-12 — Throughput with Mock LLM

### Hypothesis
Local skill execution (3.99M ops/sec) doesn't compare to competitor numbers (~5 rps full agent loop). Need a `MockLlmBackend` simulating realistic 50ms LLM latency to measure framework overhead vs competitors.

### Implementation
Built `MockLlmBackend` returning `LlmResponse::Done("OK")` after `tokio::time::sleep(50ms)`. Ran two measurements:
1. **Sequential single-turn latency** (50 samples)
2. **Concurrent throughput** (100 parallel agents)

### Results

| Metric | Argentor | Competitor (Python/Rust) | Notes |
|--------|----------|-------------------------|-------|
| Single-turn latency | **52.04ms** | 50ms LLM + framework overhead | **Argentor framework overhead: ~2ms** |
| Concurrent throughput | **1,795 rps** | AutoAgents 4.97, Rig 4.44, LangChain 4.26 | **~360x more throughput** vs best Python |

### Caveat (honest disclosure)
Competitor numbers from DEV.to 2026 are with REAL LLM calls (network latency variance). Our 1795 rps is with mock LLM (deterministic 50ms). The fair comparison is the **framework overhead per turn**: **2ms (Argentor) vs ~250ms (LangChain framework abstraction overhead reported in Speakeasy 2026)**.

### Impact
- Framework overhead per agent turn: **~2ms** (LangChain reports >1s in some cases)
- 100 concurrent agents handled trivially in single process
- Memory per concurrent agent: still negligible

---

---

## Round 3 — TBD — Multi-turn Loop Latency

### Hypothesis
Single-turn metrics don't capture context window growth penalty. Measure 5-turn conversation latency.

---

## Round 4 — TBD — Streaming Throughput

### Hypothesis
SSE streaming should produce tokens at near-LLM rate. Measure chars/sec emitted.

---

## Methodology Notes

- All runs use `cargo run -p argentor-comparison --release`
- 1000 sample iterations per metric (10 warmup)
- Measurements include p50/p95/p99 to catch tail latency regressions
- Each round documents: hypothesis → investigation → fix → results table → impact at scale → tests
- Failed iterations (no improvement) are still documented to avoid repeating bad approaches
