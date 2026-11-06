#!/usr/bin/env bash
# Builds the campaign delivery artifacts for one exact candidate commit and checks them.
# Usage: make_delivery.sh REPO BASE_SHA FINAL_SHA OUT_DIR REF   (REF: a ref that points at FINAL_SHA)
# Outputs (OUT_DIR): cumulative diff and stat against the base, a source archive, a Git bundle,
# SHA-256 sums, and checks.txt (tree identity of the extracted archive, safe archive paths, bundle
# verification, relative-link check, a bounded replay of a documented command from extraction).
set -euo pipefail
repo=$1 base=$2 final=$3 out=$4 ref=$5
[[ $(git -C "$repo" rev-parse "$ref") == $(git -C "$repo" rev-parse "$final") ]] || { echo "FAIL $ref does not point at $final"; exit 1; }
mkdir -p "$out"
short=$(git -C "$repo" rev-parse --short=8 "$final")
tree=$(git -C "$repo" rev-parse "$final^{tree}")
name="concord_api-$short"
checks="$out/checks.txt"
: >"$checks"
log() { echo "$*" | tee -a "$checks"; }

log "base:  $(git -C "$repo" rev-parse "$base")"
log "final: $(git -C "$repo" rev-parse "$final")  tree: $tree"

git -C "$repo" diff --binary "$base" "$final" >"$out/$name.diff"
git -C "$repo" diff --stat "$base" "$final" | tail -1 | tee -a "$checks"
git -C "$repo" archive --format=tar.gz --prefix="$name/" -o "$out/$name.tar.gz" "$final"
git -C "$repo" bundle create "$out/$name.bundle" "$ref" "^$base"
git -C "$repo" bundle create "$out/$name-full.bundle" "$ref"

# Safe archive paths: no absolute path, no `..` component, everything under the prefix.
if tar -tzf "$out/$name.tar.gz" | grep -Eq '^/|(^|/)\.\.(/|$)'; then
  log "FAIL unsafe archive path"; exit 1
fi
if tar -tzf "$out/$name.tar.gz" | grep -vq "^$name/"; then
  log "FAIL archive entry outside $name/"; exit 1
fi
log "ok    archive paths safe ($(tar -tzf "$out/$name.tar.gz" | wc -l) entries under $name/)"

# Tree identity: the extracted archive, added to a fresh repository, has the commit's tree.
scratch="$out/extract"
rm -rf "$scratch" && mkdir -p "$scratch"
tar -xzf "$out/$name.tar.gz" -C "$scratch"
(
  cd "$scratch/$name"
  git init -q
  git add -f -A
  got=$(git write-tree)
  if [[ $got != "$tree" ]]; then echo "FAIL extracted tree $got != $tree"; exit 1; fi
  echo "ok    extracted archive tree equals the commit tree ($tree)"
) | tee -a "$checks"

# Bundles verify against the repository (the incremental one needs the base).
git -C "$repo" bundle verify "$out/$name.bundle" >/dev/null 2>&1 && log "ok    incremental bundle verifies (prerequisite $base)"
git -C "$repo" bundle verify "$out/$name-full.bundle" >/dev/null 2>&1 && log "ok    full bundle verifies"
clone="$out/bundle-clone"
rm -rf "$clone"
git clone -q "$out/$name-full.bundle" "$clone" 2>/dev/null || true
git -C "$clone" checkout -q --detach "$final"
[[ $(git -C "$clone" rev-parse HEAD^{tree}) == "$tree" ]] && log "ok    clone of the full bundle checks out the same tree"
rm -rf "$clone"

# Relative links in the campaign records and the docs resolve inside the extracted tree.
python3 -I - "$scratch/$name" <<'PY' | tee -a "$checks"
import os, re, sys
root = sys.argv[1]
docs = []
for d in ("v4/sdk20", "docs"):
    for f in sorted(os.listdir(os.path.join(root, d))):
        if f.endswith(".md"):
            docs.append(os.path.join(d, f))
docs += ["README.md", "CHANGELOG.md"]
bad = []
for doc in docs:
    text = open(os.path.join(root, doc), encoding="utf-8").read()
    for target in re.findall(r"\]\(([^)\s]+)\)", text):
        if re.match(r"[a-z]+:", target) or target.startswith("#"):
            continue
        path = target.split("#")[0]
        if not path:
            continue
        full = os.path.normpath(os.path.join(root, os.path.dirname(doc), path))
        if not os.path.exists(full):
            bad.append(f"{doc}: {target}")
if bad:
    print("FAIL broken relative links:")
    print("\n".join(bad))
    sys.exit(1)
print(f"ok    relative links resolve ({len(docs)} documents)")
PY

# Bounded replay of a documented command from the extraction (not a full build of every state).
(
  cd "$scratch/$name"
  CARGO_TARGET_DIR="$out/replay-target" CARGO_BUILD_JOBS=6 \
    cargo test --locked --offline -q -p concord-example-quickstart >"$out/replay.log" 2>&1
  echo "ok    replay from extraction: cargo test --locked --offline -p concord-example-quickstart ($(grep -o 'test result: ok. [0-9]* passed' "$out/replay.log" | awk '{s+=$4} END {print s}') tests)"
) | tee -a "$checks"

(cd "$out" && sha256sum "$name.diff" "$name.tar.gz" "$name.bundle" "$name-full.bundle" >SHA256SUMS)
cat "$out/SHA256SUMS" | tee -a "$checks"
rm -rf "$scratch" "$out/replay-target"
