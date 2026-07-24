default:
    @just --list

fmt:
    cargo fmt --all -- --check

check:
    cargo metadata --format-version 1 --all-features --locked
    cargo check --workspace --all-targets --locked
    cargo check --workspace --all-targets --no-default-features --locked
    cargo check --workspace --all-targets --all-features --locked
    cargo check -p concord --lib --no-default-features --features json --locked
    cargo check -p concord --lib --no-default-features --features oauth2-client --locked
    cargo check -p concord --lib --no-default-features --features multipart --locked
    cargo check -p concord --lib --no-default-features --features dangerous-raw-response --locked
    cargo check -p concord --lib --no-default-features --features dangerous-custom-retry --locked
    cargo run --locked -p concord-xtask -- policy --locked

lint:
    cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
    cargo test --workspace --lib --tests --examples --locked
    cargo test --workspace --lib --tests --examples --all-features --locked

ui:
    cargo +stable test -p concord-macros --lib --locked
    cargo +stable test -p concord --test ui --no-default-features --locked
    cargo +stable test -p concord --test ui --all-features --locked
    cargo +stable test -p concord-core --test ui --no-default-features --locked

msrv:
    cargo +1.85.0 check -p concord-core --lib --all-features --locked
    cargo +1.85.0 check -p concord-macros --lib --locked
    cargo +1.85.0 check -p concord-extras --lib --all-features --locked
    cargo +1.85.0 check -p concord --lib --all-features --locked
    cargo +1.85.0 check -p concord-core -p concord-macros -p concord-extras -p concord --lib --no-default-features --locked
    cargo +1.85.0 run --locked -p concord-xtask -- msrv-ui --locked

fixtures:
    cargo run --locked -p concord-xtask -- fixtures --locked

docs:
    cargo run --locked -p concord-xtask -- docs --locked
    cargo test --workspace --doc --all-features --locked
    cargo rustdoc -p concord --all-features --locked -- -D warnings
    cargo rustdoc -p concord-core --all-features --locked -- -D warnings
    cargo rustdoc -p concord-extras --all-features --locked -- -D warnings

package:
    cargo run --locked -p concord-xtask -- package --locked

deny:
    cargo deny check

analyzer:
    rust-analyzer analysis-stats . --run-all-ide-things
    rust-analyzer diagnostics .

bench:
    cargo bench -p concord-perf --bench concord_overhead --no-run --locked

verify: fmt check lint test ui msrv fixtures docs package deny analyzer bench

source-archive:
    git archive --format=zip --output=target/concord-source.zip HEAD
