#!/bin/bash
# ============================================================================
# Argentor v1.1.1 — Fix & Re-run Script
# ============================================================================
# Run AFTER adding the 3 repository secrets (CARGO_REGISTRY_TOKEN, PYPI_TOKEN, NPM_TOKEN).
#
# This script:
#   1. Fixes the Dockerfile (missing experiments/ directory)
#   2. Commits the fix
#   3. Deletes the v1.1.1 tag (local + remote)
#   4. Re-creates the tag on the new commit
#   5. Pushes to trigger the release workflow again
#
# Prerequisites (do these MANUALLY first):
#   1. Add CARGO_REGISTRY_TOKEN → https://github.com/fboiero/Argentor/settings/secrets/actions
#      Get token from: https://crates.io/settings/tokens/new
#      Scopes: publish-new, publish-update
#
#   2. Add PYPI_TOKEN → https://github.com/fboiero/Argentor/settings/secrets/actions
#      Get token from: https://pypi.org/manage/account/token/
#      Scope: Entire account (first time) or project-scoped after first publish
#
#   3. Add NPM_TOKEN → https://github.com/fboiero/Argentor/settings/secrets/actions
#      Get token from: https://www.npmjs.com/settings/<username>/tokens
#      Type: Automation (publish)
#      NOTE: Create @argentor npm org first at https://www.npmjs.com/org/create
#
# Usage:
#   cd /Users/fboiero/Documents/GitHub/Agentor
#   chmod +x fix-and-rerun-v1.1.1.sh
#   ./fix-and-rerun-v1.1.1.sh
# ============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Argentor v1.1.1 Fix & Re-run${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# ── Step 0: Verify we're in the right directory ──────────────────────────────
if [ ! -f "Cargo.toml" ] || ! grep -q "argentor" Cargo.toml; then
    echo -e "${RED}ERROR: Run this from the Argentor repo root${NC}"
    echo "  cd /Users/fboiero/Documents/GitHub/Agentor"
    exit 1
fi

# ── Step 1: Fix Dockerfile ──────────────────────────────────────────────────
echo -e "${YELLOW}[1/5] Fixing Dockerfile...${NC}"

# Add experiments/ directory to Docker COPY if not already present
if ! grep -q "COPY experiments/ experiments/" Dockerfile; then
    sed -i '' '/COPY wit\/ wit\//a\
COPY experiments/ experiments/
' Dockerfile
    echo "  ✓ Added 'COPY experiments/ experiments/' to Dockerfile"
else
    echo "  ✓ Dockerfile already has experiments/ COPY (no change needed)"
fi

# Verify the fix
echo ""
echo "  Dockerfile builder stage now copies:"
grep "^COPY" Dockerfile | grep -v "^COPY --from" | while read line; do
    echo "    $line"
done

echo -e "${GREEN}✓ Dockerfile fixed${NC}"
echo ""

# ── Step 2: Commit the fix ──────────────────────────────────────────────────
echo -e "${YELLOW}[2/5] Committing Dockerfile fix...${NC}"
git add Dockerfile
git commit -m "fix: add experiments/ to Dockerfile for workspace build

The Docker build failed because Cargo.toml references
experiments/comparison as a workspace member, but the
Dockerfile only copied crates/ and wit/ directories."

git push origin master
echo -e "${GREEN}✓ Fix committed and pushed${NC}"
echo ""

# ── Step 3: Delete old tag ──────────────────────────────────────────────────
echo -e "${YELLOW}[3/5] Removing old v1.1.1 tag...${NC}"
git tag -d v1.1.1 2>/dev/null || true
git push origin :refs/tags/v1.1.1 2>/dev/null || true
echo -e "${GREEN}✓ Old tag removed${NC}"
echo ""

# ── Step 4: Wait for GitHub to process tag deletion ─────────────────────────
echo -e "${YELLOW}[4/5] Waiting 5 seconds for GitHub to process...${NC}"
sleep 5
echo -e "${GREEN}✓ Ready${NC}"
echo ""

# ── Step 5: Re-create and push tag ─────────────────────────────────────────
echo -e "${YELLOW}[5/5] Creating new v1.1.1 tag...${NC}"
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

# ── Monitor ────────────────────────────────────────────────────────────────
echo -e "${YELLOW}Monitor the release:${NC}"
echo "  https://github.com/fboiero/Argentor/actions"
echo ""
echo "  Expected results after secrets are configured:"
echo "    ✅ test"
echo "    ✅ create-release"
echo "    ✅ publish-crates (13 crates, ~7 min)"
echo "    ✅ publish-python (argentor-sdk to PyPI)"
echo "    ✅ publish-typescript (@argentor/sdk to npm)"
echo "    ✅ publish-docker (ghcr.io/fboiero/argentor)"
echo "    ✅ upload-binaries (4 platform builds)"
echo ""
echo "  After ~15 min, verify:"
echo "    cargo search argentor-core"
echo "    pip install argentor-sdk"
echo "    npm info @argentor/sdk version"
echo "    docker pull ghcr.io/fboiero/argentor:1.1.1"
echo ""

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Fix applied & re-launched! 🚀${NC}"
echo -e "${GREEN}========================================${NC}"
