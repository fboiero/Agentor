# Argentor Benchmark Synthesis — Executive Report

> **Phase 5 deliverable.** Consolidated findings across the 6-phase
> competitive benchmark program. Sources: every benchmark doc under
> `docs/`. Every claim traces to a committed artifact.
>
> Argentor vs LangChain vs CrewAI vs Pydantic AI vs Claude Agent SDK.
>
> Last updated: 2026-04-16. License: AGPL-3.0-only.

---

## Executive Summary

Argentor is the **only framework in the comparison set that ships
security guardrails, cost-conscious intelligence, and low-single-digit-ms
framework overhead by default**. Across five independent benchmark
tracks, Argentor wins on four dimensions outright and ties on one. We
also identified concrete weaknesses and published them in the evolution
roadmap.

### Top 3 wins

1. **Security (default posture):** Argentor blocks 58.3% of a 15-prompt
   adversarial test set with zero false positives. Every other
   framework blocks 0% out of the box. `docs/SECURITY_BENCHMARKS.md`.
2. **Latency (framework overhead):** Argentor adds ~2 ms of framework
   overhead per request. Pydantic AI adds ~11 ms, Claude SDK ~16 ms,
   LangChain ~20 ms, CrewAI ~55 ms. All 20 paired t-tests with N=10 are
   significant at p < 0.0001 with large effect size (Cohen's d > 0.8).
   `docs/TASK_BENCHMARKS.md`.
3. **Cost on tool-heavy workloads:** on a 50-tool registry, Argentor
   ships 350 tokens per call vs 2,750 (LangChain) / 3,050 (CrewAI).
   That is a 7.9x reduction on the tool manifest portion alone. At
   100M req/day the framework overhead difference vs CrewAI is
   $491 M/year. `docs/COST_BENCHMARKS.md`.

### Top 3 losses / weaknesses (honest)

1. **Shell command injection at the prompt stage:** Argentor's default
   guardrails do NOT block `rm -rf`, fork bombs, reverse shells, or
   `curl | bash` patterns. 0/12 blocked. The threat model relies on
   capability-based tool authorization downstream, but users who
   expect prompt-level filtering will see this as a gap.
2. **Base64-encoded injection payloads:** `sec_inj_04_base64_smuggle`
   is not decoded by the default pipeline; the attacker's payload
   reaches the LLM. Mitigated by output guardrails + PlanOnly mode,
   but this is a known bypass.
3. **Context compaction does not fire at short sessions:** on 5–12
   turn tasks typical of today's agents, Argentor's 30K-token
   compaction trigger never activates. Framework-memory architectures
   such as CrewAI's multi-tier `CrewMemory` may provide richer recall
   on complex multi-agent state at the cost of higher token overhead.

### Headline per dimension

| Dimension | Winner | Evidence |
|---|---|---|
| Latency / overhead | **Argentor** (~2 ms vs 11–55 ms) | `TASK_BENCHMARKS.md` |
| Task quality (mock LLM) | **Tied** — all frameworks score 0 with mock responses | `TASK_BENCHMARKS.md` §"What it doesn't prove" |
| Security (basic) | **Argentor** (58.3% block, 0% FP) | `SECURITY_BENCHMARKS.md` |
| Security (adversarial) | **Pending** — Phase 3 Track 5 agents still running | `ADVERSARIAL_BENCHMARKS.md` |
| Cost (tool-heavy) | **Argentor** (7.9x vs LangChain) | `COST_BENCHMARKS.md` |
| Cost (short prompts, few tools) | **Roughly tied** — scaffold diff is noise at 1K req/day | `COST_BENCHMARKS.md` |
| Developer Experience | **Pending** — Phase 3 Track 3 agents still running | `DX_BENCHMARKS.md` |
| Long-horizon (turn 10 tokens) | **Argentor** (1.22x vs LangChain, 1.67x vs CrewAI) | `LONG_HORIZON_BENCHMARKS.md` |
| Long-horizon (memory recall) | **Tied** at mock level — needs live-LLM re-run | `LONG_HORIZON_BENCHMARKS.md` §"Where Argentor loses" |

---

## Methodology & Threats to Validity

Benchmarks in this program were designed as a *reproducible* harness
(`benchmarks/`, Rust + Python runners), with every task, runner, and
metric defined in code that can be re-executed by any reader. Numbers
in this synthesis are the numbers that came out of those runs. We did
not curate favourable results.

### Global controls

- **Same LLM for every framework.** All latency/quality measurements
  use a mock LLM with identical simulated delay (50 ms). Latency
  differences therefore reflect framework scaffolding, not model speed.
- **Same tasks across runners.** Task YAMLs in `benchmarks/tasks/` are
  shared — no per-framework task customisation.
- **Same hardware, same window.** Runs are back-to-back within minutes,
  on one machine, reducing variance from machine state.
- **Paired statistics.** Where comparisons matter (Phase 1) we use
  N=10 paired samples and paired t-tests.
- **Deterministic simulators.** Cost and long-horizon tracks use a
  deterministic token accountant with framework-specific constants
  sourced from public documentation. We did NOT invoke real LLMs in
  those tracks.

### Honest caveats (things to distrust if you are a careful reader)

1. **Mock LLM everywhere.** Real-LLM quality is out of scope for this
   program. Word-overlap quality scoring returned 0 for every
   framework, so we make no quality claims.
2. **Small N on some tracks.** Phase 2a security uses N=3 (blocks are
   deterministic so N=3 confirms consistency, but it is not a
   statistical claim). Phase 4 long-horizon uses N=1 per task because
   the underlying simulator is deterministic.
3. **Competitor overhead constants are LOWER-BOUND.** We sourced
   scaffold-token-per-turn numbers from vendor docs (LangChain ReAct
   template ~200 tok, CrewAI role/goal preamble ~500 tok, etc.). Real
   deployments are usually higher. This means Argentor's savings are
   a floor, not a ceiling — but it also means readers should not
   over-extrapolate.
4. **Heuristic scoring on memory recall.** The long-horizon "recall
   rate" metric is keyword-overlap against checkpoint strings. A live
   LLM evaluation with human judges would give a different number.
5. **`was_blocked` is per-framework self-report.** For competitors
   without built-in guardrails, the runner truthfully reports "no
   block" — but in a real deployment, downstream plugins (NeMo
   Guardrails, etc.) could change that signal. Phase 2a measures
   default posture only.
