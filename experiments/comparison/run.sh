#!/bin/bash
# Argentor Comparison Experiment Runner
# Usage: ./experiments/comparison/run.sh [--baseline | --compare | --json]
#
# --baseline   Save current run as the new baseline (overwrites baseline.json)
# --compare    Compare current run against the saved baseline (default)
# --json       Output only JSON, no human-readable headers

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RESULTS_DIR="$PROJECT_ROOT/experiments/comparison/results"
BASELINE_FILE="$RESULTS_DIR/baseline.json"
TIMESTAMP=$(date -u +%Y%m%d_%H%M%S)
CURRENT_FILE="$RESULTS_DIR/run_${TIMESTAMP}.json"

mkdir -p "$RESULTS_DIR"

cd "$PROJECT_ROOT"

MODE="${1:---compare}"

case "$MODE" in
    --baseline)
        echo "Running baseline measurement..."
        cargo run -p argentor-comparison --release --quiet 2>/dev/null | \
            awk '/^\[/{flag=1} flag{print}' > "$BASELINE_FILE"
        echo "✓ Baseline saved to $BASELINE_FILE"
        ;;
    --json)
        cargo run -p argentor-comparison --release --quiet 2>/dev/null | \
            awk '/^\[/{flag=1} flag{print}'
        ;;
    --compare)
        if [ ! -f "$BASELINE_FILE" ]; then
            echo "ERROR: No baseline found. Run with --baseline first."
            exit 1
        fi
        echo "Running current measurement..."
        cargo run -p argentor-comparison --release --quiet 2>/dev/null | \
            awk '/^\[/{flag=1} flag{print}' > "$CURRENT_FILE"
        echo "✓ Current run saved to $CURRENT_FILE"
        echo ""
        echo "Comparison vs baseline:"
        echo "----------------------------------------"
        python3 - "$BASELINE_FILE" "$CURRENT_FILE" <<'PYEOF'
import json
import sys

with open(sys.argv[1]) as f:
    baseline = {(m['scenario'], m['metric']): m for m in json.load(f)}
with open(sys.argv[2]) as f:
    current = {(m['scenario'], m['metric']): m for m in json.load(f)}

print(f"{'Scenario':<20} {'Metric':<35} {'Baseline':>12} {'Current':>12} {'Change':>10}")
print("-" * 95)

regressions = []
improvements = []

for key in sorted(set(baseline.keys()) | set(current.keys())):
    scenario, metric = key
    b = baseline.get(key)
    c = current.get(key)
    if b is None:
        print(f"{scenario:<20} {metric:<35} {'NEW':>12} {c['value']:>10.3f}{c['unit']}")
        continue
    if c is None:
        print(f"{scenario:<20} {metric:<35} {b['value']:>10.3f}{b['unit']} {'REMOVED':>12}")
        continue

    b_val = b['value']
    c_val = c['value']
    if b_val == 0:
        change = "n/a"
    else:
        pct = ((c_val - b_val) / b_val) * 100
        # For latency metrics (lower is better), negative change = improvement
        # For throughput (higher is better), positive change = improvement
        is_better_when_higher = b['unit'] in ('rps', 'ops/sec')
        if is_better_when_higher:
            if pct > 5:
                change = f"+{pct:.1f}% ✓"
                improvements.append((key, pct))
            elif pct < -5:
                change = f"{pct:.1f}% ✗"
                regressions.append((key, pct))
            else:
                change = f"{pct:+.1f}%"
        else:
            if pct < -5:
                change = f"{pct:.1f}% ✓"
                improvements.append((key, -pct))
            elif pct > 5:
                change = f"+{pct:.1f}% ✗"
                regressions.append((key, pct))
            else:
                change = f"{pct:+.1f}%"

    print(f"{scenario:<20} {metric:<35} {b_val:>10.3f}{b['unit']} {c_val:>10.3f}{c['unit']} {change:>10}")

print()
print(f"Summary: {len(improvements)} improvements, {len(regressions)} regressions")
if regressions:
    print("⚠️  REGRESSIONS DETECTED:")
    for (s, m), pct in regressions:
        print(f"   {s}/{m}: {pct:+.1f}%")
    sys.exit(1)
PYEOF
        ;;
    *)
        echo "Usage: $0 [--baseline | --compare | --json]"
        exit 1
        ;;
esac
