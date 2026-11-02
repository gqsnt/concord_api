#!/usr/bin/env bash
# Compiles every coherence probe on the pinned toolchain and the MSRV and records the compiler
# output under out/<toolchain>/. Positive probes must pass; `neg_*` probes must FAIL to compile.
# Usage: exp/coherence/run.sh            (offline; uses the lab's own Cargo.lock)
set -uo pipefail
cd "$(dirname "$0")"

status=0
for tc in 1.97.1 1.85; do
  out="out/$tc"
  mkdir -p "$out"
  if cargo +"$tc" test --offline --locked >"$out/positive.txt" 2>&1; then
    echo "PASS  $tc positive (cargo test)"
  else
    echo "FAIL  $tc positive (see $out/positive.txt)"; status=1
  fi
  for f in neg_display_blanket neg_display_blanket_str neg_from_blanket neg_marker; do
    if cargo +"$tc" check --offline --locked -p design_a --features "$f" --color never >"$out/$f.txt" 2>&1; then
      echo "UNEXPECTED-PASS  $tc design_a/$f"; status=1
    else
      echo "fails-as-probed  $tc design_a/$f: $(grep -m1 -E '^error' "$out/$f.txt")"
    fi
  done
  for f in neg_a_foreign_display neg_a_option neg_a_orphan neg_a_float neg_a_query neg_a_literal_infer \
           neg_b_int_ref neg_b_local_display neg_b_orphan neg_c_neither neg_c_generic \
           neg_d_missing_keyword neg_d_missing_keyword_plain; do
    if cargo +"$tc" check --offline --locked -p user --features "$f" --color never >"$out/$f.txt" 2>&1; then
      echo "UNEXPECTED-PASS  $tc user/$f"; status=1
    else
      echo "fails-as-probed  $tc user/$f: $(grep -m1 -E '^error' "$out/$f.txt")"
    fi
  done
done
exit $status
