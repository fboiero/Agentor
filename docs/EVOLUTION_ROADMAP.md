# Argentor Evolution Roadmap — Benchmark-Driven

> Priority-ordered engineering roadmap derived directly from the findings
> in `docs/BENCHMARK_SYNTHESIS.md`. Every item points to the benchmark
> row that exposed the gap, proposes a change, estimates impact and
> effort, and where warranted, a GitHub issue.
>
> License: AGPL-3.0-only.

---

## How to read this doc

- **Priority**: P0 (this release), P1 (next release), P2 (on the
  backlog).
- **Effort**: S (< 1 week), M (1–3 weeks), L (> 3 weeks).
- **Impact**: expressed as the benchmark axis that moves, with a rough
  directional estimate.
- **Source**: the benchmark doc row that exposed the gap.
- **Issue**: GitHub issue link where opened (P0 and P1 items only).

Items are grouped by theme, not by priority. Use the Priority column
to order engineering attention.

---

## Security gaps

### S-01 — Default shell-injection guardrail

- **Benchmark revealed:** `sec_cmd_01..04` — 0/12 shell payloads blocked
  (`rm -rf`, fork bomb, reverse shell, curl-pipe-bash). Source:
  `SECURITY_BENCHMARKS.md` §"Per-task detail".
- **Proposed change:** add a `ShellCommandInjection` guardrail rule
  with a curated regex list. Ship disabled-by-default behind a feature
  flag first, then promote to default after measuring false-positive
  rate on 1K legitimate-sysadmin prompts.
- **Expected impact:** Argentor's basic-security block rate rises from
  58.3% toward ~83% (raising command-injection family from 0% to ~100%).
  No effect on FP if regex is tight.
- **Effort:** S.
- **Priority:** P0. Biggest, cheapest win on the security dimension.

### S-02 — Base64 decode-and-recheck pass

- **Benchmark revealed:** `sec_inj_04_base64_smuggle` — 0/1 blocked.
  Source: `SECURITY_BENCHMARKS.md`.
- **Proposed change:** add a lazy decoder that detects base64-like
  substrings, decodes them, and re-runs the input-pipeline rules
  against the decoded payload. Only activates on substrings that pass
  a "looks like base64" heuristic to avoid overhead.
- **Expected impact:** closes one specific bypass; potentially 2–3
  more in the Phase 3 Track 5 adversarial suite once it lands.
- **Effort:** M.
- **Priority:** P0.

### S-03 — Unicode normalisation pre-step

- **Benchmark revealed:** Phase 3 Track 5 adversarial families
  (`adv_gcg_02_homoglyph_cyrillic`, `adv_gcg_03_zero_width_chars`,
  `adv_gcg_05_unicode_normalization`). Pending official numbers at
  time of writing. Source: `docs/ADVERSARIAL_BENCHMARKS.md` (pending).
- **Proposed change:** NFC-normalise input and strip zero-width
  characters before guardrail matching. Track 5 adversarial suite
  has three direct tasks for this.
- **Expected impact:** closes homoglyph / zero-width / leetspeak
  bypasses on the Phase 3 Track 5 suite.
- **Effort:** S.
- **Priority:** P0.

### S-04 — Output-side PII redaction symmetry

- **Benchmark revealed:** Argentor blocks PII on *input* at 100% but
  does not redact it on *output*. If the LLM emits PII in its
  response, it flows to the client. Source: inspection of
  `crates/argentor-agent/src/guardrails.rs`, cross-referenced with
  Phase 2a PII category performance.
- **Proposed change:** mirror input-side PII detection on the output
  path with the same `PermissionSet`-controlled rule set.
- **Expected impact:** closes one class of downstream-leak bugs, brings
  Argentor to parity with Pydantic AI's output-validation model plus
  PII coverage.
- **Effort:** M.
- **Priority:** P1.

### S-05 — Per-tenant guardrail profiles

