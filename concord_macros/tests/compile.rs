use std::path::{Path, PathBuf};
use std::process::Output;

use concord_test_support::cargo_fixture::{check_manifest, diagnostics, run, run_nextest};

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
    check_fixture_with(name, &[])
}

fn check_fixture_with(name: &str, arguments: &[&str]) -> Output {
    let manifest = fixture_root().join(name).join("Cargo.toml");
    check_manifest(
        &workspace_root(),
        &format!("macros/{name}"),
        &manifest,
        arguments,
    )
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
fn renamed_facade_dependency_resolves_through_package_metadata() {
    assert_fixture_passes("renamed_facade");
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
            "records",
            vec!["--lib", "--no-default-features", "--features", "records"],
        ),
        (
            "csv",
            vec!["--lib", "--no-default-features", "--features", "csv"],
        ),
        (
            "ndjson",
            vec!["--lib", "--no-default-features", "--features", "ndjson"],
        ),
        (
            "csv-ndjson",
            vec!["--lib", "--no-default-features", "--features", "csv,ndjson"],
        ),
        (
            "all-extension-features",
            vec![
                "--lib",
                "--no-default-features",
                "--features",
                "json,multipart,csv,ndjson",
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
    for relative in [
        "extension_certification/src/lib.rs",
        "extension_certification/tests/extension_workflows_runtime.rs",
        "extension_certification/tests/pagination_runtime.rs",
    ] {
        let path = fixture_root().join(relative);
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
                "extension fixture {relative} exposed prohibited surface `{forbidden}`"
            );
        }
        for line in source
            .lines()
            .filter(|line| line.contains("concord_core::"))
        {
            assert!(
                line.contains("concord_core::advanced") || line.contains("concord_core::prelude"),
                "extension fixture {relative} imported an unapproved core namespace: {line}"
            );
        }
    }
}

#[test]
fn downstream_pagination_runtime_executes_generated_construction_and_binding() {
    let manifest = fixture_root().join("Cargo.toml");
    let output = run_nextest(
        &workspace_root(),
        "macros/extension-certification-pagination-runtime",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "-p",
            "concord-fixture-extension-certification",
            "--test",
            "pagination_runtime",
            "--no-tests",
            "fail",
            "--success-output",
            "final",
        ],
    );
    let diagnostic = diagnostics(&output);
    assert!(
        output.status.success(),
        "downstream pagination runtime failed\n{diagnostic}"
    );
    assert!(
        diagnostic.contains("1 test run: 1 passed"),
        "downstream pagination runtime did not select exactly one passing test\n{diagnostic}"
    );
    assert!(
        diagnostic.contains(
            "CONCORD_DOWNSTREAM_PAGINATION_RUNTIME_OK pages=2 items=3 construction=1 validation=2 next=2"
        ),
        "downstream pagination runtime omitted its post-assertion proof sentinel\n{diagnostic}"
    );

    let runtime_path = fixture_root().join("extension_certification/tests/pagination_runtime.rs");
    let runtime = std::fs::read_to_string(&runtime_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", runtime_path.display()));
    for assertion in [
        ".paginate(10_u32)",
        "assert_eq!(items, [1, 2, 3])",
        "cursor=initial",
        "cursor=continuation",
        "assert_eq!(probe_counts, (1, 2, 2))",
    ] {
        assert!(
            runtime.contains(assertion),
            "downstream pagination runtime omitted required assertion `{assertion}`"
        );
    }
}

