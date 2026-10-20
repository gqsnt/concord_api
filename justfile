# Windows-specific shell selection supported by the pinned just 1.46.0.
set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

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
    cargo check -p concord --lib --no-default-features --features records --locked
    cargo check -p concord --lib --no-default-features --features csv --locked
    cargo check -p concord --lib --no-default-features --features ndjson --locked
    cargo check -p concord --lib --no-default-features --features sse --locked
    cargo check -p concord --lib --no-default-features --features json --locked
    cargo check -p concord --lib --no-default-features --features oauth2-client --locked
    cargo check -p concord --lib --no-default-features --features multipart --locked
    cargo run --locked -p concord-xtask -- policy --locked

lint:
    cargo clippy --workspace --all-targets --all-features  -- -D warnings

test:
    cargo nextest --version
    cargo nextest run --workspace --exclude concord-macros --lib --tests --examples
    cargo nextest run --workspace --exclude concord-macros --lib --tests --examples --all-features

ui:
    cargo nextest --version
    cargo nextest run -p concord-macros --lib
    cargo nextest run -p concord --test ui --no-default-features --test-threads 1
    cargo nextest run -p concord --test ui --all-features --test-threads 1
    cargo nextest run -p concord-core --test ui --no-default-features

msrv:
    cargo +1.86.0 check -p concord-core --lib --all-features --locked
    cargo +1.86.0 check -p concord-macros --lib --locked
    cargo +1.86.0 check -p concord-extras --lib --all-features --locked
    cargo +1.86.0 check -p concord --lib --all-features --locked
    cargo +1.86.0 check -p concord-core -p concord-macros -p concord-extras -p concord --lib --no-default-features --locked
    cargo +1.86.0 run  -p concord-xtask --locked -- msrv-ui

fixtures:
    cargo run  -p concord-xtask --locked -- fixtures

docs:
    cargo run --locked -p concord-xtask -- docs-check --locked
    cargo run --locked -p concord-xtask -- doc-audit --locked
    cargo test --workspace --doc --all-features --locked
    cargo rustdoc -p concord-core --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord-extras --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord --no-default-features --locked -- -D warnings
    cargo rustdoc -p concord-core --features records --locked -- -D warnings
    cargo rustdoc -p concord --features records --locked -- -D warnings
    cargo rustdoc -p concord --features rate-limit --locked -- -D warnings
    cargo rustdoc -p concord-extras --features rate-limit --locked -- -D warnings
    cargo rustdoc -p concord --features csv --locked -- -D warnings
    cargo rustdoc -p concord --features ndjson --locked -- -D warnings
    cargo rustdoc -p concord --features sse --locked -- -D warnings
    cargo rustdoc -p concord --features json --locked -- -D warnings
    cargo rustdoc -p concord --all-features --locked -- -D warnings
    cargo rustdoc -p concord-core --all-features --locked -- -D warnings
    cargo rustdoc -p concord-extras --all-features --locked -- -D warnings

docs-check:
    cargo run --locked -p concord-xtask -- docs-check --locked

release-audit:
    cargo run --locked -p concord-xtask -- release-audit --locked

source-export:
    cargo run --locked -p concord-xtask -- source-export --locked

package:
    pwsh -NoLogo -NoProfile -File scripts/verify_packaged_downstream.ps1

deny:
    cargo deny check

workflow:
    actionlint .github/workflows/ci.yml

analyzer:
    rust-analyzer analysis-stats . --run-all-ide-things
    rust-analyzer diagnostics .

bench:
    cargo bench -p concord-perf --bench concord_overhead --no-run --locked

verify: fmt check lint test ui msrv fixtures docs package deny workflow analyzer bench source-export release-audit

# Complete machine validation owned by the unpublished 3.0.0-beta.1
# implementation (spec §28.2): format, check/clippy/test across default,
# no-default and all features, facade/core UI suites (named explicitly: their
# targets set `test = false`), MSRV library checks, feature fixtures, docs, the
# two-crate downstream workspace and extracted archive consumers, xtask tools
# (V3 contract inspection), benchmark
# compilation and the clean source export. It uses plain `cargo test` (the
# documented nextest equivalent). External license/registry/tag/human-release
# gates (`deny`, `workflow`, `release-audit`) stay in the separate
# release report and are deliberately not part of this recipe.
verify-implementation: fmt check lint-matrix test-matrix ui-matrix msrv fixtures docs downstream package tools bench source-export

lint-matrix:
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

test-matrix:
    cargo test --workspace --locked
    cargo test --workspace --no-default-features --locked
    cargo test --workspace --all-features --locked
    cargo test --workspace --doc --all-features --locked
    cargo test --locked -p concord --no-default-features
    cargo test --locked -p concord-core --no-default-features
    cargo test --locked -p concord-extras --no-default-features

# The `ui` test targets set `test = false`; they run only when named.
ui-matrix:
    cargo test --locked -p concord-macros --lib
    cargo test --locked -p concord --test ui --all-features
    cargo test --locked -p concord --test ui --no-default-features
    cargo test --locked -p concord-core --test ui --no-default-features

downstream:
    cargo test --locked -p concord-downstream-api -p concord-downstream-app

tools:
    cargo test --locked -p concord-xtask
    cargo test --locked -p concord-xtask --features examples
    cargo test --locked -p concord-inspect --all-features
    cargo run --locked -p concord-xtask -- policy
