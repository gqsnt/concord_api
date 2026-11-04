# Sourced (never run) by scripts/package-check.sh, scripts/consumers.sh and scripts/tests/output-dirs.sh.
#
# Each script that extracts packages owns one designated output directory, marked by a file that
# names it. Before writing there, `fresh_output DIR OWNER` makes DIR a new, empty directory of OWNER:
#   * DIR does not exist, or is an empty directory: created (with its parents) and marked;
#   * DIR holds OWNER's mark (OWNER's own earlier output): removed entirely, recreated and marked;
#   * anything else (a file, a symbolic link, a non-empty directory without OWNER's mark, such as a
#     stale extraction put there by hand or another script's output): refused and left untouched.
# So a run never builds from sources an earlier run (or anyone else) left behind, and never deletes
# a directory it did not create. `same_sources` then compares each extracted crate with the
# repository sources it was packaged from, so a stale `.crate` archive is detected too.

CONCORD_OUTPUT_MARK=.concord-output-owner

# fresh_output DIR OWNER
fresh_output() {
  local dir=${1:-} owner=${2:-}
  if [[ -z $dir || -z $owner ]]; then
    echo "usage: fresh_output DIR OWNER" >&2
    return 2
  fi
  if [[ -L $dir ]]; then
    echo "refusing $dir: a symbolic link, not a directory of $owner" >&2
    return 1
  fi
  if [[ -e $dir && ! -d $dir ]]; then
    echo "refusing $dir: it exists and is not a directory" >&2
    return 1
  fi
  if [[ -d $dir ]]; then
    local mark="$dir/$CONCORD_OUTPUT_MARK"
    if [[ -f $mark && ! -L $mark && "$(<"$mark")" == "$owner" ]]; then
      rm -rf "$dir" || return 1
    elif [[ -n "$(ls -A "$dir")" ]]; then
      echo "refusing $dir: not empty and not an earlier output of $owner (no $CONCORD_OUTPUT_MARK" \
        "naming it); remove it yourself or choose another directory" >&2
      return 1
    fi
  fi
  mkdir -p "$dir" && printf '%s\n' "$owner" >"$dir/$CONCORD_OUTPUT_MARK"
}

# same_sources EXTRACTED CRATE: the extracted package holds exactly the repository crate's current
# sources (every src/**/*.rs, byte for byte, and nothing else under src/) and its original manifest
# (Cargo.toml.orig). Prints the first difference.
same_sources() {
  local extracted=$1 crate=$2 listed expected f
  if ! cmp -s "$extracted/Cargo.toml.orig" "$crate/Cargo.toml"; then
    echo "stale: $extracted/Cargo.toml.orig differs from $crate/Cargo.toml"
    return 1
  fi
  listed=$(cd "$extracted" && find src -type f | LC_ALL=C sort) || return 1
  expected=$(cd "$crate" && find src -type f -name '*.rs' | LC_ALL=C sort) || return 1
  if [[ $listed != "$expected" ]]; then
    echo "stale: the files under $extracted/src differ from $crate/src:"
    diff <(printf '%s\n' "$listed") <(printf '%s\n' "$expected")
    return 1
  fi
  while IFS= read -r f; do
    if ! cmp -s "$extracted/$f" "$crate/$f"; then
      echo "stale: $extracted/$f differs from $crate/$f"
      return 1
    fi
  done <<<"$expected"
}
