#!/usr/bin/env bash
# Runs every V4 quality gate and prints one PASS/FAIL line per step.
# Usage: scripts/verify.sh [--msrv] [--nightly] [--package]
#   --msrv     also runs the runtime suites on Rust 1.85
#   --nightly  also runs the conditional-`Clone` checks on nightly (reported, never fails the run)
#   --package  also builds and tests a consumer against the packaged .crate files (release check)
set -uo pipefail
cd "$(dirname "$0")/.."

msrv=0 nightly=0 package=0
for arg in "$@"; do
  case "$arg" in
    --msrv) msrv=1 ;;
    --nightly) nightly=1 ;;
    --package) package=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

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

# Fails if `concord` (any feature set) or another example gets reqwest's `multipart` feature, or
# if the pattern stops matching the crate that does enable it.
no_reqwest_multipart() {
  local args tree
  for args in "-p concord" "-p concord --no-default-features" "-p concord-example-native"; do
    # shellcheck disable=SC2086
    tree=$(cargo tree --locked -e features -i reqwest $args) || return 1
    if grep -q 'reqwest feature "multipart"' <<<"$tree"; then
      echo "reqwest multipart reaches: $args"; return 1
    fi
    echo "no reqwest multipart: $args"
  done
  tree=$(cargo tree --locked -e features -i reqwest -p concord-example-multipart) || return 1
  grep 'reqwest feature "multipart"' <<<"$tree"
}

mkdir -p target
rm -f target/verify-*.log  # the summary below reads only this run's logs
echo "head: $(git rev-parse HEAD)  dirty: $(git status --porcelain --untracked-files=no | wc -l) file(s)"
echo "rustc: $(rustc --version)"

step fmt cargo fmt --all -- --check
step records python3 -I scripts/check-records.py
step migrate-via-self-test python3 -I scripts/migrate_via.py --self-test
step clippy cargo clippy --locked --workspace --all-targets -- -D warnings
step test cargo test --workspace --locked
step test-no-default cargo test --locked -p concord --no-default-features
step test-json-only cargo test --locked -p concord --no-default-features --features json
step test-reqwest-defaults-only cargo test --locked -p concord --no-default-features --features reqwest-defaults
# `form` is off by default: alone (form in, text out; the disabled-`json` diagnostics are not run
# because `form` is on) and with the default features for form in / JSON out.
step test-form-only cargo test --locked -p concord --no-default-features --features form
step test-form-json cargo test --locked -p concord --features form --test forms
# Separate consumers: reqwest's `multipart` feature belongs to examples/multipart only, so a
# workspace-wide build (which unifies features) cannot hide a core dependency on it.
step feature-isolation no_reqwest_multipart
step test-multipart-alone cargo test --locked -p concord-example-multipart
step doc env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps
step doc-no-default env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features
step doc-json-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features json
step doc-form-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features form
step doc-form env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --features form

if [[ $msrv == 1 ]]; then
  # Every runtime suite (with `form`, for `forms`); the UI suites (ui, oracle_ui) pin rustc 1.97.1
  # wording and are not run.
  step msrv-concord cargo +1.85 test --locked -p concord --features form --lib \
    --test slice --test values --test composition --test bodies --test representations \
    --test sensitivity --test hygiene --test tls --test attributes --test display_errors \
    --test native_limits --test native_headers --test d15_surfaces --test oracle_composition \
    --test oracle_sensitivity --test formats --test forms
  step msrv-examples cargo +1.85 test --locked -p concord-macros -p concord-example-shapes \
    -p concord-example-renamed -p concord-example-native -p concord-example-quickstart -p concord-heap-check \
    -p concord-clone-compat
  step msrv-multipart-alone cargo +1.85 test --locked -p concord-example-multipart
fi

if [[ $package == 1 ]]; then
  step package-check scripts/package-check.sh target/package-check
fi

if [[ $nightly == 1 ]]; then
  # Non-blocking, like the CI lane: the conditional `Clone` relies on rustc not checking global
  # bounds under a binder (rust-lang/rust#48214); nightly is where a change would show first.
  saved=$status
  step nightly-clone-compat cargo +nightly test --locked -p concord-clone-compat
  status=$saved
fi

for log in target/verify-test*.log target/verify-msrv*.log target/verify-nightly*.log; do
  [[ -f "$log" ]] || continue
  passed=$(grep -o 'test result: ok. [0-9]* passed' "$log" | awk '{s+=$4} END {print s+0}')
  failed=$(grep -o '[0-9]* failed' "$log" | awk '{s+=$1} END {print s+0}')
  echo "      $(basename "$log" .log): ${passed} passed, ${failed} failed"
done
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "FAIL  tracked files changed during verification (e.g. Cargo.lock)"
  status=1
fi
exit $status
