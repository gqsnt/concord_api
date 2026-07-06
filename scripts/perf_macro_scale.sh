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

ROOT_DIR="$PWD"
FIXTURE_ROOT="$ROOT_DIR/target/perf-macro-scale"

if [[ "${CONCORD_PERF_CLEAN:-0}" == "1" ]]; then
  section "opt-in clean"
  emit "cleaning generated fixtures under: $FIXTURE_ROOT"
  rm -rf -- "$FIXTURE_ROOT"
fi

mkdir -p -- "$FIXTURE_ROOT"

if [[ "${CONCORD_PERF_FULL:-0}" == "1" ]]; then
  SIZES=(1 10 50 100 250 500 1000)
else
  SIZES=(1 10 50 100 250)
fi

emit "fixture_root: $FIXTURE_ROOT"
emit "sizes: ${SIZES[*]}"
if [[ "${CONCORD_PERF_FULL:-0}" == "1" ]]; then
  emit "full_mode: enabled"
else
  emit "full_mode: disabled"
fi

generate_fixture() {
  local size="$1"
  local fixture_dir="$FIXTURE_ROOT/size-$size"
  local src_dir="$fixture_dir/src"
  local root_path="$ROOT_DIR"
  mkdir -p -- "$src_dir"

  cat >"$fixture_dir/Cargo.toml" <<EOF
[package]
name = "perf_macro_scale_${size}"
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

        defaults {
            retry read
            rate_limit app
        }
    }

    scope scale {
        path ["scale"]
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

  emit "generated fixture: $fixture_dir"
}

run_fixture() {
  local size="$1"
  local fixture_dir="$FIXTURE_ROOT/size-$size"
  generate_fixture "$size"
  run_timed_cmd "cargo check macro scale size ${size}" \
    "${CARGO[@]}" check --manifest-path "$fixture_dir/Cargo.toml" || {
      emit "fixture failed: $fixture_dir"
      return 1
    }
}

for size in "${SIZES[@]}"; do
  run_fixture "$size"
done
