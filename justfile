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
    cargo rustdoc -p concord_examples -- -D warnings

doc-all:
    cargo rustdoc -p concord_core --all-features -- -D warnings
    cargo rustdoc -p concord_macros --all-features -- -D warnings
    cargo rustdoc -p concord_test_support --all-features -- -D warnings
    cargo rustdoc -p concord_examples --all-features -- -D warnings

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
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-raw-response --test dangerous_features --test dangerous_feature_ui --test raw_response
    cargo check -p concord_core --no-default-features --features dangerous-custom-retry,dangerous-raw-response --all-targets
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-custom-retry,dangerous-raw-response --test dangerous_features --test dangerous_feature_ui --test ui_dangerous --test raw_response
    cargo check -p concord_core --no-default-features --features dangerous-custom-retry,dangerous-dev-tools,dangerous-raw-response --all-targets
    cargo nextest run --profile ci -p concord_core --no-default-features --features dangerous-custom-retry,dangerous-dev-tools,dangerous-raw-response --test dangerous_features --test dangerous_feature_ui --test ui_dangerous --test raw_response

[private]
_feature-matrix:
    cargo check -p concord_core --no-default-features --features json --lib
    cargo check -p concord_core --no-default-features --features multipart --lib
    cargo check -p concord_core --no-default-features --features gzip --lib
    cargo check -p concord_core --no-default-features --features brotli --lib
    cargo check -p concord_core --no-default-features --features deflate --lib
    cargo check -p concord_core --no-default-features --features dangerous-custom-retry --lib
    cargo check -p concord_core --no-default-features --features dangerous-dev-tools --lib
    cargo check -p concord_core --no-default-features --features dangerous-raw-response --lib
    cargo check -p concord_macros --all-targets
    cargo check -p concord_examples --all-targets
    cargo check -p concord_examples --all-features --all-targets
    cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-core --locked
    cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --locked

[private]
_examples:
    cargo check -p concord_examples --all-targets
    cargo check -p concord_examples --all-features --all-targets
    cargo nextest run --profile ci -p concord_examples --all-features --test canonical_examples

[private]
_benchmarks:
    cargo bench -p concord_perf --bench concord_overhead --no-run
    cargo bench -p concord_perf --bench concord_overhead -- --smoke

[private]
_msrv:
    rustc +1.85.0 --version --verbose
    cargo +1.85.0 check -p concord_core --no-default-features --lib
    cargo +1.85.0 check -p concord_core --all-features --lib
    cargo +1.85.0 check -p concord_macros --all-targets
    cargo +1.85.0 check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-core --locked
    cargo +1.85.0 check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --locked
    cargo +1.85.0 check -p concord_examples --all-features --all-targets
    cargo +1.85.0 check -p concord_examples --no-default-features --features json,dangerous-raw-response --example dangerous_raw_response

[private]
_grammar-and-diagnostics:
    cargo nextest run --profile ci -p concord_macros --lib --test grammar_registry --test ui --test compile

[private]
_reqwest-assumptions:
    cargo nextest run --profile ci -p concord_core --all-features --test reqwest_contract --test release_registries --test raw_response
    cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(production_managed_reqwest_inserts_auth_over_real_tls_loopback) | test(raw_response_preserves_authentication_recovery_and_logical_native_url_separation) | test(default_never_preserves_static_material_and_query_auth_exists_only_in_native_url) | test(reconstructible_factory_runs_once_per_concord_attempt_and_no_third_attempt_occurs) | test(hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle)'

[private]
_security-and-properties:
    cargo nextest run --profile ci -p concord_core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test pagination_api --test pagination_rate_limit --test raw_response --test release_registries
    cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(path_segmentation_property_preserves_one_owned_segment) | test(safe_url_property_auth_values_never_survive_rendering) | test(credential_versions_remain_monotonic_across_deterministic_mutations) | test(immediate_two_state_and_long_cycles_stop_before_protected_page_work) | test(complete_body_requires_explicit_access_and_owning_extraction) | test(provider_http_data_stays_outside_application_hooks_debug_and_channel)'

[private]
_rustdoc-audits:
    cargo nextest run --profile ci -p concord_core --all-features --test public_rustdoc
    cargo nextest run --profile ci -p concord_macros --test compile -E 'test(generated_public_rustdoc_uses_stable_paths_without_private_integration_names)'

[private]
_release-audits:
    & ./scripts/audit-release-naming.ps1
    & ./scripts/audit-release-artifacts.ps1
    & ./scripts/audit-release-registry.ps1 -Mode Source

release: fmt-check check check-all check-no-default clippy clippy-all _test-ci-default test-all test-no-default _test-dangerous-features _feature-matrix _examples _benchmarks _msrv _grammar-and-diagnostics _reqwest-assumptions _security-and-properties doctest doctest-all doc doc-all _rustdoc-audits deny _release-audits diff-check
