use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("façade crate should be inside crates/ in the workspace")
        .to_owned()
}

#[test]
fn stable_request_terminals_are_semantic_and_private_in_rendered_rustdoc() {
    let root = workspace_root();
    let target_key = "rustdoc/core-public-surface";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "-p",
            "concord-core",
            "--all-features",
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        output.status.success(),
        "core public Rustdoc failed\n{}",
        diagnostics(&output)
    );

    let documentation = root
        .join("target/fixtures")
        .join(target_key)
        .join("doc/concord_core");
    let core_index_path = documentation.join("index.html");
    let core_index = std::fs::read_to_string(&core_index_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", core_index_path.display()));
    for hidden in ["__benchmark", "__private", "__development"] {
        assert!(
            !core_index.contains(hidden),
            "core's ordinary Rustdoc index exposed hidden integration {hidden}"
        );
    }

    let facade_target_key = "rustdoc/facade-public-surface";
    let facade_output = run(
        &root,
        facade_target_key,
        "rustdoc",
        [
            "-p",
            "concord",
            "--all-features",
            "--lib",
            "--",
            "-Dwarnings",
        ],
    );
    assert!(
        facade_output.status.success(),
        "façade public Rustdoc failed\n{}",
        diagnostics(&facade_output)
    );
    let facade_index_path = root
        .join("target/fixtures")
        .join(facade_target_key)
        .join("doc/concord/index.html");
    let facade_index = std::fs::read_to_string(&facade_index_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", facade_index_path.display()));
    for hidden in ["__benchmark", "__private", "__development"] {
        assert!(
            !facade_index.contains(hidden),
            "façade's ordinary Rustdoc index exposed hidden integration {hidden}"
        );
    }

    let request_path = documentation.join("struct.Request.html");
    let response_path = documentation.join("struct.ResponseRequest.html");
    let request = std::fs::read_to_string(&request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", request_path.display()));
    let response = std::fs::read_to_string(&response_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", response_path.display()));
    let multipart_body_path = documentation.join("advanced/struct.MultipartBody.html");
    let multipart_part_path = documentation.join("advanced/struct.MultipartPart.html");
    let multipart_factory_path = documentation.join("advanced/trait.MultipartFactory.html");
    let byte_stream_path = documentation.join("advanced/struct.ByteStream.html");
    let multipart_body = std::fs::read_to_string(&multipart_body_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", multipart_body_path.display())
    });
    let multipart_part = std::fs::read_to_string(&multipart_part_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", multipart_part_path.display())
    });
    let multipart_factory =
        std::fs::read_to_string(&multipart_factory_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}",
                multipart_factory_path.display()
            )
        });
    let byte_stream = std::fs::read_to_string(&byte_stream_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", byte_stream_path.display()));
    let page_limit_path = documentation.join("pagination/enum.PageLimit.html");
    let paginated_request_path = documentation.join("pagination/struct.PaginatedRequest.html");
    let page_limit = std::fs::read_to_string(&page_limit_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", page_limit_path.display()));
    let paginated_request =
        std::fs::read_to_string(&paginated_request_path).unwrap_or_else(|error| {
            panic!(
                "failed to read {}: {error}",
                paginated_request_path.display()
            )
        });
    let pagination_pages = [
        "struct.PaginationError.html",
        "trait.PaginationController.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("pagination").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    })
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        !documentation
            .join("advanced/struct.PaginationRequest.html")
            .exists(),
        "removed PaginationRequest unexpectedly remains in public Rustdoc"
    );
    let rate_pages = [
        "struct.RateLimitError.html",
        "struct.RateLimitId.html",
        "struct.RateLimitKeyPart.html",
        "struct.RateLimitKey.html",
        "struct.RateLimitWindow.html",
        "struct.RateLimitRule.html",
        "struct.RateLimitPlan.html",
        "struct.RateLimitPermit.html",
        "struct.RateLimitRequest.html",
        "struct.RateLimitResponse.html",
        "struct.RateLimitResponseHead.html",
        "trait.RateLimiter.html",
        "trait.RateLimitObserver.html",
        "struct.NoopRateLimiter.html",
        "struct.Cooldown.html",
        "enum.CooldownScope.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("rate_limit").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    })
    .collect::<Vec<_>>()
    .join("\n");
    for moved in [
        "struct.GovernorRateLimiter.html",
        "struct.DefaultRateLimitObserver.html",
    ] {
        assert!(
            !documentation.join("rate_limit").join(moved).exists(),
            "extras-owned rate-limit implementation remains in core Rustdoc: {moved}"
        );
    }
    for removed in [
        "struct.PreparedBuffered.html",
        "struct.PreparedBytes.html",
        "struct.PreparedNoContent.html",
        "struct.PreparedStream.html",
    ] {
        assert!(
            !documentation.join("advanced").join(removed).exists(),
            "removed prepared response marker remains in Rustdoc: {removed}"
        );
    }
    let runtime_endpoint_pages = [
        "struct.RuntimeEndpointBuilder.html",
        "struct.RuntimeEndpoint.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("advanced").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    })
    .collect::<Vec<_>>()
    .join("\n");
    let endpoint_id_path = documentation.join("advanced/struct.EndpointId.html");
    let endpoint_id = std::fs::read_to_string(&endpoint_id_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", endpoint_id_path.display()));
    let runtime_config_path = documentation.join("advanced/struct.RuntimeConfigBuilder.html");
    let runtime_config = std::fs::read_to_string(&runtime_config_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", runtime_config_path.display())
    });
    let generic_spec_record_contracts = [
        (
            "advanced/trait.RequestBodySpec.html",
            &[
                "single managed outgoing-body lifecycle",
                "before the before-send hook",
                "must not encode data",
                "hook veto",
                "derives replayability",
                "operation deadline",
                "Inputs may be sensitive",
            ][..],
        ),
        (
            "advanced/trait.ResponseBodySpec.html",
            &[
                "ordinary non-2xx status handling",
                "known-length preflight",
                "before the first body poll",
                "three sealed modes",
                "operation deadline",
                "No native response",
                "sensitive application data",
            ][..],
        ),
        (
            "advanced/trait.ResponseMode.html",
            &[
                "core-owned successful-response acquisition mode",
                "cannot implement another mode",
                "native or unbounded response body",
            ][..],
        ),
        (
            "advanced/struct.ResponseHead.html",
            &[
                "logical application URL",
                "actual successful response headers",
                "owns their logging and storage",
                "No native response or transport handle",
            ][..],
        ),
        (
            "advanced/struct.ApiResponse.html",
            &[
                "single metadata wrapper",
                "never formats",
                "logical URL",
                "actual successful headers",
                "id=\"method.into_value\"",
            ][..],
        ),
        (
            "advanced/struct.ByteStream.html",
            &[
                "media-independent",
                "post-decompression",
                "first chunk",
                "withheld",
                "Dropping the stream cancels",
                "neither response metadata nor a media marker",
            ][..],
        ),
        (
            "records/trait.RecordDecoder.html",
            &[
                "before the first body poll",
                "one decoder per returned record stream",
                "without a core lock",
                "network chunks are never record boundaries",
                "positive finite staging bound",
            ][..],
        ),
        (
            "records/struct.RecordStream.html",
            &[
                "bounded independently",
                "rejected without retaining its bytes",
                "stream items",
                "cancels native body consumption",
                "never formats records",
            ][..],
        ),
        (
            "records/struct.RecordBatches.html",
            &[
                "One-record streaming remains the primitive",
                "positive batch size",
                "never formats records",
            ][..],
        ),
    ];
    let extension_contracts = [
        (
            "codec/trait.MediaType.html",
            &[
                "concurrent operations",
                "no runtime callback",
                "non-sensitive",
                "mutable global state",
            ][..],
        ),
        (
            "codec/trait.EncodeBody.html",
            &[
                "synchronous work",
                "operations may occur concurrently",
                "operation deadline",
                "cancellation",
                "metric cardinality",
                "external I/O",
                "logging or storage",
            ][..],
        ),
        (
            "codec/trait.DecodeBody.html",
            &[
                "synchronous callbacks",
                "operations may occur concurrently",
                "operation deadline",
                "cancellation",
                "metric cardinality",
                "external I/O",
                "logging or storage",
            ][..],
        ),
        (
            "trait.BodyFactory.html",
            &[
                "operations may overlap",
                "sensitive",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "trait.MultipartFactory.html",
            &[
                "operations may overlap",
                "sensitive",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "trait.CredentialMaterial.html",
            &[
                "overlapping credential operations",
                "sensitive application-owned state",
                "logging, storage",
                "metric cardinality",
                "external I/O",
                "operation deadlines",
                "cancellation",
                "automatic limits",
            ][..],
        ),
        (
            "trait.SecretCredential.html",
            &[
                "native authentication boundary",
                "logging, storage",
                "concurrency",
                "cancellation",
                "deadline",
                "boundedness obligations",
            ][..],
        ),
        (
            "trait.CredentialProvider.html",
            &[
                "Calls may overlap",
                "sensitive credential material",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "struct.CredentialRequest.html",
            &[
                "Bounded context",
                "request_body_limit",
                "encoded output",
                "active runtime ceiling",
            ][..],
        ),
        (
            "trait.ProviderBodyFactory.html",
            &[
                "operations may overlap",
                "sensitive",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "Cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "pagination/trait.PaginationController.html",
            &[
                "concurrent pagination operations",
                "sensitive",
                "logging and storage",
                "metric cardinality",
                "no external I/O",
                "outside the per-page operation deadline",
                "Cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "rate_limit/trait.RateLimiter.html",
            &[
                "Calls may overlap",
                "potentially sensitive",
                "logs and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "cancellation",
                "does not automatically bound",
            ][..],
        ),
        (
            "rate_limit/trait.RateLimitObserver.html",
            &[
                "Calls for independent responses may overlap",
                "resolved rate keys",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "operation deadline",
                "Cancellation",
                "not automatically bounded",
            ][..],
        ),
        (
            "trait.RuntimeHooks.html",
            &[
                "Calls may overlap",
                "operationally sensitive",
                "logging and storage",
                "metric cardinality",
                "external I/O",
                "outside that deadline",
                "cancellation",
                "does not automatically time-limit completion callbacks",
            ][..],
        ),
        (
            "struct.ProviderHttp.html",
            &[
                "separate managed provider client",
                "finite body limits",
                "credential operation deadline",
                "Dropping the provider future",
                "sensitive",
                "logging and storage",
                "active semantic request-body ceiling",
                "native transport state",
                "method.request_body_limit",
            ][..],
        ),
    ];
    let prelude = std::fs::read_to_string(documentation.join("prelude/index.html"))
        .expect("ordinary prelude Rustdoc");
    let dangerous = std::fs::read_to_string(documentation.join("dangerous/index.html"))
        .expect("dangerous namespace Rustdoc");
    let raw_response_path = documentation.join("dangerous/struct.RawResponse.html");
    let raw_request_path = documentation.join("dangerous/struct.RawResponseRequest.html");
    let raw_response = std::fs::read_to_string(&raw_response_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", raw_response_path.display()));
    let raw_request = std::fs::read_to_string(&raw_request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", raw_request_path.display()));

    // Phase 6 moved ordinary terminal discovery to generated named request
    // wrappers. The generic core carrier retains capability-bound integration
    // methods for runtime/generated delegation, but their private endpoint
    // bounds must not leak into its public Rustdoc.
    for hidden_terminal in ["id=\"method.response\"", "id=\"method.raw_response\""] {
        assert!(
            !request.contains(hidden_terminal),
            "generic Request Rustdoc exposed generated capability terminal {hidden_terminal:?}"
        );
    }
    for expected in [
        "pub struct RawResponse",
        "id=\"method.url\"",
        "id=\"method.status\"",
        "id=\"method.version\"",
        "id=\"method.headers\"",
        "id=\"method.metadata\"",
        "id=\"method.body\"",
        "id=\"method.into_body\"",
        "actual successful",
        "may contain secrets",
        "callers assume responsibility",
        "never exposes a Reqwest",
        "response. Unlike ordinary",
        "Fields are intentionally private",
    ] {
        assert!(
            raw_response.contains(expected),
            "RawResponse Rustdoc omitted {expected:?}"
        );
    }
    for expected in [
        "pub struct RawResponseRequest",
        "ordinary status classification",
        "retry containment",
        "actual headers and complete bounded body",
        "callers assume responsibility",
        "Fields are intentionally private",
    ] {
        assert!(
            raw_request.contains(expected),
            "RawResponseRequest Rustdoc omitted {expected:?}"
        );
    }
    assert!(
        dangerous.contains("RawResponse")
            && dangerous.contains("RawResponseRequest")
            && dangerous.contains("ReqwestRetryBuilder")
            && dangerous.contains("unified for the compiled crate")
            && dangerous.contains("Neither capability is enabled by default"),
        "dangerous namespace Rustdoc omitted feature-unification or owned capabilities"
    );
    assert!(
        !prelude.contains("RawResponse") && !runtime_endpoint_pages.contains("RawResponse"),
        "raw response escaped its dangerous namespace or entered runtime-endpoint docs"
    );
    for forbidden in [
        "RawResponseEndpoint",
        "BytesResponse",
        "ClientCore",
        "execute_request_as",
        "__private",
    ] {
        assert!(
            !raw_response.contains(forbidden) && !raw_request.contains(forbidden),
            "dangerous raw Rustdoc exposed private integration {forbidden}"
        );
    }
    for expected in [
        "id=\"method.runtime_endpoint_timeout\"",
        "id=\"method.disable_runtime_endpoint_timeout\"",
        "runtime endpoints",
        "Generated endpoints retain",
    ] {
        assert!(
            runtime_config.contains(expected),
            "runtime configuration Rustdoc omitted scoped timeout contract {expected:?}"
        );
    }
    for forbidden in [
        "id=\"method.operation_timeout\"",
        "id=\"method.disable_operation_timeout\"",
    ] {
        assert!(
            !runtime_config.contains(forbidden),
            "runtime configuration Rustdoc retained misleading timeout API {forbidden:?}"
        );
    }
    assert!(
        endpoint_id.contains("MAX_ENDPOINT_ID_BYTES")
            && !endpoint_id.contains("id=\"method.from_static\""),
        "endpoint identity Rustdoc omitted its public bound or exposed static generated construction"
    );
    assert!(
        response.contains("<title>ResponseRequest in concord_core"),
        "ResponseRequest Rustdoc page omitted its public identity"
    );
    assert!(
        response.contains("pub struct ResponseRequest"),
        "ResponseRequest Rustdoc omitted its public declaration"
    );
    assert!(
        multipart_body.contains("owned request-side multipart recipe")
            && multipart_body.contains("boundary generation")
            && multipart_part.contains("opaque part")
            && multipart_factory.contains("fresh complete multipart recipe"),
        "multipart Rustdoc omitted its public lifecycle and ownership semantics"
    );
    assert!(
        byte_stream.contains("bounded media-independent lazy successful response body")
            && byte_stream.contains("caller-driven polling")
            && byte_stream.contains("cumulative stream limit"),
        "ByteStream Rustdoc omitted its bounded lazy semantics"
    );
    for (relative, expected) in generic_spec_record_contracts {
        let path = documentation.join(relative);
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let normalized = page.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in expected {
            assert!(
                normalized.contains(expected),
                "generic specification or records Rustdoc {relative} omitted {expected:?}"
            );
        }
        for forbidden in [
            "ResponseHandler",
            "ResponseInput",
            "ApplicationResponse",
            "ClientCore",
            "PreparedBody",
            "PreparedRequestBody",
            "reqwest",
        ] {
            assert!(
                !normalized.contains(forbidden),
                "generic specification or records Rustdoc {relative} exposed private integration {forbidden}"
            );
        }
    }
    for relative in [
        "advanced/struct.Buffered.html",
        "advanced/struct.Streaming.html",
        "advanced/struct.Discard.html",
        "advanced/struct.ResponseBodyError.html",
        "advanced/enum.ResponseBodyErrorKind.html",
        "records/struct.Records.html",
        "records/struct.RecordError.html",
        "records/enum.RecordErrorKind.html",
    ] {
        assert!(
            documentation.join(relative).is_file(),
            "generic specification or records Rustdoc omitted {relative}"
        );
    }
    assert!(
        !documentation
            .join("advanced/type.DecodedResponse.html")
            .exists(),
        "removed DecodedResponse compatibility alias unexpectedly remains in public Rustdoc"
    );
    assert!(
        !documentation
            .join("advanced/struct.StreamResponse.html")
            .exists(),
        "removed StreamResponse unexpectedly remains in public Rustdoc"
    );
    for expected in ["Take", "HardCap", "finite requested pagination limit"] {
        assert!(
            page_limit.contains(expected),
            "page-limit Rustdoc omitted {expected:?}"
        );
    }
    assert!(
        paginated_request.contains("opaque directly awaitable sequential pagination operation")
            && paginated_request.contains("pub struct PaginatedRequest"),
        "paginated-request Rustdoc omitted its opaque directly awaitable contract"
    );
    for expected in [
        "id=\"method.new\"",
        "id=\"method.with_safe_message\"",
        "id=\"method.kind\"",
        "id=\"method.validate\"",
        "id=\"tymethod.next\"",
        "id=\"method.short_page_size\"",
        "moves from",
    ] {
        assert!(
            pagination_pages.contains(expected),
            "pagination Rustdoc omitted {expected:?}"
        );
    }
    for moved in [
        "struct.OffsetPagination.html",
        "struct.PagePagination.html",
        "struct.CursorPagination.html",
        "trait.HasNextCursor.html",
    ] {
        assert!(
            !documentation.join("pagination").join(moved).exists()
                && !documentation.join("advanced").join(moved).exists(),
            "extras-owned pagination item remains in core Rustdoc: {moved}"
        );
    }
    for forbidden in [
        "PaginatedEndpoint",
        "paginate_with",
        "PaginationPatch",
        "CanonicalIdentity",
        "CycleDetector",
        "controller-rustdoc-sentinel-73a4d1",
        "cursor-rustdoc-sentinel-73a4d1",
        "reqwest",
    ] {
        assert!(
            !pagination_pages.contains(forbidden)
                && !paginated_request.contains(forbidden)
                && !page_limit.contains(forbidden),
            "pagination Rustdoc exposed private integration or continuation text {forbidden}"
        );
    }
    for expected in [
        "id=\"method.builder\"",
        "id=\"method.path_segment\"",
        "id=\"method.query\"",
        "id=\"method.query_optional\"",
        "id=\"method.query_values\"",
        "id=\"method.header\"",
        "id=\"method.body\"",
        "id=\"method.body_spec\"",
        "id=\"method.body_codec\"",
        "id=\"method.auth\"",
        "id=\"method.timeout\"",
        "id=\"method.disable_timeout\"",
        "id=\"method.inherit_timeout\"",
        "id=\"method.rate_limit\"",
        "id=\"method.response\"",
        "id=\"method.response_bytes\"",
        "id=\"method.response_no_content\"",
        "id=\"method.response_stream\"",
        "id=\"method.build\"",
        "id=\"method.description\"",
        "exactly one URL-encoded path segment",
        "Fields are intentionally private",
    ] {
        assert!(
            runtime_endpoint_pages.contains(expected),
            "runtime endpoint Rustdoc omitted {expected:?}"
        );
    }
    for forbidden in [
        "id=\"method.paginate\"",
        "id=\"method.raw_path\"",
        "id=\"method.raw_response\"",
        "PreparedRaw",
        "PreparedPagination",
    ] {
        assert!(
            !runtime_endpoint_pages.contains(forbidden),
            "runtime endpoint Rustdoc exposed prohibited capability {forbidden:?}"
        );
    }
    for (name, expected) in extension_contracts {
        let path = if name.contains('/') {
            documentation.join(name)
        } else {
            documentation.join("advanced").join(name)
        };
        let page = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let page = page.split_whitespace().collect::<Vec<_>>().join(" ");
        for expected in expected {
            assert!(
                page.contains(expected),
                "extension Rustdoc {name} omitted obligation {expected:?}"
            );
        }
    }
    for expected in [
        "id=\"method.granted\"",
        "id=\"method.from_parts\"",
        "id=\"method.total_bytes\"",
        "id=\"method.empty\"",
        "id=\"method.try_from_rules\"",
        "id=\"method.acquire\"",
        "id=\"method.observe\"",
        "id=\"method.preparation_key_limit\"",
        "cooldown_cap",
        "DEFAULT_RATE_LIMIT_KEY_BYTES",
        "operationally sensitive",
    ] {
        assert!(
            rate_pages.contains(expected),
            "rate-limit Rustdoc omitted {expected:?}"
        );
    }
    for forbidden in [
        "GovernorState",
        "QuotaEntry",
        "CooldownEntry",
        "BucketKey",
        "OBSERVATION_COOLDOWN_CAP",
        "fallback_limiter",
        "as_any",
        "id=\"method.from_rules\"",
        "rate-key-rustdoc-sentinel-64c2e9",
    ] {
        assert!(
            !rate_pages.contains(forbidden),
            "rate-limit Rustdoc exposed private state or key text {forbidden}"
        );
    }
    assert!(
        !prelude.contains("RateLimitPlan") && !prelude.contains("GovernorRateLimiter"),
        "ordinary prelude unexpectedly exported advanced rate-limit contracts"
    );
    assert!(
        prelude.contains("PageLimit")
            && !prelude.contains("PaginationController")
            && !prelude.contains("PaginationRequest"),
        "ordinary prelude pagination exports were not exact"
    );
    assert!(
        !prelude.contains("RuntimeEndpoint") && !prelude.contains("RuntimeEndpointBuilder"),
        "ordinary prelude unexpectedly exported runtime endpoint contracts"
    );

    for forbidden in [
        "MetadataResponse",
        "__private",
        "EndpointSpec",
        "ResponseHandler",
        "ClientCore",
        "PreparedBody",
        "execute_request",
        "reqwest",
        "Replayability",
        "NativeMultipartStream",
        "ManagedRequestStream",
        "StreamResponseHandler",
        "PreparedResponseMarker",
        "PreparedRequestBody",
    ] {
        assert!(
            [
                &request,
                &response,
                &multipart_body,
                &multipart_part,
                &multipart_factory,
                &byte_stream,
                &page_limit,
                &paginated_request,
                &pagination_pages,
                &rate_pages,
                &runtime_endpoint_pages,
                &raw_response,
                &raw_request,
            ]
            .iter()
            .all(|page| !page.contains(forbidden)),
            "stable public Rustdoc exposed private integration name {forbidden}"
        );
    }
}

#[test]
fn record_surfaces_are_structural_private_and_value_free_in_rendered_rustdoc() {
    let root = workspace_root();
    let configurations = [("rustdoc/core-records-only", "records")];

    for (target_key, features) in configurations {
        let output = run(
            &root,
            target_key,
            "rustdoc",
            [
                "-p",
                "concord-core",
                "--no-default-features",
                "--features",
                features,
                "--lib",
                "--",
                "-Dwarnings",
            ],
        );
        assert!(
            output.status.success(),
            "isolated records Rustdoc `{features}` failed\n{}",
            diagnostics(&output)
        );

        let documentation = root
            .join("target/fixtures")
            .join(target_key)
            .join("doc/concord_core");
        let records_index = documentation.join("records/index.html");
        assert!(records_index.is_file());
        let expected = [
            "records/struct.Records.html",
            "records/struct.RecordStream.html",
            "records/struct.RecordBatches.html",
            "records/trait.RecordDecoder.html",
            "records/struct.RecordError.html",
            "records/enum.RecordErrorKind.html",
        ];
        for relative in expected {
            assert!(
                documentation.join(relative).is_file(),
                "isolated `{features}` Rustdoc omitted {relative}"
            );
        }
        assert!(
            !documentation.join("records/struct.Csv.html").is_file()
                && !documentation.join("records/struct.NdJson.html").is_file(),
            "extras-owned record decoders remain in core Rustdoc"
        );

        let records_index = std::fs::read_to_string(&records_index)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", records_index.display()));
        for forbidden in [
            "serde::",
            "serde_json",
            "csv::Reader",
            "struct.Csv",
            "struct.NdJson",
        ] {
            assert!(
                !records_index.contains(forbidden),
                "records-only Rustdoc claimed parser dependency or type {forbidden}"
            );
        }

        for relative in expected {
            let path = documentation.join(relative);
            let page = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            for forbidden in [
                "__private",
                "__concord",
                "ApplicationResponse",
                "ResponseInput",
                "ClientCore",
                "EndpointSpec",
                "ResponseHandler",
                "PreparedBody",
                "reqwest::",
                "reqwest::Response",
                "decoder_state",
                "staging_bytes",
                "native_body",
            ] {
                assert!(
                    !page.contains(forbidden),
                    "isolated `{features}` Rustdoc {relative} exposed {forbidden}"
                );
            }
        }

        for relative in [
            "records/struct.Records.html",
            "records/struct.RecordStream.html",
            "records/struct.RecordBatches.html",
            "records/struct.RecordError.html",
        ] {
            let path = documentation.join(relative);
            let page = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            assert!(
                !page.contains("id=\"structfield."),
                "isolated `{features}` Rustdoc {relative} exposed private fields"
            );
        }

        let record_stream =
            std::fs::read_to_string(documentation.join("records/struct.RecordStream.html"))
                .expect("RecordStream Rustdoc");
        let record_batches =
            std::fs::read_to_string(documentation.join("records/struct.RecordBatches.html"))
                .expect("RecordBatches Rustdoc");
        assert!(
            record_stream.contains("never formats")
                && record_stream.contains("decoder state")
                && record_stream.contains("staged bytes")
                && record_batches.contains("never formats records"),
            "isolated `{features}` Rustdoc omitted structural Debug guarantees"
        );
        for accessor in [
            "id=\"method.body\"",
            "id=\"method.decoder\"",
            "id=\"method.staging\"",
            "id=\"method.record_bytes\"",
            "id=\"method.into_inner\"",
        ] {
            assert!(
                !record_stream.contains(accessor) && !record_batches.contains(accessor),
                "isolated `{features}` Rustdoc exposed record data accessor {accessor}"
            );
        }
    }
}
