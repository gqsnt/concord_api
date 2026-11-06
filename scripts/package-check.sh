#!/usr/bin/env bash
# Builds and tests a downstream consumer against the *packaged* sources of the production crates (the
# extracted `.crate` files), not the repository: what a release would ship. Each package must hold
# only sources, manifests, the README and the licences, with nothing that looks like a credential,
# and the extracted sources must be the repository's current ones (never a stale extraction).
# Usage: scripts/package-check.sh [OUT_DIR]   (default: target/packaged/package-check; never published)
# OUT_DIR is this script's own output: recreated when it holds the script's mark, created when
# absent or empty, and refused (left untouched) otherwise (scripts/fresh-output.sh).
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
. scripts/fresh-output.sh
out=${1:-target/packaged/package-check}
fresh_output "$out" package-check || exit 1
out=$(cd "$out" && pwd)
# The packages are extracted, and the consumer built, under a deep directory whose path has
# non-ASCII components, so the native Windows CI job builds from such a path too (the build output
# stays in $out/target).
component="ünïcødé-路径"
src="$out/extracted/$component/nested-directory-one/nested-directory-two/nested-directory-three"
mkdir -p "$src"

# The production crates are not on crates.io, so each is packaged with crates.io patches pointing at the
# local crates it depends on (packaging resolves dependencies even with `--no-verify`), and without
# cargo's own verification build: the consumers below build the packaged crates together instead.
# The "ignoring test" warnings list the test targets deliberately left out of the packages.
crates="concord-core concord-codecs concord-extras concord-macros concord concord-pagination concord-retry"
patches=()
for crate in $crates; do
  patches+=(--config "patch.crates-io.$crate.path=\"crates/$crate\"")
done
for crate in $crates; do
  cargo package --offline --locked --no-verify -p "$crate" --allow-dirty "${patches[@]}" \
    >>"$out/package.log" 2>&1 || { cat "$out/package.log"; exit 1; }
done
# Cargo's effective target directory (it honours CARGO_TARGET_DIR and configuration).
read -r version target_dir < <(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -I -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]=="concord"), m["target_directory"])')
target_dir=${target_dir%$'\r'}  # Python on Windows ends its line with CR LF
# The archive is read from standard input: on Windows the target directory is `D:\...`, and GNU
# tar takes a name with a colon before its first slash for a remote `host:path`.
for crate in $crates; do
  [[ ! -e $src/$crate-$version ]] || { echo "FAIL: $src/$crate-$version exists before extraction"; exit 1; }
  tar -xzf - -C "$src" <"$target_dir/package/$crate-$version.crate"
  same_sources "$src/$crate-$version" "crates/$crate" || { echo "FAIL: stale $crate package"; exit 1; }
done
echo "extracted sources match the repository: $src"
echo "packaged files:"
for crate in $crates; do
  (cd "$src/$crate-$version" && find . -type f | sort | sed "s#^\./#  $crate/#")
done
for crate in $crates; do
  if find "$src/$crate-$version" -path '*/tests/*' | grep -q .; then
    echo "FAIL: test-only material in the $crate package"; exit 1
  fi
  # No build output, logs, fixtures or evidence: only these kinds of files are shipped.
  unexpected=$(cd "$src/$crate-$version" && find . -type f | sed 's#^\./##' | grep -vE \
    '^(src/([a-z_]+/)*[a-z_]+\.rs|Cargo\.toml|Cargo\.toml\.orig|Cargo\.lock|README\.md|LICENSE-(MIT|APACHE)|\.cargo_vcs_info\.json)$' || true)
  if [[ -n $unexpected ]]; then
    echo "FAIL: unexpected files in the $crate package:"; echo "$unexpected"; exit 1
  fi
  # Nothing credential-like: private keys, Riot API keys, GitHub or AWS tokens.
  if grep -rEn 'BEGIN [A-Z ]*PRIVATE KEY|RGAPI-[0-9A-Fa-f-]{8,}|gh[pousr]_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}' \
    "$src/$crate-$version"; then
    echo "FAIL: credential-like material in the $crate package"; exit 1
  fi
done

mkdir -p "$src/consumer/src" "$src/consumer/tests"
cat >"$src/consumer/Cargo.toml" <<EOF
[package]
name = "package-consumer"
version = "0.0.0"
edition = "2024"
publish = false

