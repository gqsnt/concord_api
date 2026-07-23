# Local contributor commands. Recipes are deterministic and use only local resources.
set shell := ["powershell.exe", "-c"]


default:
    @just --list

fmt:
    cargo fmt --all

check:
    cargo check --workspace --all-targets

check-all:
    cargo check --workspace --all-targets --all-features

lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --workspace

test-all:
    cargo nextest run --profile ci --workspace --all-features

test-no-default:
    cargo nextest run -p concord-core --no-default-features --lib -E 'test(=http::build::tests::no_default_http_works_and_https_preflight_is_typed)'

docs:
    cargo test --workspace --doc
    cargo test --workspace --doc --all-features
    cargo rustdoc -p concord --all-features -- -D warnings
    cargo rustdoc -p concord-core --all-features -- -D warnings
    cargo rustdoc -p concord-extras --all-features -- -D warnings
    cargo rustdoc -p concord-macros --all-features -- -D warnings
    cargo rustdoc -p concord-test-support --all-features -- -D warnings
    cargo rustdoc -p concord-examples --all-features -- -D warnings

examples:
    cargo check -p concord-examples --all-targets
    cargo check -p concord-examples --all-features --all-targets
    cargo nextest run --profile ci -p concord-examples --all-features --test canonical_examples

features:
    cargo check -p concord-core --all-targets --no-default-features
    cargo check -p concord-core --all-targets --all-features
    cargo check -p concord-core --all-targets --no-default-features --features records
    cargo check -p concord-extras --all-targets --all-features
    cargo check -p concord --all-targets --no-default-features --features json
    cargo check -p concord --all-targets --no-default-features --features csv
    cargo check -p concord --all-targets --no-default-features --features ndjson
    cargo check -p concord --all-targets --no-default-features --features csv,ndjson
    cargo check -p concord-core --all-targets --no-default-features --features multipart
    cargo check -p concord-extras --all-targets --no-default-features --features rate-limit-governor
    cargo check -p concord --all-targets --no-default-features --features rate-limit-governor
    cargo check -p concord-core --all-targets --no-default-features --features dangerous-raw-response
    cargo check -p concord-core --all-targets --no-default-features --features dangerous-custom-retry
    cargo check -p concord-core --all-targets --no-default-features --features dangerous-dev-tools
    cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --offline --locked
    cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --offline --locked

deny:
    cargo deny check

bench:
    cargo bench -p concord-perf --bench concord_overhead --no-run
    cargo bench -p concord-perf --bench concord_overhead -- --smoke

# Comprehensive local gate. `bench` is intentionally separate because it is
# observational and slower than correctness validation.
verify: check check-all lint test test-all test-no-default docs examples features deny
