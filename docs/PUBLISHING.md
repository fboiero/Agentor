# Argentor Publishing Guide

How to publish Argentor crates, SDKs, and container images to their respective registries.

## Workspace Overview

The Argentor workspace contains 14 crates. Of these, **13 are publishable** to crates.io and **1 is private** (`argentor-cli`, marked `publish = false`).

All publishable crates inherit shared metadata from `[workspace.package]`:

- **version**: `1.0.0`
- **edition**: `2021`
- **license**: `AGPL-3.0-only`
- **repository**: `https://github.com/fboiero/Argentor`
- **authors**: `Federico Boiero <fboiero@xcapit.com>`

## Crates.io Publish Order

Crates must be published in strict topological (dependency) order. Each crate can only be published after all of its internal dependencies are available on the registry.

| # | Crate | Internal Dependencies |
|---|-------|-----------------------|
| 1 | `argentor-core` | _(none)_ |
| 2 | `argentor-security` | core |
| 3 | `argentor-session` | core |
| 4 | `argentor-memory` | core |
| 5 | `argentor-skills` | core, security |
| 6 | `argentor-channels` | core |
| 7 | `argentor-compliance` | core |
| 8 | `argentor-mcp` | core, skills, security |
| 9 | `argentor-builtins` | core, skills, security, memory |
| 10 | `argentor-agent` | core, security, session, skills, mcp |
| 11 | `argentor-a2a` | core |
| 12 | `argentor-orchestrator` | core, agent, security, session, skills, mcp, compliance |
| 13 | `argentor-gateway` | core, security, agent, mcp, session, channels, skills, a2a, memory, orchestrator |

> **Note**: Positions 6 (`channels`), 7 (`compliance`), and 11 (`a2a`) only depend on `core`, so they could technically be published earlier (right after step 1). The order above matches the CI workflow in `.github/workflows/release.yml` which is the source of truth.

## Publishing to crates.io

### Prerequisites

- A crates.io account with ownership of the `argentor-*` namespace
- The `CARGO_REGISTRY_TOKEN` secret configured

### Manual publish

```bash
# Authenticate
cargo login <your-crates-io-token>

# Dry-run first to catch issues
cargo publish --dry-run --allow-dirty -p argentor-core

# Publish in dependency order with a 30s delay between each
# (crates.io needs time to update its index)
CRATES=(
  argentor-core
  argentor-security
  argentor-session
  argentor-memory
  argentor-skills
  argentor-channels
  argentor-compliance
  argentor-mcp
  argentor-builtins
  argentor-agent
  argentor-a2a
  argentor-orchestrator
  argentor-gateway
)

for crate in "${CRATES[@]}"; do
  echo "Publishing $crate..."
  cargo publish -p "$crate" --no-verify
  sleep 30
done
```

> **`--no-verify`**: The CI workflow uses this flag because verification (build from the packaged source) is already performed by the `test` job. For manual publishing, omit it unless you have already run `cargo test --workspace` beforehand.

### Automated publish (CI)

Publishing is fully automated via the **Release** workflow (`.github/workflows/release.yml`). It triggers on version tags:

```bash
git tag v1.0.0
git push origin v1.0.0
```

The workflow will:
1. Run the full test suite
2. Publish all 13 crates to crates.io in dependency order
3. Create a GitHub Release with auto-generated notes
4. Build and upload platform binaries (linux x86_64/aarch64, macOS x86_64/aarch64)
5. Publish the Python SDK to PyPI
6. Publish the TypeScript SDK to npm
7. Build and push a Docker image to GHCR

## Publishing the Python SDK to PyPI

```bash
cd sdks/python

# Install build tools (if not already)
pip install build twine

# Build sdist and wheel
python -m build

# Upload to PyPI
twine upload dist/*
```

The CI workflow handles this automatically using `TWINE_USERNAME=__token__` with the `PYPI_TOKEN` secret.

## Publishing the TypeScript SDK to npm

```bash
cd sdks/typescript

# Install dependencies
npm install

# Compile TypeScript
npx tsc

# Publish (scoped public package)
npm publish --access public
```

The CI workflow handles this automatically using the `NPM_TOKEN` secret via `NODE_AUTH_TOKEN`.

## Docker Image

The Docker image is published to GitHub Container Registry (GHCR) automatically:

```
ghcr.io/fboiero/argentor:<version>
ghcr.io/fboiero/argentor:latest
```

No additional secrets are needed -- it uses `GITHUB_TOKEN`.

## Required GitHub Secrets

| Secret | Registry | Purpose |
|--------|----------|---------|
| `CARGO_REGISTRY_TOKEN` | crates.io | Publish Rust crates |
| `PYPI_TOKEN` | PyPI | Publish Python SDK |
| `NPM_TOKEN` | npm | Publish TypeScript SDK |
| `GITHUB_TOKEN` | GitHub / GHCR | Releases, binaries, Docker (auto-provided) |

## Verification Checklist

Before tagging a release:

- [ ] All tests pass: `cargo test --workspace`
- [ ] Clippy clean: `cargo clippy --workspace -- -D warnings`
- [ ] Format clean: `cargo fmt --check`
- [ ] Dry-run publish succeeds for the leaf crate: `cargo publish --dry-run --allow-dirty -p argentor-core`
- [ ] Version bumped in `Cargo.toml` `[workspace.package]`
- [ ] Python SDK version updated in `sdks/python/pyproject.toml` (or equivalent)
- [ ] TypeScript SDK version updated in `sdks/typescript/package.json`
- [ ] CHANGELOG updated (if maintained)
