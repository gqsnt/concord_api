#!/usr/bin/env bash
# Run a heavy command (cargo, just, a Codex lane) in its own systemd scope, outside the group of the app that
# started it, with a ceiling on memory. Why: on 13 November systemd-oomd killed the Claude app three times;
# every build of every lane lived in the app's group and it reached 26 GB of 30.
# usage: build/limited.sh <command> [args...]      (LIMIT_MEM=8G by default; the slice as a whole: 18G)
set -u
systemctl --user set-property --runtime concord-build.slice MemoryMax=18G MemorySwapMax=2G 2>/dev/null
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" NEXTEST_TEST_THREADS="${NEXTEST_TEST_THREADS:-6}"
# Already inside the slice (a lane started through this script), or no bus to reach (a Codex sandbox): the
# ceiling of the group we are in holds; run the command as it is, with the limits on jobs.
if grep -q concord-build.slice /proc/self/cgroup 2>/dev/null; then exec "$@"; fi
if ! systemctl --user show-environment >/dev/null 2>&1; then exec "$@"; fi
exec systemd-run --user --scope --quiet --collect --unit="concord-$$-$RANDOM" --slice=concord-build.slice \
  -p MemoryMax="${LIMIT_MEM:-8G}" -p MemorySwapMax=1G -- "$@"
