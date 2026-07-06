#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

if command -v cargo >/dev/null 2>&1; then
  CARGO=(cargo)
elif command -v cmd.exe >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
else
  echo "error: cargo not found" >&2
  exit 127
fi

OUT_FILE="${CONCORD_PERF_OUT:-}"
if [[ -n "$OUT_FILE" ]]; then
  mkdir -p -- "$(dirname -- "$OUT_FILE")"
  : >"$OUT_FILE"
fi

emit() {
  printf '%s\n' "$1"
  if [[ -n "$OUT_FILE" ]]; then
    printf '%s\n' "$1" >>"$OUT_FILE"
  fi
}

section() {
  emit ""
  emit "==> $1"
}

quote_cmd() {
  local rendered="+"
  local arg
  for arg in "$@"; do
    rendered+=" $(printf '%q' "$arg")"
  done
  emit "$rendered"
}

run_cmd() {
  local label="$1"
  shift
  section "$label"
  quote_cmd "$@"
  local output status
  set +e
  output="$("$@" 2>&1)"
  status=$?
  set -e
  emit "$output"
  if [[ $status -ne 0 ]]; then
    emit "command exited with status $status"
    return "$status"
  fi
}

run_timed_cmd() {
  local label="$1"
  shift
  section "$label"
  if [[ -x /usr/bin/time ]]; then
    quote_cmd /usr/bin/time -p "$@"
    local output status
    set +e
    output="$({ /usr/bin/time -p "$@"; } 2>&1)"
    status=$?
    set -e
    emit "$output"
    if [[ $status -ne 0 ]]; then
      emit "command exited with status $status"
      return "$status"
    fi
    return 0
  fi

  quote_cmd "$@"
  local start end output status elapsed
  start=$(date +%s)
  set +e
  output="$("$@" 2>&1)"
  status=$?
  end=$(date +%s)
  set -e
  elapsed=$((end - start))
  emit "$output"
  emit "timing: real ${elapsed}s (fallback; /usr/bin/time -p unavailable)"
  if [[ $status -ne 0 ]]; then
    emit "command exited with status $status"
    return "$status"
  fi
}

metadata_report() {
  local label="$1"
  shift
  section "$label"
  quote_cmd "${CARGO[@]}" metadata --no-deps --format-version 1 "$@"
  local metadata_json
  metadata_json="$("${CARGO[@]}" metadata --no-deps --format-version 1 "$@")"
  emit "$metadata_json"
  if [[ "$metadata_json" == *'"name":"perf"'* ]]; then
    emit "perf_present_in_packages: yes"
  else
    emit "perf_present_in_packages: no"
  fi
}

if [[ "${CONCORD_PERF_CLEAN:-0}" == "1" ]]; then
  run_cmd "opt-in cold clean" "${CARGO[@]}" clean
fi

section "workspace shape"
quote_cmd grep -nE '^\[workspace\]|^members =|^exclude =|^default-members =' Cargo.toml
workspace_manifest="$(grep -nE '^\[workspace\]|^members =|^exclude =|^default-members =' Cargo.toml || true)"
emit "$workspace_manifest"
metadata_report "workspace metadata summary" --manifest-path Cargo.toml
metadata_report "perf package metadata summary" --manifest-path perf/Cargo.toml

run_cmd "concord_core tree --no-default-features" "${CARGO[@]}" tree -p concord_core --no-default-features
run_cmd "concord_core tree --features json" "${CARGO[@]}" tree -p concord_core --features json
run_cmd "concord_core tree --all-features" "${CARGO[@]}" tree -p concord_core --all-features
run_cmd "concord_core feature tree --no-default-features" "${CARGO[@]}" tree -p concord_core -e features --no-default-features
run_cmd "concord_core feature tree --features json" "${CARGO[@]}" tree -p concord_core -e features --features json
run_cmd "concord_core feature tree --all-features" "${CARGO[@]}" tree -p concord_core -e features --all-features

section "reqwest footprint"
run_cmd "concord_core reqwest inverse tree --all-features" "${CARGO[@]}" tree -p concord_core --all-features -i reqwest
run_cmd "concord_core reqwest inverse tree --no-default-features" "${CARGO[@]}" tree -p concord_core --no-default-features -i reqwest

reqwest_no_default_output="$("${CARGO[@]}" tree -p concord_core --no-default-features -i reqwest)"
if [[ "$reqwest_no_default_output" == *"reqwest v"* ]]; then
  emit "no-default concord_core includes reqwest: yes"
else
  emit "no-default concord_core includes reqwest: no"
fi

section "macro footprint"
macro_default_output="$("${CARGO[@]}" tree -p concord_macros)"
macro_no_default_output="$("${CARGO[@]}" tree -p concord_macros --no-default-features)"
quote_cmd "${CARGO[@]}" tree -p concord_macros
emit "$macro_default_output"
quote_cmd "${CARGO[@]}" tree -p concord_macros --no-default-features
emit "$macro_no_default_output"
if [[ "$macro_default_output" == "$macro_no_default_output" ]]; then
  emit "concord_macros default/no-default trees: identical"
else
  emit "concord_macros default/no-default trees: differ"
fi
if [[ "$macro_default_output" == *"serde_json v"* || "$macro_no_default_output" == *"serde_json v"* ]]; then
  emit "serde_json present in concord_macros tree: yes"
else
  emit "serde_json present in concord_macros tree: no"
fi

section "examples footprint"
run_cmd "concord_examples tree" "${CARGO[@]}" tree -p concord_examples
run_cmd "concord_examples tree --features" "${CARGO[@]}" tree -p concord_examples -e features

section "perf package footprint"
run_cmd "perf tree" "${CARGO[@]}" tree --manifest-path perf/Cargo.toml
run_cmd "perf feature tree" "${CARGO[@]}" tree --manifest-path perf/Cargo.toml -e features

section "build timing"
run_timed_cmd "cargo check concord_core --no-default-features" "${CARGO[@]}" check -p concord_core --no-default-features
run_timed_cmd "cargo check concord_core --features json" "${CARGO[@]}" check -p concord_core --features json
run_timed_cmd "cargo check concord_core --all-features" "${CARGO[@]}" check -p concord_core --all-features
run_timed_cmd "cargo check concord_macros" "${CARGO[@]}" check -p concord_macros
run_timed_cmd "cargo check perf package" "${CARGO[@]}" check --manifest-path perf/Cargo.toml
