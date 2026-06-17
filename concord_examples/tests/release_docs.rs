fn workspace_doc(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path)).expect("read workspace doc")
}

#[test]
fn release_docs_describe_current_grouped_config_model() {
    let dsl = workspace_doc("docs/dsl.md");
    let auth = workspace_doc("docs/auth.md");
    let policies = workspace_doc("docs/cache_retry_rate_limit.md");
    let mental_model = workspace_doc("docs/mental_model.md");

    assert!(dsl.contains("## Compatibility syntax"));
    assert!(auth.contains("auth {"));
    assert!(auth.contains("For compact examples"));
    assert!(policies.contains("policies {"));
    assert!(policies.contains("defaults {"));
    assert!(policies.contains("Flat `retry`, `cache`, and `rate_limit`"));
    assert!(mental_model.contains("behaviors {") || mental_model.contains("Behavior profiles"));
}

#[test]
fn examples_binary_mentions_compiled_public_dsl_guide() {
    let main_rs = include_str!("../src/main.rs");

    assert!(main_rs.contains("docs_dsl"));
}
