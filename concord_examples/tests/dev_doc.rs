fn workspace_doc(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path)).expect("read workspace doc")
}

fn assert_doc(path: &str, anchors: &[&str]) {
    let doc = workspace_doc(path);
    for anchor in anchors {
        assert!(doc.contains(anchor), "{path} should contain `{anchor}`");
    }
}

#[test]
fn dev_doc_files_exist_and_cover_architecture() {
    assert_doc(
        "dev_doc/README.md",
        &["Developer documentation", "architecture.md"],
    );
    assert_doc(
        "dev_doc/architecture.md",
        &["concord_macros", "concord_core", "concord_examples"],
    );
    assert_doc(
        "dev_doc/dsl_pipeline.md",
        &["raw parser AST", "semantic model", "codegen"],
    );
    assert_doc(
        "dev_doc/macro_parser.md",
        &[
            "auth { ... }",
            "policies { ... }",
            "response",
            "Vec<BehaviorUseSpec>",
        ],
    );
    assert_doc(
        "dev_doc/sema.md",
        &[
            "client defaults",
            "outer scopes",
            "endpoint",
            "max_body N unit",
            "Cross-layer reuse remains valid",
        ],
    );
    assert_doc(
        "dev_doc/codegen.md",
        &["facade", "endpoints::*", "request plan", "max body bytes"],
    );
    assert_doc(
        "dev_doc/core_runtime.md",
        &["fresh cache", "rate-limit observation", "decode"],
    );
    assert_doc(
        "dev_doc/policies_and_behaviors.md",
        &[
            "declarations from attachments",
            "behavior",
            "capacity entries",
            "same behavior more than once",
        ],
    );
    assert_doc(
        "dev_doc/auth_runtime.md",
        &[
            "credential slots",
            "endpoint-backed credentials",
            "`401 Unauthorized`",
            "`403 Forbidden`",
            "AuthChallengePolicy::NeverRefresh",
        ],
    );
    assert_doc(
        "dev_doc/pagination_and_codecs.md",
        &["BodyCodec", "PaginationController"],
    );
    assert_doc(
        "dev_doc/testing.md",
        &["trybuild", "New DSL feature checklist"],
    );
    assert_doc(
        "dev_doc/release_gate.md",
        &[
            "Local v1 release gate",
            "Core invariants",
            "Macro/codegen invariants",
            "Auth/redaction invariants",
            "Body-limit invariants",
            "./scripts/check_v1.sh",
            "does not package, publish, or run any crates.io step",
        ],
    );
    assert_doc(
        "dev_doc/release_checklist.md",
        &[
            "cargo clippy",
            "cargo doc",
            "Query auth secrets are redacted",
            "`401`/`403` auth rejection behavior",
            "Redaction tests cover debug output",
        ],
    );
}
