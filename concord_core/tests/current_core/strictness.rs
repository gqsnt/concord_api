fn workspace_file(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core crate has workspace parent")
        .to_path_buf();
    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace file {path}: {err}"))
}

#[test]
fn source_does_not_reintroduce_unknown_host_fallback() {
    let source = workspace_file("concord_core/src/rate_limit/governor_runtime.rs");
    let forbidden = concat!("unknown", "-", "host");
    assert!(
        !source.contains(forbidden),
        "rate-limit keying must not invent unknown host fallback values"
    );
}

#[test]
fn semantic_runtime_code_does_not_use_saturating_arithmetic() {
    for path in [
        "concord_core/src/client/execute.rs",
        "concord_core/src/client/auth_http.rs",
        "concord_core/src/auth/credentials.rs",
        "concord_core/src/rate_limit/governor_runtime.rs",
        "concord_macros/src/sema/cache.rs",
    ] {
        let source = workspace_file(path);
        for forbidden in [
            "saturating_add",
            "saturating_mul",
            "saturating_sub",
            "saturating_div",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} must not use silent saturating arithmetic for semantic runtime/config values"
            );
        }
    }
}

#[test]
fn credential_generation_overflow_has_no_runtime_expect() {
    let source = workspace_file("concord_core/src/auth/credentials.rs");
    assert!(
        !source.contains("expect(\"credential generation counter overflowed\")"),
        "credential generation overflow must return a typed AuthError, not panic"
    );
}