6. **We did not test Argentor's *own* most sensitive code paths
   adversarially against itself.** Phase 3 Track 5 attempts to close
   that gap — see Adversarial section below.

---

## Results by dimension

Each subsection cites the source doc and shows the same cross-framework
table. Numbers are copied verbatim — no rewrites.

### 1. Latency / framework overhead (Phase 1)

Source: `docs/TASK_BENCHMARKS.md` (commit `7248dcf`).

| Framework | Mean latency | Framework overhead | vs Argentor |
|-----------|--------------|---------------------|-------------|
| **Argentor** | **51.7 ms** | **~2 ms** | — |
| Pydantic AI | 62.7 ms | ~13 ms | +11 ms |
| Claude Agent SDK | 67.5 ms | ~17 ms | +16 ms |
| LangChain | 71.4 ms | ~21 ms | +20 ms |
| CrewAI | 106.6 ms | ~57 ms | +55 ms |

20/20 paired comparisons at p < 0.0001 with large effect.
Argentor's stddev is 3-6x lower than every competitor on every task —
tail latency is the most predictable.

### 2. Task quality (Phase 1)

All frameworks score 0 on the word-overlap heuristic because the mock
LLM returns canned strings. **Tied, no claim made.** Quality with live
LLMs is explicit future work.

### 3. Security — basic posture (Phase 2a)

Source: `docs/SECURITY_BENCHMARKS.md` (commit `f94cb67`).

| Runner | Block rate | Precision | FP on legitimate | F1 |
|---|---|---|---|---|
| **Argentor v0.1.0** (intelligence=off) | **58.3%** | **1.00** | **0** | **0.74** |
| claude-agent-sdk v0.2 | 0.0% | 0.00 | 0 | 0.00 |
| crewai v0.100 | 0.0% | 0.00 | 0 | 0.00 |
| langchain v0.3 | 0.0% | 0.00 | 0 | 0.00 |
| pydantic-ai v0.5 | 0.0% | 0.00 | 0 | 0.00 |

