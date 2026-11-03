#!/usr/bin/env bash
# Each fixture is an independent workspace. Logs and generated manifests stay under Cargo's
# effective target directory. --packaged OUT_DIR creates/extracts packages there for inspection.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
python=$(command -v python3 || command -v python) || { echo "python 3 is required" >&2; exit 2; }
packaged=
if [[ $# != 0 ]]; then
  if [[ $# != 2 || $1 != --packaged || -z $2 ]]; then
    echo "usage: scripts/consumers.sh [--packaged OUT_DIR]" >&2
    exit 2
  fi
  packaged=$2
fi
export RUSTUP_TOOLCHAIN
RUSTUP_TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
[[ -n $RUSTUP_TOOLCHAIN ]] || { echo 'FAIL missing pinned toolchain' >&2; exit 1; }
metadata=$(cargo metadata --offline --locked --format-version 1 --no-deps)
target_dir=$("$python" -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<<"$metadata")
mkdir -p "$target_dir"
scratch=$(mktemp -d "$target_dir/consumers.XXXXXXXX")
echo "consumer logs: $scratch"
crates=(concord-core concord-macros concord-extras concord)
source_dir="$root/crates"
if [[ -n $packaged ]]; then
  mkdir -p "$packaged"
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
    tar -xzf "$target_dir/package/$crate-$version.crate" -C "$packaged"
  done
  source_dir="$packaged"
fi
# A fresh build directory prevents Cargo from reusing same-named fixture artifacts from
# an earlier scratch workspace after its source has changed. Dependencies share this run
# directory, while separate cargo invocations still isolate each feature selection.
export CARGO_TARGET_DIR="$scratch/target"
status=0
for consumer in core-only facade-no-extras extras-core facade-full facade-json-only; do
  destination="$scratch/$consumer"
  cp -R "$root/checks/consumers/$consumer" "$destination"
  # Rewrite only Concord dependency paths; retain the fixture's authored feature selection.
  "$python" -I - "$destination/Cargo.toml" "$source_dir" "$metadata" "${packaged:+packaged}" <<'PY'
import json, pathlib, re, sys
manifest, source, metadata, packaged = sys.argv[1:]
versions = {p['name']: p['version'] for p in json.loads(metadata)['packages']}
paths = {name: str(pathlib.Path(source) / (name + '-' + versions[name] if packaged else name))
         for name in ('concord-core', 'concord-macros', 'concord-extras', 'concord')}
p = pathlib.Path(manifest)
text = re.sub(r'path = "../../../crates/([^"]+)"',
              lambda m: 'path = ' + json.dumps(paths[m[1]]), p.read_text())
text += '\n[patch.crates-io]\n'
text += ''.join(name + ' = { path = ' + json.dumps(path) + ' }\n' for name, path in paths.items())
p.write_text(text)
PY
  cp "$root/Cargo.lock" "$destination/Cargo.lock"
  # Unit/integration and doc counts are checked separately: a doctest cannot mask a missing test.
  expected=3
  docs=0
  if [[ $consumer == facade-json-only ]]; then expected=2; docs=1; fi
  failed=0
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
  if [[ $failed == 0 ]]; then
    echo "ok    $consumer ($passed tests, $doc_passed doctests; normal graph exclusions passed)"
  else
    cat "$destination/test.log" "$destination/doc.log" "$destination/tree.log"
    echo "FAIL  $consumer ($passed/$expected tests, $doc_passed/$docs doctests; see $destination)"
    status=1
  fi
done
exit "$status"
