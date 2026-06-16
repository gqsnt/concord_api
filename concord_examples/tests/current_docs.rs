use std::path::PathBuf;

#[test]
fn current_public_docs_exist() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    for rel in [
        "docs/quick_start.md",
        "docs/mental_model.md",
        "docs/design_invariants.md",
        "docs/dsl.md",
        "docs/generated_client.md",
        "docs/auth.md",
        "docs/pagination.md",
        "docs/customization.md",
        "docs/cache_retry_rate_limit.md",
        "docs/runtime_config.md",
        "docs/advanced_endpoints.md",
        "docs/internals.md",
    ] {
        let path = workspace.join(rel);
        assert!(
            path.is_file(),
            "missing current public doc: {}",
            path.display()
        );
    }
}

#[test]
fn quick_start_matches_minimal_example_surface() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();
    let quick_start =
        std::fs::read_to_string(workspace.join("docs/quick_start.md")).expect("read quick start");

    for expected in [
        "client MinimalApi",
        "GET GetUser(id: u64)",
        "as get_user",
        "let api = minimal_api::MinimalApi::new();",
        "api.users().get_user(42).await?",
        "execute_decoded().await?",
    ] {
        assert!(
            quick_start.contains(expected),
            "quick start should contain current minimal example fragment `{expected}`"
        );
    }
}
