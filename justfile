set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-targets

check-all:
    cargo check --workspace --all-targets --all-features

check-no-default:
    cargo check -p concord_core --all-targets --no-default-features

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

clippy-all:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --workspace

test-all:
    cargo nextest run --profile ci --workspace --all-features

test-no-default:
    cargo nextest run -p concord_core --no-default-features --lib -E 'test(=http::build::tests::no_default_http_works_and_https_preflight_is_typed)'

doctest:
    cargo test --workspace --doc

doctest-all:
    cargo test --workspace --doc --all-features

doc:
    # Rust 1.85 cargo-rustdoc has no workspace selector, so cover each library.
    cargo rustdoc -p concord_core -- -D warnings
    cargo rustdoc -p concord_macros -- -D warnings
    cargo rustdoc -p concord_test_support -- -D warnings

doc-all:
    cargo rustdoc -p concord_core --all-features -- -D warnings
    cargo rustdoc -p concord_macros --all-features -- -D warnings
    cargo rustdoc -p concord_test_support --all-features -- -D warnings

deny:
    cargo deny check

diff-check:
    git diff --check

[private]
_test-ci-default:
    cargo nextest run --profile ci --workspace

[private]
_test-dangerous-features:
    cargo nextest run --profile ci -p concord_core --test dangerous_features --test dangerous_feature_ui
    cargo check -p concord_core --no-default-features --features dangerous-custom-retry --all-targets
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-custom-retry --test dangerous_features --test dangerous_feature_ui --test ui_dangerous
    cargo check -p concord_core --no-default-features --features dangerous-dev-tools --all-targets
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-dev-tools --test dangerous_features --test dangerous_feature_ui
    cargo check -p concord_core --no-default-features --features dangerous-raw-response --all-targets
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-raw-response --test dangerous_features --test dangerous_feature_ui

release: fmt-check check check-all check-no-default clippy clippy-all _test-ci-default test-all test-no-default _test-dangerous-features doctest doctest-all doc doc-all deny diff-check
