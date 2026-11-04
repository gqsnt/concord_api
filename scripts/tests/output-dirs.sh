#!/usr/bin/env bash
# Controls for scripts/fresh-output.sh and the two scripts that extract packages into a designated
# output directory (scripts/package-check.sh, scripts/consumers.sh --packaged): an output directory
# is recreated only when it holds its owner's mark, anything else is refused and left untouched, and
# an extracted crate that differs from the repository's sources is detected. Builds nothing: the
# real scripts must refuse before they package anything. Run by scripts/verify.sh (`output-dirs`).
set -uo pipefail
cd "$(dirname "$0")/../.."
. scripts/fresh-output.sh
mkdir -p target
work=$(mktemp -d "$PWD/target/output-dirs.XXXXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
failures=0
check() {
  local name=$1
  shift
  if "$@"; then
    echo "ok    $name"
  else
    echo "FAIL  $name"
    failures=$((failures + 1))
  fi
}
owned_by() { [[ -f $1/$CONCORD_OUTPUT_MARK && "$(<"$1/$CONCORD_OUTPUT_MARK")" == "$2" ]]; }

new_directory() {
  fresh_output "$work/new/deep/out" demo && owned_by "$work/new/deep/out" demo &&
    [[ "$(ls -A "$work/new/deep/out")" == "$CONCORD_OUTPUT_MARK" ]]
}
empty_directory() {
  mkdir -p "$work/empty" && fresh_output "$work/empty" demo && owned_by "$work/empty" demo
}
# An earlier output of the same owner, holding a stale extracted crate, is removed and recreated.
own_stale_output() {
  fresh_output "$work/own" demo && mkdir -p "$work/own/concord-0.0.0/src" &&
    echo stale >"$work/own/concord-0.0.0/src/stale.rs" &&
    fresh_output "$work/own" demo && [[ ! -e $work/own/concord-0.0.0 ]] && owned_by "$work/own" demo
}
# A stale extraction put there by other means (no mark): refused, every file left as it was.
unmarked_stale_extraction() {
  mkdir -p "$work/foreign/concord-0.0.0/src" && echo keep >"$work/foreign/concord-0.0.0/src/lib.rs" || return 1
  if fresh_output "$work/foreign" demo 2>"$work/foreign.err"; then return 1; fi
  grep -q refusing "$work/foreign.err" && [[ "$(<"$work/foreign/concord-0.0.0/src/lib.rs")" == keep ]] &&
    [[ ! -e $work/foreign/$CONCORD_OUTPUT_MARK ]]
}
other_owner() {
  fresh_output "$work/other" someone-else && echo x >"$work/other/file" || return 1
  if fresh_output "$work/other" demo 2>/dev/null; then return 1; fi
  [[ -f $work/other/file ]] && owned_by "$work/other" someone-else
}
a_file() {
  echo x >"$work/file" || return 1
  if fresh_output "$work/file" demo 2>/dev/null; then return 1; fi
  [[ "$(<"$work/file")" == x ]]
}
# A symbolic link to an owner's directory is refused (skipped where `ln -s` copies, e.g. MSYS).
a_link() {
  fresh_output "$work/target-of-link" demo || return 1
  if ! ln -s "$work/target-of-link" "$work/link" 2>/dev/null || [[ ! -L $work/link ]]; then
    echo "      (no symbolic links here: skipped)"
    return 0
  fi
  if fresh_output "$work/link" demo 2>/dev/null; then return 1; fi
  owned_by "$work/target-of-link" demo
}

check "fresh_output creates a new directory and marks it" new_directory
check "fresh_output uses an empty directory" empty_directory
check "fresh_output recreates its own earlier output (stale extraction removed)" own_stale_output
check "fresh_output refuses an unmarked stale extraction and leaves it untouched" unmarked_stale_extraction
check "fresh_output refuses another owner's output" other_owner
check "fresh_output refuses a file" a_file
check "fresh_output refuses a symbolic link" a_link

# same_sources against a faithful copy of a crate's packaged sources, then four stale variants.
copy() {
  rm -rf "$work/pkg" && mkdir -p "$work/pkg" && cp -R crates/concord-core/src "$work/pkg/src" &&
    cp crates/concord-core/Cargo.toml "$work/pkg/Cargo.toml.orig"
}
faithful() { copy && same_sources "$work/pkg" crates/concord-core >/dev/null; }
changed_byte() { copy && printf ' ' >>"$work/pkg/src/lib.rs" && ! same_sources "$work/pkg" crates/concord-core >/dev/null; }
extra_file() { copy && echo '//' >"$work/pkg/src/stale.rs" && ! same_sources "$work/pkg" crates/concord-core >/dev/null; }
missing_file() { copy && rm "$work/pkg/src/lib.rs" && ! same_sources "$work/pkg" crates/concord-core >/dev/null; }
changed_manifest() { copy && echo '#' >>"$work/pkg/Cargo.toml.orig" && ! same_sources "$work/pkg" crates/concord-core >/dev/null; }
check "same_sources accepts the current sources" faithful
check "same_sources detects a changed byte" changed_byte
check "same_sources detects an extra file" extra_file
check "same_sources detects a missing file" missing_file
check "same_sources detects a changed manifest" changed_manifest

# The real scripts refuse an unmarked, non-empty output directory before packaging anything.
refuses() {
  local dir=$1 out rc
  shift
  mkdir -p "$dir/concord-4.1.0/src" && echo keep >"$dir/concord-4.1.0/src/lib.rs" || return 1
  out=$("$@" 2>&1)
  rc=$?
  [[ $rc != 0 && $out == *refusing* && "$(<"$dir/concord-4.1.0/src/lib.rs")" == keep ]] &&
    [[ "$(ls -A "$dir")" == concord-4.1.0 ]] || { echo "$out"; return 1; }
}
check "package-check.sh refuses an unmarked stale directory" refuses "$work/pc" scripts/package-check.sh "$work/pc"
check "consumers.sh --packaged refuses an unmarked stale directory" \
  refuses "$work/cp" scripts/consumers.sh --packaged "$work/cp"

echo "$failures failure(s)"
((failures == 0))
