#!/usr/bin/env bash
# Paired compile-time comparison of several generated suites (one per Concord source variant).
#
# usage: benches/scale/ab.sh OUT SUITE... (suites made by `gen.py suite`, each with its own
#        --concord snapshot and --syntax). Every row runs on every suite, rotating the suite order
#        from row to row; each suite builds in its own target directory. Results: OUT/<row>.<suite>.json
#        (hyperfine), OUT/sizes.tsv, OUT/env.txt. Summarize with benches/scale/ab_summary.py OUT.
# Run alone on the machine (no concurrent builds). Environment: RUNS (3), WARMUP (1),
# TOOLCHAIN (1.97.1), CARGO_BUILD_JOBS (6), ROWS (space-separated subset of row names),
# WARM (cargo package selection warmed before timing; default --workspace).
set -euo pipefail
out=$(realpath -m "$1"); shift
suites=(); for s in "$@"; do suites+=("$(realpath "$s")"); done
runs=${RUNS:-3}; warmup=${WARMUP:-1}; tc=${TOOLCHAIN:-1.97.1}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-6}
mkdir -p "$out"
{
  date -Is; uname -a; nproc; "cargo" "+$tc" --version; "rustc" "+$tc" --version --verbose
  echo "RUNS=$runs WARMUP=$warmup CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"
  for s in "${suites[@]}"; do echo "suite $s"; jq -c '{syntax, generator_version, cargo_lock_sha256}' "$s/suite.json" 2>/dev/null || true; done
} > "$out/env.txt"

# name | file touched before each run (relative to the suite) | cargo arguments
ROWS_ALL=(
  "check-scale1000|scale1000/src/lib.rs|check -p scale1000"
  "dev-scale1000|scale1000/src/lib.rs|build -p scale1000"
  "rel-scale1000|scale1000/src/lib.rs|build --release -p scale1000"
  "dev-editapi-use100|scale1000/src/lib.rs|build -p use100"
  "rel-editapi-use5|scale1000/src/lib.rs|build --release -p use5"
  "rel-editapi-use100|scale1000/src/lib.rs|build --release -p use100"
  "rel-editapi-use1000|scale1000/src/lib.rs|build --release -p use1000"
  "rel-editapi-fan_app|scale1000/src/lib.rs|build --release -p fan_app"
  "rel-editconsumer-use5|use5/src/main.rs|build --release -p use5"
  "rel-editconsumer-use100|use100/src/main.rs|build --release -p use100"
  "rel-editconsumer-use1000|use1000/src/main.rs|build --release -p use1000"
  "rel-editconsumer-fan_app|fan_a/src/lib.rs|build --release -p fan_app"
  "check-scale_deep|scale_deep/src/lib.rs|check -p scale_deep"
  "rel-scale_deep|scale_deep/src/lib.rs|build --release -p scale_deep"
  "check-scale_shared|scale_shared/src/lib.rs|check -p scale_shared"
  "rel-scale_shared|scale_shared/src/lib.rs|build --release -p scale_shared"
  "check-scale_typed|scale_typed/src/lib.rs|check -p scale_typed"
  "rel-scale_typed|scale_typed/src/lib.rs|build --release -p scale_typed"
  "check-scale_fanout|scale_fanout/src/lib.rs|check -p scale_fanout"
)

# Warm every suite (dependencies and all members, dev and release) outside timing.
warm=(${WARM:---workspace})
for s in "${suites[@]}"; do
  CARGO_TARGET_DIR="$s/target" cargo "+$tc" build -q --offline --locked --manifest-path "$s/Cargo.toml" "${warm[@]}"
  CARGO_TARGET_DIR="$s/target" cargo "+$tc" build -q --offline --locked --manifest-path "$s/Cargo.toml" "${warm[@]}" --release
done

r=0
for row in "${ROWS_ALL[@]}"; do
  IFS='|' read -r name touchf args <<< "$row"
  if [[ -n "${ROWS:-}" && " $ROWS " != *" $name "* ]]; then continue; fi
  n=${#suites[@]}
  for ((i = 0; i < n; i++)); do
    s=${suites[$(( (i + r) % n ))]}
    tag=$(basename "$s")
    CARGO_TARGET_DIR="$s/target" hyperfine -N --style basic --warmup "$warmup" --runs "$runs" \
      --export-json "$out/$name.$tag.json" --prepare "touch $s/$touchf" \
      "cargo +$tc $args --manifest-path $s/Cargo.toml --offline --locked -q"
  done
  r=$((r + 1))
done

# Release artifact sizes and out-of-line copies of generated `prepare` functions.
printf 'suite\tartifact\tbytes\tprepare_symbols\n' > "$out/sizes.tsv"
for s in "${suites[@]}"; do
  for b in use5 use100 use1000 fan_app; do
    f="$s/target/release/$b"
    [[ -f "$f" ]] || continue
    syms=$(nm -C "$f" 2>/dev/null | grep -c '::prepare$' || true)
    printf '%s\t%s\t%s\t%s\n' "$(basename "$s")" "$b" "$(stat -c %s "$f")" "$syms" >> "$out/sizes.tsv"
  done
  for l in scale1000 scale_deep scale_shared scale_typed; do
    f=$(ls "$s"/target/release/lib"$l".rlib 2>/dev/null | head -1 || true)
    [[ -n "$f" ]] && printf '%s\t%s\t%s\t-\n' "$(basename "$s")" "lib$l.rlib" "$(stat -c %s "$f")" >> "$out/sizes.tsv"
  done
done
echo "done: $out"
