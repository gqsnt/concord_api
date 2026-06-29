use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn check_v1_invokes_feature_matrix() {
    let script = read_repo_file("scripts/check_v1.sh");
    assert!(script.contains("set -euo pipefail"));
    assert!(script.contains("bash ./scripts/check_features.sh"));
    assert!(script.contains("nextest run -p concord_macros --test trybuild_current"));
    assert!(script.contains("nextest run -p concord_macros --test main"));
    assert!(script.contains("nextest run -p concord_core"));
    assert!(script.contains("nextest run -p concord_examples"));
    assert!(script.contains("nextest run --workspace --all-targets"));
    assert!(script.contains("RUSTDOCFLAGS=\"-D warnings\""));
}

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
        "pagination-loop-snapshot-behavior",
        "semantic-ir-codegen-diagnostics",
        "behavior-profile-semantic-only-sugar",
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
        "custom_codec",
        "custom_pagination",
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
        "execute_stream",
        "execute_records",
        "execute_multipart",
        "execute_sse",
        "execute_websocket",
        "StreamBody",
        "RecordBody",
        "MultipartBody",
        "SseStream",
        "WebSocketClient",
    ] {
        assert!(
            endpoint_io.contains(anchor),
            "endpoint I/O example should contain `{anchor}`"
        );
    }

    let minimal = read_repo_file("concord_examples/tests/integration/minimal.rs");
    assert!(minimal.contains(".execute()"));
    assert!(minimal.contains(".execute_decoded()"));

    let policy = read_repo_file("concord_examples/tests/integration/policy_stack.rs");
    assert!(policy.contains("rate_limiter"));
    assert!(policy.contains("retry_only"));

    let pagination = read_repo_file("concord_examples/tests/integration/pagination.rs");
    assert!(pagination.contains(".paginate("));
    assert!(pagination.contains("for_each_page"));

    let explicit = read_repo_file("concord_examples/src/explicit_endpoint.rs");
    assert!(explicit.contains("execute_raw"));
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
