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
  socket_snapshot "after:${name}"
}

# Diagnostic only, never a gate: machine-wide TCP counters (TIME_WAIT, the dynamic port range a
# `bind` to port 0 draws from, its excluded and in-use ports, connections opened) appended to
# target/verify-socket-pressure.log after every step and around every example package; a summary
# is printed at the end. The 4.3 Windows job 126834420624 failed a loopback bind with WSAENOBUFS
# (10055); these counters show how close each runner came to the dynamic-port limit and how much
# of it the gate's own tests used (scripts/socket-snapshot.py documents what they can show).
socket_snapshot() {
  "$python" -I scripts/socket-snapshot.py "$@" >>target/verify-socket-pressure.log 2>&1 || true
}

# Fails if `concord` (any feature set) or another example gets reqwest's `multipart` feature or a
# response decompression feature (an application enables those: checks/consumers/facade-gzip), or
# if the pattern stops matching the crate that does enable multipart. The two SDKs on Concord's own
# multipart format (feature `multipart`) show that it does not bring reqwest's.
no_reqwest_multipart() {
  local args tree
  for args in "-p concord" "-p concord --no-default-features" "-p concord --all-features" \
    "-p concord-core" "-p concord-codecs --all-features" "-p concord-extras --all-features" "-p concord-example-native" \
    "-p concord-sdk-openai" "-p concord-sdk-google-drive"; do
    # shellcheck disable=SC2086
    tree=$(cargo tree --locked -e features -i reqwest $args) || return 1
    if grep -Eq 'reqwest feature "(multipart|gzip|brotli|zstd|deflate)"' <<<"$tree"; then
      echo "reqwest multipart or decompression reaches: $args"; return 1
    fi
    echo "no reqwest multipart or decompression: $args"
  done
  tree=$(cargo tree --locked -e features -i reqwest -p concord-example-multipart) || return 1
  grep 'reqwest feature "multipart"' <<<"$tree"
}

mkdir -p target
rm -f target/verify-*.log  # the summary below reads only this run's logs
rm -f target/socket-snapshot.disabled  # scripts/socket-snapshot.py: re-enabled for every run

