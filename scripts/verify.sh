#!/usr/bin/env bash
# The one inventory of V4's quality gates, run locally and by CI (.github/workflows/ci.yml calls
# this script; scripts/check-ci.py fails if the workflow lists its own cargo commands). Prints one
# PASS/FAIL line per step.
#
# Usage: scripts/verify.sh [--package] [--bench] [--nightly | --nightly-only]
#   (no flag)       every gate that runs on all platforms, on the pinned toolchain
#   --package       also builds and tests consumers against the packaged .crate files
#   --bench         also compiles the benchmarks (never runs them)
#   --nightly       also runs the conditional-`Clone` checks on nightly (reported, never fails)
#   --nightly-only  only the nightly checks (the non-blocking CI lane)
#
# The minimum supported Rust (`rust-version`) and the pin in rust-toolchain.toml are the same
# release, so every step below is also the minimum-version check; `toolchain` verifies that.
set -uo pipefail
cd "$(dirname "$0")/.."

package=0 bench=0 nightly=0 only_nightly=0
for arg in "$@"; do
  case "$arg" in
    --package) package=1 ;;
    --bench) bench=1 ;;
    --nightly) nightly=1 ;;
    --nightly-only) nightly=1 only_nightly=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done
python=$(command -v python3 || command -v python) || { echo "python 3 is required" >&2; exit 2; }

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
  for args in "-p concord" "-p concord --no-default-features" "-p concord-core" \
    "-p concord-extras --all-features" "-p concord-example-native"; do
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

# Every step runs python, cargo and tar through scripts/argv-guard.sh, which refuses a command line
# over the Windows limit on every platform (a consumers.sh argument once failed only on Windows).
argv_guard() {
  local dir="$PWD/target/argv-guard" name real
  rm -rf "$dir" && mkdir -p "$dir" || return 1
  for name in python3 python cargo tar; do
    real=$(command -v "$name") || continue
    printf '%s' "$real" >"$dir/$name.real"
    cp scripts/argv-guard.sh "$dir/$name" && chmod +x "$dir/$name" || return 1
  done
  PATH="$dir:$PATH"
}
argv_guard || { echo "argv-guard could not be installed" >&2; exit 2; }
python=$(command -v python3 || command -v python)  # now the guarded one
echo "head: $(git rev-parse HEAD)  dirty: $(git status --porcelain --untracked-files=no | wc -l) file(s)"
echo "rustc: $(rustc --version)"

if [[ $only_nightly == 0 ]]; then
step toolchain "$python" -I scripts/check-ci.py --toolchain
step ci-inventory "$python" -I scripts/check-ci.py --workflow
# The production edges of the four crates (manifests, then one resolved graph per package).
step graph "$python" -I scripts/check-graph.py
step fmt cargo fmt --all -- --check
step records "$python" -I scripts/check-records.py
# Generated test files (oracles, the synthetic SDK) match their generators.
generated() {
  local g
  for g in oracle_composition oracle_sensitivity synthetic_sdk; do
    "$python" -I "tests-gen/$g.py" --check || { echo "stale: tests-gen/$g.py"; return 1; }
  done
}
step generated generated
step clippy cargo clippy --locked --workspace --all-targets -- -D warnings
step test cargo test --workspace --locked
step test-no-default cargo test --locked -p concord --no-default-features
step test-json-only cargo test --locked -p concord --no-default-features --features json
step test-reqwest-defaults-only cargo test --locked -p concord --no-default-features --features reqwest-defaults
# `form` is off by default: alone (form in, text out; the disabled-`json` diagnostics are not run
# because `form` is on) and with the default features for form in / JSON out.
step test-form-only cargo test --locked -p concord --no-default-features --features form
step test-form-json cargo test --locked -p concord --features form --test forms
step test-extras-only cargo test --locked -p concord --no-default-features --features extras
# The runtime and the standard formats on their own, each feature separately.
step test-core cargo test --locked -p concord-core
step test-extras-none cargo test --locked -p concord-extras --no-default-features
step test-extras-json cargo test --locked -p concord-extras --no-default-features --features json
step test-extras-form cargo test --locked -p concord-extras --no-default-features --features form
# Lints of the feature subsets the workspace build unifies away (code used only by some formats).
clippy_features() {
  local features
  for features in "" json form; do
    cargo clippy --locked -p concord-extras --no-default-features ${features:+--features "$features"} \
      --all-targets -- -D warnings || return 1
  done
  cargo clippy --locked -p concord --no-default-features --features extras -- -D warnings
}
step clippy-features clippy_features
# The architecture consumers (checks/consumers): core alone, the facade without extras (renamed),
# extras + core without the macro, the full facade, the facade with `json` only; each its own
# workspace, so feature unification cannot help them.
step consumers scripts/consumers.sh
# Separate consumers: reqwest's `multipart` feature belongs to examples/multipart only, so a
# workspace-wide build (which unifies features) cannot hide a core dependency on it.
step feature-isolation no_reqwest_multipart
step test-multipart-alone cargo test --locked -p concord-example-multipart
step doc env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps
step doc-no-default env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features
step doc-json-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features json
step doc-form-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features form
step doc-form env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --features form
step doc-extras-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features extras
step doc-core env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-core --no-deps
step doc-extras env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-extras --no-deps --all-features
step doc-extras-none env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-extras --no-deps --no-default-features

if [[ $bench == 1 ]]; then
  step bench-build cargo bench --locked -p concord-bench --no-run
fi

if [[ $package == 1 ]]; then
  step package-check scripts/package-check.sh target/package-check
  step consumers-packaged scripts/consumers.sh --packaged target/consumer-packages
fi
fi

if [[ $nightly == 1 ]]; then
  # Non-blocking, like the CI lane: the conditional `Clone` relies on rustc not checking global
  # bounds under a binder (rust-lang/rust#48214); nightly is where a change would show first.
  saved=$status
  step nightly-clone-compat cargo +nightly test --locked -p concord-clone-compat
  status=$saved
fi

for log in target/verify-test*.log target/verify-nightly*.log; do
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
