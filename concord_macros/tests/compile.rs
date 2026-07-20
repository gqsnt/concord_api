use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics, run};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("macro crate should be inside the workspace")
        .to_owned()
}

fn check_fixture(name: &str) -> Output {
    let manifest = fixture_root().join(name).join("Cargo.toml");
    check_manifest(&workspace_root(), &format!("macros/{name}"), &manifest, &[])
}

fn assert_fixture_passes(name: &str) {
    let output = check_fixture(name);
    assert!(
        output.status.success(),
        "success fixture `{name}` failed\n{}",
        diagnostics(&output)
    );
}

fn assert_fixture_fails_with(name: &str, expected: &str) {
    let output = check_fixture(name);
    let diagnostic = diagnostics(&output);
    assert!(
        !output.status.success(),
        "compile-fail fixture `{name}` unexpectedly succeeded\n{diagnostic}"
    );
    assert!(
        diagnostic.contains(expected),
        "compile-fail fixture `{name}` omitted intentional diagnostic `{expected}`\n{diagnostic}"
    );
}

fn check_extension_fixture(arguments: &[&str], key: &str) -> Output {
    let manifest = fixture_root().join("extension_certification/Cargo.toml");
    check_manifest(
        &workspace_root(),
        &format!("macros/extension-certification-{key}"),
        &manifest,
        arguments,
    )
}

#[test]
fn renamed_core_dependency_resolves_through_package_metadata() {
    assert_fixture_passes("renamed_core");
}

#[test]
fn complete_extension_fixture_compiles_through_public_namespaces_for_every_owned_feature_set() {
    for (key, arguments) in [
        ("default", vec!["--lib"]),
        ("no-default", vec!["--lib", "--no-default-features"]),
        (
            "json",
            vec!["--lib", "--no-default-features", "--features", "json"],
        ),
        (
            "multipart",
            vec!["--lib", "--no-default-features", "--features", "multipart"],
        ),
        (
            "all-extension-features",
            vec![
                "--lib",
                "--no-default-features",
                "--features",
                "json,multipart",
            ],
        ),
    ] {
        let output = check_extension_fixture(&arguments, key);
        assert!(
            output.status.success(),
            "extension fixture `{key}` failed\n{}",
            diagnostics(&output)
        );
    }
}

#[test]
fn complete_extension_fixture_has_no_private_native_or_integration_escape() {
    let path = fixture_root().join("extension_certification/src/lib.rs");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    for forbidden in [
        "__private",
        "__development",
        "reqwest",
        "ClientCore",
        "EndpointSpec",
        "ResponseHandler",
        "PreparedHead",
        "ExecutionPlan",
        "PaginatedEndpoint",
        "downcast",
        "unsafe {",
    ] {
        assert!(
            !source.contains(forbidden),
            "extension fixture exposed prohibited surface `{forbidden}`"
        );
    }
    for line in source
        .lines()
        .filter(|line| line.contains("concord_core::"))
    {
        assert!(
            line.contains("concord_core::advanced") || line.contains("concord_core::prelude"),
            "extension fixture imported an unapproved core namespace: {line}"
        );
    }
}

#[test]
fn missing_core_dependency_has_a_focused_macro_diagnostic() {
    assert_fixture_fails_with("missing_core", "requires the `concord_core` package");
}

#[test]
fn incompatible_core_contract_fails_during_compilation() {
    assert_fixture_fails_with("contract_mismatch", "concord macro/core contract mismatch");
}

#[test]
fn json_and_oauth_dsl_without_runtime_feature_have_a_focused_diagnostic() {
    assert_fixture_fails_with(
        "json_disabled",
        "Concord JSON DSL requires the `concord_core/json` feature",
    );
    assert_fixture_fails_with(
        "oauth_disabled",
        "Concord JSON DSL requires the `concord_core/json` feature",
    );
}

#[test]
fn multipart_dsl_without_runtime_feature_has_a_focused_diagnostic() {
    assert_fixture_fails_with(
        "multipart_disabled",
        "Concord multipart DSL requires the `concord_core/multipart` feature",
    );
}

#[test]
fn custom_retry_dsl_without_runtime_feature_has_a_focused_diagnostic() {
    assert_fixture_fails_with(
        "custom_retry_disabled",
        "Concord `retry custom PATH` requires the `concord_core/dangerous-custom-retry` feature",
    );
}

#[test]
fn custom_retry_dsl_with_runtime_feature_and_http_override_compiles() {
    assert_fixture_passes("custom_retry_enabled");
}

