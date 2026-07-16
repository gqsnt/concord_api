set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo +1.85.0 check --workspace --all-targets

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

nextest:
    cargo nextest run --workspace

deny:
    cargo deny check

release:
    cargo fmt --all -- --check
    cargo +1.85.0 check --workspace --all-targets
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo check --workspace --no-default-features
    cargo check --workspace --all-features
    cargo nextest run --workspace
    cargo deny check
    cargo doc --workspace --no-deps
    git diff --check
