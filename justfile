# Portable contributor commands. `just` selects the native shell on each host.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-targets --locked
    cargo check --workspace --all-targets --no-default-features --locked

check-all:
    cargo check --workspace --all-targets --all-features --locked

lint:
    cargo fmt --all -- --check
    cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings

test:
    cargo test --workspace --locked

test-all:
    cargo test --workspace --all-features --locked

test-msrv:
    cargo +1.85.0 check --workspace --all-targets --no-default-features --locked
    cargo +1.85.0 test -p concord-core --lib --no-default-features --locked
    cargo +1.85.0 test -p concord-extras --all-features --locked
    cargo +1.85.0 test -p concord --test ui_msrv_semantic --no-default-features --locked
    cargo +1.85.0 test -p concord-core --test ui_msrv_semantic --no-default-features --locked

ui:
    cargo +stable test -p concord --test ui --no-default-features --locked
    cargo +stable test -p concord --test ui --all-features --locked
    cargo +stable test -p concord-core --test ui --no-default-features --locked

fixtures:
    cargo run --locked -p concord-xtask -- fixtures

examples:
    cargo check -p concord-examples --all-targets --no-default-features --locked
    cargo test -p concord-examples --all-features --locked

docs:
    cargo run --locked -p concord-xtask -- docs
    cargo test --workspace --doc --locked
    cargo test --workspace --doc --all-features --locked
    cargo rustdoc --locked -p concord --all-features -- -D warnings
    cargo rustdoc --locked -p concord-core --all-features -- -D warnings
    cargo rustdoc --locked -p concord-extras --all-features -- -D warnings

security:
    cargo test -p concord-core --test security --all-features --locked
    cargo test -p concord-core --test bounds --all-features --locked
    cargo test -p concord-core --test lifecycle --all-features --locked
    cargo test -p concord --test endpoint_planning --all-features --locked
    cargo test -p concord --test generated_authentication --all-features --locked
    cargo test -p concord --test generated_lifecycle --all-features --locked
    cargo test -p concord --test runtime_endpoint_lifecycle --all-features --locked

features:
    cargo check -p concord --all-targets --no-default-features --locked
    cargo check -p concord --all-targets --all-features --locked
    cargo check -p concord --all-targets --no-default-features --features json --locked
    cargo check -p concord --all-targets --no-default-features --features csv --locked
    cargo check -p concord --all-targets --no-default-features --features ndjson --locked
    cargo check -p concord --all-targets --no-default-features --features oauth2-client --locked
    cargo check -p concord --all-targets --no-default-features --features rate-limit-governor --locked
    cargo check -p concord --all-targets --no-default-features --features multipart --locked
    cargo check -p concord --all-targets --no-default-features --features dangerous-raw-response --locked
    cargo check -p concord --all-targets --no-default-features --features dangerous-custom-retry --locked

policy:
    cargo run --locked -p concord-xtask -- policy

package:
    cargo run --locked -p concord-xtask -- package

deny:
    cargo deny check

perf-smoke:
    cargo bench -p concord-perf --bench concord_overhead --no-run --locked
    cargo bench -p concord-perf --bench concord_overhead --locked -- --smoke

perf-scale:
    cargo run --locked -p concord-perf --bin expansion_scale

# Reproducible stable checks available on one development host.
verify-local: fmt-check check check-all lint test test-all ui fixtures examples docs security features policy

# Rust 1.85 compilation and semantic diagnostic ownership.
verify-msrv: test-msrv

# Local archive construction and extracted package-graph verification.
verify-package: package

# Complete locally executable release proof. Performance remains observational.
verify-release-local: verify-local verify-msrv verify-package deny perf-smoke
