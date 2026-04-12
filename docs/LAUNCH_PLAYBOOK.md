# Argentor v1.0.0 Launch Playbook

> Prompt para un agente/coworker que ejecute el lanzamiento completo.
> Copiar TODO este documento como prompt.

---

## Context

You are launching Argentor v1.0.0 — a Rust-based AI agent framework. The code is 100% ready, pushed to https://github.com/fboiero/Argentor (branch: master). Everything below needs to be executed in order.

The repo has:
- 15 Rust crates (workspace at root Cargo.toml)
- Python SDK at `sdks/python/` (package name: `argentor-sdk`)
- TypeScript SDK at `sdks/typescript/` (package name: `@argentor/sdk`)
- React dashboard at `dashboard/`
- GitHub Actions workflows for automated release at `.github/workflows/release.yml`
- Blog post draft at `docs/BLOG_v1.md`
- Competitive comparison at `docs/COMPARISON.md`
- Benchmark results at `docs/BENCHMARKS.md`
- Publishing guide at `docs/PUBLISHING.md`

## Task 1 — Configure GitHub Secrets

Go to https://github.com/fboiero/Argentor/settings/secrets/actions and add:

1. **`CARGO_REGISTRY_TOKEN`**
   - Go to https://crates.io/settings/tokens
   - Click "New Token"
   - Name: `argentor-github-actions`
   - Scope: `publish-update`
   - Copy the token and paste it as the secret value

2. **`PYPI_TOKEN`**
   - Go to https://pypi.org/manage/account/token/
   - Click "Add API token"
   - Name: `argentor-github-actions`
   - Scope: "Entire account" (first time) or project-scoped to `argentor-sdk` (if project exists)
   - Copy the token (starts with `pypi-`) and paste it as the secret value

3. **`NPM_TOKEN`**
   - Run `npm login` if not logged in
   - Go to https://www.npmjs.com/settings/YOUR_USERNAME/tokens
   - Click "Generate New Token" → Type: "Automation"
   - Copy the token and paste it as the secret value

4. **Verify `GITHUB_TOKEN`** is already available (it is by default in GitHub Actions — no action needed)

## Task 2 — Create npm organization (if needed)

If the npm scope `@argentor` doesn't exist yet:
- Go to https://www.npmjs.com/org/create
- Organization name: `argentor`
- Plan: Free (Unlimited public packages)

OR if you don't want to create an org, edit `sdks/typescript/package.json` and change `"name": "@argentor/sdk"` to `"name": "argentor-sdk"`, commit, and push.

## Task 3 — Trigger the release

```bash
cd /path/to/Argentor
git tag -a v1.0.1 -m "Argentor v1.0.1 — Production release with Agent Intelligence"
git push origin v1.0.1
```

This triggers the release workflow that automatically:
1. Runs the full test suite (4520 tests)
2. Creates a GitHub Release with auto-generated release notes
3. Builds platform binaries (Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64)
4. Publishes 13 crates to crates.io in dependency order (30s between each)
5. Publishes Python SDK to PyPI
6. Publishes TypeScript SDK to npm
7. Builds and pushes Docker image to ghcr.io/fboiero/argentor

Monitor progress at: https://github.com/fboiero/Argentor/actions

## Task 4 — Verify publication (wait ~15 minutes after Task 3)

### crates.io
```bash
cargo search argentor-core
# Should show: argentor-core = "1.0.0"
```

### PyPI
```bash
pip install argentor-sdk
python -c "from argentor import ArgentorClient, __version__; print(f'OK: v{__version__}')"
```

### npm
```bash
npm info @argentor/sdk version
# Should show: 1.0.0
```

### Docker
```bash
docker pull ghcr.io/fboiero/argentor:1.0.1
docker run --rm ghcr.io/fboiero/argentor:1.0.1 --version
```

### GitHub Release
Check https://github.com/fboiero/Argentor/releases — should have v1.0.1 with binaries attached.

## Task 5 — Update GitHub repository settings

1. Go to https://github.com/fboiero/Argentor
2. Click the gear icon next to "About" (top right of repo page)
3. Set:
   - **Description**: `Secure multi-agent AI framework in Rust — WASM sandbox, 50+ skills, 14 LLM providers, agent intelligence, compliance modules`
   - **Website**: `https://fboiero.github.io/Argentor`
   - **Topics**: `rust`, `ai-agents`, `wasm`, `mcp`, `llm`, `multi-agent`, `security`, `compliance`, `agent-framework`
4. Check "Releases" and "Packages" in the sidebar options

## Task 6 — Verify GitHub Pages

1. Go to https://github.com/fboiero/Argentor/settings/pages
2. Ensure Source is set to: Deploy from branch → `master` → `/docs`
3. Verify https://fboiero.github.io/Argentor loads the landing page
4. Check that all sections render: hero, features, Agent Intelligence cards, SDK section

## Task 7 — Publish blog post

The draft is at `docs/BLOG_v1.md` in the repo. Publish to these platforms:

