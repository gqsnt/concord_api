#!/usr/bin/env bash
# Experiment G1, downstream evolution check. For each SDK version in versions/, installs it as
# sdk/src/lib.rs and builds the saved consumers, which are never edited (their hashes are
# printed). Expected: `consumer` (inputs, documented style) and `consumer-setters` build against
# v1 and v2; against v3 the setter consumer still builds and the field consumer does not;
# `literal` never builds (E0639: the input is #[non_exhaustive]). Exits 1 on any other outcome.
set -uo pipefail
cd "$(dirname "$0")"
sha() { sha256sum "$@" | awk '{print substr($1,1,16)}'; }
echo "consumer sources: consumer $(sha consumer/src/main.rs)  consumer-setters $(sha consumer-setters/src/main.rs)  literal $(sha literal/src/main.rs)"
fail=0
expect() { # version package expected(pass|fail) [error code]
  local log out rc
  log=$(cargo build --offline -q -p "$2" 2>&1); rc=$?
  if [[ $rc == 0 ]]; then out=pass; else out=fail; fi
  if [[ $out == fail && -n "${4:-}" ]] && ! grep -q "error\[$4\]" <<<"$log"; then out="fail-without-$4"; fi
  printf '%-3s %-22s %-24s expected %s\n' "$1" "$2" "$out" "$3"
  [[ $out == "$3" ]] || { fail=1; echo "$log" | grep -E '^error' | head -5; }
}
for v in v1 v2 v3; do
  cp "versions/$v.rs" sdk/src/lib.rs
  expect "$v" g1-evo-consumer-setters pass
  if [[ $v == v3 ]]; then expect "$v" g1-evo-consumer fail E0308; else expect "$v" g1-evo-consumer pass; fi
  expect "$v" g1-evo-literal fail E0639
done
cp versions/v1.rs sdk/src/lib.rs
echo "consumer sources after: consumer $(sha consumer/src/main.rs)  consumer-setters $(sha consumer-setters/src/main.rs)"
exit $fail
