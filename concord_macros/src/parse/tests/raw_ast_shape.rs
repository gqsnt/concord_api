use super::helpers::{endpoint_at_top_level, endpoint_in_scope, parse_ok, scope_at_top_level};
use crate::ast::{FmtPiece, KeySpec, PolicyStmt, PolicyValue, RouteAtom};
use syn::Expr;

#[test]
fn parses_current_api_into_raw_ast_with_endpoint_line_metadata() {
    let ast = parse_ok(
        r#"
        client Api {
            base "https://example.com"
        }

        scope users(id: u64) {
            path ["users", id]

            GET Show
                as show
                path ["profile"]
                query {
                    id
                }
                -> Json<String>
        }
        "#,
    );

    assert_eq!(ast.client.name, "Api");
    assert_eq!(ast.items.len(), 1);
    let scope = scope_at_top_level(&ast, 0);
    assert_eq!(
        scope
            .scope_name
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("users")
    );
    assert_eq!(scope.items.len(), 1);

    let endpoint = endpoint_in_scope(scope, 0);
    assert_eq!(endpoint.line.method, "GET");
    assert_eq!(endpoint.line.name, "Show");
    assert_eq!(
        endpoint
            .line
            .alias
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("show")
    );
    assert!(endpoint.policy.query.is_some());
}

#[test]
fn parses_custom_paginate_syntax() {
    let ast = parse_ok(
        r#"
        client Api {
            base "https://example.com"
        }

        GET List
            path ["items"]
            paginate HeaderPagePagination
            -> Json<Vec<String>>
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    let paginate = endpoint.paginate.as_ref().expect("paginate");
    let ctrl_ty = &paginate.ctrl_ty;
    assert_eq!(quote::quote!(#ctrl_ty).to_string(), "HeaderPagePagination");
    assert!(paginate.assigns.is_empty());
}

#[test]
fn parses_custom_paginate_syntax_with_assignments() {
    let ast = parse_ok(
        r#"
        client Api {
            base "https://example.com"
        }

        GET List(page: u64 = 1, count: u64 = 2)
            path ["items"]
            paginate HeaderPagePagination {
                page = page,
                count = count
            }
            -> Json<Vec<String>>
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    let paginate = endpoint.paginate.as_ref().expect("paginate");
    let ctrl_ty = &paginate.ctrl_ty;
    assert_eq!(quote::quote!(#ctrl_ty).to_string(), "HeaderPagePagination");
    assert_eq!(paginate.assigns.len(), 2);
    assert_eq!(paginate.assigns[0].key.to_string(), "page");
    assert_eq!(paginate.assigns[1].key.to_string(), "count");
}

#[test]
fn parses_cursor_paginate_with_explicit_string_type() {
    let ast = parse_ok(
        r#"
        client Api {
            base "https://example.com"
        }

        GET List(cursor?: String, count: u64 = 2)
            path ["items"]
            paginate CursorPagination<String> {
                cursor = cursor,
                per_page = count,
                send_cursor_on_first = true,
                stop_when_cursor_missing = false
            }
            -> Json<Vec<String>>
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    let paginate = endpoint.paginate.as_ref().expect("paginate");
    let ctrl_ty = &paginate.ctrl_ty;
    assert_eq!(
        quote::quote!(#ctrl_ty).to_string(),
        "CursorPagination < String >"
    );
    assert_eq!(paginate.assigns.len(), 4);
}

#[test]
fn parses_scope_with_host_and_path_preserves_raw_shape() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope tenant(tenant_id: String) {
                host [fmt["tenant-", tenant_id], "api"]
                path ["v1"]

                GET Ping
                    path ["ping"]
                    -> Json<String>
            }
        }
        "#,
    );

    let scope = scope_at_top_level(&ast, 0);
    let host = scope.host_route.as_ref().expect("host route");
    let path = scope.path_route.as_ref().expect("path route");
    match (&host.atoms[..], &path.atoms[..]) {
        (
            [RouteAtom::Fmt(host_fmt), RouteAtom::Static(host_tail)],
            [RouteAtom::Static(path_atom)],
        ) => {
            assert_eq!(host_tail.value(), "api");
            assert_eq!(path_atom.value(), "v1");
            assert_eq!(host_fmt.pieces.len(), 2);
            assert!(matches!(host_fmt.pieces[0], FmtPiece::Lit(_)));
            assert!(matches!(host_fmt.pieces[1], FmtPiece::Ref(_)));
        }
        other => panic!("expected raw route atoms, got {other:?}"),
    }

    let endpoint = endpoint_in_scope(scope, 0);
    assert_eq!(endpoint.line.name, "Ping");
    match endpoint.route.atoms.as_slice() {
        [RouteAtom::Static(atom)] => assert_eq!(atom.value(), "ping"),
        other => panic!("expected raw endpoint route atom, got {other:?}"),
    }
}

#[test]
fn endpoint_clauses_before_and_after_response_parse() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search(q: String, page?: u32, count: u32 = 20)
                path ["search"]
                -> Json<String>
                query {
                    q
                    page
                    count
                }
                timeout 10
        }
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    assert_eq!(endpoint.params.len(), 3);
    assert!(endpoint.policy.query.is_some());
    assert!(endpoint.policy.timeout.is_some());
}

#[test]
fn fmt_passes_in_host_path_query_and_header_contexts() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var trace_id: String
            }

            scope tenant(tenant_id: String) {
                host [fmt["tenant-", tenant_id], "api"]
                path [fmt["tenant-", tenant_id]]

                GET Search(q: String)
                    path ["search"]
                    headers {
                        "x-trace" = fmt["trace-", vars.trace_id]
                    }
                    query {
                        "q" = fmt["prefix:", q]
                    }
                    -> Json<String>
            }
        }
        "#,
    );

    let scope = scope_at_top_level(&ast, 0);
    match scope
        .host_route
        .as_ref()
        .expect("host route")
        .atoms
        .as_slice()
    {
        [RouteAtom::Fmt(fmt), RouteAtom::Static(tail)] => {
            assert_eq!(tail.value(), "api");
            assert_eq!(fmt.pieces.len(), 2);
            assert!(matches!(fmt.pieces[0], FmtPiece::Lit(_)));
            assert!(matches!(fmt.pieces[1], FmtPiece::Ref(_)));
        }
        other => panic!("expected raw host fmt route, got {other:?}"),
    }
    match scope
        .path_route
        .as_ref()
        .expect("path route")
        .atoms
        .as_slice()
    {
        [RouteAtom::Fmt(fmt)] => assert_eq!(fmt.pieces.len(), 2),
        other => panic!("expected raw path fmt route, got {other:?}"),
    }

    let endpoint = endpoint_in_scope(scope, 0);
    match endpoint
        .policy
        .headers
        .as_ref()
        .expect("headers")
        .stmts
        .as_slice()
    {
        [
            PolicyStmt::Set {
                key: KeySpec::Str(key),
                value: PolicyValue::Fmt(fmt),
            },
        ] => {
            assert_eq!(key.value(), "x-trace");
            assert_eq!(fmt.pieces.len(), 2);
        }
        other => panic!("expected raw header fmt value, got {other:?}"),
    }
    match endpoint
        .policy
        .query
        .as_ref()
        .expect("query")
        .stmts
        .as_slice()
    {
        [
            PolicyStmt::Set {
                key: KeySpec::Str(key),
                value: PolicyValue::Fmt(fmt),
            },
        ] => {
            assert_eq!(key.value(), "q");
            assert_eq!(fmt.pieces.len(), 2);
        }
        other => panic!("expected raw query fmt value, got {other:?}"),
    }
}

