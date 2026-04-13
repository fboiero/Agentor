# Test Count Baseline

> The CI job `test-count-regression` compares the current test count against
> `.github/test-count.baseline` and fails PRs that drop below baseline - 0.5%.

## How the gate works

1. After every successful `test` job on a PR/push, `test-count-regression` runs.
2. It counts all passing tests across the workspace.
3. It compares against the baseline file.
4. If current < baseline - 0.5% tolerance, CI fails.

## When to update the baseline

**Update** when:
- A major new feature adds many tests — baseline should reflect the new floor
- Tests were intentionally consolidated (one mega-test replaced many small ones) — explain in PR description

**Don't update** when:
- You accidentally deleted a test (restore it instead)
- A flaky test was disabled temporarily (fix the flakiness, don't lower baseline)

## How to update

```bash
# Run full test suite locally, get the count
cargo test --workspace 2>&1 | grep "test result:" | awk '{sum += $4} END {print sum}'

# Update the baseline file
echo <NEW_COUNT> > .github/test-count.baseline

# Commit with explanation
git commit -m "ci: raise test-count baseline to N (added X new tests in Y)"
```

## Tolerance rationale

The 0.5% tolerance (currently ~26 tests on a 5,239 baseline) accounts for:
- Tests that genuinely got removed (e.g., removing a dead module)
- Tests consolidated into larger tests
- Flaky tests removed for maintenance (with intent to re-add)

It does NOT accommodate:
- Accidental deletion of whole test files
- Disabling tests to "make CI green"
- Regression that broke test compilation

## Current baseline
See `.github/test-count.baseline` for the authoritative number.
Last set: 2026-04-13 (v1.1.1, after regression + scalability + security test sprint).
