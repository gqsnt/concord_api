use super::helpers::{normalize_ok, top_endpoint, top_scope};
use crate::ast::{AuthUseKind, KeySpec, PolicyStmt, PolicyValue, RouteAtom};
use crate::model::norm::NormNode;
use quote::ToTokens;
use syn::Expr;

#[test]
fn normalized_tree_preserves_client_and_top_level_endpoint_fields() {
    let norm = normalize_ok(
        r#"
        api! {
            client NormApi {
                base "https://example.com"
                var tenant: String
                secret token: String
                credential key = api_key(secret.token)

                default {
                    retry read
                    rate_limit api
                }

                retry read {
                    max_attempts 2
                }

                rate_limit api {
                    bucket request by [endpoint] {
                        10 / 1s
                    }
                }

                auth bearer key
            }

            GET Show(id: u64, body: Json<ShowBody>)
                as show
                path ["show", id]
                query {
                    id
                }
                headers {
                    "x-trace" = vars.tenant
                }
                retry read
                rate_limit api
                -> Json<ShowResponse>
        }
        "#,
    );

    assert_eq!(norm.client.name, "NormApi");
    assert_eq!(norm.client.host.value(), "example.com");
    assert_eq!(norm.client.vars.as_ref().expect("vars").decls.len(), 1);
    assert_eq!(
        norm.client.auth_vars.as_ref().expect("secrets").decls.len(),
        1
    );
    assert_eq!(
        norm.client
            .auth
            .as_ref()
            .expect("credentials")
            .credentials
            .len(),
        1
    );
    assert!(norm.client.default_behavior_uses.is_empty());
    assert!(norm.client.retry_profiles.is_some());
    assert!(norm.client.rate_limit.is_some());
    assert_eq!(norm.items.len(), 1);

    let endpoint = top_endpoint(&norm, 0);
    assert_eq!(endpoint.method, "GET");
    assert_eq!(endpoint.name, "Show");
    assert_eq!(
        endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
        Some("show")
    );
    assert_eq!(endpoint.params.len(), 1);
    assert!(endpoint.body.is_some());
    assert_eq!(
        endpoint.response.marker,
        syn::parse_quote!(Json<ShowResponse>)
    );
    assert!(matches!(
        endpoint.route.atoms.as_slice(),
        [RouteAtom::Static(_), RouteAtom::Ref(_)]
    ));
    assert!(endpoint.policy.query.is_some());
    assert!(endpoint.policy.headers.is_some());
    assert!(endpoint.retry.is_some());
    assert!(endpoint.rate_limit.is_some());
    assert!(endpoint.paginate.is_none());

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
                key: KeySpec::Ident(key),
                value: PolicyValue::Expr(Expr::Path(path)),
            },
        ] => {
            assert_eq!(key.to_string(), "id");
            assert_eq!(path.path.segments[0].ident, "id");
        }
        other => panic!("unexpected query policy shape: {other:?}"),
    }

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
                value: PolicyValue::Expr(Expr::Field(field)),
            },
        ] => {
            assert_eq!(key.value(), "x-trace");
            match field.base.as_ref() {
                Expr::Path(path) => assert_eq!(path.path.segments[0].ident, "vars"),
                other => panic!("unexpected header value base: {other:?}"),
            }
            assert_eq!(field.member.to_token_stream().to_string(), "tenant");
        }
        other => panic!("unexpected header policy shape: {other:?}"),
    }
}

