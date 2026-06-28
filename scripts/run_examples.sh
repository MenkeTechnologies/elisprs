#!/bin/sh
# run_examples.sh — run every examples/*.el through the elisp binary and fail if
# any exits non-zero. Each example is a self-test: it checks its results with an
# `expect` helper and raises an elisp `error` (→ non-zero exit) on a mismatch, so
# this is the example-script regression gate used by CI (the `examples` job).
#
# Binary resolution: $ELISP, else target/release/elisp, else target/debug/elisp.
#   sh scripts/run_examples.sh
#   ELISP=/path/to/elisp sh scripts/run_examples.sh
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BIN="${ELISP:-}"
if [ -z "$BIN" ]; then
  if [ -x target/release/elisp ]; then
    BIN=target/release/elisp
  elif [ -x target/debug/elisp ]; then
    BIN=target/debug/elisp
  else
    echo "no elisp binary found — build first (cargo build --release)" >&2
    exit 2
  fi
fi
echo "running examples with: $BIN"

fail=0
total=0
for f in examples/*.el; do
  total=$((total + 1))
  stem="$(basename "$f" .el)"
  if "$BIN" "$f" >/dev/null 2>&1; then
    echo "ok   $stem"
  else
    code=$?
    echo "FAIL $stem (exit $code)"
    # Re-run showing output so the CI log captures the failure detail.
    "$BIN" "$f" 2>&1 | sed 's/^/     | /'
    fail=$((fail + 1))
  fi
done

echo "---"
echo "$((total - fail))/$total example scripts passed"
[ "$fail" -eq 0 ] || exit 1