#[test]
fn governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter() {
    let root = workspace_root();
    let manifest = fixture_root().join("no_governor_rate_limits/Cargo.toml");
    let output = run_nextest(
        &root,
        "macros/no-governor-rate-limits-runtime",
        [
            "--manifest-path",
            manifest.to_str().expect("no-governor fixture manifest"),
            "-p",
            "concord-fixture-no-governor-rate-limits",
            "--test",
            "no_governor_rate_limits",
            "--no-tests",
            "fail",
            "--success-output",
            "final",
        ],
    );
    let diagnostic = diagnostics(&output);
    assert!(
        output.status.success(),
        "governor-disabled fixture failed\n{diagnostic}"
    );
    assert!(
        diagnostic.contains("1 test run: 1 passed"),
        "governor-disabled fixture did not select exactly one passing test\n{diagnostic}"
    );
    assert!(
        diagnostic.contains("CONCORD_NO_GOVERNOR_RATE_LIMITS_OK generated=4 prepared=4 fallback_sends=0 observer_defaults=0"),
        "governor-disabled fixture omitted its post-assertion sentinel\n{diagnostic}"
    );

    for relative in [
        "no_governor_rate_limits/src/lib.rs",
        "no_governor_rate_limits/tests/no_governor_rate_limits.rs",
    ] {
        let source_path = fixture_root().join(relative);
        let source = std::fs::read_to_string(&source_path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", source_path.display()));
        for forbidden in [
            "__private",
            "__development",
            "reqwest",
            "ClientCore",
            "GovernorRateLimiter",
            "rate-limit-governor",
            "unsafe {",
        ] {
            assert!(
                !source.contains(forbidden),
                "no-governor fixture {relative} exposed prohibited surface `{forbidden}`"
            );
        }
    }

    let feature_graph = run(
        &root,
        "macros/no-governor-rate-limits-features",
        "tree",
        [
            "--manifest-path",
            manifest.to_str().expect("no-governor fixture manifest"),
            "-e",
            "features",
            "-p",
            "concord-fixture-no-governor-rate-limits",
        ],
    );
    let feature_diagnostic = diagnostics(&feature_graph);
    assert!(
        feature_graph.status.success(),
        "governor-disabled feature graph failed\n{feature_diagnostic}"
    );
    assert!(
        !feature_diagnostic.contains("rate-limit-governor"),
        "governor-disabled feature graph activated the governor feature\n{feature_diagnostic}"
    );
    assert!(
        feature_diagnostic.contains("concord_core"),
        "governor-disabled feature graph omitted concord_core\n{feature_diagnostic}"
    );
}

#[test]
fn downstream_credential_limiter_and_hooks_execute_through_public_runtime() {
    let manifest = fixture_root().join("Cargo.toml");
    let output = run_nextest(
        &workspace_root(),
        "macros/extension-certification-workflows-runtime",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "-p",
            "concord-fixture-extension-certification",
            "--test",
            "extension_workflows_runtime",
            "--no-tests",
            "fail",
            "--success-output",
            "final",
        ],
    );
    let diagnostic = diagnostics(&output);
    assert!(
        output.status.success(),
        "downstream credential/limiter/hook runtime failed\n{diagnostic}"
    );
    assert!(
        diagnostic.contains("1 test run: 1 passed"),
        "downstream workflow runtime did not select exactly one passing test\n{diagnostic}"
    );
    assert!(
        diagnostic.contains(
            "CONCORD_DOWNSTREAM_EXTENSION_RUNTIME_OK provider=1 limiter_acquire=1 limiter_observe=1 hook_before=1 hook_response=1 hook_complete=1"
        ),
        "downstream workflow runtime omitted its post-assertion proof sentinel\n{diagnostic}"
    );
}

#[test]
fn missing_core_dependency_has_a_focused_macro_diagnostic() {
    assert_fixture_fails_with("missing_core", "requires the `concord` package");
}

#[test]
fn incompatible_core_contract_fails_during_compilation() {
    assert_fixture_fails_with(
        "contract_mismatch",
        "concord macro/core contract mismatch: core 999, generated 20",
    );
}

#[test]
fn json_body_and_response_without_runtime_feature_fail_through_core_type_availability() {
    for arguments in [&[][..], &["--features", "ndjson"][..]] {
        let output = check_fixture_with("json_disabled", arguments);
        let diagnostic = diagnostics(&output);
        assert!(
            !output.status.success(),
            "JSON-disabled fixture unexpectedly compiled with {arguments:?}\n{diagnostic}"
        );
        assert!(
            diagnostic.contains("cannot find type `Json` in this scope"),
            "JSON-disabled fixture omitted the type-availability diagnostic with {arguments:?}\n{diagnostic}"
        );
    }
}

#[test]
fn oauth_provider_is_absent_without_its_facade_feature() {
    assert_fixture_fails_with("oauth_disabled", "no `OAuth2ClientCredentials` in `auth`");
}