#[test]
fn normalized_tree_contains_only_canonical_endpoint_constructs() {
    let norm = normalize_ok(
        r#"
        api! {
            client NormCanonical {
                base "https://example.com"
                var trace_id: String
            }

            POST Create(q: String, tag?: String, body: Json<CreateBody>)
                as create
                path [fmt["items-", q]]
                query {
                    q
                    "tag" = tag
                }
                headers {
                    "x-trace" = vars.trace_id
                }
                paginate HeaderPagePagination
                -> Json<CreateResponse>
        }
        "#,
    );

    let endpoint = top_endpoint(&norm, 0);
    assert_eq!(endpoint.method, "POST");
    assert_eq!(endpoint.name, "Create");
    assert_eq!(
        endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
        Some("create")
    );
    assert_eq!(endpoint.params.len(), 2);
    assert!(matches!(
        endpoint.route.atoms.as_slice(),
        [RouteAtom::Fmt(_)]
    ));
    assert!(endpoint.body.is_some());
    assert!(endpoint.paginate.is_some());

    let query = endpoint.policy.query.as_ref().expect("query policy");
    assert_eq!(query.stmts.len(), 2);
    match &query.stmts[0] {
        PolicyStmt::Set {
            key: KeySpec::Ident(key),
            value: PolicyValue::Expr(Expr::Path(path)),
        } => {
            assert_eq!(key.to_string(), "q");
            assert_eq!(path.path.segments[0].ident, "q");
        }
        other => panic!("unexpected normalized query shorthand: {other:?}"),
    }
    match &query.stmts[1] {
        PolicyStmt::Set {
            key: KeySpec::Str(key),
            ..
        } => assert_eq!(key.value(), "tag"),
        other => panic!("unexpected normalized query assignment: {other:?}"),
    }

    let headers = endpoint.policy.headers.as_ref().expect("headers policy");
    match &headers.stmts[0] {
        PolicyStmt::Set {
            key: KeySpec::Str(key),
            value: PolicyValue::Expr(Expr::Field(field)),
        } => {
            assert_eq!(key.value(), "x-trace");
            match field.base.as_ref() {
                Expr::Path(path) => assert_eq!(path.path.segments[0].ident, "vars"),
                other => panic!("unexpected header assignment base: {other:?}"),
            }
            assert_eq!(field.member.to_token_stream().to_string(), "trace_id");
        }
        other => panic!("unexpected normalized header assignment: {other:?}"),
    }
}

#[test]
fn normalized_tree_preserves_client_shape_without_raw_auth_groups() {
    let norm = normalize_ok(
        r#"
        api! {
            client NormApi {
                base "https://example.com"
                var tenant: String
                secret token: String
                credential key = api_key(secret.token)

                retry read {
                    max_attempts 2
                }

                rate_limit api {
                    bucket request by [endpoint] {
                        10 / 1s
                    }
                }

                auth bearer key
            }

            scope protected(user_id: u64) {
                path ["users", user_id]
                auth header "X-Token" = key

                GET Show(count: u64 = 20)
                    as show
                    path ["profile"]
                    query {
                        count
                    }
                    -> Json<String>
            }
        }
        "#,
    );

    assert_eq!(norm.client.name, "NormApi");
    assert!(norm.client.vars.is_some());
    assert!(norm.client.auth_vars.is_some());
    assert_eq!(norm.client.auth_uses.len(), 1);
    assert!(norm.client.retry_profiles.is_some());
    assert!(norm.client.rate_limit.is_some());

    let scope = top_scope(&norm, 0);
    assert_eq!(
        scope
            .scope_name
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("protected")
    );
    assert_eq!(scope.params.len(), 1);
    assert_eq!(scope.auth_uses.len(), 1);
    let endpoint = match scope.items.as_slice() {
        [NormNode::Endpoint(endpoint)] => endpoint,
        other => panic!("expected endpoint under normalized scope, got {other:?}"),
    };
    assert_eq!(endpoint.name, "Show");
    assert_eq!(
        endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
        Some("show")
    );
    assert_eq!(endpoint.params.len(), 1);
    assert_eq!(
        endpoint.policy.query.as_ref().expect("query").stmts.len(),
        1
    );

    let auth_use = &scope.auth_uses[0];
    match &auth_use.kind {
        AuthUseKind::Header { header, credential } => {
            assert_eq!(header.value(), "X-Token");
            assert_eq!(credential.to_string(), "key");
        }
        other => panic!("unexpected auth use in normalized scope: {other:?}"),
    }
}