### DEV.to
1. Go to https://dev.to/new
2. Paste the full markdown content from `docs/BLOG_v1.md`
3. Set title: "Introducing Argentor v1.0 — The Secure AI Agent Framework in Rust"
4. Add tags: `rust`, `ai`, `agents`, `opensource`
5. Add cover image: use the Argentor logo or a relevant AI/Rust image
6. Publish

### Reddit (3 posts)
1. **r/rust**: Title: "Argentor v1.0 — AI agent framework with WASM sandboxing, 50+ skills, 14 LLM providers (4500+ tests, 187K LOC)" — Link post to the DEV.to article or GitHub repo
2. **r/MachineLearning**: Title: "[P] Argentor: Secure AI agent framework in Rust — 5x less memory than Python alternatives" — Self post with summary + link
3. **r/artificial**: Title: "Open-source AI agent framework with built-in security (WASM sandbox) and compliance (GDPR, ISO 27001)" — Link post

### Hacker News
1. Go to https://news.ycombinator.com/submit
2. Title: "Show HN: Argentor – Secure AI agent framework in Rust with WASM sandboxed plugins"
3. URL: https://github.com/fboiero/Argentor

### LinkedIn
Post a professional announcement:
```
Excited to announce Argentor v1.0 — an open-source AI agent framework built in Rust.

Why it matters:
→ WASM-sandboxed plugins (no more supply chain attacks like OpenClaw)
→ 5x less memory, 14x faster cold start than Python frameworks
→ Built-in compliance: GDPR, ISO 27001, ISO 42001
→ 50+ skills, 14 LLM providers, 10 agent intelligence modules
→ 4,500+ tests, zero clippy warnings

Perfect for regulated industries (finance, healthcare, government) that need AI agents they can actually trust in production.

Try it: https://github.com/fboiero/Argentor
Benchmarks: [link to BENCHMARKS.md]
Comparison vs LangChain/CrewAI/IronClaw: [link to COMPARISON.md]

#AI #Rust #OpenSource #AIAgents #Security
```

### Twitter/X thread
```
🧵 Introducing Argentor v1.0 — the secure AI agent framework in Rust

1/ The AI agent security crisis is real. OpenClaw had 512 CVEs. Python frameworks use 5x more memory. We built something better.

2/ Argentor: 15 Rust crates, 187K LOC, 4500+ tests, ZERO unsafe code in production.

Every plugin runs in a WASM sandbox. Every tool call is audit-logged. Every agent has capability-based permissions.

3/ 10 Agent Intelligence modules:
- Extended Thinking (multi-pass reasoning)
- Self-Critique (Reflexion pattern)
- Dynamic Tool Discovery
- Agent Handoffs
- State Checkpointing
- Process Reward Scoring
- Learning Feedback Loop
...and more

4/ Performance vs Python:
- 14x faster cold start (4ms vs 56ms)
- 5x less memory (1GB vs 5GB)
- 2x less CPU usage

5/ Built for regulated industries:
- GDPR, ISO 27001, ISO 42001 compliance modules
- SSO/SAML authentication
- Multi-region data routing
- Encrypted credential vault (AES-256-GCM)

6/ Get started in 5 minutes:
→ GitHub: https://github.com/fboiero/Argentor
→ Python SDK: pip install argentor-sdk
→ TypeScript SDK: npm install @argentor/sdk

Star ⭐ if you believe AI agents deserve better security.
```

## Task 8 — Optional: Rename repository

The repo is currently named `Agentor` but the project is `Argentor`:
1. Go to https://github.com/fboiero/Argentor/settings (General)
2. Under "Repository name", change to `Argentor` (if not already done)
3. GitHub will automatically redirect the old URL

## Task 9 — Track results

After 48 hours, check:
- [ ] GitHub stars count
- [ ] crates.io download count: https://crates.io/crates/argentor-core
- [ ] PyPI downloads: https://pypistats.org/packages/argentor-sdk
- [ ] npm downloads: https://www.npmjs.com/package/@argentor/sdk
- [ ] DEV.to views/reactions
- [ ] HN points
- [ ] Reddit upvotes
- [ ] Any GitHub issues opened (community engagement)

## Troubleshooting

### Release workflow fails on crates.io
- Check that `CARGO_REGISTRY_TOKEN` is correct
- Some crate names might be taken — check https://crates.io/crates/argentor-core
- If a crate fails mid-publish, re-run the workflow (it uses `--skip-existing`)

### PyPI publish fails
- Check that `PYPI_TOKEN` is correct
- The package name `argentor-sdk` might be taken — check https://pypi.org/project/argentor-sdk/
- If taken, change `name` in `sdks/python/pyproject.toml` to something unique

### npm publish fails
- Check that `NPM_TOKEN` is correct and the `@argentor` org exists
- If no org, change package name to `argentor-sdk` (without scope)

### Docker build fails
- Usually a dependency issue — check the Dockerfile
- The `GITHUB_TOKEN` secret is automatic, no config needed
