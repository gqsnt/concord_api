#!/usr/bin/env bash
# Builds and tests a downstream consumer against the *packaged* sources of concord and
# concord-macros (the extracted `.crate` files), not the repository: what a release would ship.
# Usage: scripts/package-check.sh [OUT_DIR]   (default: target/package-check; never published)
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
out=${1:-target/package-check}
rm -rf "$out"
mkdir -p "$out"
out=$(cd "$out" && pwd)

# concord-macros is not on crates.io, so concord is packaged with a crates.io patch pointing at the
# local crate (packaging resolves dependencies even with `--no-verify`), and without cargo's own
# verification build: the consumer below builds both packaged crates together instead. The
# "ignoring test" warnings list the test targets deliberately left out of the package.
cargo package --offline --locked --no-verify -p concord-macros --allow-dirty >"$out/package.log" 2>&1 \
  || { cat "$out/package.log"; exit 1; }
cargo package --offline --locked --no-verify -p concord --allow-dirty \
  --config 'patch.crates-io.concord-macros.path="crates/concord-macros"' >>"$out/package.log" 2>&1 \
  || { cat "$out/package.log"; exit 1; }
# Cargo's effective target directory (it honours CARGO_TARGET_DIR and configuration).
read -r version target_dir < <(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -I -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]=="concord"), m["target_directory"])')
for crate in concord-macros concord; do
  tar -xzf "$target_dir/package/$crate-$version.crate" -C "$out"
done
echo "packaged files:"
for crate in concord-macros concord; do
  (cd "$out/$crate-$version" && find . -type f | sort | sed "s#^\./#  $crate/#")
done
if find "$out/concord-$version" -path '*/tests/*' | grep -q .; then
  echo "FAIL: test-only material in the concord package"; exit 1
fi

mkdir -p "$out/consumer/src" "$out/consumer/tests"
cat >"$out/consumer/Cargo.toml" <<EOF
[package]
name = "package-consumer"
version = "0.0.0"
edition = "2024"
publish = false

[features]
default = ["json"]
json = ["concord/json"]

[dependencies]
concord = { path = "../concord-$version", default-features = false, features = ["reqwest-defaults"] }
serde = { version = "1", features = ["derive"] }

[patch.crates-io]
concord-macros = { path = "../concord-macros-$version" }

[workspace]
EOF
cat >"$out/consumer/src/lib.rs" <<'EOF'
//! A consumer of the packaged crates: formats (in place and defaults), a native header.
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
            request json;
            response json;
            GET item(id: u64) -> Item { path "items/{id}"; }
            POST create(body item: Item) -> Item { path "items"; }
        }
        GET health() -> text String { path "health"; }
        PUT upload(bytes("image/png") png: concord::bytes::Bytes) { path "image"; }
    }
}
EOF
cat >"$out/consumer/tests/consumer.rs" <<'EOF'
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
cp "$root/Cargo.lock" "$out/consumer/Cargo.lock" 2>/dev/null || true
cd "$out/consumer"
export CARGO_TARGET_DIR="$out/target"
# Each run must pass exactly the expected number of consumer tests (not just compile).
run() {
  local expected="$1"; shift
  local log passed
  log=$("$@" 2>&1) || { echo "$log"; echo "FAIL  $*"; exit 1; }
  passed=$(grep -o 'test result: ok. [0-9]* passed' <<<"$log" | awk '{s+=$4} END {print s+0}')
  if [[ "$passed" != "$expected" ]]; then echo "$log"; echo "FAIL  $* ($passed passed, expected $expected)"; exit 1; fi
  echo "ok    $* ($passed tests)"
}
run 2 cargo test --offline
run 1 cargo test --offline --no-default-features
run 2 cargo +1.85 test --offline
echo "PASS  package-check (consumer built against the extracted .crate files)"
