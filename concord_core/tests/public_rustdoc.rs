use std::path::{Path, PathBuf};

use concord_test_support::cargo_fixture::{diagnostics, run};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate should be inside the workspace")
        .to_owned()
}

#[test]
fn stable_request_terminals_are_semantic_and_private_in_rendered_rustdoc() {
    let root = workspace_root();
    let target_key = "core/public-rustdoc";
    let output = run(
        &root,
        target_key,
        "rustdoc",
        [
            "-p",
            "concord_core",
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
        .join("target/cargo-fixtures")
        .join(target_key)
        .join("doc/concord_core");
    let request_path = documentation.join("struct.Request.html");
    let response_path = documentation.join("struct.ResponseRequest.html");
    let request = std::fs::read_to_string(&request_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", request_path.display()));
    let response = std::fs::read_to_string(&response_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", response_path.display()));
    let multipart_body_path = documentation.join("advanced/struct.MultipartBody.html");
    let multipart_part_path = documentation.join("advanced/struct.MultipartPart.html");
    let multipart_factory_path = documentation.join("advanced/trait.MultipartFactory.html");
    let stream_response_path = documentation.join("advanced/struct.StreamResponse.html");
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
    let stream_response = std::fs::read_to_string(&stream_response_path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", stream_response_path.display())
    });
    let page_limit_path = documentation.join("enum.PageLimit.html");
    let paginated_request_path = documentation.join("struct.PaginatedRequest.html");
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
        "struct.OffsetPagination.html",
        "struct.PagePagination.html",
        "struct.CursorPagination.html",
        "trait.PaginationController.html",
        "trait.HasNextCursor.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("advanced").join(name);
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
        "struct.GovernorRateLimiter.html",
        "struct.DefaultRateLimitObserver.html",
        "struct.NoopRateLimiter.html",
        "struct.Cooldown.html",
        "enum.CooldownScope.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("advanced").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    })
    .collect::<Vec<_>>()
    .join("\n");
    let prepared_pages = [
        "struct.PreparedBuffered.html",
        "struct.PreparedBytes.html",
        "struct.PreparedNoContent.html",
        "struct.PreparedStream.html",
        "struct.PreparedEndpointBuilder.html",
        "struct.PreparedEndpoint.html",
    ]
    .into_iter()
    .map(|name| {
        let path = documentation.join("advanced").join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    })
    .collect::<Vec<_>>()
    .join("\n");
    let extension_contracts = [
        (
            "trait.MediaType.html",
            &[
                "concurrent operations",
                "no runtime callback",
                "non-sensitive",
                "mutable global state",
            ][..],
        ),
        (
            "trait.EncodeBody.html",
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
            "trait.DecodeBody.html",
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
            "trait.PaginationController.html",
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
            "trait.HasNextCursor.html",
            &[
                "sensitive",
                "logging and storage",
                "metric cardinality",
                "no external I/O",
                "concurrent operations",
                "outside the per-page operation deadline",
                "cancellation",
            ][..],
        ),
        (
            "trait.RateLimiter.html",
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
            "trait.RateLimitObserver.html",
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

    for expected in [
        "id=\"method.response\"",
        "Selects successful response metadata",
        "ResponseRequest",
        "buffered, bytes, or no-content",
    ] {
        assert!(
            request.contains(expected),
            "Request Rustdoc omitted the public response terminal fact {expected:?}"
        );
    }
    for expected in [
        "id=\"method.raw_response\"",
        "dangerous-raw-response",
        "complete bounded successful response",
        "Generated streaming requests and public",
        "prepared endpoints intentionally do not",
        "callers assume responsibility",
    ] {
        assert!(
            request.contains(expected),
            "Request Rustdoc omitted the gated raw terminal fact {expected:?}"
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
        !prelude.contains("RawResponse") && !prepared_pages.contains("RawResponse"),
        "raw response escaped its dangerous namespace or entered prepared docs"
    );
    for forbidden in [
        "RawResponseEndpoint",
        "DecodedResponse",
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
        stream_response.contains("successful response head together with its bounded lazy body"),
        "stream response Rustdoc omitted its bounded lazy semantics"
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
        "id=\"method.offset\"",
        "id=\"method.page_size\"",
        "id=\"method.page\"",
        "id=\"method.cursor\"",
        "id=\"method.validate\"",
        "id=\"tymethod.next\"",
        "id=\"method.short_page_size\"",
        "id=\"tymethod.next_cursor\"",
        "id=\"structfield.offset\"",
        "id=\"structfield.page_size\"",
        "id=\"structfield.page\"",
        "id=\"structfield.cursor\"",
        "moves from",
        "absent cursor from a present",
    ] {
        assert!(
            pagination_pages.contains(expected),
            "pagination Rustdoc omitted {expected:?}"
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
        "id=\"method.body_codec\"",
        "id=\"method.auth\"",
        "id=\"method.timeout\"",
        "id=\"method.disable_timeout\"",
        "id=\"method.rate_limit\"",
        "id=\"method.response\"",
        "id=\"method.response_bytes\"",
        "id=\"method.response_no_content\"",
        "id=\"method.response_stream\"",
        "id=\"method.build\"",
        "exactly one URL-encoded path segment",
        "Fields are intentionally private",
    ] {
        assert!(
            prepared_pages.contains(expected),
            "prepared Rustdoc omitted {expected:?}"
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
            !prepared_pages.contains(forbidden),
            "prepared Rustdoc exposed prohibited capability {forbidden:?}"
        );
    }
    for (name, expected) in extension_contracts {
        let path = documentation.join("advanced").join(name);
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
        "id=\"method.acquire\"",
        "id=\"method.observe\"",
        "id=\"method.quota_capacity\"",
        "id=\"method.cooldown_capacity\"",
        "id=\"method.idle_ttl\"",
        "id=\"method.key_limit\"",
        "best-effort process-local pacing",
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
        "preparation_key_limit",
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
        !prelude.contains("PreparedEndpoint")
            && !prelude.contains("PreparedEndpointBuilder")
            && !prelude.contains("PreparedNoContent"),
        "ordinary prelude unexpectedly exported prepared endpoint contracts"
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
                &stream_response,
                &page_limit,
                &paginated_request,
                &pagination_pages,
                &rate_pages,
                &prepared_pages,
                &raw_response,
                &raw_request,
            ]
            .iter()
            .all(|page| !page.contains(forbidden)),
            "stable public Rustdoc exposed private integration name {forbidden}"
        );
    }
}
