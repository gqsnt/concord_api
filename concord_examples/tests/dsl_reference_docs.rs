fn workspace_doc(path: &str) -> String {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();

    std::fs::read_to_string(workspace.join(path)).expect("read workspace doc")
}

#[test]
fn dsl_reference_mentions_public_v1_keyword_groups() {
    let dsl = workspace_doc("docs/dsl.md");

    for snippet in [
        "api! {",
        "client Name",
        "base \"https://api.example.com\"",
        "scope name(args...)",
        "host [...]",
        "path [...]",
        "GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS",
        "auth {",
        "policies {",
        "behaviors {",
        "defaults {",
        "default { ... }",
        "var name: Type",
        "secret name: Type",
        "cursor?: String",
        "count: u64 = 20",
        "region?: String =",
        "body: Json<",
    ] {
        assert!(
            dsl.contains(snippet),
            "docs/dsl.md should mention `{snippet}`"
        );
    }
}

#[test]
fn dsl_reference_mentions_auth_retry_cache_and_rate_limit_surface() {
    let dsl = workspace_doc("docs/dsl.md");
    let auth = workspace_doc("docs/auth.md");
    let policies = workspace_doc("docs/cache_retry_rate_limit.md");
    let combined = format!("{dsl}\n{auth}\n{policies}");

    for snippet in [
        "credential api = api_key",
        "credential session = bearer",
        "credential login = basic",
        "oauth2_client",
        "credential acquired = endpoint",
        "auth header",
        "auth query",
        "auth basic",
        "auth certificate",
        "max_attempts",
        "on transport",
        "retry_after",
        "idempotency header",
        "cache only",
        "cache off",
        "cache http",
        "cache stale_on_error",
        "capacity 10_000 entries",
        "max_body N bytes|kb|kib|mb|mib|gb|gib",
        "shared",
        "kib",
        "on_error ignore",
        "on_error serve_stale",
        "rate_limit only",
        "rate_limit off",
        "cost",
        "observe rate_limit",
        "rate_limit key",
        "duplicate",
        "empty",
        "same behavior also cannot be attached more than once",
        "Reusing a behavior across separate layers remains valid",
    ] {
        assert!(
            combined.contains(snippet),
            "public docs should mention `{snippet}`"
        );
    }
}

#[test]
fn dsl_reference_mentions_endpoint_policy_and_pagination_surface() {
    let dsl = workspace_doc("docs/dsl.md");

    for snippet in [
        "query {",
        "headers {",
        "query \"tenant\" = tenant_id",
        "query \"tag\" += tag",
        "header \"X-Request-Id\" = request_id",
        "fmt[",
        "timeout",
        "as alias",
        "paginate Controller",
        "OffsetLimitPagination",
        "CursorPagination",
        "PageItems",
        "HasNextCursor",
        "map Type",
    ] {
        assert!(
            dsl.contains(snippet),
            "docs/dsl.md should mention `{snippet}`"
        );
    }
}

#[test]
fn dsl_reference_marks_reserved_or_unsupported_syntax() {
    let dsl = workspace_doc("docs/dsl.md");

    for snippet in [
        "Unsupported or reserved syntax",
        "body ...",
        "params { ... }",
        "prefix",
        "part[...]",
        "auth none",
        "access_token",
    ] {
        assert!(
            dsl.contains(snippet),
            "docs/dsl.md should mention `{snippet}`"
        );
    }
}

#[test]
fn compiled_advanced_example_covers_less_common_public_syntax() {
    let source = include_str!("../src/docs_advanced_dsl.rs");

    for snippet in [
        "credential basic_login = basic",
        "credential oauth_session = oauth2_client",
        "on transport [Timeout, Connect]",
        "idempotency header \"Idempotency-Key\"",
        "cache short",
        "on_error ignore",
        "capacity 1_000 entries",
        "max_body 512 kib",
        "shared",
        "bucket method by [host, endpoint, method, \"tenant\", tenant_key]",
        "cost 2",
        "observe rate_limit AdvancedRateLimitHeaders",
        "header \"X-Request-Id\" = request_id",
        "query \"tenant\" = tenant_id",
        "cache stale_on_error",
        "rate_limit only tenant",
    ] {
        assert!(
            source.contains(snippet),
            "docs_advanced_dsl.rs should contain `{snippet}`"
        );
    }
}
