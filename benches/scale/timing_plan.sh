#!/usr/bin/env bash
# Compile-time matrix for a suite produced by `benches/scale/gen.py suite OUT`.
# NOT run by the compiler track (timing is reserved for the coordinator's exclusive slot).
#
# usage: timing_plan.sh SUITE_DIR [run]
#   without `run`: prints the hyperfine commands (dry run)
#   with `run`:    executes them; JSON results go to SUITE_DIR/timings/
# For an A/B comparison generate one suite per Concord snapshot (different OUT directories, so
# different target directories) and run this script on each, alternating A/B between rows.
# Environment: RUNS (default 5), WARMUP (default 1), TOOLCHAIN (default 1.99.1, the pin).
set -euo pipefail
suite=$(realpath "$1"); mode=${2:-dry}
runs=${RUNS:-5}; warmup=${WARMUP:-1}; tc=${TOOLCHAIN:-1.99.1}
out=$suite/timings; mkdir -p "$out"
export CARGO_TARGET_DIR=$suite/target CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-6}
cargo="cargo +$tc"
m="--manifest-path $suite/Cargo.toml --offline --locked -q"

row() { # name, file to touch, cargo arguments
  local name=$1 touchf=$2; shift 2
  local cmd=(hyperfine -N --warmup "$warmup" --runs "$runs" --export-json "$out/$name.json"
             --prepare "touch $suite/$touchf" "$cargo $* $m")
  if [[ $mode == run ]]; then "${cmd[@]}"; else printf '%q ' "${cmd[@]}"; echo; fi
}

if [[ $mode == run ]]; then $cargo build $m --workspace; $cargo build $m --workspace --release; fi

# Declaring crate alone: edit the API crate.
for api in scale50 scale200 scale1000 scale_deep scale_shared; do
  row "check-$api" "$api/src/lib.rs" check -p "$api"
  row "dev-$api"   "$api/src/lib.rs" build -p "$api"
  row "rel-$api"   "$api/src/lib.rs" build --release -p "$api"
done
# Edit the API crate, rebuild a consumer (API crate + consumer).
for c in use5 use100 use1000 fan_app; do
  row "dev-editapi-$c" scale1000/src/lib.rs build -p "$c"
  row "rel-editapi-$c" scale1000/src/lib.rs build --release -p "$c"
done
# Edit one consumer only.
for c in use5 use100 use1000; do
  row "dev-editbin-$c" "$c/src/main.rs" build -p "$c"
  row "rel-editbin-$c" "$c/src/main.rs" build --release -p "$c"
done
row "rel-editfan-fan_app" fan_a/src/lib.rs build --release -p fan_app

# Sizes (not timing): artifacts after the release builds above.
if [[ $mode == run ]]; then
  ls -l "$CARGO_TARGET_DIR"/release/lib*.rlib "$CARGO_TARGET_DIR"/release/{use5,use100,use1000,fan_app} \
    | tee "$out/sizes.txt"
fi