#[test]
fn endpoint_response_after_response_parses() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            POST Login(body: Json<LoginResponse>)
                path ["login"]
                -> Json<LoginResponse>
        }
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    assert!(endpoint.body.is_some());
    assert_eq!(
        endpoint.response.marker,
        syn::parse_quote!(Json<LoginResponse>)
    );
    assert!(endpoint.response.had_angle_args);
}

#[test]
fn query_and_header_policy_operations_parse_in_order() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Search(q: String, cursor?: String, trace_id: String)
                path ["search"]
                query {
                    q
                    "cursor" = cursor
                    "tag" = q,
                    -"old"
                }
                headers {
                    "x-trace" = trace_id,
                    -"x-old"
                }
                -> Json<String>
        }
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    let query = endpoint.policy.query.as_ref().expect("query block");
    let headers = endpoint.policy.headers.as_ref().expect("headers block");
    assert_eq!(query.stmts.len(), 4);
    assert_eq!(headers.stmts.len(), 2);

    match &query.stmts[0] {
        PolicyStmt::Set {
            key: KeySpec::Ident(key),
            value: PolicyValue::Expr(Expr::Path(path)),
        } => {
            assert_eq!(key.to_string(), "q");
            assert_eq!(path.path.segments.len(), 1);
            assert_eq!(path.path.segments[0].ident, "q");
        }
        other => panic!("query shorthand should remain raw syntax: {other:?}"),
    }

    match &query.stmts[2] {
        PolicyStmt::Set {
            key: KeySpec::Str(key),
            ..
        } => assert_eq!(key.value(), "tag"),
        other => panic!("query assignment should remain raw syntax: {other:?}"),
    }

    match &query.stmts[3] {
        PolicyStmt::Remove {
            key: KeySpec::Str(key),
        } => assert_eq!(key.value(), "old"),
        other => panic!("query removal should remain raw syntax: {other:?}"),
    }

    match &headers.stmts[0] {
        PolicyStmt::Set {
            key: KeySpec::Str(key),
            value: PolicyValue::Expr(Expr::Path(path)),
        } => {
            assert_eq!(key.value(), "x-trace");
            assert_eq!(path.path.segments[0].ident, "trace_id");
        }
        other => panic!("header assignment should remain raw syntax: {other:?}"),
    }

    match &headers.stmts[1] {
        PolicyStmt::Remove {
            key: KeySpec::Str(key),
        } => assert_eq!(key.value(), "x-old"),
        other => panic!("header removal should remain raw syntax: {other:?}"),
    }
}