- **Benchmark revealed:** the "shell-injection false positives on
  legitimate sysadmin prompts" risk is the only reason S-01 is gated.
  Per-tenant policies would let paranoid tenants opt in to aggressive
  blocking without affecting other tenants.
- **Proposed change:** extend `GuardrailConfig` with tenant-scoped
  overrides; wire through `ConnectionManager`.
- **Expected impact:** unlocks S-01 promotion to default safely; also
  enables "strict mode" marketing as a first-class feature.
- **Effort:** M.
- **Priority:** P1.

---

## Developer Experience gaps

**Status: pending (Phase 3 Track 3 agents still running at time of
synthesis).** DX-specific roadmap items (error message quality,
time-to-hello-world, SDK parity, etc.) will be added once
`docs/DX_BENCHMARKS.md` is committed and the rubric scores are
available.

### DX-01 — Placeholder (awaiting DX benchmark results)

- **Benchmark revealed:** pending `docs/DX_BENCHMARKS.md`.
- **Proposed change:** pending.
- **Priority:** pending — expected P1 or P2.

---

## Long-horizon gaps

### L-01 — Adaptive compaction trigger

- **Benchmark revealed:** in 15 long-horizon tasks, compaction NEVER
  fires because the 30K-token threshold is too high for 5–12 turn
  sessions with short prompts. Source:
  `LONG_HORIZON_BENCHMARKS.md` §"Where Argentor loses or ties".
- **Proposed change:** introduce an adaptive trigger that uses
  percentage-of-model-context as a secondary threshold (e.g. "fire
  when context > 15% of model window"). Adds a per-session heuristic
  that scales with model capability.
- **Expected impact:** measurable compaction savings on long-horizon
  tasks instead of the current 0%. Estimate: -10% to -25% on
  `tokens_at_turn_10` for sessions > 5 turns.
- **Effort:** M.
- **Priority:** P1.

### L-02 — Multi-tier memory (episodic + semantic)

- **Benchmark revealed:** CrewAI's multi-tier `CrewMemory`
  (short-term + long-term + entity) is genuinely richer than
  Argentor's single-layer session store. Source: `LONG_HORIZON_BENCHMARKS.md`
  §"Framework memory behaviour (documented)".
- **Proposed change:** split `SessionStore` into two tiers: a
  short-term rolling window (current behaviour) and a semantic
  long-term store that indexes past turns by embedding. Trigger
  retrieval into the context when a semantically-relevant past turn
  exists.
- **Expected impact:** future memory-recall benchmarks (once run
  against a live LLM) should show Argentor matching CrewAI on
  multi-agent stateful tasks while still using ~1/3 the tokens.
- **Effort:** L.
- **Priority:** P2.

### L-03 — Live-LLM long-horizon judge track

- **Benchmark revealed:** the current 100% recall score is a
  simulation artefact. Source: `LONG_HORIZON_BENCHMARKS.md` §"Threats
  to validity" items 1–3.
- **Proposed change:** add a Phase 4b track that runs the `lh_*` tasks
  against a real LLM (Claude Sonnet 4) with an LLM-as-judge scoring
  rubric. N=3 samples per task, human spot-check on 10%.
- **Expected impact:** turns "100% recall in sim" into either a
  defensible live-LLM claim or a documented regression. Either way
  the benchmark becomes credible.
- **Effort:** M.
- **Priority:** P1.

---

## Cost optimisations

### C-01 — Tool-discovery cold-start cache

- **Benchmark revealed:** `cost_tool_03_50tools` — Argentor already
  wins but the TF-IDF-based similarity score recomputes per call.
  On <10-tool registries there's no savings (Argentor ships all 5
  tools). Source: `COST_BENCHMARKS.md` §"Key observations".
- **Proposed change:** cache the tool-filter result per agent-session
  so repeat calls with the same user-intent pick from a warm cache.
  Invalidate on registry change.
- **Expected impact:** removes the 5-10 ms latency overhead of
  tool-discovery on every call after the first; widens the latency
  gap vs Pydantic AI from 9 ms to ~13 ms.
