#!/usr/bin/env bash
# Runs every V4 quality gate and prints one PASS/FAIL line per step.
# Usage: scripts/verify.sh [--msrv]   (--msrv also runs the runtime suites on Rust 1.85)
set -uo pipefail
cd "$(dirname "$0")/.."

status=0
step() {
  local name="$1"; shift
  if "$@" >"target/verify-${name}.log" 2>&1; then
    echo "PASS  ${name}"
  else
    echo "FAIL  ${name}  (see target/verify-${name}.log)"
    status=1
  fi
}

mkdir -p target
echo "head: $(git rev-parse HEAD)  dirty: $(git status --porcelain --untracked-files=no | wc -l) file(s)"
echo "rustc: $(rustc --version)"

step fmt cargo fmt --all -- --check
step clippy cargo clippy --workspace --all-targets -- -D warnings
step test cargo test --workspace
step test-no-default cargo test -p concord --no-default-features
step test-json-only cargo test -p concord --no-default-features --features json
step test-reqwest-defaults-only cargo test -p concord --no-default-features --features reqwest-defaults
step doc env RUSTDOCFLAGS="-D warnings" cargo doc -p concord --no-deps

if [[ "${1:-}" == "--msrv" ]]; then
  step msrv-concord cargo +1.85 test -p concord --lib \
    --test slice --test values --test composition --test bodies --test representations \
    --test sensitivity --test hygiene --test tls
  step msrv-examples cargo +1.85 test -p concord-example-shapes -p concord-example-renamed
fi

for log in target/verify-test*.log target/verify-msrv*.log; do
  [[ -f "$log" ]] || continue
  passed=$(grep -o 'test result: ok. [0-9]* passed' "$log" | awk '{s+=$4} END {print s+0}')
  failed=$(grep -o '[0-9]* failed' "$log" | awk '{s+=$1} END {print s+0}')
  echo "      $(basename "$log" .log): ${passed} passed, ${failed} failed"
done
exit $status
