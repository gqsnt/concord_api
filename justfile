

[windows]
set shell := ["pwsh", "-NoLogo", "-NoProfileLoadTime", "-Command"]


default:
    @just --list

fmt:
    cargo fmt --all -- --check

check:
    cargo metadata --format-version 1 --all-features
    cargo check --workspace --all-targets
    cargo check --workspace --all-targets --no-default-features
    cargo check --workspace --all-targets --all-features
    cargo check -p concord --lib --no-default-features --features rate-limit
    cargo check -p concord --lib --no-default-features --features records
    cargo check -p concord --lib --no-default-features --features csv
    cargo check -p concord --lib --no-default-features --features ndjson
    cargo check -p concord --lib --no-default-features --features json
    cargo check -p concord --lib --no-default-features --features oauth2-client
    cargo check -p concord --lib --no-default-features --features multipart
    cargo run  -p concord-xtask -- policy

lint:
    cargo clippy --workspace --all-targets --all-features  -- -D warnings

test:
    cargo nextest --version
    cargo nextest run --workspace --exclude concord-macros --lib --tests --examples
    cargo nextest run --workspace --exclude concord-macros --lib --tests --examples --all-features

ui:
    cargo nextest --version
    cargo nextest run -p concord-macros --lib
    cargo nextest run -p concord --test ui --no-default-features
    cargo nextest run -p concord --test ui --all-features
    cargo nextest run -p concord-core --test ui --no-default-features

msrv:
    cargo +1.86.0 check -p concord-core --lib --all-features
    cargo +1.86.0 check -p concord-macros --lib
    cargo +1.86.0 check -p concord-extras --lib --all-features
    cargo +1.86.0 check -p concord --lib --all-features
    cargo +1.86.0 check -p concord-core -p concord-macros -p concord-extras -p concord --lib --no-default-features
    cargo +1.86.0 run  -p concord-xtask -- msrv-ui

fixtures:
    cargo run  -p concord-xtask -- fixtures

docs:
    cargo test --workspace --doc --all-features
    cargo rustdoc -p concord-core --no-default-features  -- -D warnings
    cargo rustdoc -p concord-extras --no-default-features  -- -D warnings
    cargo rustdoc -p concord --no-default-features  -- -D warnings
    cargo rustdoc -p concord-core --features records  -- -D warnings
    cargo rustdoc -p concord --features records  -- -D warnings
    cargo rustdoc -p concord --features rate-limit  -- -D warnings
    cargo rustdoc -p concord-extras --features rate-limit  -- -D warnings
    cargo rustdoc -p concord --features csv  -- -D warnings
    cargo rustdoc -p concord --features ndjson  -- -D warnings
    cargo rustdoc -p concord --features json  -- -D warnings
    cargo rustdoc -p concord --all-features  -- -D warnings
    cargo rustdoc -p concord-core --all-features  -- -D warnings
    cargo rustdoc -p concord-extras --all-features  -- -D warnings

package:
    cargo run  -p concord-xtask -- package

deny:
    cargo deny check

workflow:
    actionlint .github/workflows/ci.yml

analyzer:
    rust-analyzer analysis-stats . --run-all-ide-things
    rust-analyzer diagnostics .

bench:
    cargo bench -p concord-perf --bench concord_overhead --no-run

verify: fmt check lint test ui msrv fixtures docs package deny workflow analyzer bench

source-archive:
    cargo run  -p concord-xtask -- source-archive
