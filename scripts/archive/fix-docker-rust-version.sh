#!/bin/bash
# ============================================================================
# Fix Docker Rust version & re-trigger release
# ============================================================================
# The Docker build fails because getrandom v0.4.2 requires Rust edition 2024
# which needs Rust 1.85+, but the Dockerfile pins rust:1.83.
#
# This script:
#   1. Commits the already-edited Dockerfile (1.83 → 1.87)
#   2. Pushes the fix
#   3. Deletes the v1.1.1 tag (local + remote)
#   4. Re-creates the tag on the new commit
#   5. Pushes to trigger the release workflow again
# ============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Fix Docker Rust Version & Re-trigger${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Verify we're in the right directory
if [ ! -f "Cargo.toml" ] || ! grep -q "argentor" Cargo.toml; then
    echo -e "${RED}ERROR: Run this from the Argentor repo root${NC}"
    exit 1
fi

# Step 1: Verify the Dockerfile change
echo -e "${YELLOW}[1/5] Verifying Dockerfile change...${NC}"
if grep -q "rust:1.87-slim-bookworm" Dockerfile; then
    echo "  ✓ Dockerfile already updated to rust:1.87"
else
    echo "  Updating Dockerfile from rust:1.83 to rust:1.87..."
    sed -i '' 's/rust:1.83-slim-bookworm/rust:1.87-slim-bookworm/g' Dockerfile
    echo "  ✓ Dockerfile updated"
fi
echo ""

# Step 2: Commit
echo -e "${YELLOW}[2/5] Committing Dockerfile fix...${NC}"
git add Dockerfile
git commit -m "fix: bump Dockerfile Rust version to 1.87 for edition2024 support

getrandom v0.4.2 requires Rust edition 2024, which was stabilized
in Rust 1.85. The Dockerfile was pinned to rust:1.83 which doesn't
support this edition, causing Docker build failures."

git push origin master
echo -e "${GREEN}✓ Fix committed and pushed${NC}"
echo ""

# Step 3: Delete old tag
echo -e "${YELLOW}[3/5] Removing old v1.1.1 tag...${NC}"
git tag -d v1.1.1 2>/dev/null || true
git push origin :refs/tags/v1.1.1 2>/dev/null || true
echo -e "${GREEN}✓ Old tag removed${NC}"
echo ""

# Step 4: Wait
echo -e "${YELLOW}[4/5] Waiting 5 seconds for GitHub to process...${NC}"
sleep 5
echo -e "${GREEN}✓ Ready${NC}"
echo ""

# Step 5: Re-create and push tag
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

echo -e "${YELLOW}Monitor: https://github.com/fboiero/Argentor/actions${NC}"
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Done! 🚀${NC}"
echo -e "${GREEN}========================================${NC}"
