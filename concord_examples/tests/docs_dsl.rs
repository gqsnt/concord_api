fn source_contains_in_order(source: &str, snippets: &[&str]) -> bool {
    let mut search_from = 0;

    for snippet in snippets {
        let Some(relative) = source[search_from..].find(snippet) else {
            return false;
        };
        search_from += relative + snippet.len();
    }

    true
}

fn dsl_docs() -> &'static str {
    include_str!("../../docs/dsl.md")
}

#[test]
fn docs_dsl_example_uses_grouped_config_and_response_last_order() {
    let source = include_str!("../src/docs_dsl.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "auth {",
            "policies {",
            "behaviors {",
            "defaults {",
            "behavior read",
            "GET GetMatch",
            "rate_limit key match_key = match_id",
            "behavior match_read",
            "-> Json<MatchDto>",
        ],
    ));
}

#[test]
fn dsl_docs_link_compiled_guide_example() {
    let docs = dsl_docs();

    assert!(docs.contains("concord_examples/src/docs_dsl.rs"));
}

#[test]
fn dsl_docs_and_compiled_example_share_grouped_config_vocabulary() {
    let docs = dsl_docs();
    let source = include_str!("../src/docs_dsl.rs");

    for snippet in [
        "auth {",
        "policies {",
        "behaviors {",
        "defaults {",
        "behavior read",
        "rate_limit key",
        "fmt[",
        "query {",
        "headers {",
        "-> Json<",
    ] {
        assert!(
            docs.contains(snippet),
            "docs/dsl.md should mention `{snippet}`"
        );
        assert!(
            source.contains(snippet),
            "docs_dsl.rs should contain `{snippet}`"
        );
    }
}

#[test]
fn dsl_docs_cover_compiled_body_mapping_and_pagination_shapes() {
    let docs = dsl_docs();
    let source = include_str!("../src/docs_dsl.rs");

    for snippet in [
        "body: Json<",
        "map ",
        "paginate OffsetLimitPagination",
        "paginate CursorPagination",
        "region?: String =",
    ] {
        assert!(
            docs.contains(snippet),
            "docs/dsl.md should mention `{snippet}`"
        );
        assert!(
            source.contains(snippet),
            "docs_dsl.rs should contain `{snippet}`"
        );
    }
}

#[test]
fn dsl_docs_cover_behavior_extends_and_optional_defaults() {
    let docs = dsl_docs();
    let source = include_str!("../src/docs_dsl.rs");

    assert!(docs.contains("extends"));
    assert!(docs.contains("Some(default)"));
    assert!(docs.contains("region?: String ="));
    assert!(source.contains("region?: String ="));
}

#[test]
fn dsl_docs_and_compiled_example_show_response_last_order() {
    let docs = dsl_docs();
    let source = include_str!("../src/docs_dsl.rs");

    assert!(source_contains_in_order(
        docs,
        &[
            "GET EndpointName(args...)",
            "behavior ...",
            "cache/retry/rate_limit/auth ...",
            "-> Json<Response>",
        ],
    ));

    assert!(source_contains_in_order(
        source,
        &[
            "GET GetMatch",
            "rate_limit key match_key = match_id",
            "behavior match_read",
            "-> Json<MatchDto>",
        ],
    ));
}

#[test]
fn docs_dsl_example_covers_body_mapping_and_pagination_shapes() {
    let source = include_str!("../src/docs_dsl.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "POST CreateUser(body: Json<CreateUser>)",
            "-> Json<User>",
            "POST Login(body: Json<LoginRequest>)",
            "-> Json<LoginResponse>",
            "map GuideAccessToken",
            "GET ListItems",
            "paginate OffsetLimitPagination",
            "GET ListCursor",
            "paginate CursorPagination",
        ],
    ));
}