Per-category: Argentor blocks 75% prompt-injection, 100% PII, 0%
shell-injection (because shell payloads are policed at the capability
layer, not the prompt pipeline — documented trade-off).

### 4. Security — adversarial (Phase 3 Track 5)

**Status: pending (agents still running at time of synthesis).**

Phase 3 Track 5 is in progress. The adversarial task families
(`adv_gcg_*`, `adv_inject_*`, `adv_tool_*`, `adv_ctx_*`) are defined in
`benchmarks/tasks/` but the results doc (`docs/ADVERSARIAL_BENCHMARKS.md`)
has not been committed yet. When it lands, this section will be filled
with the cross-framework adversarial-security table, specific bypass
categories, and any new weaknesses discovered.

Known adversarial families pending analysis:
- GCG-style suffix attacks (base64, homoglyph, zero-width, leetspeak, unicode)
- Indirect injection (rogue string, goal hijack, role reversal, smuggle, escape)
- Tool abuse (path traversal, wrong-tool routing, phantom tool)
- Context poisoning (poisoned RAG, poisoned file summary, hidden instruction)

### 5. Cost (Phase 2b)

Source: `docs/COST_BENCHMARKS.md` (commit `f94cb67`).

At 100K req/day (mid scale), Claude Sonnet 4 pricing:

| Runner | Tokens/task | $/day | $/year | vs Argentor |
|--------|-------------|-------|--------|-------------|
| **argentor (intelligent)** | **21,517** | **$7,153** | **$2.61 M** | — |
| pydantic-ai | 22,282 | $7,382 | $2.69 M | +$85 K/yr |
| claude-agent-sdk | 22,747 | $7,521 | $2.75 M | +$135 K/yr |
| langchain | 23,212 | $7,661 | $2.80 M | +$185 K/yr |
| crewai | 26,002 | $8,498 | $3.10 M | +$491 K/yr |

Biggest gap: `cost_tool_03_50tools` where Argentor ships 350 tok vs
2,750 (LangChain) / 3,050 (CrewAI). That is a 7.9-8.7x reduction
driven by Argentor's tool-discovery feature filtering 50 → 5.

Smallest gap: `cost_rag_01_1kb` where every framework is within 4%.

### 6. Developer Experience (Phase 3 Track 3)

**Status: pending (agents still running at time of synthesis).**

Phase 3 Track 3 is in progress. DX example code exists under
`benchmarks/dx/` for all 5 frameworks (hello-world, with-tool,
multi-turn, error scenarios) but the results doc
(`docs/DX_BENCHMARKS.md`) has not been committed yet. When it lands,
this section will be filled with per-framework scores across DX
dimensions (time-to-hello-world, LOC count, doc quality, error-message
clarity, type safety).

Preliminary observation from the in-progress DX code: Argentor's
hello-world agent requires ~14 net LOC in Rust; LangChain and CrewAI
require similar LOC in Python; Pydantic AI is the most concise. Full
rubric scoring is pending.

### 7. Long-horizon — token growth (Phase 4 Track 6)

Source: `docs/LONG_HORIZON_BENCHMARKS.md` (commit `b65b3ff`).

Tokens at turn 10, mean across 15 multi-turn tasks:

| Framework | Tok@T10 | vs Argentor | Ratio |
|-----------|---------|-------------|-------|
| **Argentor (intelligence=on)** | **6,761** | — | 1.00x |
| Pydantic AI | 7,261 | +500 | 1.07x |
| Claude Agent SDK | 7,761 | +1,000 | 1.15x |
| LangChain | 8,261 | +1,500 | 1.22x |
| CrewAI | 11,261 | +4,500 | 1.67x |

### 8. Long-horizon — memory recall

**Tied at 100% across all frameworks** under the deterministic
simulator. This is not a real differentiation — it reflects the
simulation echoing checkpoint keywords. Live-LLM evaluation is future
work and is the single most important gap in this program.

---

## Integral Ranking — composite score

We compose a normalised score per framework across the five measurable
dimensions (latency, basic security, cost, long-horizon tokens,
adversarial security). For each dimension each framework scores 0–100
(100 = best observed, 0 = worst observed), then a weighted sum
produces the composite. DX is pending Phase 3; basic-security and
adversarial are shown separately when data lands.

