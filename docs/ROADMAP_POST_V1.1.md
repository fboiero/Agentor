# Argentor Roadmap — Post v1.1.0

> Last updated: 2026-04-12
> Current version: v1.1.0
> Branch: master (clean, published to GitHub)

## Context

v1.1.0 closed massive ecosystem gaps vs LangChain/CrewAI through 3 parallel strategies:
1. **Native implementations** (5,096 tests, 17 crates)
2. **Protocol extension** (MCP registry, 5,800+ integrations available)
3. **Language bridges** (argentor-langchain-bridge on separate repo)

What remains is **real-world validation** — moving from "technically impressive" to "proven in production."

---

## Phase 1 — Publication & Distribution (Weeks 1-2)

**Goal**: v1.1.0 available on all major channels.

### Blockers to unblock
- [ ] Configure GitHub Secrets: `CARGO_REGISTRY_TOKEN`, `PYPI_TOKEN`, `NPM_TOKEN`
- [ ] Trigger v1.1.0 release workflow (already automated via tag push — just needs secrets)
- [ ] Verify crates.io publication (13 crates in dependency order)
- [ ] Verify PyPI publication (`argentor-sdk` v1.1.0)
- [ ] Verify npm publication (`@argentor/sdk` v1.1.0)
- [ ] Publish `argentor-langchain-bridge` to PyPI (separate repo, separate workflow)

### Nice-to-have
- [ ] Docker image to Docker Hub (currently only GHCR)
- [ ] Homebrew tap for CLI binary install on macOS
- [ ] Scoop manifest for Windows
- [ ] APT/RPM packages for Linux distros

**Owner**: User (requires credentials)
**Estimated effort**: 1-2 days (setup + verification)

---

## Phase 2 — Real Integration Validation (Weeks 3-6)

**Goal**: Move from stub implementations to at least 3 real HTTP integrations working end-to-end.

### Priority A — Real HTTP for critical providers
- [ ] Wire real HTTP for 1 vector store (Pinecone → largest ecosystem)
- [ ] Wire real HTTP for 1 document loader (PDF with `lopdf` or `pdfium`)
- [ ] Wire real HTTP for 1 embedding provider (OpenAI → most used)
- [ ] Wire real HTTP for 1 voice backend (Whisper → most mature)
- [ ] Wire real HTTP for 1 vision backend (Claude — already has key)

### Priority B — First MCP real deployment
- [ ] Pick 3 MCP servers from registry and validate connection flow end-to-end
- [ ] Document any discovered issues in `docs/MCP_INTEGRATION_GUIDE.md`
- [ ] Add "verified" badge to registry entries we've tested

### Priority C — argentor-langchain-bridge working
- [ ] Implement `test_server.py` with real MCP session
- [ ] Test against Argentor via MCP client
- [ ] Document 3 example LangChain tools working through the bridge

**Owner**: Dev team
**Estimated effort**: 3-4 weeks (1 real integration per week)

---

## Phase 3 — First Production Beta (Weeks 7-12)

**Goal**: 3 real users running Argentor in non-trivial scenarios.

### Target user profiles
1. **Solo dev / hacker**: tries Argentor for a personal project, gives honest feedback
2. **Small startup (5-20 people)**: uses Argentor for an internal tool (customer support, docs assistant, ...)
3. **Enterprise PoC (100+ people)**: security-conscious industry evaluating Argentor for a regulated use case

