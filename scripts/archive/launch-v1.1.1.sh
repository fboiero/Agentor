#!/bin/bash
# ============================================================================
# Argentor v1.1.1 — Complete Launch Script
# ============================================================================
# Run this from your local machine after configuring GitHub secrets.
#
# Prerequisites (do these MANUALLY first):
#   1. Add CARGO_REGISTRY_TOKEN to https://github.com/fboiero/Argentor/settings/secrets/actions
#   2. Add PYPI_TOKEN to https://github.com/fboiero/Argentor/settings/secrets/actions
#   3. Add NPM_TOKEN to https://github.com/fboiero/Argentor/settings/secrets/actions
#   4. Create npm org @argentor at https://www.npmjs.com/org/create (Free plan)
#
# Usage:
#   cd /Users/fboiero/Documents/GitHub/Agentor
#   chmod +x launch-v1.1.1.sh
#   ./launch-v1.1.1.sh
# ============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Argentor v1.1.1 Launch Script${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# ── Step 0: Verify we're in the right directory ──────────────────────────────
if [ ! -f "Cargo.toml" ] || ! grep -q "argentor" Cargo.toml; then
    echo -e "${RED}ERROR: Run this from the Argentor repo root${NC}"
    echo "  cd /Users/fboiero/Documents/GitHub/Agentor"
    exit 1
fi

# ── Step 1: Pull latest changes ─────────────────────────────────────────────
echo -e "${YELLOW}[1/6] Pulling latest changes...${NC}"
git pull origin master
echo -e "${GREEN}✓ Up to date${NC}"
echo ""

# ── Step 2: Bump version to 1.1.1 everywhere ────────────────────────────────
echo -e "${YELLOW}[2/6] Bumping version to 1.1.1...${NC}"

# Root Cargo.toml (workspace version)
sed -i '' 's/^version = "1.1.0"/version = "1.1.1"/' Cargo.toml
echo "  ✓ Cargo.toml workspace version → 1.1.1"

# Python SDK
sed -i '' 's/^version = "1.0.0"/version = "1.1.1"/' sdks/python/pyproject.toml
echo "  ✓ sdks/python/pyproject.toml → 1.1.1"

# TypeScript SDK
# Use python for reliable JSON editing
python3 -c "
import json
with open('sdks/typescript/package.json', 'r') as f:
    data = json.load(f)
data['version'] = '1.1.1'
with open('sdks/typescript/package.json', 'w') as f:
    json.dump(data, f, indent=2)
    f.write('\n')
"
echo "  ✓ sdks/typescript/package.json → 1.1.1"

# Verify
echo ""
echo "  Verification:"
echo "    Cargo.toml:     $(grep '^version' Cargo.toml | head -1)"
echo "    pyproject.toml: $(grep '^version' sdks/python/pyproject.toml)"
echo "    package.json:   $(python3 -c "import json; print('version =', json.dumps(json.load(open('sdks/typescript/package.json'))['version']))")"

echo -e "${GREEN}✓ All versions bumped to 1.1.1${NC}"
echo ""

# ── Step 3: Commit version bump ─────────────────────────────────────────────
echo -e "${YELLOW}[3/6] Committing version bump...${NC}"
git add Cargo.toml sdks/python/pyproject.toml sdks/typescript/package.json
git commit -m "chore: bump version to 1.1.1 for first publication

Bump workspace Cargo.toml, Python SDK, and TypeScript SDK
to 1.1.1 for the first crates.io / PyPI / npm publication."

git push origin master
echo -e "${GREEN}✓ Version bump committed and pushed${NC}"
echo ""

# ── Step 4: Create and push tag ─────────────────────────────────────────────
echo -e "${YELLOW}[4/6] Creating tag v1.1.1...${NC}"
git tag -a v1.1.1 -m "Argentor v1.1.1 — first publication to crates.io, PyPI, npm

First official release to package registries:
- 17 Rust crates published to crates.io
- argentor-sdk published to PyPI
- @argentor/sdk published to npm
- Docker image pushed to ghcr.io/fboiero/argentor
- Platform binaries for Linux and macOS (x86_64 + aarch64)

AGPL-3.0-only license."

git push origin v1.1.1
echo -e "${GREEN}✓ Tag v1.1.1 pushed — release workflow triggered!${NC}"
echo ""

# ── Step 5: Show monitoring links ───────────────────────────────────────────
echo -e "${YELLOW}[5/6] Monitor the release...${NC}"
echo ""
echo "  Watch the workflow:"
echo "    https://github.com/fboiero/Argentor/actions"
echo ""
echo "  The workflow will automatically:"
echo "    1. Run all tests"
echo "    2. Create GitHub Release with auto-generated notes"
echo "    3. Build binaries (Linux x86/arm64, macOS x86/arm64)"
echo "    4. Publish 13 crates to crates.io (takes ~7 min)"
echo "    5. Publish Python SDK to PyPI"
echo "    6. Publish TypeScript SDK to npm"
echo "    7. Build & push Docker image to ghcr.io"
echo ""
echo "  Estimated time: 15-20 minutes"
echo ""

# ── Step 6: Verification commands ───────────────────────────────────────────
echo -e "${YELLOW}[6/6] After ~15 minutes, verify with:${NC}"
echo ""
echo "  # crates.io"
echo "  cargo search argentor-core"
echo ""
echo "  # PyPI"
echo "  pip install argentor-sdk"
echo "  python -c \"from argentor import ArgentorClient, __version__; print(f'OK: v{__version__}')\""
echo ""
echo "  # npm"
echo "  npm info @argentor/sdk version"
echo ""
echo "  # Docker"
echo "  docker pull ghcr.io/fboiero/argentor:1.1.1"
echo ""
echo "  # GitHub Release"
echo "  open https://github.com/fboiero/Argentor/releases/tag/v1.1.1"
echo ""

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Launch initiated! 🚀${NC}"
echo -e "${GREEN}========================================${NC}"