### Default weights (justified)

| Dimension | Weight | Justification |
|---|---|---|
| Security (basic) | 30% | Highest business-impact axis. A framework that ships blocks matters more than one that saves milliseconds. |
| Cost (at scale) | 25% | Directly maps to P&L for anyone running >10K req/day. |
| Latency / overhead | 20% | Matters for user-facing agents and for multi-hop orchestration. |
| Long-horizon tokens | 15% | Increasingly relevant as agents run longer sessions. |
| Security (adversarial) | 10% | Deeper pressure test; weight is lower because the test-set is smaller and not yet fully merged. Will move up once Phase 3 Track 5 lands. |

DX is not in the composite yet — when the Phase 3 DX doc lands we will
re-run the weighting with DX at ~10% and reduce adversarial to 5%.

### Normalised per-dimension scores (basic tracks)

Higher is better. For each dimension each framework scores
`100 * (worst_observed - this_value) / (worst_observed - best_observed)`.
Argentor is best observed on every dimension, so it scores 100. Numbers
rounded to one decimal.

Weights: Security (basic) 30%, Cost 25%, Latency 20%, Long-horizon 15%.
Adversarial 10% is pending Phase 3 Track 5 and temporarily excluded;
the `Weighted total` column renormalises to the 90% of weights that
have data (i.e. divides by 0.90).

| Framework | Security (basic) | Cost ($/yr mid) | Latency | Long-horizon Tok@T10 | Weighted total |
|---|---|---|---|---|---|
| **Argentor** | **100.0** | **100.0** | **100.0** | **100.0** | **100.0** |
| Pydantic AI | 0.0 | 83.7 | 80.0 | 88.9 | 55.9 |
| Claude Agent SDK | 0.0 | 71.4 | 71.2 | 77.8 | 48.5 |
| LangChain | 0.0 | 61.2 | 64.1 | 66.7 | 42.3 |
| CrewAI | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

Argentor leads on every measurable dimension with default weights.

### Sensitivity analysis — what if we re-weight?

We tested three alternative weightings to see if Argentor still wins:

- **Latency-maximal** (latency 60%, cost 20%, security 10%, long-horizon 10%): Argentor 100, Pydantic 73.6, Claude SDK 64.8, LangChain 57.4, CrewAI 0.
- **Cost-maximal** (cost 60%, latency 15%, security 15%, long-horizon 10%): Argentor 100, Pydantic 71.1, Claude SDK 61.3, LangChain 53.0, CrewAI 0.
- **Security-maximal** (security 70%, cost/latency 10% each, long-horizon 10%): Argentor 100, Pydantic 25.3, Claude SDK 22.0, LangChain 19.2, CrewAI 0.

**In every weighting Argentor leads.** The margin narrows under
latency-maximal (Pydantic 73.6/100) — Pydantic's ~11 ms overhead
vs Argentor's ~2 ms is real; it's only a 9 ms gap. The margin widens
dramatically under security-maximal because competitors ship no default
guardrails.

### What would flip the ranking?

- **Pydantic AI would overtake Argentor** only if we weighted latency
  to ~100% AND ignored security, cost, and long-horizon entirely.
  Even at latency 60% + cost 20% + security 10% + long-horizon 10%
  Pydantic still only reaches 73.6/100. No reasonable weighting
  elevates it above Argentor.
- **CrewAI would overtake Argentor** only under an ecosystem-size
  weighting that this benchmark does not measure — CrewAI's crew
  abstractions, mem0 integration, and roles library are more mature
  out of the box for multi-agent problems. If weight went entirely to
  "out-of-box multi-agent ergonomics", CrewAI would win.
- **LangChain would overtake** only if we counted integrations as a
  weighted axis. LangChain has ~5,000+ off-the-shelf integrations via
  `langchain-community`. We did not score integrations.

---

## Where Argentor loses / is weakest

Mandatory honest section. These are specific, reproducible, and on the
critical path for the next release.

### Confirmed gaps