### What we measure
- [ ] Time-to-first-agent (install → working agent)
- [ ] First-week blockers (what stops adoption)
- [ ] Feature requests (gaps we didn't identify)
- [ ] Performance in real workloads (vs our synthetic benchmarks)
- [ ] Security observations (any issues reported)

### Outreach channels
- [ ] r/rust — "Show HN-style" post with benchmarks
- [ ] Hacker News — aim for front page with honest positioning
- [ ] LinkedIn — CTO/CISO outreach in regulated industries
- [ ] Rust Discord — #showcase channel
- [ ] AI Engineer Summit — talk submission

**Owner**: User + marketing/outreach
**Estimated effort**: 6 weeks with active outreach

---

## Phase 4 — Feedback-Driven Iteration (Weeks 13-20)

**Goal**: Fix the top 10 friction points beta users identify.

### Likely areas (pre-feedback educated guesses)
- [ ] **Documentation**: more cookbooks, video tutorials, live playground
- [ ] **Error messages**: make Rust compile errors approachable for Python devs
- [ ] **Python ergonomics**: first-class async from Python SDK
- [ ] **Dashboard polish**: real React app fit-and-finish, auth UI
- [ ] **Observability**: OTel instrumentation depth, trace replay UI
- [ ] **Hot-reload**: WASM skill hot-reload during development
- [ ] **Debugging**: better debug recorder → VS Code extension
- [ ] **Testing helpers**: mock LLM harness for unit-testing agents
- [ ] **Migration guides**: "moving from LangChain to Argentor" tutorial
- [ ] **Performance under load**: concurrent-session stress tests

**Owner**: Dev team reacting to beta feedback
**Estimated effort**: 8 weeks

---

## Phase 5 — Argentor Cloud MVP (Weeks 21-32)

**Goal**: Hosted offering generating first revenue.

The `argentor-cloud` crate exists as scaffolding. To make it real:

### Technical milestones
- [ ] Real Postgres backend (replace in-memory HashMap for tenants/quotas)
- [ ] Redis for active session cache
- [ ] S3 (or R2/GCS) for audit logs and artifacts
- [ ] Stripe integration for real billing
- [ ] OAuth2/SSO login (not just API keys)
- [ ] Hosted dashboard (reuse React app + multi-tenancy)
- [ ] Deploy to at least 2 regions (US-East + EU-West for GDPR)

### Business milestones
- [ ] Pricing page live at argentor.cloud
- [ ] First 10 beta cloud customers (free tier)
- [ ] First 3 paying customers on Starter tier ($99/mo)
- [ ] First 1 enterprise customer
- [ ] SOC 2 Type 1 audit scheduled

**Owner**: User (business) + dev team (technical)
**Estimated effort**: 3 months

---

## Phase 6 — TEE Production Integration (Weeks 33-52)

**Goal**: Real TEE support for compliance customers.

`argentor-tee` exists as scaffolding. Real implementation:

### Milestones
- [ ] **AWS Nitro Enclaves first** (easiest, most accessible): implement real SDK integration
- [ ] Attestation document parsing + validation
- [ ] Hot-path benchmarking (CPU penalty for TEE vs non-TEE)
- [ ] Intel SGX via Gramine or Occlum runtime
- [ ] AMD SEV-SNP via `sev-guest` crate
- [ ] Case study with at least 1 compliance customer (banking / healthcare / gov)

**Owner**: Dev team + compliance specialist
**Estimated effort**: 5-6 months (complex territory)

---

## Phase 7 — Growth & Ecosystem (Year 2)

**Goal**: Self-sustaining ecosystem with multiple contributors.

- [ ] Plugin marketplace with revenue share (30% to authors)
- [ ] Developer certification program
- [ ] Annual Argentor Summit (online conference)
- [ ] Bug bounty program
- [ ] Second language support (Go or Python native rewrite of parts?)
- [ ] Formal security audit by third party
- [ ] ISO 42001 certification for Argentor itself

**Owner**: Community + business
**Estimated effort**: 12 months

---

## Immediate Next Actions (this week)

Ordered by blocking impact:

1. **Configure GitHub Secrets** (30 min, user action required)
   - Go to https://github.com/fboiero/Argentor/settings/secrets/actions
   - Add `CARGO_REGISTRY_TOKEN`, `PYPI_TOKEN`, `NPM_TOKEN`
   - Guide in `docs/LAUNCH_PLAYBOOK.md`

2. **Re-trigger v1.1.0 release workflow** (5 min)
   - Once secrets are in place, delete and re-push v1.1.0 tag OR run workflow manually
   - Verify publication at crates.io / PyPI / npm

3. **Test argentor-langchain-bridge end-to-end** (2-3 hours)
   - Install in a Python venv, connect Argentor, run a LangChain tool
   - Fix any integration issues
   - Publish to PyPI via the repo's workflow

4. **Write launch blog post** (1 day)
   - `docs/BLOG_v1.md` already exists — update for v1.1 specifics
   - Post to DEV.to + Reddit + HN + LinkedIn (content in `docs/LAUNCH_PLAYBOOK.md`)

5. **Invite 3-5 beta testers** (1 week)
   - Personal network + Rust Discord
   - Give them `docs/GETTING_STARTED.md` + your contact
   - Promise 30-min calls for feedback

---

## Non-goals (for next 3 months)

Deliberately NOT doing these, to stay focused:

- ❌ Adding more LLM providers (19 native + HF gateway is plenty)
- ❌ Adding more built-in skills (56 native + 5,800 MCP is plenty)
- ❌ Adding more embedding providers (10 is plenty)
- ❌ Refactoring existing code "for fun" (no hypothetical futures)
- ❌ Building a second dashboard / playground (the one we have is enough for MVP)
- ❌ Porting to another language (Rust is the positioning)
- ❌ Chasing every shiny new protocol (MCP + A2A is the right bet)

---

## Success metrics by phase

| Phase | Timeline | Success looks like |
|-------|----------|--------------------|
| 1 | Week 1-2 | v1.1.0 on crates.io / PyPI / npm / Docker |
| 2 | Week 3-6 | 5+ real HTTP integrations working |
| 3 | Week 7-12 | 10+ beta users, 1 case study published |
| 4 | Week 13-20 | Top 10 friction points addressed |
| 5 | Week 21-32 | Argentor Cloud MVP + first 3 paying customers |
| 6 | Week 33-52 | 1 real TEE deployment, compliance customer |
| 7 | Year 2 | 100+ production deployments, self-sustaining |

---

## What happens if we deviate from this plan

That's fine. This is a GUIDE, not a contract. Pivoting based on beta feedback is expected.
The only thing we commit to: **no shipping beta to users without real HTTP integrations working end-to-end (Phase 2)**.
Everything else can adjust based on learning.
