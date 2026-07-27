# Use the native shell on Windows; recipe bodies remain portable commands.

set shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

fmt:
    cargo fmt --all -- --check

check:
    cargo metadata --format-version 1 --all-features --locked
    cargo check --workspace --all-targets --locked
    cargo check --workspace --all-targets --no-default-features --locked
    cargo check --workspace --all-targets --all-features --locked
    cargo check -p concord --lib --no-default-features --features rate-limit --locked
    cargo check -p concord --lib --no-default-features --features json --locked
    cargo check -p concord --lib --no-default-features --features oauth2-client --locked
    cargo check -p concord --lib --no-default-features --features multipart --locked
    cargo run --locked -p concord-xtask -- policy --locked

lint:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
    cargo test --workspace --exclude concord-macros --lib --tests --examples --locked
    cargo test --workspace --exclude concord-macros --lib --tests --examples --all-features --locked

ui:
    cargo test -p concord-macros --lib --locked
    cargo test -p concord --test ui --no-default-features --locked
    cargo test -p concord --test ui --all-features --locked
    cargo test -p concord-core --test ui --no-default-features --locked

msrv:
    cargo +1.85.0 check -p concord-core --lib --all-features --locked
    cargo +1.85.0 check -p concord-macros --lib --locked
    cargo +1.85.0 check -p concord-extras --lib --all-features --locked
    cargo +1.85.0 check -p concord --lib --all-features --locked
    cargo +1.85.0 check -p concord-core -p concord-macros -p concord-extras -p concord --lib --no-default-features --locked
    cargo +1.85.0 run --locked -p concord-xtask -- msrv-ui --locked

fixtures:
    cargo run --locked -p concord-xtask -- fixtures --locked

site:
    cargo run --locked -p concord-xtask -- docs --locked
    mdbook build

site-serve:
    cargo run --locked -p concord-xtask -- docs --locked
    mdbook serve

docs: site
    cargo test --workspace --doc --all-features --locked
    cargo rustdoc -p concord-core --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord-extras --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord --features rate-limit --locked -- -D warnings
    cargo rustdoc -p concord-extras --features rate-limit --locked -- -D warnings
    cargo rustdoc -p concord --features csv --locked -- -D warnings
    cargo rustdoc -p concord --features json --locked -- -D warnings
    cargo rustdoc -p concord --all-features --locked -- -D warnings
    cargo rustdoc -p concord-core --all-features --locked -- -D warnings
    cargo rustdoc -p concord-extras --all-features --locked -- -D warnings

package:
    cargo run --locked -p concord-xtask -- package --locked

deny:
    cargo deny check

workflow:
    actionlint .github/workflows/ci.yml

analyzer:
    rust-analyzer analysis-stats . --run-all-ide-things
    rust-analyzer diagnostics .

bench:
    cargo bench -p concord-perf --bench concord_overhead --no-run --locked

verify: fmt check lint test ui msrv fixtures docs package deny workflow analyzer bench

source-archive:
    cargo run --locked -p concord-xtask -- source-archive
