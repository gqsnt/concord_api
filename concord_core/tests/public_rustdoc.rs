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
    let prelude = std::fs::read_to_string(documentation.join("prelude/index.html"))
        .expect("ordinary prelude Rustdoc");

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
    ] {
        assert!(
            [
                &request,
                &response,
                &multipart_body,
                &multipart_part,
                &multipart_factory,
                &stream_response,
                &rate_pages,
            ]
            .iter()
            .all(|page| !page.contains(forbidden)),
            "stable public Rustdoc exposed private integration name {forbidden}"
        );
    }
}