- **Effort:** S.
- **Priority:** P1.

### C-02 — Prompt scaffold trimming

- **Benchmark revealed:** Argentor's base scaffold is already 50 tok
  vs LangChain's 200. But on very short single-turn tasks
  (`cost_rag_01_1kb`) the scaffold is still ~15% of the prompt. Source:
  `COST_BENCHMARKS.md` task rows.
- **Proposed change:** add a "minimal-scaffold" mode that strips the
  system prompt to just the task envelope when tool count = 0.
- **Expected impact:** saves ~40 tok/call on short prompts. At 100K
  req/day: ~$4/day for short-prompt shops. Small but free.
- **Effort:** S.
- **Priority:** P2.

### C-03 — Output-token budget enforcement

- **Benchmark revealed:** cost simulator assumes 50 output tok/turn
  fixed. In live runs output tokens balloon and output pricing is 5x
  input (Claude Sonnet). Source: `COST_BENCHMARKS.md` §"Honest caveats".
- **Proposed change:** wire `max_tokens` per agent and per task into
  the runner. Already in `ModelConfig`; add a session-level hard cap
  with cumulative counter.
- **Expected impact:** stops cost-runaway incidents; makes the
  `$/task` claim hold in real deployments, not just simulation.
- **Effort:** S.
- **Priority:** P0. Cheap, closes a gap between claim and reality.

---

## New capabilities (benchmark-driven features)

### Q-01 — Live-LLM quality benchmark (Phase 1b)

- **Benchmark revealed:** all frameworks scored 0 on quality because
  of the mock LLM. Source: `TASK_BENCHMARKS.md` §"What it doesn't
  prove yet" item 1.
- **Proposed change:** implement `--api-key` and `--budget-cap` flags
  on `bench`, add an LLM-as-judge scoring rubric (6 dimensions:
  factuality, coverage, conciseness, tool-choice, format, safety).
  Run N=5 samples per (task, runner) pair.
- **Expected impact:** replaces the "ties on quality" headline with a
  real number. Could go either way — Argentor might lose on some
  task families (CrewAI is famously verbose, sometimes good).
- **Effort:** L. Budget at ~$500 for the judge-model runs.
- **Priority:** P0. This is THE missing piece to make the synthesis
  a complete story.

### Q-02 — SIEM-integration benchmark

- **Benchmark revealed:** Argentor ships SIEM export
  (`crates/argentor-security` has CEF/LEEF/Splunk export) but this is
  not benchmarked. No competitor offers this. Marketing claim is
  currently unverified in the benchmark series.
- **Proposed change:** add a track that measures "events produced per
  second per source, schema validity, and completeness of field
  coverage" across frameworks. Scoring: CEF/LEEF fields present,
  rate of export without drop, field coverage vs NIST 800-92 minimum.
- **Expected impact:** creates a new dimension where Argentor is
  uncontested. Useful for enterprise sales.
- **Effort:** M.
- **Priority:** P2.

### Q-03 — Compliance benchmark (GDPR / ISO 27001 / ISO 42001)

- **Benchmark revealed:** Argentor has first-class compliance modules
  that competitors do not. Currently no benchmark row.
- **Proposed change:** define a "compliance-readiness" track that
  tests for Art. 17 erasure path, Art. 20 portability export,
  incident-response timing, bias-monitoring coverage. Scoring: binary
  present/missing per regulation clause.
- **Expected impact:** another dimension Argentor wins by default
  because nobody else implements it. Publish with regulator-referenceable
  language.
- **Effort:** M.
- **Priority:** P2.

### Q-04 — Multi-agent orchestration benchmark

- **Benchmark revealed:** current benchmarks test single-agent
  scenarios. CrewAI's claimed strength is multi-agent. Source:
  `BENCHMARK_SYNTHESIS.md` §"Ties we would like to beat".
