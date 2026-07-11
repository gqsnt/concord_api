# Concord's cross-platform canonical command surface.

set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command"]

export RUSTDOCFLAGS := "-D warnings"

# Show the documented command surface.
default:
    @just --list

# Check the core tools used by maintained workspace validation.
tools-core:
    cargo --version
    rustc --version
    rustfmt --version
    rustdoc --version

tools-nextest:
    cargo nextest --version

tools-supply-chain:
    cargo deny --version

# Check every tool required by the complete release gate.
tools: tools-core tools-nextest tools-supply-chain

# Format all workspace Rust source in place.
fmt:
    cargo fmt --all

# Verify formatting without modifying files.
fmt-check:
    cargo fmt --all -- --check

# Compile every workspace target with every feature.
check:
    cargo check --workspace --all-targets --all-features

# Run strict Clippy over every workspace target with every feature.
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run all executable workspace tests through Nextest.
test: tools-nextest
    cargo nextest run --workspace --all-targets --all-features --no-tests fail --no-fail-fast --retries 0

# Run Rust doctests. Nextest does not execute doctests.
doctest:
    cargo test --workspace --doc --all-features

# Build workspace rustdoc with warnings denied.
docs:
    cargo doc --workspace --no-deps --all-features

# Check advisories, bans, licenses, and sources using the repository deny policy.
supply-chain: tools-supply-chain
    cargo deny check

# Deferred perf diagnostics. These are not part of release until the historical
# perf package is updated or removed in a later PR.
perf-check:
    cargo check --manifest-path perf/Cargo.toml

perf-test: tools-nextest
    cargo nextest run --manifest-path perf/Cargo.toml --no-tests fail --no-fail-fast --retries 0

bench-check:
    cargo bench --manifest-path perf/Cargo.toml --no-run

# Focused executable-test recipes for local diagnosis.
test-core: tools-nextest
    cargo nextest run -p concord_core --no-tests fail --no-fail-fast --retries 0

test-macros: tools-nextest
    cargo nextest run -p concord_macros --no-tests fail --no-fail-fast --retries 0

test-ui: tools-nextest
    cargo nextest run -p concord_macros --test trybuild_current --no-tests fail --no-fail-fast --retries 0

check-default:
    cargo check --workspace --all-targets

clippy-default:
    cargo clippy --workspace --all-targets -- -D warnings

test-default: tools-nextest
    cargo nextest run --workspace --no-tests fail --no-fail-fast --retries 0

# Complete release validation, with one command per validation dimension.
release: tools fmt-check check clippy test doctest docs supply-chain

# CI uses the same canonical release gate.
alias ci := release
