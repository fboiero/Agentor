#!/bin/bash
# ============================================================================
# argentor-langchain-bridge v0.1.0 — Publish Script
# ============================================================================
# Run AFTER the main Argentor v1.1.1 release is verified.
#
# Prerequisite:
#   Add PYPI_TOKEN to https://github.com/fboiero/argentor-langchain-bridge/settings/secrets/actions
#   (This is a SEPARATE token from the main repo)
#
# Usage:
#   cd /Users/fboiero/Documents/GitHub/argentor-langchain-bridge
#   chmod +x launch-bridge-v0.1.0.sh
#   ./launch-bridge-v0.1.0.sh
# ============================================================================

set -euo pipefail

echo "=========================================="
echo "  argentor-langchain-bridge v0.1.0"
echo "=========================================="
echo ""

# Verify we're in the right repo
if [ ! -f "pyproject.toml" ] || ! grep -q "argentor" pyproject.toml; then
    echo "ERROR: Run this from the argentor-langchain-bridge repo root"
    echo "  cd /Users/fboiero/Documents/GitHub/argentor-langchain-bridge"
    exit 1
fi

# Verify license is AGPL-3.0-only
LICENSE=$(grep 'license' pyproject.toml | head -1)
if [[ ! "$LICENSE" == *"AGPL"* ]]; then
    echo "⚠️  WARNING: License is NOT AGPL-3.0-only!"
    echo "  Found: $LICENSE"
    echo "  Fix this before publishing."
    exit 1
fi
echo "✓ License check: AGPL-3.0-only"

git pull origin main 2>/dev/null || git pull origin master
echo "✓ Up to date"

git tag -a v0.1.0 -m "argentor-langchain-bridge v0.1.0 — First release

LangChain integration bridge for Argentor AI agent framework.
AGPL-3.0-only license."

git push origin v0.1.0
echo "✓ Tag v0.1.0 pushed"
echo ""
echo "Monitor: https://github.com/fboiero/argentor-langchain-bridge/actions"
echo ""
echo "After publishing, verify:"
echo "  pip install argentor-langchain-bridge"
echo "  python -c \"import argentor_langchain; print('OK')\""
