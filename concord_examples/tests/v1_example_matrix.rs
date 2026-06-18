fn example(path: &str) -> String {
    std::fs::read_to_string(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|err| panic!("read example {path}: {err}"))
}

fn workspace_doc(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();
    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace doc {path}: {err}"))
}

fn assert_contains_all(label: &str, haystack: &str, snippets: &[&str]) {
    for snippet in snippets {
        assert!(
            haystack.contains(snippet),
            "{label} should contain `{snippet}`"
        );
    }
}

#[test]
fn v1_examples_cover_core_user_paths() {
    let minimal = example("src/minimal.rs");
    let minimal_tests = example("tests/minimal.rs");
    let explicit = example("src/explicit_endpoint.rs");
    let main = example("src/main.rs");

    assert_contains_all(
        "minimal example",
        &(minimal + &minimal_tests),
        &[
            "api.users().get_user(42).await",
            ".execute().await",
            ".execute_decoded().await",
        ],
    );
    assert_contains_all(
        "explicit endpoint example",
        &explicit,
        &[
            "endpoints::users::GetUser::new(42)",
            "api.request(endpoint).execute().await",
            "api.request(endpoint).execute_raw().await",
        ],
    );
    assert_contains_all(
        "examples binary",
        &main,
        &[
            "minimal",
            "docs_dsl",
            "docs_advanced_dsl",
            "auth_session",
            "pagination",
            "custom_pagination",
            "custom_codec",
            "policy_stack",
            "explicit_endpoint",
            "riot",
            "ddragon",
        ],
    );
}

#[test]
fn v1_examples_cover_auth_and_policy_surface() {
    let docs = example("src/docs_dsl.rs");
    let advanced = example("src/docs_advanced_dsl.rs");
    let auth = example("src/auth_session.rs");
    let policies = example("src/policy_stack.rs");
    let combined = [docs, advanced, auth, policies].join("\n");

    assert_contains_all(
        "auth and policy examples",
        &combined,
        &[
            "credential api_token = bearer",
            "credential basic_login = basic",
            "credential oauth_session = oauth2_client",
            "credential session = endpoint",
            "auth header",
            "auth query",
            "auth basic",
            "auth bearer",
            "max_attempts",
            "on [429, 500",
            "on transport [Timeout, Connect]",
            "retry_after",
            "idempotency header \"Idempotency-Key\"",
            "cache standard",
            "cache short",
            "ttl",
            "revalidate",
            "on_error ignore",
            "on_error serve_stale",
            "cache stale_on_error",
            "capacity 1_000 entries",
            "max_body 512 kib",
            "shared",
            "bucket",
            "cost 2",
            "\"tenant\"",
            "rate_limit key tenant_key = tenant_id",
            "observe rate_limit",
            "rate_limit only tenant",
            "behavior tenant_read",
        ],
    );
}

#[test]
fn v1_examples_cover_endpoint_and_pagination_surface() {
    let docs = example("src/docs_dsl.rs");
    let advanced = example("src/docs_advanced_dsl.rs");
    let pagination = example("src/pagination.rs");
    let pagination_tests = example("tests/pagination.rs");
    let custom_pagination = example("src/custom_pagination.rs");
    let custom_codec = example("src/custom_codec.rs");
    let combined = [
        docs,
        advanced,
        pagination,
        pagination_tests,
        custom_pagination,
        custom_codec,
    ]
    .join("\n");

    assert_contains_all(
        "endpoint and pagination examples",
        &combined,
        &[
            "headers {",
            "header \"X-Request-Id\" = request_id",
            "header \"X-Debug\" -",
            "query {",
            "query \"tenant\" = tenant_id",
            "query \"tag\" += tag",
            "query \"debug\" -",
            "timeout: std::time::Duration::from_secs(5)",
            "fmt[\"trace-\", trace_id]",
            "trace_id: String",
            "verbose?: bool",
            "start: u64 = 0",
            "region?: String =",
            "body: Json<CreateUser>",
            "map GuideAccessToken",
            "paginate OffsetLimitPagination",
            "paginate CursorPagination",
            "impl PaginationController",
            "impl BodyCodec",
            "impl ResponseCodec",
            ".paginate()",
        ],
    );
}

#[test]
fn v1_docs_cover_non_compiled_caveats_and_diagnostics() {
    let dsl = workspace_doc("docs/dsl.md");
    let auth = workspace_doc("docs/auth.md");
    let generated = workspace_doc("docs/generated_client.md");
    let combined = [dsl, auth, generated].join("\n");

    assert_contains_all(
        "public docs",
        &combined,
        &[
            "auth certificate",
            "runtime-provided credential material",
            "same behavior also cannot be attached more than once",
            "Reusing a behavior across separate layers remains valid",
            "behavior [read, protected_read]",
            "default {",
            "defaults {",
            ".execute_raw()",
        ],
    );
}
