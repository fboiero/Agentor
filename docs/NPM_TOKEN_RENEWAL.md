# NPM Token Renewal

> Current token expires: **April 19, 2026** (token name: `argentor-ci-v3`)
> Renewal needed by: **April 17, 2026** (2-day buffer before expiry)

## Why this matters

The `NPM_TOKEN` in GitHub Secrets is what allows `.github/workflows/release.yml`
and `.github/workflows/publish-sdks.yml` to publish `@argentor/sdk` to npm
automatically on tag push.

If the token expires:
- Release workflows fail silently or with 401
- Users can't `npm install @argentor/sdk` for the new version (old versions still work)
- Must manually publish from a developer's laptop to recover

## Renewal process (15 min)

### Step 1 — Generate new token

Option A (recommended, doesn't require 2FA each publish):
1. Log in at https://www.npmjs.com with your account
2. Go to https://www.npmjs.com/settings/YOUR_USERNAME/tokens
3. Click "Generate New Token" → "Granular Access Token"
4. Settings:
   - **Token name**: `argentor-ci-v4` (or next version number)
   - **Expiration**: 365 days (or "No expiration" if allowed)
   - **Allowed IP ranges**: leave empty (GitHub Actions IPs change)
   - **Packages and scopes**:
     - Type: "Read and write"
     - Select: `@argentor/*` scope (not entire account — principle of least privilege)
   - **Organizations**:
     - Type: "Read and write"
     - Select: `argentor` org
5. **IMPORTANT**: Check "Bypass 2FA" checkbox
   - Required because automated publishes can't solve 2FA challenges
   - Still requires 2FA for token creation itself (the account MFA remains on)
6. Click "Generate Token"
7. Copy the token (starts with `npm_...`) — **visible only once**

Option B (legacy, "Classic Token"):
- Deprecated by npm. Granular tokens are the way forward.

### Step 2 — Update GitHub Secret

1. Go to https://github.com/fboiero/Argentor/settings/secrets/actions
2. Find `NPM_TOKEN` in the list
3. Click the pencil icon to edit
4. Paste the new token
5. Click "Update secret"

### Step 3 — Revoke the old token

1. Go back to https://www.npmjs.com/settings/YOUR_USERNAME/tokens
2. Find `argentor-ci-v3` (the expiring one)
3. Click "Revoke"
4. Confirm

This prevents old token from being used if it somehow leaks before expiring.

### Step 4 — Verify

Trigger a test publish to confirm the new token works:

```bash
# Option A: trigger the manual workflow
gh workflow run publish-sdks.yml -f dry_run=true

# Monitor
gh run list --workflow=publish-sdks.yml --limit 1
gh run watch <RUN_ID>
```

If the dry-run succeeds, the token is working. If it fails with 401, double-check:
- Token has `@argentor/*` scope selected
- "Bypass 2FA" was checked
- `NPM_TOKEN` secret was saved correctly (no trailing whitespace)

## Best practices going forward

1. **Set a calendar reminder** 7 days before any token expires
2. **Prefer 365-day expiry** for CI tokens (balance between security and ops burden)
3. **Never commit tokens** to any file in the repo — use `.gitignore` for `.npmrc`
4. **Keep token inventory** — maintain a list of all active tokens with expiry dates

## Current token inventory

| Name | Service | Expires | Renew By |
|------|---------|---------|----------|
| `argentor-ci-v3` | npm | 2026-04-19 | 2026-04-17 |
| `CARGO_REGISTRY_TOKEN` | crates.io | Check crates.io settings | — |
| `PYPI_TOKEN` | PyPI | Check PyPI settings | — |
| `GITHUB_TOKEN` | GitHub | Auto-managed by Actions | — |

## If you forgot and it already expired

1. Generate new token (Step 1 above)
2. Update GitHub Secret (Step 2)
3. Re-trigger the failed workflow:
   ```bash
   gh run rerun <FAILED_RUN_ID> --failed
   ```
4. If that doesn't work, tag a patch version to force a clean run:
   ```bash
   # Assuming v1.1.2 is next
   git tag -a v1.1.2 -m "Re-publish after token renewal"
   git push origin v1.1.2
   ```

## Automation idea (future)

Consider setting up a GitHub Action that runs weekly and opens an issue
when any token is within 14 days of expiring. Example:
- https://github.com/marketplace/actions/check-npm-token-expiry

Not implemented yet — manual calendar reminder is enough for now.
