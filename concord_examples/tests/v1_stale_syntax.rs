fn workspace_file(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

fn assert_absent(path: &str, haystack: &str, snippets: &[&str]) {
    for snippet in snippets {
        assert!(
            !haystack.contains(snippet),
            "{path} must not contain stale valid-syntax claim `{snippet}`"
        );
    }
}

#[test]
fn public_docs_and_examples_do_not_reintroduce_stale_v1_syntax_claims() {
    let public_paths = [
        "README.md",
        "docs/dsl.md",
        "docs/auth.md",
        "docs/cache_retry_rate_limit.md",
        "docs/pagination.md",
        "docs/generated_client.md",
        "docs/advanced_endpoints.md",
        "docs/README.md",
        "dev_doc/README.md",
        "dev_doc/architecture.md",
        "dev_doc/dsl_pipeline.md",
        "dev_doc/macro_parser.md",
        "dev_doc/sema.md",
        "dev_doc/codegen.md",
        "dev_doc/core_runtime.md",
        "dev_doc/policies_and_behaviors.md",
        "dev_doc/auth_runtime.md",
        "dev_doc/pagination_and_codecs.md",
        "dev_doc/testing.md",
        "dev_doc/release_checklist.md",
        "concord_examples/src/docs_dsl.rs",
        "concord_examples/src/docs_advanced_dsl.rs",
        "concord_examples/src/main.rs",
    ];

    let stale_valid_syntax_claims = [
        "body ... endpoint clause is supported",
        "params { ... } blocks are supported",
        "part[...] is supported",
        "prefix is public",
        "host [...] endpoint",
        "host endpoint clause",
        "reserved for future cache configuration",
        "capacity reserved",
        "max_body reserved",
        "shared reserved",
        "behavior [read, read] is valid",
        "behavior [] is valid",
        "rate_limit [] is valid",
        "same behavior can be attached more than once",
    ];

    for path in public_paths {
        let content = workspace_file(path);
        assert_absent(path, &content, &stale_valid_syntax_claims);
    }
}

#[test]
fn dsl_reference_states_final_v1_syntax_invariants() {
    let dsl = workspace_file("docs/dsl.md");

    for snippet in [
        "There is no `body ...` endpoint clause",
        "`params { ... }` blocks are not supported",
        "`part[...]` is not public syntax",
        "Cache sizing fields are runtime-backed",
        "capacity N entries",
        "max_body N bytes|kb|kib|mb|mib|gb|gib",
        "same behavior also cannot be attached more than once",
        "Reusing a behavior across separate layers remains valid",
        "`host` is scope-level v1 syntax",
        "`default { ... }` is accepted as an alias for `defaults { ... }`",
    ] {
        assert!(
            dsl.contains(snippet),
            "docs/dsl.md should contain `{snippet}`"
        );
    }
}
