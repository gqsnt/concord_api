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
    cargo +1.85.0 check --workspace --all-targets --all-features
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo nextest run --workspace --all-features
    cargo test --workspace --doc --all-features
    $env:RUSTDOCFLAGS = "-D warnings"; cargo doc --workspace --all-features --no-deps
    cargo check -p concord_core --no-default-features
    cargo test -p concord_core --no-default-features --test no_default
    cargo +1.85.0 check -p concord_core --no-default-features
    cargo +1.85.0 check -p concord_core --all-features
    cargo +1.85.0 check -p concord_macros
    cargo test -p concord_core auth_model
    cargo deny check
    git diff --check
