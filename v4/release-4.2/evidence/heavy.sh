#!/usr/bin/env bash
# Run one heavy command (cargo build/test/bench) under one of two campaign build slots.
# Usage: heavy.sh COMMAND ARGS...      (waits for a free slot; exit status is the command's)
#        heavy.sh --exclusive COMMAND  (holds both slots: quiet performance measurements)
set -uo pipefail
dir=/home/canarit/projects/concord-v42/locks
mkdir -p "$dir"
if [[ "${1:-}" == "--exclusive" ]]; then
  shift
  exec 8>"$dir/slot1.lock" 9>"$dir/slot2.lock"
  flock 8 && flock 9 || exit 75
  echo "[heavy] exclusive lease acquired $(date -u +%FT%TZ)" >&2
  "$@"
  exit $?
fi
while :; do
  for s in 1 2; do
    exec {fd}>"$dir/slot$s.lock"
    if flock -n "$fd"; then
      echo "[heavy] slot $s acquired $(date -u +%FT%TZ)" >&2
      "$@"
      rc=$?
      flock -u "$fd"
      exit $rc
    fi
    exec {fd}>&-
  done
  sleep 3
done
