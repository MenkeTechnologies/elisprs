#!/usr/bin/env bash
# elisprs test runner — the full polish-gate suite plus the example self-tests.
# Mirrors the sibling repos' `pnpm test` (scripts/test.sh): fmt, clippy, doc,
# `cargo test`, and every examples/*.el run through the built binary.
#
#   pnpm test            # or: bash scripts/test.sh
set -uo pipefail
cd "$(dirname "$0")/.."

# ── minimal styling (no external deps) ──
if [ -t 1 ]; then C='\033[36m'; G='\033[32m'; R='\033[31m'; D='\033[2m'; N='\033[0m'; else C= G= R= D= N=; fi
section() { printf "${C}==> %s${N}\n" "$1"; }
ok()      { printf "${G}  ✓ %s${N}\n" "$1"; }
fail()    { printf "${R}  ✗ %s${N}\n" "$1"; }

rc=0
run() { # run <label> <cmd...>
  label="$1"; shift
  section "$label"
  if "$@"; then ok "$label"; else fail "$label"; rc=1; fi
  echo
}

run "fmt (cargo fmt --all --check)"        cargo fmt --all --check
run "clippy (-D warnings)"                 cargo clippy --all-targets --locked -- -D warnings
run "doc (-D warnings)"                    env RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
run "unit + integration tests"             cargo test --locked

# Example self-tests through the release binary (each examples/*.el is ERT-driven
# and exits non-zero on a failed assertion).
section "example scripts (examples/*.el)"
if cargo build --release --locked && sh scripts/run_examples.sh; then ok "examples"; else fail "examples"; rc=1; fi
echo

if [ "$rc" = "0" ]; then
  printf "${G}ALL CHECKS PASSED.${N}\n"
else
  printf "${R}>>> CHECKS FAILED — fix before shipping <<<${N}\n"
fi
exit "$rc"
