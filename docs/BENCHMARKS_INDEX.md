# Argentor Benchmarks — Index

> One page. Every benchmark. Every claim traceable to code.
>
> License: AGPL-3.0-only.

Argentor's competitive benchmark program compares Argentor to the four
other leading AI agent frameworks (LangChain, CrewAI, Pydantic AI,
Claude Agent SDK) across six dimensions. Each benchmark is
reproducible from the `benchmarks/` harness in this repo.

---

## Start here

- [`BENCHMARK_SYNTHESIS.md`](BENCHMARK_SYNTHESIS.md) — **the executive
  report.** Consolidated findings, integral ranking with sensitivity
  analysis, commercial narrative, and honest losses.
- [`EVOLUTION_ROADMAP.md`](EVOLUTION_ROADMAP.md) — prioritised
  engineering roadmap derived from benchmark findings. ~20 items across
  security, DX, long-horizon, cost, and new capabilities.

---

## Per-track documents

| Track | Status | What it measures | Argentor result | Doc |
|---|---|---|---|---|
| Phase 0 — Harness | shipped | Reproducible Rust benchmark infrastructure | baseline | `benchmarks/` |
| Phase 1 — Task quality & latency | shipped | Framework overhead across 5 tasks, N=10 paired samples | ~2 ms (best) | [`TASK_BENCHMARKS.md`](TASK_BENCHMARKS.md) |
| Phase 2a — Security (basic) | shipped | Default-posture block rate on 15 prompt / PII / command-injection tasks | 58.3% blocked, 0% FP | [`SECURITY_BENCHMARKS.md`](SECURITY_BENCHMARKS.md) |
| Phase 2b — Cost | shipped | Tokens shipped to LLM across multi-turn / tool-heavy / RAG workloads | 7.9x cheaper on 50-tool tasks | [`COST_BENCHMARKS.md`](COST_BENCHMARKS.md) |
| Phase 3 Track 3 — DX | pending | Developer-experience rubric (setup, docs, errors, examples, type safety) | TBD | [`DX_BENCHMARKS.md`](DX_BENCHMARKS.md) |
| Phase 3 Track 5 — Adversarial | pending | Adversarial-suffix, injection, context-poisoning, tool-abuse attacks | TBD | [`ADVERSARIAL_BENCHMARKS.md`](ADVERSARIAL_BENCHMARKS.md) |
| Phase 4 Track 6 — Long-horizon | shipped | Multi-turn token growth + memory recall across 15 stateful tasks | 1.22x vs LangChain at turn 10 | [`LONG_HORIZON_BENCHMARKS.md`](LONG_HORIZON_BENCHMARKS.md) |
| Phase 5 — Synthesis | this doc | Cross-track executive report + roadmap | consolidated | [`BENCHMARK_SYNTHESIS.md`](BENCHMARK_SYNTHESIS.md) |

Phase 3 docs land as those tracks complete. This index will be updated
with the actual result numbers once they commit.

---

## Top-line claims (cite these)

Every number below cites the source doc. Do not cite the index — cite
the underlying track.

1. **Argentor has the lowest framework overhead** of any framework
   measured — ~2 ms vs Pydantic AI's 11 ms, Claude SDK's 16 ms,
   LangChain's 20 ms, CrewAI's 55 ms. All 20 paired t-tests at N=10,
   p < 0.0001, large effect. — `TASK_BENCHMARKS.md`
2. **Argentor blocks 58.3% of adversarial prompts with zero false
   positives out of the box.** Competitors block 0% in default posture.
   — `SECURITY_BENCHMARKS.md`
3. **Argentor ships 7.9x fewer tokens on 50-tool workloads** (350 vs
   2,750 for LangChain). At 100K req/day, Argentor saves $185 K/year
   vs LangChain and $491 K/year vs CrewAI. — `COST_BENCHMARKS.md`
4. **Argentor uses 1.22x fewer tokens at turn 10** of a 15-task
   long-horizon suite vs LangChain (1.67x vs CrewAI). — `LONG_HORIZON_BENCHMARKS.md`

---

## How to reproduce any benchmark

Every track has a "How to reproduce" section in its doc. The common
pattern:

```bash
# Build Rust harness
cargo build -p argentor-benchmarks --release

# Install all 4 Python runners
for r in langchain_runner crewai_runner pydantic_ai_runner \
         claude_agent_sdk_runner; do
  python3 -m pip install --break-system-packages \
    -e "benchmarks/external/$r[dev]"
done

# Wire runner binaries
export ARGENTOR_LC_RUNNER=$(which argentor-lc-runner)
export ARGENTOR_CREWAI_RUNNER=$(which argentor-crewai-runner)
export ARGENTOR_PYDANTIC_AI_RUNNER=$(which argentor-pydantic-ai-runner)
export ARGENTOR_CLAUDE_AGENT_SDK_RUNNER=$(which argentor-claude-agent-sdk-runner)

# Run any track (subcommand varies)
./target/release/bench --tasks-dir benchmarks/tasks <subcommand> ...
```

Subcommands: `run-all` (task quality/latency), `security`, `cost`,
`long-horizon`, and Phase 3 subcommands once those tracks land.

Raw JSON per run persists to `benchmarks/results/`.

---

## Honest caveats (read before citing)

- **Mock LLM everywhere.** Live-LLM quality is future work (roadmap
  item Q-01).
- **Lower-bound competitor constants.** We source overhead from vendor
  docs and do not inflate. Real deployments often show higher competitor
  overhead.
- **Small N on some tracks.** Phase 2a uses N=3 per task (blocks are
  deterministic). Phase 4 uses N=1 per task.
- **Memory-recall heuristic is coarse.** The 100% recall number in
  long-horizon is a simulation artefact; live-LLM recall is Q-01 /
  L-03 on the roadmap.

---

## License

This document and all benchmark artifacts are released under
AGPL-3.0-only, consistent with the rest of the Argentor project.