# Every step runs python, cargo and tar through scripts/argv-guard.sh, which refuses a command line
# over the Windows limit on every platform (a consumers.sh argument once failed only on Windows).
argv_guard() {
  local dir="$PWD/target/argv-guard" name real
  rm -rf "$dir" && mkdir -p "$dir" || return 1
  printf '%s' "$python" >"$dir/interpreter" && cp scripts/argv-length.py "$dir/" || return 1
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
socket_snapshot start --details

if [[ $only_nightly == 0 ]]; then
# The guard's length model (MSVC quoting, UTF-16 units) against known exact lengths.
step argv-length "$python" -I scripts/argv-length.py --self-test
step toolchain "$python" -I scripts/check-ci.py --toolchain
step ci-inventory "$python" -I scripts/check-ci.py --workflow
# The tools' own controls: the workflow guard on the real workflow, its supported forms and its
# negatives (inert, disabled, swallowed, missing-platform gates), each with its intended message;
# the retired mutation helper refuses to run and has no maintained caller.
step tool-tests "$python" -I -B -m unittest discover -s scripts/tests -p "test_*.py" -v
# Designated output directories (scripts/fresh-output.sh): recreated only from their owner's mark,
# anything else refused and left untouched, stale extracted sources detected; builds nothing.
step output-dirs scripts/tests/output-dirs.sh
# The production edges of the crates (manifests, then one resolved graph per package).
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
# `multipart` is off by default too: the facade with it alone (multipart in, text out).
step test-multipart-only cargo test --locked -p concord --no-default-features --features multipart
step test-extras-only cargo test --locked -p concord --no-default-features --features extras
# The runtime and the standard formats on their own, each feature separately.
step test-core cargo test --locked -p concord-core
step test-codecs-none cargo test --locked -p concord-codecs --no-default-features
step test-codecs-json cargo test --locked -p concord-codecs --no-default-features --features json
step test-codecs-form cargo test --locked -p concord-codecs --no-default-features --features form
# The incremental-body infrastructure alone, and each streaming format alone (CSV without
# serde_json, JSON Lines without csv, server-sent events without serde), so none can lean on
# another's dependencies.
step test-codecs-stream cargo test --locked -p concord-codecs --no-default-features --features stream
step test-codecs-csv cargo test --locked -p concord-codecs --no-default-features --features csv
step test-codecs-jsonl cargo test --locked -p concord-codecs --no-default-features --features jsonl
# The multipart request format alone (no serializer, no stream infrastructure).
step test-codecs-multipart cargo test --locked -p concord-codecs --no-default-features --features multipart
step test-codecs-sse cargo test --locked -p concord-codecs --no-default-features --features sse
step test-codecs-all cargo test --locked -p concord-codecs --all-features
step test-extras-facade-none cargo test --locked -p concord-extras --no-default-features
step test-extras-facade-all cargo test --locked -p concord-extras --all-features
# The pagination crate alone (it has no features and knows no Concord crate).
step test-pagination cargo test --locked -p concord-pagination
# Lints of the feature subsets the workspace build unifies away (code used only by some formats).
clippy_features() {
  local features
  for features in "" json form stream csv jsonl multipart sse; do
    cargo clippy --locked -p concord-codecs --no-default-features ${features:+--features "$features"} \
      --all-targets -- -D warnings || return 1
  done
  cargo clippy --locked -p concord --no-default-features --features extras -- -D warnings
}
step clippy-features clippy_features
# The architecture consumers (checks/consumers): core alone, the facade without extras (renamed),
# extras + core without the macro, the full facade, the facade with `json` only, an application
# with reqwest's gzip support (fetches its own dependencies); each its own workspace, so feature
# unification cannot help them.
step consumers scripts/consumers.sh
# Separate consumers: reqwest's `multipart` feature belongs to examples/multipart only, so a
# workspace-wide build (which unifies features) cannot hide a core dependency on it.
step feature-isolation no_reqwest_multipart
# Every example package alone, one `cargo test -p` each (never one command for several): features
# are unified only across the packages selected, so no other member can supply a feature that an
# example forgot to declare (the renamed example once relied on that for `extras`). Every directory
# under examples/ must be a workspace member, and each package must run at least one test.
examples_isolated() {
  local names name log rc passed failed=0
  names=$(cargo metadata --locked --no-deps --format-version 1 | "$python" -I -c '
import json, os, sys
m = json.loads(sys.stdin.buffer.read())
examples = os.path.join(m["workspace_root"], "examples")
key = lambda p: os.path.normcase(os.path.realpath(p))
members = {key(os.path.dirname(p["manifest_path"])): p["name"] for p in m["packages"]}
names, missing = [], []
for entry in sorted(os.listdir(examples)):
    if os.path.isfile(os.path.join(examples, entry, "Cargo.toml")):
        name = members.get(key(os.path.join(examples, entry)))
        if name:
            names.append(name)
        else:
            missing.append(entry)
if missing:
    sys.exit("examples that are not workspace members: " + ", ".join(missing))
print(" ".join(names))
') || return 1
  names=${names%$'\r'}
  [[ -n $names ]] || { echo "no example packages"; return 1; }
  for name in $names; do
    socket_snapshot "examples:${name}:before"
    log=$(cargo test --locked -p "$name" 2>&1)
    rc=$?
    socket_snapshot "examples:${name}:after"
    echo "$log"
    passed=$(grep -o 'test result: ok. [0-9]* passed' <<<"$log" | awk '{s+=$4} END {print s+0}')
    if [[ $rc != 0 || $passed == 0 ]]; then
      echo "FAIL  $name alone (exit $rc, $passed tests)"
      failed=1
    else
      echo "ok    $name alone ($passed tests)"
    fi
  done
  return "$failed"
}
step test-examples-isolated examples_isolated
step doc env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps
step doc-no-default env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features
step doc-json-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features json
step doc-form-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features form
step doc-form env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --features form
step doc-extras-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features extras
step doc-core env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-core --no-deps
step doc-codecs env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --all-features
step doc-codecs-none env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features
step doc-codecs-json env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features json
step doc-codecs-form env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features form
step doc-codecs-stream env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features stream
step doc-codecs-csv env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features csv
step doc-codecs-jsonl env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features jsonl
step doc-streaming env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features csv,jsonl,sse
step doc-multipart-only env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord --no-deps --no-default-features --features multipart
step doc-codecs-multipart env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features multipart
step doc-codecs-sse env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-codecs --no-deps --no-default-features --features sse
step doc-extras-facade env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-extras --no-deps --all-features
step doc-extras-facade-none env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-extras --no-deps --no-default-features
step doc-pagination env RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-pagination --no-deps

if [[ $bench == 1 ]]; then
  step bench-build cargo bench --locked -p concord-bench --no-run
fi

if [[ $package == 1 ]]; then
  # Each extracts into its own designated directory (recreated, or refused if foreign).
  step package-check scripts/package-check.sh target/packaged/package-check
  step consumers-packaged scripts/consumers.sh --packaged target/packaged/consumers
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
echo "      $("$python" -I scripts/socket-snapshot.py --summary target/verify-socket-pressure.log 2>&1)"
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo "FAIL  tracked files changed during verification (e.g. Cargo.lock)"
  status=1
fi
exit $status
