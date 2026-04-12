# Argentor — Session Context
> Last updated: 2026-04-12 (v1.1.1 Launch — COMPLETED)

## Current Goal
v1.1.1 launch is **COMPLETE**. All primary registries verified.

## What's Completed
- Version bump to 1.1.1 across Cargo.toml, pyproject.toml, package.json
- Tag v1.1.1 created and pushed (commit d180a4e — includes Dockerfile Rust bump)
- GitHub Release created with auto-generated notes
- All 5096 tests pass
- GitHub Pages verified
- Repository settings updated (description, topics, website)
- 2/4 platform binaries built (x86_64-linux, aarch64-apple-darwin)
- Blog/social content prepared (LAUNCH_CONTENT.md)
- Launch scripts created and executed
- Dockerfile fixed: `COPY experiments/` added (commit 55b9693)
- Dockerfile fixed: Rust 1.83 → 1.87 for edition2024 support (commit d180a4e)
- Docker image pushed to ghcr.io/fboiero/argentor (publish-docker: 16s)
- argentor-sdk published to PyPI v1.1.1 (publish-python: 17s)
- **@argentor/sdk@1.1.1 published to npm** (manual publish from CLI with bypass-2FA token)
- **All 3 GitHub secrets configured:**
  - CARGO_REGISTRY_TOKEN — configured
  - PYPI_TOKEN — configured
  - NPM_TOKEN — updated with argentor-ci-v3 token (bypass 2FA enabled, expires Apr 19, 2026)

## Publication Status

| Registry | Package | Version | Status |
|----------|---------|---------|--------|
| **npm** | @argentor/sdk | 1.1.1 | ✅ Published — https://www.npmjs.com/package/@argentor/sdk |
| **PyPI** | argentor-sdk | 1.1.1 | ✅ Published — https://pypi.org/project/argentor-sdk/ |
| **Docker** | ghcr.io/fboiero/argentor | 1.1.1 + latest | ✅ Published — https://github.com/fboiero/Argentor/pkgs/container/argentor |
| **crates.io** | argentor-* (13 crates) | 1.1.1 | ❌ Failed silently — `|| echo` masked error, 0 crates found on crates.io |

## What's Pending
1. **Fix crates.io publish** — publish-crates job failed silently; need to investigate CARGO_REGISTRY_TOKEN and re-publish 13 crates
2. **Renew NPM_TOKEN** — current token expires Apr 19, 2026 (7 days); create a longer-lived token
3. **Fix `|| echo` anti-pattern** in release.yml — remove silent error masking from publish steps
4. (Optional) Fix cross-compilation for aarch64-linux and x86_64-apple-darwin binaries
5. Publish blog + social media (content in LAUNCH_CONTENT.md)
6. Publish argentor-langchain-bridge v0.1.0

## Key Decisions
- v1.1.1 chosen as first published version (v1.1.0 was workspace version, bumped for first registry publication)
- AGPL-3.0-only license enforced across all 17 crates and SDKs
- npm publish required bypass-2FA on granular token (account has 2FA for auth+writes)
- npm publish done manually from CLI after CI failures; CI workflow still has stale token behavior
- Cross-compilation failures for aarch64-linux and x86_64-darwin are secondary — can fix in v1.1.2
- Dockerfile pinned to rust:1.87-slim-bookworm (both wasm-builder and builder stages)

## Relevant Files
- `/Agentor/Dockerfile` — Multi-stage build, rust:1.87-slim-bookworm
- `/Agentor/fix-docker-rust-version.sh` — Script that fixed Rust version and re-tagged v1.1.1
- `/Agentor/LAUNCH_CONTENT.md` — Blog/social content
- `.github/workflows/release.yml` — Main release workflow (7 jobs)

## npm Token Info
- Token name: argentor-ci-v3
- Has bypass 2FA: yes
- Packages: Read and Write (all packages)
- Organizations: Read and Write (argentor org selected)
- Expires: Apr 19, 2026 (7 days from creation)
