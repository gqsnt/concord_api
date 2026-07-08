#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."
repo_root="$PWD"
CARGO_WRAPPER_DIR=""

cleanup() {
  if [[ -n "$CARGO_WRAPPER_DIR" ]]; then
    rm -rf -- "$CARGO_WRAPPER_DIR"
  fi
}

trap cleanup EXIT

prefer_cargo_with_tools() {
  if command -v cargo >/dev/null 2>&1 &&
    cargo nextest --version >/dev/null 2>&1 &&
    cargo deny --version >/dev/null 2>&1; then
    return
  fi

  local candidate
  local candidates=()
  if [[ -n "${CONCORD_CARGO_BIN:-}" ]]; then
    candidates+=("$CONCORD_CARGO_BIN")
  fi
  if [[ -n "${USER:-}" ]]; then
    candidates+=("/mnt/c/Users/$USER/.cargo/bin")
  fi
  if [[ -n "${USERNAME:-}" ]]; then
    candidates+=("/mnt/c/Users/$USERNAME/.cargo/bin")
  fi
  if command -v cargo-nextest.exe >/dev/null 2>&1; then
    candidates+=("$(dirname -- "$(command -v cargo-nextest.exe)")")
  fi
  if command -v cargo-deny.exe >/dev/null 2>&1; then
    candidates+=("$(dirname -- "$(command -v cargo-deny.exe)")")
  fi
  candidates+=("$HOME/.cargo/bin")

  for candidate in "${candidates[@]}"; do
    local cargo_exe="$candidate/cargo.exe"
    if [[ -x "$cargo_exe" ]] &&
      "$cargo_exe" nextest --version >/dev/null 2>&1 &&
      "$cargo_exe" deny --version >/dev/null 2>&1; then
      CARGO_WRAPPER_DIR="$(mktemp -d "${TMPDIR:-/tmp}/concord-perf-gate.XXXXXX")"
      cat >"$CARGO_WRAPPER_DIR/cargo" <<EOF
#!/usr/bin/env bash
exec "$cargo_exe" "\$@"
EOF
      chmod +x "$CARGO_WRAPPER_DIR/cargo"
      export PATH="$CARGO_WRAPPER_DIR:$candidate:$PATH"
      return
    fi
  done
}

prefer_cargo_with_tools

if command -v cargo >/dev/null 2>&1; then
  CARGO=(cargo)
elif command -v cmd.exe >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
else
  echo "error: cargo not found" >&2
  exit 127
fi

OUT_FILE="${CONCORD_PERF_OUT:-}"
STRICT="${CONCORD_PERF_STRICT:-0}"

if [[ -n "$OUT_FILE" ]]; then
  mkdir -p -- "$(dirname -- "$OUT_FILE")"
  : >"$OUT_FILE"
  exec > >(tee -a "$OUT_FILE") 2>&1
fi

failed_labels=()
failed_commands=()
skipped_labels=()
skipped_reasons=()
commands_run=0

if [[ -x /usr/bin/time ]]; then
  HAS_USR_BIN_TIME=1
else
  HAS_USR_BIN_TIME=0
fi

if "${CARGO[@]}" nextest --version >/dev/null 2>&1; then
  HAS_NEXTEST=1
  NEXTEST_VERSION="$("${CARGO[@]}" nextest --version 2>/dev/null | head -n 1)"
else
  HAS_NEXTEST=0
  NEXTEST_VERSION="unavailable"
fi

if "${CARGO[@]}" deny --version >/dev/null 2>&1; then
  HAS_DENY=1
  DENY_VERSION="$("${CARGO[@]}" deny --version 2>/dev/null | head -n 1)"
else
  HAS_DENY=0
  DENY_VERSION="unavailable"
fi

timestamp() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

quote_cmd() {
  local rendered="+"
  local arg
  for arg in "$@"; do
    rendered+=" $(printf '%q' "$arg")"
  done
  printf '%s\n' "$rendered"
}

section() {
  printf '\n==> %s\n' "$1"
}

record_failure() {
  local label="$1"
  shift
  failed_labels+=("$label")
  failed_commands+=("$(printf '%q ' "$@")")
}

record_skip() {
  local label="$1"
  local reason="$2"
  skipped_labels+=("$label")
  skipped_reasons+=("$reason")
  printf 'SKIPPED: %s\n' "$reason"
  if [[ "$STRICT" == "1" ]]; then
    failed_labels+=("$label")
    failed_commands+=("skipped: $reason")
  fi
}

run_timed() {
  local label="$1"
  shift

  section "$label"
  commands_run=$((commands_run + 1))

  local status=0
  if [[ "$HAS_USR_BIN_TIME" == "1" ]]; then
    quote_cmd /usr/bin/time -p "$@"
    set +e
    /usr/bin/time -p "$@"
    status=$?
    set -e
  else
    quote_cmd "$@"
    local start end elapsed
    start="$(date +%s)"
    set +e
    "$@"
    status=$?
    set -e
    end="$(date +%s)"
    elapsed=$((end - start))
    printf 'timing: real %ss (fallback; /usr/bin/time -p unavailable)\n' "$elapsed"
  fi

  if [[ "$status" -ne 0 ]]; then
    printf 'FAILED: %s exited with status %s\n' "$label" "$status"
    record_failure "$label" "$@"
  else
    printf 'PASSED: %s\n' "$label"
  fi
}

run_skip() {
  local label="$1"
  local reason="$2"
  section "$label"
  record_skip "$label" "$reason"
}