#[test]
fn generated_public_rustdoc_uses_stable_paths_without_private_integration_names() {
    let root = workspace_root();
    let manifest = fixture_root().join("renamed_core/Cargo.toml");
    let target_key = "macros/renamed-core-rustdoc";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        output.status.success(),
        "renamed generated Rustdoc failed\n{}",
        diagnostics(&output)
    );

    let documentation = root
        .join("target/cargo-fixtures")
        .join(target_key)
        .join("doc/concord_fixture_renamed_core");
    for relative in [
        "struct.RenamedApi.html",
        "struct.RenamedApiBuilder.html",
        "struct.RenamedRateApi.html",
        "struct.RenamedRateApiBuilder.html",
        "renamed_api/facades/resources/struct.ResourcesFacade.html",
        "renamed_api/credentials/struct.RenamedApiCredentials.html",
        "renamed_api/credentials/struct.ApiKeyCredentialHandle.html",
        "renamed_api/endpoints/resources/struct.ById.html",
        "renamed_api/endpoints/resources/struct.Custom.html",
        "renamed_api/endpoints/resources/struct.StreamUpload.html",
        "renamed_api/endpoints/resources/struct.StreamDownload.html",
        "renamed_api/endpoints/resources/struct.MultipartUpload.html",
        "renamed_api/endpoints/resources/struct.Pages.html",
        "renamed_rate_api/endpoints/struct.Read.html",
    ] {
        let path = documentation.join(relative);
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for forbidden in [
            "__private",
            "__concord_endpoint_seal",
            "ClientCore",
            "EndpointSpec",
            "RawResponseEndpoint",
            "MetadataResponse",
            "ResponseHandler",
            "CodecBody",
            "BufferedResponse",
            "PreparedBody",
            "BodySpec",
            "StreamRequestBody",
            "StreamResponseHandler",
            "PreparedResponseMarker",
            "PreparedRequestBody",
            "ManagedRequestStream",
            "Replayability",
            "reqwest",
            "Sealed",
            "renamed-rate-key-sentinel",
        ] {
            assert!(
                !page.contains(forbidden),
                "public generated Rustdoc {} exposed {forbidden}",
                path.display()
            );
        }
        if relative.contains("facades") {
            assert!(
                page.contains("Request"),
                "generated facade Rustdoc omitted the stable Request wrapper"
            );
        }
        if relative.contains("endpoints") {
            let expected: &[&str] = match relative {
                path if path.ends_with("struct.ById.html") => &[
                    "RenamedApi::resources::ById",
                    "POST",
                    "Text",
                    "scope_value",
                    "endpoint_value",
                    "body",
                    "custom header",
                    "Credential values are absent",
                ],
                path if path.ends_with("struct.Custom.html") => {
                    &["RenamedApi::resources::Custom", "scope_value", "body"]
                }
                path if path.ends_with("struct.StreamUpload.html") => &[
                    "RenamedApi::resources::StreamUpload",
                    "POST",
                    "Stream&lt;OctetStream&gt;",
                    "one-shot",
                    "StreamBody",
                    "scope_value",
                    "body",
                ],
                path if path.ends_with("struct.StreamDownload.html") => &[
                    "RenamedApi::resources::StreamDownload",
                    "GET",
                    "StreamResponse",
                    "finite stream-response limit",
                    "intentionally unavailable",
                    "scope_value",
                ],
                path if path.ends_with("struct.MultipartUpload.html") => &[
                    "RenamedApi::resources::MultipartUpload",
                    "POST",
                    "MultipartBody",
                    "concord_core/multipart",
                    "aggregate framing limits",
                    "scope_value",
                    "body",
                ],
                path if path.ends_with("struct.Pages.html") => {
                    &["RenamedApi::resources::Pages", "GET", "PageCodec", "cursor"]
                }
                path if path.ends_with("renamed_rate_api/endpoints/struct.Read.html") => &[
                    "RenamedRateApi::Read",
                    "GET",
                    "tenant_quota",
                    "Resolved key values are absent",
                ],
                _ => panic!("unexpected generated endpoint Rustdoc path {relative}"),
            };
            for expected in expected.iter().copied() {
                assert!(
                    page.contains(expected),
                    "generated endpoint Rustdoc omitted public identity or parameter {expected}"
                );
            }
            let raw_expectations: &[&str] = if relative.ends_with("struct.StreamDownload.html") {
                &["raw_response", "intentionally unavailable"]
            } else {
                &[
                    "concord_core/dangerous-raw-response",
                    "raw_response",
                    "complete bounded successful bytes",
                    "reduced secrecy guarantees",
                ]
            };
            for expected in raw_expectations {
                assert!(
                    page.contains(expected),
                    "generated endpoint Rustdoc omitted raw-terminal fact {expected}"
                );
            }
            for forbidden in ["__concord_scope_0", "__concord_endpoint_0"] {
                assert!(
                    !page.contains(forbidden),
                    "generated endpoint Rustdoc exposed internal constructor parameter {forbidden}"
                );
            }
        }
    }

    let client_page = std::fs::read_to_string(documentation.join("struct.RenamedApi.html"))
        .expect("generated client Rustdoc should be readable");
    assert!(
        client_page.contains("RenamedApiEndpoint"),
        "generated client Rustdoc omitted its API-specific endpoint capability"
    );
    for expected in [
        "tenant",
        "locale",
        "page_size",
        "protected secret input",
        "static API key (replace)",
        "Application native retry is disabled",
        "stable advanced prepared endpoint",
    ] {
        assert!(
            client_page.contains(expected),
            "generated client Rustdoc omitted value/retry behavior {expected}"
        );
    }
    assert!(
        !documentation
            .join("renamed_api/trait.RenamedApiEndpoint.html")
            .exists(),
        "generated Rustdoc exposed the hidden endpoint sealing capability implementation"
    );

    let facade_page = std::fs::read_to_string(
        documentation.join("renamed_api/facades/resources/struct.ResourcesFacade.html"),
    )
    .expect("generated facade Rustdoc should be readable");
    assert!(
        facade_page.contains("RenamedApi::resources::ById"),
        "facade and endpoint Rustdoc must use the same complete endpoint identity"
    );
    for expected in ["scope_value", "region", "scope_limit", "reset_scope_limit"] {
        assert!(
            facade_page.contains(expected),
            "generated facade Rustdoc omitted scope value/control {expected}"
        );
    }

    let prelude_path = documentation.join("renamed_api/prelude/index.html");
    let prelude_page = std::fs::read_to_string(&prelude_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", prelude_path.display()));
    assert!(
        prelude_page.contains("RenamedApiResourcesByIdRequestExt"),
        "generated prelude Rustdoc omitted the deterministic request-extension alias"
    );
    assert!(
        prelude_page.contains("RenamedApiResourcesPagesRequestExt"),
        "generated prelude Rustdoc omitted the paginated request-extension alias"
    );

    let credentials_page = std::fs::read_to_string(
        documentation.join("renamed_api/credentials/struct.ApiKeyCredentialHandle.html"),
    )
    .expect("generated credential handle Rustdoc should be readable");
    for expected in ["credential", "replace", "SecretString", "CredentialRef"] {
        assert!(
            credentials_page.contains(expected),
            "generated credential Rustdoc omitted {expected}"
        );
    }
    for forbidden in ["protected-api-key", "SlotSelector", "waiter", "reqwest"] {
        assert!(
            !credentials_page.contains(forbidden),
            "generated credential Rustdoc exposed {forbidden}"
        );
    }

    let request_ext_path =
        documentation.join("renamed_api/endpoints/resources/trait.ByIdRequestExt.html");
    let request_ext_page = std::fs::read_to_string(&request_ext_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", request_ext_path.display()));
    for expected in [
        "Request&lt;ById&gt;",
        "prelude",
        "RenamedApiResourcesByIdRequestExt",
        "verbose",
        "clear_verbose",
        "limit",
        "reset_limit",
        "without re-evaluation",
    ] {
        assert!(
            request_ext_page.contains(expected),
            "generated request-extension Rustdoc omitted {expected}"
        );
    }
    for forbidden in [
        "__private",
        "__concord_",
        "EndpointSpec",
        "ClientCore",
        "reqwest",
        "Sealed",
    ] {
        assert!(
            !request_ext_page.contains(forbidden),
            "generated request-extension Rustdoc exposed {forbidden}"
        );
    }

    let pages_request_ext_path =
        documentation.join("renamed_api/endpoints/resources/trait.PagesRequestExt.html");
    let pages_request_ext_page =
        std::fs::read_to_string(&pages_request_ext_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}",
                pages_request_ext_path.display()
            )
        });
    for expected in [
        "Request&lt;Pages&gt;",
        "RenamedApiResourcesPagesRequestExt",
        "id=\"tymethod.paginate\"",
        "PageLimit",
        "PaginatedRequest",
    ] {
        assert!(
            pages_request_ext_page.contains(expected),
            "generated pagination Rustdoc omitted {expected}"
        );
    }
    for forbidden in [
        "__private",
        "__concord_",
        "PaginatedEndpoint",
        "PaginationController",
        "RenamedController",
        "renamed_controller",
        "reqwest",
        "cursor-rustdoc-sentinel-73a4d1",
    ] {
        assert!(
            !pages_request_ext_page.contains(forbidden),
            "generated pagination Rustdoc exposed {forbidden}"
        );
    }
}
