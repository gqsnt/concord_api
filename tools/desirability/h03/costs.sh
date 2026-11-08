#!/usr/bin/env bash
# H03 cost re-measurement: benches/scale/ab.sh (unchanged) run one row per invocation, so that the
# load average is recorded before (and after) each row, with the suite order rotated from row to
# row (arms interleaved per row, as ab.sh itself does across rows), and each row timed only when
# the 1-minute load average is below LOAD_MAX. Usage:
#   tools/desirability/h03/costs.sh OUT SUITE_BASELINE SUITE_ARM [SUITE..]
# Suites come from `benches/scale/gen.py suite OUT --concord <arm tree>/crates/concord` (the
# baseline's gen.py for every arm, so the workloads are byte-identical apart from the Concord path).
# Environment: RUNS (3), CARGO_BUILD_JOBS (6), TOOLCHAIN (1.99.1), ROW_LIST (space-separated rows;
# default: every row of ab.sh, in its order), ROW_OFFSET (0: rotation start), LOAD_MAX (8: a row
# starts only when the 1-minute load average is below it), POLL (15 s), DEADLINE (epoch seconds;
# default now + 3 h: a row that cannot start before it is recorded as not measured).
# Warm-up: one explicit phase first (ab.sh's own warm: `--workspace`, dev and release, every
# suite); each row's ab.sh invocation then warms only `-p scale_fanout` (WARM), since hyperfine's
# own warmup run (WARMUP=1) rebuilds whatever the previous row touched before timing.
# Writes OUT/<row>.<suite>.json (hyperfine), OUT/load.tsv (row, status, ISO start, load before,
# load after, waited seconds, suite order, RUNS), OUT/<row>.log, OUT/env.txt, OUT/sizes.tsv.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
ab=$here/../../../benches/scale/ab.sh
out=$(realpath -m "$1"); shift
suites=()
for s in "$@"; do suites+=("$(realpath "$s")"); done
tc=${TOOLCHAIN:-1.99.1}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-6}
load_max=${LOAD_MAX:-8}; poll=${POLL:-15}; deadline=${DEADLINE:-$(( $(date +%s) + 10800 ))}
mkdir -p "$out"
if [[ -n "${ROW_LIST:-}" ]]; then read -r -a rows <<< "$ROW_LIST"; else
  mapfile -t rows < <(sed -n 's/^  "\([a-z0-9_-]*\)|.*/\1/p' "$ab")
fi
[[ -f "$out/load.tsv" ]] || printf 'row\tstatus\tstart\tload_before\tload_after\twaited_s\torder\truns\n' > "$out/load.tsv"
load1() { cut -d' ' -f1 /proc/loadavg; }
below() { awk -v l="$(load1)" -v m="$load_max" 'BEGIN { exit !(l < m) }'; }
if [[ -z "${SKIP_WARM:-}" ]]; then
  for s in "${suites[@]}"; do
    CARGO_TARGET_DIR="$s/target" cargo "+$tc" build -q --offline --locked --manifest-path "$s/Cargo.toml" --workspace
    CARGO_TARGET_DIR="$s/target" cargo "+$tc" build -q --offline --locked --manifest-path "$s/Cargo.toml" --workspace --release
  done
fi
r=${ROW_OFFSET:-0}
for row in "${rows[@]}"; do
  n=${#suites[@]}; order=()
  for ((i = 0; i < n; i++)); do order+=("${suites[$(( (i + r) % n ))]}"); done
  ord=$(for s in "${order[@]}"; do basename "$s"; done | paste -sd,)
  t0=$(date +%s)
  while ! below && [[ $(date +%s) -lt $deadline ]]; do sleep "$poll"; done
  waited=$(( $(date +%s) - t0 ))
  if ! below; then
    printf '%s\tnot measured (1-min load never below %s before the deadline)\t%s\t%s\t-\t%s\t%s\t%s\n' "$row" "$load_max" \
      "$(date -Is)" "$(cut -d' ' -f1-3 /proc/loadavg)" "$waited" "$ord" "${RUNS:-3}" >> "$out/load.tsv"
    r=$((r + 1)); continue
  fi
  start=$(date -Is); before=$(cut -d' ' -f1-3 /proc/loadavg)
  ROWS="$row" WARM="-p scale_fanout" bash "$ab" "$out" "${order[@]}" > "$out/$row.log" 2>&1
  printf '%s\tmeasured\t%s\t%s\t%s\t%s\t%s\t%s\n' "$row" "$start" "$before" "$(cut -d' ' -f1-3 /proc/loadavg)" "$waited" \
    "$ord" "${RUNS:-3}" >> "$out/load.tsv"
  r=$((r + 1))
done
echo "done: $out"
