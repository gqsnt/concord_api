use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn release_gate_documents_all_required_invariants() {
    let doc = read_repo_file("dev_doc/release_gate.md");
    for heading in [
        "body-auth-redaction-safety",
        "url-host-path-hardening",
        "body-limit-behavior",
        "feature-dependency-matrix",
        "runtimeconfig-defaults-precedence",
        "public-error-taxonomy-diagnostics",
        "deterministic-async-harness",
        "timeout-cancellation-drop-semantics",
        "concurrency-shared-state-isolation",
        "execute-raw-bypass-contract",
        "pagination-loop-determinism",
        "semantic-ir-codegen-diagnostics",
        "behavior-profile-semantic-only-sugar",
        "endpoint-io-contract-current",
    ] {
        assert!(
            doc.contains(heading),
            "release gate doc should contain invariant anchor `{heading}`"
        );
    }
}

#[test]
fn examples_cover_v1_usage_surface() {
    let examples_main = read_repo_file("concord_examples/tests/main.rs");
    for module in [
        "minimal",
        "auth_session",
        "policy_stack",
        "pagination",
        "custom_pagination",
        "custom_codec",
        "custom_cursor_pagination",
        "endpoint_io",
        "riot_large",
    ] {
        assert!(
            examples_main.contains(module),
            "examples test module `{module}` should be registered"
        );
    }

    let endpoint_io = read_repo_file("concord_examples/src/endpoint_io.rs");
    for anchor in [
        "Text<",
        "Text<String>",
        "Stream<OctetStream>",
        "Multipart<",
        "NoContent",
        "BytesResponse",
        "-> Bytes",
        "bytes::Bytes",
        "execute_stream",
        "execute_stream",
        "StreamBody",
        "MultipartBody",
    ] {
        assert!(
            endpoint_io.contains(anchor),
            "endpoint I/O example should contain `{anchor}`"
        );
    }

    let endpoint_docs = read_repo_file("docs/advanced_endpoints.md");
    for anchor in ["ContentType", "Stream<", "Multipart<", "execute_stream"] {
        assert!(
            endpoint_docs.contains(anchor),
            "advanced endpoint docs should contain `{anchor}`"
        );
    }

    let customization = read_repo_file("docs/customization.md");
    for anchor in ["try_content_type", "try_accept", "NoContent"] {
        assert!(
            customization.contains(anchor),
            "customization docs should contain `{anchor}`"
        );
    }

    let minimal = read_repo_file("concord_examples/tests/integration/minimal.rs");
    assert!(minimal.contains(".execute()"));
    assert!(minimal.contains(".response()"));

    let policy = read_repo_file("concord_examples/tests/integration/policy_stack.rs");
    assert!(policy.contains("rate_limiter"));
    assert!(policy.contains("retry_only"));

    let pagination = read_repo_file("concord_examples/tests/integration/pagination.rs");
    assert!(pagination.contains(".paginate("));
    assert!(pagination.contains(".collect()"));

    let explicit = read_repo_file("concord_examples/src/explicit_endpoint.rs");
    assert!(explicit.contains("execute_raw_response"));
}

fn read_repo_file(path: impl AsRef<Path>) -> String {
    let path = repo_root().join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("concord_core should have a workspace parent")
        .to_path_buf()
}