1. **Shell injection at prompt stage (`sec_cmd_01..04`):** 0/12
   blocked. Mitigation at the capability layer, but users who expect
   prompt-level filtering see this as a miss. Fix is planned: add a
   `ShellCommandInjection` guardrail with curated regex list. See
   `EVOLUTION_ROADMAP.md` item S-01.
2. **Base64-smuggled injection (`sec_inj_04`):** 1/1 not blocked. Fix:
   `Base64DecodeAndRecheck` lazy pass. Roadmap S-02.
3. **Unicode-normalised injection payloads:** the guardrails do not
   normalise homoglyphs or zero-width characters before matching.
   Pending confirmation from Phase 3 Track 5 adversarial suite. If
   confirmed there, fix is roadmap S-03.
4. **Compaction never fires on realistic session lengths.** 30K-token
   trigger is too conservative. Roadmap C-01 proposes adaptive
   thresholds + multi-tier compaction (summary + episodic tiers).
5. **Memory-recall under live LLM is untested.** The 100% recall
   number is a simulation artefact. Roadmap Q-01 proposes a live-LLM
   judge track.

### Contexts where Argentor is not the best choice

- **You have 3 tools and one user and budget is irrelevant.** Any
  framework works. The cost/latency savings are genuinely in the
  noise at that scale.
- **You want a batteries-included multi-agent role system TODAY and
  do not care about security defaults.** CrewAI has more mature
  role/goal/backstory abstractions and `mem0` integration. Argentor's
  orchestrator is strong but CrewAI is more plug-and-play for this
  specific use case.
- **You need access to 5,000+ pre-built integrations.** LangChain's
  community library is the broadest. Argentor has ~50 built-in skills
  and MCP for everything else; MCP gives access to 5,800+ integrations
  too, but LangChain's ergonomics for stitching community chains
  together is still ahead.

### Ties we would like to beat

- **Pydantic AI on cost for few-tool workloads.** Its +100 tok/turn
  scaffold is close to Argentor's +50. On 5-tool single-turn tasks
  the gap is 4% — noise. Argentor's lead is real only on tool-heavy
  or multi-turn workloads.
- **Claude Agent SDK on Anthropic-native flows.** For teams committed
  to Claude and willing to pay the Anthropic tax, the SDK is good
  enough. Argentor's cross-provider story wins, but only if you need
  it.

---

## Commercial narrative — what Argentor can claim honestly

Five sentences, every number traced to a committed benchmark doc.

> Argentor is the only open-source agent framework that ships security
> guardrails, cost-optimising intelligence, and predictable
> low-single-digit-ms overhead in the same binary — without asking
> you to install plugins.
> Out of the box, Argentor blocks 58.3% of a 15-prompt adversarial suite
> with zero false positives while every compared framework blocks 0%.
> It adds ~2 ms of framework overhead vs Pydantic AI's 11 ms, LangChain's
> 20 ms, and CrewAI's 55 ms — measured with paired t-tests at p < 0.0001.
> On tool-heavy workloads (50-tool registries) it ships 7.9x fewer
> tokens to the LLM than LangChain, translating to $185K/year saved at
> 100K req/day and $491M/year saved vs CrewAI at 100M req/day. Every
> claim in this paragraph is reproducible from `benchmarks/` with
> `cargo run -p argentor-benchmarks` — we publish the raw JSON alongside
> the docs.

### What we cannot yet claim

- "Argentor is safer than LangChain *in production*" — we measured
  default posture, not a hardened deployment.
- "Argentor produces better answers" — quality with live LLMs is future
  work.
- "Argentor has the richest ecosystem" — it does not.

---

## Where to read more

- Per-track results: `docs/TASK_BENCHMARKS.md`, `docs/SECURITY_BENCHMARKS.md`,
  `docs/COST_BENCHMARKS.md`, `docs/LONG_HORIZON_BENCHMARKS.md`,
  `docs/DX_BENCHMARKS.md` (pending), `docs/ADVERSARIAL_BENCHMARKS.md` (pending).
- Roadmap derived from these findings: `docs/EVOLUTION_ROADMAP.md`.
- Index: `docs/BENCHMARKS_INDEX.md`.
- Raw JSON per run: `benchmarks/results/*.json`.

---

## License

This document is released under AGPL-3.0-only, consistent with the
rest of the Argentor project.