run_nextest_or_fallback() {
  local label="$1"
  shift
  local fallback_label="$1"
  shift

  if [[ "$HAS_NEXTEST" == "1" ]]; then
    run_timed "$label" "${CARGO[@]}" nextest run "$@"
  else
    run_skip "$label" "cargo-nextest is unavailable; running fallback \`$fallback_label\`"
    run_timed "$fallback_label" "${CARGO[@]}" test "$@"
  fi
}

print_heading() {
  printf '# Concord Release-Gate Timing Report\n'
  printf 'date_utc: %s\n' "$(timestamp)"
  printf 'repository_root: %s\n' "$repo_root"
  printf 'nextest: %s\n' "$NEXTEST_VERSION"
  printf 'cargo_deny: %s\n' "$DENY_VERSION"
  if [[ -n "$OUT_FILE" ]]; then
    printf 'output_file_mode: enabled (%s)\n' "$OUT_FILE"
  else
    printf 'output_file_mode: disabled\n'
  fi
  if [[ "$STRICT" == "1" ]]; then
    printf 'strict_mode: enabled\n'
  else
    printf 'strict_mode: disabled\n'
  fi
  if [[ "$HAS_USR_BIN_TIME" == "1" ]]; then
    printf 'timer: /usr/bin/time -p\n'
  else
    printf 'timer: shell timestamp fallback\n'
  fi
  printf 'report_only: true\n'
  printf 'timing_thresholds: none\n'
  printf 'decision_rule_pr11: proceed if redundant per-crate steps are >=~20%% of total gate wall time; otherwise defer\n'
}

print_summary() {
  printf '\n==> Summary\n'
  printf 'commands_run: %s\n' "$commands_run"
  printf 'failed: %s\n' "${#failed_labels[@]}"
  printf 'skipped: %s\n' "${#skipped_labels[@]}"

  printf 'failed_labels:\n'
  if [[ "${#failed_labels[@]}" -eq 0 ]]; then
    printf '  none\n'
  else
    local idx
    for idx in "${!failed_labels[@]}"; do
      printf '  - %s\n' "${failed_labels[$idx]}"
      printf '    command: %s\n' "${failed_commands[$idx]}"
    done
  fi

  printf 'skipped_labels:\n'
  if [[ "${#skipped_labels[@]}" -eq 0 ]]; then
    printf '  none\n'
  else
    local idx
    for idx in "${!skipped_labels[@]}"; do
      printf '  - %s\n' "${skipped_labels[$idx]}"
      printf '    reason: %s\n' "${skipped_reasons[$idx]}"
    done
  fi
}

print_heading

section "Architecture/check scripts"
run_timed "architecture boundary" bash ./scripts/check_architecture.sh
run_timed "feature matrix" bash ./scripts/check_features.sh

section "Format/lint/doc"
run_timed "format check" "${CARGO[@]}" fmt --check
run_timed "clippy workspace all targets" "${CARGO[@]}" clippy --workspace --all-targets

section "Test commands"
if [[ "$HAS_NEXTEST" == "1" ]]; then
  run_timed "macro integration tests" "${CARGO[@]}" nextest run -p concord_macros integration
  run_timed "macro generated tests" "${CARGO[@]}" nextest run -p concord_macros generated
else
  run_skip "macro integration tests" "cargo-nextest is unavailable; running fallback \`cargo test -p concord_macros integration\`"
  run_timed "cargo test macro integration" "${CARGO[@]}" test -p concord_macros integration
  run_skip "macro generated tests" "cargo-nextest is unavailable; running fallback \`cargo test -p concord_macros generated\`"
  run_timed "cargo test macro generated" "${CARGO[@]}" test -p concord_macros generated
fi

if [[ "$HAS_NEXTEST" == "1" ]]; then
  run_timed "macro trybuild current" "${CARGO[@]}" nextest run -p concord_macros --test trybuild_current
  run_timed "macro trybuild sema" "${CARGO[@]}" nextest run -p concord_macros --test trybuild_sema
  run_timed "macro trybuild codegen" "${CARGO[@]}" nextest run -p concord_macros --test trybuild_codegen
else
  run_timed "cargo test macro trybuild current nocapture" "${CARGO[@]}" test -p concord_macros --test trybuild_current -- --nocapture
  run_timed "cargo test macro trybuild sema nocapture" "${CARGO[@]}" test -p concord_macros --test trybuild_sema -- --nocapture
  run_timed "cargo test macro trybuild codegen nocapture" "${CARGO[@]}" test -p concord_macros --test trybuild_codegen -- --nocapture
fi

run_nextest_or_fallback "core tests" "cargo test -p concord_core" -p concord_core
run_nextest_or_fallback "core tests all features" "cargo test -p concord_core --all-features" -p concord_core --all-features
run_nextest_or_fallback "examples tests" "cargo test -p concord_examples" -p concord_examples
run_nextest_or_fallback "examples tests all features" "cargo test -p concord_examples --all-features" -p concord_examples --all-features
run_nextest_or_fallback "workspace tests" "cargo test --workspace" --workspace
run_nextest_or_fallback "workspace tests all features" "cargo test --workspace --all-features" --workspace --all-features
run_nextest_or_fallback "workspace all-target tests" "cargo test --workspace --all-targets" --workspace --all-targets

section "Doc"
run_timed "rustdoc warnings denied" env RUSTDOCFLAGS="-D warnings" "${CARGO[@]}" doc --workspace --no-deps

print_summary

if [[ "${#failed_labels[@]}" -ne 0 ]]; then
  exit 1
fi
