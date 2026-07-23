# Portable contributor commands. `just` selects the native shell on each host.

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-targets
    cargo check --workspace --all-targets --no-default-features

check-all:
    cargo check --workspace --all-targets --all-features

lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace

test-all:
    cargo test --workspace --all-features

test-msrv:
    cargo +1.85.0 check --workspace --all-targets --no-default-features
    cargo +1.85.0 test -p concord-core --lib --no-default-features
    cargo +1.85.0 test -p concord-extras --all-features

ui:
    cargo test -p concord --test ui --no-default-features
    cargo test -p concord --test ui --all-features
    cargo test -p concord-core --test ui --no-default-features

fixtures:
    cargo run -p concord-xtask -- fixtures

examples:
    cargo check -p concord-examples --all-targets --no-default-features
    cargo test -p concord-examples --all-features

docs:
    cargo run -p concord-xtask -- docs
    cargo test --workspace --doc
    cargo test --workspace --doc --all-features
    cargo rustdoc -p concord --all-features -- -D warnings
    cargo rustdoc -p concord-core --all-features -- -D warnings
    cargo rustdoc -p concord-extras --all-features -- -D warnings

security:
    cargo test -p concord-core --test security --all-features
    cargo test -p concord-core --test bounds --all-features
    cargo test -p concord-core --test lifecycle --all-features
    cargo test -p concord --test endpoint_planning --all-features
    cargo test -p concord --test generated_authentication --all-features
    cargo test -p concord --test generated_lifecycle --all-features
    cargo test -p concord --test runtime_endpoint_lifecycle --all-features

features:
    cargo check -p concord --all-targets --no-default-features
    cargo check -p concord --all-targets --all-features
    cargo check -p concord --all-targets --no-default-features --features json
    cargo check -p concord --all-targets --no-default-features --features csv
    cargo check -p concord --all-targets --no-default-features --features ndjson
    cargo check -p concord --all-targets --no-default-features --features oauth2-client
    cargo check -p concord --all-targets --no-default-features --features rate-limit-governor
    cargo check -p concord --all-targets --no-default-features --features multipart
    cargo check -p concord --all-targets --no-default-features --features dangerous-raw-response
    cargo check -p concord --all-targets --no-default-features --features dangerous-custom-retry

deny:
    cargo deny check

perf-smoke:
    cargo bench -p concord-perf --bench concord_overhead --no-run
    cargo bench -p concord-perf --bench concord_overhead -- --smoke

# Correctness gate. Performance is observational and remains a separate lane.
verify: fmt-check check check-all lint test test-all test-msrv ui fixtures examples docs security features deny
