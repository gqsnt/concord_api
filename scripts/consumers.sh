#!/usr/bin/env bash
# Each fixture is an independent workspace. Logs and generated manifests stay under Cargo's
# effective target directory. --packaged OUT_DIR creates/extracts packages there for inspection:
# OUT_DIR is this script's own output, recreated when it holds the script's mark, created when
# absent or empty, and refused (left untouched) otherwise; the extracted sources must be the
# repository's current ones (scripts/fresh-output.sh).
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
. scripts/fresh-output.sh
python=$(command -v python3 || command -v python) || { echo "python 3 is required" >&2; exit 2; }
packaged=
if [[ $# != 0 ]]; then
  if [[ $# != 2 || $1 != --packaged || -z $2 ]]; then
    echo "usage: scripts/consumers.sh [--packaged OUT_DIR]" >&2
    exit 2
  fi
  packaged=$2
  fresh_output "$packaged" consumers-packaged || exit 1
fi
export RUSTUP_TOOLCHAIN
RUSTUP_TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
[[ -n $RUSTUP_TOOLCHAIN ]] || { echo 'FAIL missing pinned toolchain' >&2; exit 1; }
metadata=$(cargo metadata --offline --locked --format-version 1 --no-deps)
target_dir=$("$python" -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<<"$metadata")
mkdir -p "$target_dir"
scratch=$(mktemp -d "$target_dir/consumers.XXXXXXXX")
echo "consumer logs: $scratch"
# The metadata (about 50 KB) goes to Python as a file, never as an argument: Windows limits a
# whole command line to 32,767 characters.
printf '%s' "$metadata" >"$scratch/metadata.json"
crates=(concord-core concord-macros concord-extras concord)
source_dir="$root/crates"
if [[ -n $packaged ]]; then
  packaged=$(cd "$packaged" && pwd)
  patches=()
  for crate in "${crates[@]}"; do
    patches+=(--config "patch.crates-io.$crate.path=\"crates/$crate\"")
  done
  for crate in "${crates[@]}"; do
    if ! cargo package --offline --locked --no-verify -p "$crate" --allow-dirty "${patches[@]}" >>"$scratch/package.log" 2>&1; then
      cat "$scratch/package.log"
      echo "FAIL packaging $crate"
      exit 1
    fi
    version=$("$python" -I -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]==sys.argv[1]))' "$crate" <<<"$metadata")
    [[ ! -e $packaged/$crate-$version ]] || { echo "FAIL $packaged/$crate-$version exists before extraction"; exit 1; }
    tar -xzf - -C "$packaged" <"$target_dir/package/$crate-$version.crate"  # see package-check.sh
    same_sources "$packaged/$crate-$version" "crates/$crate" || { echo "FAIL stale $crate package"; exit 1; }
  done
  echo "extracted sources match the repository: $packaged"
  source_dir="$packaged"
fi
# A fresh build directory prevents Cargo from reusing same-named fixture artifacts from
# an earlier scratch workspace after its source has changed. Dependencies share this run
# directory, while separate cargo invocations still isolate each feature selection.
export CARGO_TARGET_DIR="$scratch/target"
status=0
for consumer in core-only facade-no-extras extras-core facade-full facade-json-only facade-gzip; do
  destination="$scratch/$consumer"
  cp -R "$root/checks/consumers/$consumer" "$destination"
  # Rewrite only Concord dependency paths; retain the fixture's authored feature selection.
  "$python" -I - "$destination/Cargo.toml" "$source_dir" "$scratch/metadata.json" "${packaged:+packaged}" <<'PY'
import json, pathlib, re, sys
manifest, source, metadata, packaged = sys.argv[1:]
versions = {p['name']: p['version'] for p in json.loads(pathlib.Path(metadata).read_text())['packages']}
paths = {name: str(pathlib.Path(source) / (name + '-' + versions[name] if packaged else name))
         for name in ('concord-core', 'concord-macros', 'concord-extras', 'concord')}
p = pathlib.Path(manifest)
text = re.sub(r'path = "../../../crates/([^"]+)"',
              lambda m: 'path = ' + json.dumps(paths[m[1]]), p.read_text())
text += '\n[patch.crates-io]\n'
text += ''.join(name + ' = { path = ' + json.dumps(path) + ' }\n' for name, path in paths.items())
p.write_text(text)
PY
  # A fixture with dependencies outside the workspace graph (facade-gzip: reqwest's gzip support)
  # keeps its own lockfile and fetches them; the others reuse the workspace lockfile offline.
  failed=0
  if [[ -f $destination/Cargo.lock ]]; then
    if ! (cd "$destination" && cargo fetch --locked) >"$destination/fetch.log" 2>&1; then failed=1; fi
  else
    cp "$root/Cargo.lock" "$destination/Cargo.lock"
  fi
  # Unit/integration and doc counts are checked separately: a doctest cannot mask a missing test.
  expected=3
  docs=0
  if [[ $consumer == facade-json-only ]]; then expected=2; docs=1; fi
  if [[ $consumer == facade-gzip ]]; then expected=4; fi
  if ! (cd "$destination" && cargo test --offline --lib --tests) >"$destination/test.log" 2>&1; then failed=1; fi
  if ! (cd "$destination" && cargo test --offline --doc) >"$destination/doc.log" 2>&1; then failed=1; fi
  passed=$(awk '/^test result: ok\./ {s += $4} END {print s+0}' "$destination/test.log")
  doc_passed=$(awk '/^test result: ok\./ {s += $4} END {print s+0}' "$destination/doc.log")
  if [[ $passed != "$expected" || $doc_passed != "$docs" ]]; then failed=1; fi
  if ! (cd "$destination" && cargo tree --offline -e normal --prefix none) >"$destination/tree.log" 2>&1; then failed=1; fi
  forbidden=''
  case "$consumer" in
    core-only) forbidden='concord-macros|concord-extras|concord|serde_json' ;;
    facade-no-extras) forbidden='concord-extras|serde_json|serde_urlencoded' ;;
    extras-core) forbidden='concord-macros|concord' ;;
    facade-json-only) forbidden='serde_urlencoded' ;;
  esac
  if [[ -n $forbidden ]] && grep -Eq "^($forbidden) v" "$destination/tree.log"; then failed=1; fi
  # The application, not Concord, enables decompression: the pattern must still match here.
  if [[ $consumer == facade-gzip ]] && ! (cd "$destination" && cargo tree --offline -e features -i reqwest) \
    | grep -q 'reqwest feature "gzip"'; then failed=1; fi
  # The custom-codec consumer really has no extras: no `concord feature "extras"` (nor json, form)
  # in its feature graph. The full facade is the positive control: the pattern matches there.
  features=''
  if [[ $consumer == facade-no-extras || $consumer == facade-full ]]; then
    if ! (cd "$destination" && cargo tree --offline -e features -i concord) >"$destination/features.log" 2>&1; then failed=1; fi
    if [[ $consumer == facade-no-extras ]]; then
      features='; no concord extras, json, form or concord-extras feature'
      if grep -Eq 'concord feature "(extras|json|form|concord-extras)"' "$destination/features.log"; then failed=1; fi
    elif ! grep -q 'concord feature "extras"' "$destination/features.log"; then
      failed=1
    else
      features='; concord feature "extras" present (control)'
    fi
  fi
  if [[ $failed == 0 ]]; then
    echo "ok    $consumer ($passed tests, $doc_passed doctests; normal graph exclusions passed$features)"
  else
    # Not every log exists (fetch.log only for a fixture with its own lockfile): under `set -e` a
    # failing cat would end the script here, before this FAIL line and the remaining consumers.
    cat "$destination"/fetch.log "$destination/test.log" "$destination/doc.log" "$destination/tree.log" \
      "$destination/features.log" 2>/dev/null || true
    echo "FAIL  $consumer ($passed/$expected tests, $doc_passed/$docs doctests; see $destination)"
    status=1
  fi
done
exit "$status"
