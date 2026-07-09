#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

if command -v cmd.exe >/dev/null 2>&1 && cmd.exe /c cargo expand --version >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
  CARGO_NEEDS_HOST_PATHS=1
elif command -v cargo >/dev/null 2>&1; then
  CARGO=(cargo)
  CARGO_NEEDS_HOST_PATHS=0
elif command -v cmd.exe >/dev/null 2>&1; then
  CARGO=(cmd.exe /c cargo)
  CARGO_NEEDS_HOST_PATHS=1
else
  echo "error: cargo not found" >&2
  exit 127
fi

cargo_path() {
  local path="$1"
  if [[ "$CARGO_NEEDS_HOST_PATHS" == "1" ]]; then
    if command -v wslpath >/dev/null 2>&1; then
      wslpath -w "$path"
    elif command -v cygpath >/dev/null 2>&1; then
      cygpath -w "$path"
    else
      printf '%s\n' "$path"
    fi
  else
    printf '%s\n' "$path"
  fi
}

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

ROOT_DIR="$PWD"
FIXTURE_ROOT="$ROOT_DIR/target/perf-macro-scale"
BUILD_TARGET_ROOT="$ROOT_DIR/target/perf-macro-scale-target"

if [[ "${CONCORD_PERF_CLEAN:-0}" == "1" ]]; then
  section "opt-in clean"
  emit "cleaning generated fixtures under: $FIXTURE_ROOT"
  rm -rf -- "$FIXTURE_ROOT"
  emit "cleaning generated fixture build artifacts under: $BUILD_TARGET_ROOT"
  rm -rf -- "$BUILD_TARGET_ROOT"
fi

mkdir -p -- "$FIXTURE_ROOT"

ENDPOINT_COUNTS=(5 20 50)
SCOPE_POLICY_OPS=(2 10)

emit "fixture_root: $FIXTURE_ROOT"
emit "build_target_root: $BUILD_TARGET_ROOT"
emit "endpoint_counts: ${ENDPOINT_COUNTS[*]}"
emit "scope_policy_ops: ${SCOPE_POLICY_OPS[*]}"
if "${CARGO[@]}" expand --version >/dev/null 2>&1; then
  HAS_CARGO_EXPAND=1
  emit "cargo_expand: $("${CARGO[@]}" expand --version 2>/dev/null | head -n 1)"
else
  HAS_CARGO_EXPAND=0
  emit "cargo_expand: unavailable; using source-size proxy"
fi
emit "decision_rule_pr9: proceed if expanded size or build time at 50x10 is >25% above the linear projection from the 5x2 baseline scaled by endpoint count; otherwise defer"

generate_fixture() {
  local size="$1"
  local ops="$2"
  local fixture_dir="$FIXTURE_ROOT/endpoints-${size}-scopeops-${ops}"
  local src_dir="$fixture_dir/src"
  local root_path
  root_path="$(cargo_path "$ROOT_DIR")"
  root_path="${root_path//\\//}"
  mkdir -p -- "$src_dir"

  cat >"$fixture_dir/Cargo.toml" <<EOF
[package]
name = "perf_macro_scale_${size}_${ops}"
version = "0.1.0"
edition = "2024"

[workspace]

[dependencies]
concord_core = { path = "$root_path/concord_core", version = "0.1.0", features = ["json"] }
concord_macros = { path = "$root_path/concord_macros", version = "0.1.0" }
http = "1.4"
serde = { version = "1", features = ["derive"] }
EOF

  cat >"$src_dir/lib.rs" <<'EOF'
#[allow(unused_imports)]
use concord_core::prelude::{Json, Text};
use concord_macros::api;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateItem {
    pub name: String,
    pub priority: u32,
}

api! {
    client MacroScaleApi {
        base "https://api.example.com"

        policies {
            retry read {
                max_attempts 2
                methods [GET, POST]
                on [429, 500, 502, 503]
                retry_after
            }

            rate_limit app {
                bucket application by [host] {
                    100 / 1m
                }
            }
        }

        default {
            retry read
            rate_limit app
        }
    }

    scope scale {
        path ["scale"]
        headers {
EOF

  local op_idx
  for op_idx in $(seq 1 "$ops"); do
    printf '            "x-scope-%03d" = "scope-%03d",\n' "$op_idx" "$op_idx" >>"$src_dir/lib.rs"
  done

  cat >>"$src_dir/lib.rs" <<'EOF'
        }
EOF

  local idx mod3 pad endpoint_name
  for idx in $(seq 1 "$size"); do
    pad=$(printf '%03d' "$idx")
    mod3=$((idx % 4))
    case "$mod3" in
      1)
        endpoint_name="GetItem${pad}"
        cat >>"$src_dir/lib.rs" <<EOF

        GET ${endpoint_name}(item_id: u64, page?: u32 = 1, trace_id: String)
            path ["items", item_id]
            query {
                page
            }
            header "X-Trace-Id" = trace_id
            -> Json<Item>
EOF
        ;;
      2)
        endpoint_name="CreateItem${pad}"
        cat >>"$src_dir/lib.rs" <<EOF

        POST ${endpoint_name}(trace_id: String, body: Json<CreateItem>)
            path ["items"]
            header "X-Trace-Id" = trace_id
            -> Json<Item>
EOF
        ;;
      3)
        endpoint_name="SearchItem${pad}"
        cat >>"$src_dir/lib.rs" <<EOF

        GET ${endpoint_name}(term: String, limit?: u32 = 10)
            path ["search"]
            query {
                term
                limit
            }
            -> Text<String>