#[test]
fn multipart_dsl_without_runtime_feature_fails_through_core_type_availability() {
    assert_fixture_fails_with(
        "multipart_disabled",
        "cannot find type `Multipart` in this scope",
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
fn generated_clients_compile_with_core_development_tools_enabled() {
    assert_fixture_passes("development_tools_enabled");
}

#[test]
fn generated_raw_response_terminal_is_absent_without_the_core_feature() {
    assert_fixture_fails_with("raw_response_disabled", "no method named `raw_response`");
}

#[test]
fn generated_public_rustdoc_uses_stable_paths_without_private_integration_names() {
    let root = workspace_root();
    let manifest = fixture_root().join("renamed_facade/Cargo.toml");
    let target_key = "macros/renamed-facade-rustdoc";
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
        .join("doc/concord_fixture_renamed_facade");
    let public_pages = [
        "struct.RenamedApi.html",
        "struct.RenamedApiBuilder.html",
        "struct.RenamedRateApi.html",
        "struct.RenamedRateApiBuilder.html",
        "renamed_api/trait.Endpoint.html",
        "renamed_rate_api/trait.Endpoint.html",
        "renamed_api/resources/struct.Resources.html",
        "renamed_api/credentials/struct.RenamedApiCredentials.html",
        "renamed_api/credentials/struct.ApiKeyCredentialHandle.html",
        "renamed_api/resources/struct.ById.html",
        "renamed_api/resources/struct.ByIdRequest.html",
        "renamed_api/resources/struct.Custom.html",
        "renamed_api/resources/struct.CustomRequest.html",
        "renamed_api/resources/struct.StreamUpload.html",
        "renamed_api/resources/struct.StreamUploadRequest.html",
        "renamed_api/resources/struct.StreamDownload.html",
        "renamed_api/resources/struct.StreamDownloadRequest.html",
        "renamed_api/resources/struct.MultipartUpload.html",
        "renamed_api/resources/struct.MultipartUploadRequest.html",
        "renamed_api/resources/struct.Pages.html",
        "renamed_api/resources/struct.PagesRequest.html",
        "renamed_rate_api/struct.Read.html",
        "renamed_rate_api/struct.ReadRequest.html",
    ];
    for relative in public_pages {
        let path = documentation.join(relative);
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for forbidden in [
            "__private",
            "__concord_endpoint_seal",
            "__concord_inner",
            "__Concord",
            "ClientCore",
            "EndpointSpec",
            "RawResponseEndpoint",
            "MetadataResponse",
            "ResponseHandler",
            "CodecBody",
            "BufferedResponse",
            "PreparedBody",
            "StreamRequestBody",
            "StreamResponseHandler",
            "PreparedResponseMarker",
            "PreparedRequestBody",
            "ManagedRequestStream",
            "Replayability",
            "reqwest",
            "renamed-rate-key-sentinel",
            "protected-api-key",
        ] {
            assert!(
                !page.contains(forbidden),
                "public generated Rustdoc {} exposed {forbidden}",
                path.display()
            );
        }
        if !relative.ends_with("trait.Endpoint.html") {
            assert!(
                !page.contains("Sealed"),
                "ordinary generated Rustdoc {} exposed the sealing implementation",
                path.display()
            );
        }
    }

    let client_page = std::fs::read_to_string(documentation.join("struct.RenamedApi.html"))
        .expect("generated client Rustdoc should be readable");
    assert!(
        client_page.contains("renamed_api/trait.Endpoint.html#associatedtype.Request"),
        "generated client Rustdoc omitted the sealed endpoint associated request"
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
    let endpoint_trait_page =
        std::fs::read_to_string(documentation.join("renamed_api/trait.Endpoint.html"))
            .expect("generated endpoint trait Rustdoc should be readable");
    for expected in [
        "id=\"associatedtype.Request\"",
        "id=\"associatedtype.Response\"",
        "Named inert request",
        "Response specification selected",
        "Sealed",
        "ByIdRequest",
    ] {
        assert!(
            endpoint_trait_page.contains(expected),
            "generated endpoint trait Rustdoc omitted {expected}"
        );
    }

    let facade_page =
        std::fs::read_to_string(documentation.join("renamed_api/resources/struct.Resources.html"))
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

    assert!(
        facade_page.contains("id=\"method.by_id\"") && facade_page.contains("ByIdRequest"),
        "generated scope facade Rustdoc omitted its named request return"
    );
    assert!(
        !documentation
            .join("renamed_api/prelude/index.html")
            .exists(),
        "generated Rustdoc retained the removed per-client prelude"
    );
    assert!(
        !documentation
            .join("renamed_api/facades/index.html")
            .exists()
            && !documentation
                .join("renamed_api/endpoints/index.html")
                .exists(),
        "generated Rustdoc retained split facade/endpoint module trees"
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

    let by_id_endpoint_path = documentation.join("renamed_api/resources/struct.ById.html");
    let by_id_endpoint_page =
        std::fs::read_to_string(&by_id_endpoint_path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", by_id_endpoint_path.display())
        });
    for expected in [
        "RenamedApi::resources::ById",
        "POST",
        "Text",
        "scope_value",
        "endpoint_value",
        "body",
        "Credential values are absent",
        "verbose",
        "clear_verbose",
        "limit",
        "reset_limit",
    ] {
        assert!(
            by_id_endpoint_page.contains(expected),
            "generated endpoint Rustdoc omitted {expected}"
        );
    }
    for forbidden in ["__concord_scope_0", "__concord_endpoint_0"] {
        assert!(
            !by_id_endpoint_page.contains(forbidden),
            "generated endpoint Rustdoc exposed internal constructor parameter {forbidden}"
        );
    }

    let request_path = documentation.join("renamed_api/resources/struct.ByIdRequest.html");
    let request_page = std::fs::read_to_string(&request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", request_path.display()));
    for expected in [
        "Inert named request",
        "id=\"method.timeout\"",
        "id=\"method.disable_timeout\"",
        "id=\"method.inherit_timeout\"",
        "id=\"method.debug\"",
        "id=\"method.response\"",
        "id=\"method.raw_response\"",
        "id=\"method.verbose\"",
        "id=\"method.clear_verbose\"",
        "id=\"method.limit\"",
        "id=\"method.reset_limit\"",
        "without re-evaluation",
        "Mode: RawResponseMode",
        "Endpoint.html#associatedtype.Response",
        "complete bounded successful response",
    ] {
        assert!(
            request_page.contains(expected),
            "generated named request Rustdoc omitted {expected}"
        );
    }
    for forbidden in [
        "__private",
        "__concord_",
        "__Concord",
        "EndpointSpec",
        "RawResponseEndpoint",
        "ClientCore",
        "reqwest",
        "Sealed",
        "ResponseSpecification",
    ] {
        assert!(
            !request_page.contains(forbidden),
            "generated named request Rustdoc exposed {forbidden}"
        );
    }

    let pages_request_path = documentation.join("renamed_api/resources/struct.PagesRequest.html");
    let pages_request_page = std::fs::read_to_string(&pages_request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", pages_request_path.display()));
    for expected in ["id=\"method.paginate\"", "PageLimit", "PaginatedRequest"] {
        assert!(
            pages_request_page.contains(expected),
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
            !pages_request_page.contains(forbidden),
            "generated pagination Rustdoc exposed {forbidden}"
        );
    }
    let resource_files = std::fs::read_dir(documentation.join("renamed_api/resources"))
        .expect("generated resource module should be readable")
        .map(|entry| {
            entry
                .expect("generated Rustdoc directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        resource_files
            .iter()
            .all(|name| !name.contains("RequestExt")),
        "generated Rustdoc retained request extension traits: {resource_files:?}"
    );
}

#[test]
fn generated_records_rustdoc_uses_stable_paths_and_exposes_no_record_value_channel() {
    let root = workspace_root();
    let manifest = fixture_root().join("extension_certification/Cargo.toml");
    let target_key = "macros/extension-records-rustdoc";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "--manifest-path",
            manifest.to_str().expect("fixture manifest path"),
            "--no-default-features",
            "--features",
            "csv,ndjson",
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        output.status.success(),
        "generated records Rustdoc failed\n{}",
        diagnostics(&output)
    );

    let documentation = root
        .join("target/cargo-fixtures")
        .join(target_key)
        .join("doc/concord_fixture_extension_certification");
    let endpoints = [
        (
            "record_extension_api/struct.Custom.html",
            "RecordExtensionApi::Custom",
            "Records &lt; crate :: ExtensionRecordDecoder &gt;",
        ),
        (
            "csv_extension_api/struct.Rows.html",
            "CsvExtensionApi::Rows",
            "Records &lt; Csv &lt; Vec &lt; String &gt; &gt; &gt;",
        ),
        (
            "nd_json_extension_api/struct.Values.html",
            "NdJsonExtensionApi::Values",
            "Records &lt; NdJson &lt; u32 &gt; &gt;",
        ),
    ];
    for (relative, identity, response) in endpoints {
        let path = documentation.join(relative);
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let normalized = page.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in [
            identity,
            response,
            "response specification",
            "universal successful metadata",
            "core-owned capability bounds",
            "feature availability is owned by",
        ] {
            assert!(
                normalized.contains(expected),
                "generated records Rustdoc {relative} omitted {expected:?}"
            );
        }
        for forbidden in [
            "__private",
            "__concord",
            "ResponseInput",
            "ApplicationResponse",
            "ClientCore",
            "EndpointSpec",
            "ResponseHandler",
            "reqwest",
            "decoder_state",
            "staging_bytes",
            "record_value",
            "record-rustdoc-value-sentinel-42ad91",
        ] {
            assert!(
                !normalized.contains(forbidden),
                "generated records Rustdoc {relative} exposed {forbidden}"
            );
        }
    }

    for relative in [
        "struct.RecordExtensionApi.html",
        "struct.CsvExtensionApi.html",
        "struct.NdJsonExtensionApi.html",
    ] {
        assert!(
            documentation.join(relative).is_file(),
            "generated records Rustdoc omitted {relative}"
        );
    }
    assert!(
        !documentation
            .join("__concord_record_extension_api_specifications/index.html")
            .exists(),
        "generated records Rustdoc exposed its hidden specification aliases"
    );
}