[features]
default = ["json"]
json = ["concord/json"]

[dependencies]
concord = { path = "../concord-$version", default-features = false, features = ["reqwest-defaults", "extras"] }
serde = { version = "1", features = ["derive"] }

[patch.crates-io]
concord-core = { path = "../concord-core-$version" }
concord-codecs = { path = "../concord-codecs-$version" }
concord-extras = { path = "../concord-extras-$version" }
concord-macros = { path = "../concord-macros-$version" }

[workspace]
EOF
cat >"$src/consumer/src/lib.rs" <<'EOF'
//! A consumer of the packaged crates: format statements (defaults and endpoint), a native header.
use concord::reqwest::header::HeaderValue;

pub static ACCEPT: HeaderValue = HeaderValue::from_static("application/json");

#[cfg(feature = "json")]
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Item {
    pub id: u64,
}

concord::api! {
    pub client Shop(#[sensitive] header("authorization") token: String, header("accept") accept: HeaderValue = &ACCEPT) {
        base "https://shop.example.test/v1";
        #[cfg(feature = "json")]
        scope json_api {
            request concord::Json;
            response concord::Json;
            GET item(id: u64) -> Item { path "items/{id}"; }
            POST create(body item: Item) -> Item { path "items"; }
        }
        GET health() -> String { response concord::Text; path "health"; }
        PUT upload(body png: concord::bytes::Bytes) { request concord::Binary "image/png"; path "image"; }
    }
}
EOF
cat >"$src/consumer/tests/consumer.rs" <<'EOF'
use package_consumer::*;

fn shop() -> Shop {
    Shop::new(concord::reqwest::Client::new(), ShopConfig { token: "Bearer t".into() })
}

#[test]
fn builds_requests_from_the_packaged_sources() {
    let health = shop().health().prepare().unwrap();
    assert_eq!(health.request().url().as_str(), "https://shop.example.test/v1/health");
    assert_eq!(health.request().headers()["accept"], "application/json");
    assert!(health.request().headers()["authorization"].is_sensitive());
    assert!(!format!("{health:?}").contains("Bearer t"));
    let upload = shop().upload(concord::bytes::Bytes::from_static(b"png")).prepare().unwrap();
    assert_eq!(upload.request().headers()["content-type"], "image/png");
}

#[cfg(feature = "json")]
#[test]
fn json_defaults_from_the_packaged_sources() {
    let create = shop().json_api().create(Item { id: 7 }).prepare().unwrap();
    assert_eq!(create.request().headers()["content-type"], "application/json");
    assert_eq!(create.request().body().and_then(|b| b.as_bytes()), Some(&br#"{"id":7}"#[..]));
}
EOF
cp "$root/Cargo.lock" "$src/consumer/Cargo.lock" 2>/dev/null || true
cd "$src/consumer"
# Outside the repository rust-toolchain.toml does not apply: build with the pinned toolchain (also
# the minimum supported Rust) rather than whatever the machine's default is.
RUSTUP_TOOLCHAIN=$(sed -n 's/^channel = "\(.*\)"/\1/p' "$root/rust-toolchain.toml")
export RUSTUP_TOOLCHAIN
export CARGO_TARGET_DIR="$out/target"
# Each run must pass exactly the expected number of consumer tests (not just compile).
run() {
  local expected="$1"; shift
  local passed
  log=$("$@" 2>&1) || { echo "$log"; echo "FAIL  $*"; exit 1; }
  passed=$(grep -o 'test result: ok. [0-9]* passed' <<<"$log" | awk '{s+=$4} END {print s+0}')
  if [[ "$passed" != "$expected" ]]; then echo "$log"; echo "FAIL  $* ($passed passed, expected $expected)"; exit 1; fi
  echo "ok    $* ($passed tests)"
}
run 2 cargo test --offline
# Cargo compiled the facade from the extraction directory (`Compiling concord vX (PATH)`).
if ! grep -F " concord v$version (" <<<"$log" | grep -Fq "$component"; then
  echo "$log"; echo "FAIL  concord was not compiled from $src"; exit 1
fi
echo "ok    concord compiled from $src"
run 1 cargo test --offline --no-default-features
echo "PASS  package-check (consumer built against the extracted .crate files)"