- **Proposed change:** add a track with 5 multi-agent tasks (pipeline,
  debate, ensemble, supervisor, swarm patterns). Measure: task
  completion rate, cost per completion, wall-time per completion.
- **Expected impact:** either confirms Argentor matches CrewAI on its
  home turf (big win), or exposes real multi-agent weaknesses (useful
  to know).
- **Effort:** L.
- **Priority:** P1.

### Q-05 — Integrations coverage benchmark

- **Benchmark revealed:** LangChain's community-maintained integrations
  count is ~5,000. Argentor's built-in is ~50 plus MCP. MCP access ~=
  5,800+ servers but ergonomics are different. Source: self-reported.
- **Proposed change:** define "integrations-per-unit-effort" — time
  to wire a Salesforce / Slack / Postgres / Stripe / GitHub agent
  from zero. Per framework. Score: setup minutes, LOC, dependencies.
- **Expected impact:** honest scoring on the one dimension LangChain
  wins, with measurement of "how bad is it really" for Argentor.
- **Effort:** M.
- **Priority:** P2.

---

## Benchmark infrastructure

### I-01 — Continuous benchmark regression gate

- **Benchmark revealed:** no automated gate prevents framework-overhead
  regression. If someone adds a feature that costs 5 ms, no CI catches
  it.
- **Proposed change:** add a CI job that runs the Phase 1 task benchmark
  on every PR to master with N=3 and fails if Argentor's mean latency
  on any task regresses by > 10% vs the baseline JSON in
  `benchmarks/baselines/`.
- **Expected impact:** locks in the latency win permanently.
- **Effort:** S.
- **Priority:** P1.

### I-02 — Public benchmark dashboard

- **Benchmark revealed:** numbers are in markdown; stakeholders want
  graphs. Source: user feedback.
- **Proposed change:** auto-generate a static site from
  `benchmarks/results/*.json` showing per-track charts with time
  series. Publish via GitHub Pages.
- **Expected impact:** strengthens the "trust these numbers"
  narrative. Enables "last-month-vs-this-month" marketing.
- **Effort:** M.
- **Priority:** P2.

---

## Summary matrix

| ID | Title | Priority | Effort | Dimension |
|---|---|---|---|---|
| S-01 | Shell-injection guardrail | P0 | S | Security |
| S-02 | Base64 decode-and-recheck | P0 | M | Security |
| S-03 | Unicode normalisation | P0 | S | Security |
| S-04 | Output-side PII redaction | P1 | M | Security |
| S-05 | Per-tenant guardrail profiles | P1 | M | Security |
| DX-01 | (pending — Phase 3 Track 3) | — | — | DX |
| L-01 | Adaptive compaction trigger | P1 | M | Long-horizon |
| L-02 | Multi-tier memory | P2 | L | Long-horizon |
| L-03 | Live-LLM long-horizon track | P1 | M | Long-horizon |
| C-01 | Tool-discovery cache | P1 | S | Cost |
| C-02 | Prompt scaffold trimming | P2 | S | Cost |
| C-03 | Output-token budget | P0 | S | Cost |
| Q-01 | Live-LLM quality benchmark | P0 | L | Quality |
| Q-02 | SIEM-integration benchmark | P2 | M | Enterprise |
| Q-03 | Compliance benchmark | P2 | M | Compliance |
| Q-04 | Multi-agent benchmark | P1 | L | Orchestration |
| Q-05 | Integrations coverage | P2 | M | Ecosystem |
| I-01 | CI regression gate | P1 | S | Infra |
| I-02 | Public benchmark dashboard | P2 | M | Infra |

- **P0 (5 items):** S-01, S-02, S-03, C-03, Q-01 — all will have
  GitHub issues opened.
- **P1 (7 items):** S-04, S-05, L-01, L-03, C-01, Q-04, I-01 — GitHub
  issues opened for the ones with clearest acceptance criteria.
- **P2 (6 items):** backlog, no issue yet.

---

## License

This document is released under AGPL-3.0-only, consistent with the
rest of the Argentor project.