EOF
        ;;
      *)
        endpoint_name="UpdateItem${pad}"
        cat >>"$src_dir/lib.rs" <<EOF

        POST ${endpoint_name}(item_id: u64, trace_id: String, body: Json<CreateItem>)
            path ["items", item_id]
            header "X-Trace-Id" = trace_id
            -> Json<Item>
EOF
        ;;
    esac
  done

  cat >>"$src_dir/lib.rs" <<'EOF'
    }
}

pub use self::macro_scale_api::{MacroScaleApi, endpoints};
EOF

  local source_bytes source_lines
  source_bytes=$(wc -c <"$src_dir/lib.rs" | tr -d ' ')
  source_lines=$(wc -l <"$src_dir/lib.rs" | tr -d ' ')
  emit "generated fixture: $fixture_dir"
  emit "matrix_point: endpoints=${size} scope_policy_ops=${ops} source_bytes=${source_bytes} source_lines=${source_lines}"
}

run_fixture() {
  local size="$1"
  local ops="$2"
  local fixture_dir="$FIXTURE_ROOT/endpoints-${size}-scopeops-${ops}"
  local expand_file="$fixture_dir/expanded.rs"
  local manifest_path
  local target_dir="$BUILD_TARGET_ROOT/endpoints-${size}-scopeops-${ops}"
  local target_dir_for_cargo
  manifest_path="$(cargo_path "$fixture_dir/Cargo.toml")"
  target_dir_for_cargo="$(cargo_path "$target_dir")"
  generate_fixture "$size" "$ops"
  rm -rf -- "$target_dir"
  if [[ "$HAS_CARGO_EXPAND" == "1" ]]; then
    section "cargo expand macro scale endpoints ${size} scopeops ${ops}"
    quote_cmd "${CARGO[@]}" expand --manifest-path "$manifest_path"
    "${CARGO[@]}" expand --manifest-path "$manifest_path" >"$expand_file" 2>/dev/null || true
    if [[ -s "$expand_file" ]]; then
      local expanded_bytes expanded_lines
      expanded_bytes=$(wc -c <"$expand_file" | tr -d ' ')
      expanded_lines=$(wc -l <"$expand_file" | tr -d ' ')
      emit "expanded_metrics: endpoints=${size} scope_policy_ops=${ops} expanded_bytes=${expanded_bytes} expanded_lines=${expanded_lines}"
    else
      emit "expanded_metrics: endpoints=${size} scope_policy_ops=${ops} unavailable"
    fi
  else
    emit "expanded_metrics: endpoints=${size} scope_policy_ops=${ops} unavailable"
  fi
  run_timed_cmd "cargo check macro scale endpoints ${size} scopeops ${ops}" \
    env CARGO_TARGET_DIR="$target_dir_for_cargo" "${CARGO[@]}" check --manifest-path "$manifest_path" || {
      emit "fixture failed: $fixture_dir"
      return 1
    }
}

for size in "${ENDPOINT_COUNTS[@]}"; do
  for ops in "${SCOPE_POLICY_OPS[@]}"; do
    run_fixture "$size" "$ops"
  done
done
