#!/usr/bin/env bash
# usage: run.sh NAME FILES -- CARGO TEST ARGS...   (baseline -> mutant -> restored)
set -u
SP=$(dirname "$0"); W=/home/canarit/projects/concord_v4_native_work/wt/formats
name=$1; shift; files=(); while [[ $1 != -- ]]; do files+=("$1"); shift; done; shift
cd "$W"
log=$SP/$name.log; : > "$log"
phase() {
  local label=$1
  if cargo "$@" >> "$log" 2>&1; then r=PASS; else r=FAIL; fi
  echo "$label: $r"
}
run() { if cargo test --offline --locked --no-fail-fast "$@" >> "$log" 2>&1; then echo PASS; else echo FAIL; fi; }
echo "== $name   cmd: cargo test --offline --locked $*   head: $(git rev-parse --short HEAD)" | tee -a "$log"
echo "baseline: $(run "$@")" | tee -a "$log"
python3 "$SP/mutate.py" "$name" || exit 1
git diff --stat | tail -1 | tee -a "$log"
echo "mutant:   $(run "$@")" | tee -a "$log"
grep -E "^test .*(FAILED|mismatch)|^error(\[|:)|should fail to compile" "$log" | sort -u | head -6
git checkout -- "${files[@]}"
echo "restored: $(run "$@")  (dirty: $(git status --porcelain --untracked-files=no | wc -l))" | tee -a "$log"
