use super::helpers::{normalize_ok, single_child_endpoint, single_child_scope, top_scope};
use crate::ast::RouteAtom;
use crate::model::norm::RouteLayerKind;

#[test]
fn normalize_routes_splits_scope_host_and_path_in_order() {
    let norm = normalize_ok(
        r#"
        api! {
            client RouteSplitApi {
                base "https://example.com"
            }

            scope tenant(tenant_id: String) {
                host [fmt["tenant-", tenant_id], "api"]
                path ["v1"]

                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#,
    );

    assert_eq!(norm.items.len(), 1);
    let outer = top_scope(&norm, 0);
    assert!(matches!(outer.kind, RouteLayerKind::Prefix));
    match outer.route.atoms.as_slice() {
        [RouteAtom::Fmt(_), RouteAtom::Static(tail)] => assert_eq!(tail.value(), "api"),
        other => panic!("expected canonical host route atoms, got {other:?}"),
    }
    assert_eq!(outer.items.len(), 1);

    let inner = single_child_scope(outer);
    assert!(matches!(inner.kind, RouteLayerKind::Path));
    match inner.route.atoms.as_slice() {
        [RouteAtom::Static(atom)] => assert_eq!(atom.value(), "v1"),
        other => panic!("expected canonical path route atoms, got {other:?}"),
    }
    assert!(inner.scope_name.is_none());
    assert!(inner.params.is_empty());
    assert!(inner.policy.headers.is_none());
    assert!(inner.policy.query.is_none());
    assert!(inner.auth_uses.is_empty());
    assert!(inner.rate_limit.is_none());

    let endpoint = single_child_endpoint(inner);
    assert_eq!(endpoint.name, "Show");
    assert!(matches!(
        endpoint.route.atoms.as_slice(),
        [RouteAtom::Static(_)]
    ));
}

#[test]
fn normalize_routes_keeps_single_host_scope_as_prefix_layer() {
    let norm = normalize_ok(
        r#"
        api! {
            client RouteHostApi {
                base "https://example.com"
            }

            scope tenant {
                host ["tenant"]

                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#,
    );

    let outer = top_scope(&norm, 0);
    assert!(matches!(outer.kind, RouteLayerKind::Prefix));
    match outer.route.atoms.as_slice() {
        [RouteAtom::Static(atom)] => assert_eq!(atom.value(), "tenant"),
        other => panic!("expected single host route atom, got {other:?}"),
    }
    assert_eq!(outer.items.len(), 1);
    let endpoint = single_child_endpoint(outer);
    assert_eq!(endpoint.name, "Show");
}

#[test]
fn normalize_routes_keeps_single_path_scope_as_path_layer() {
    let norm = normalize_ok(
        r#"
        api! {
            client RoutePathApi {
                base "https://example.com"
            }

            scope tenant {
                path ["v1"]

                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#,
    );

    let outer = top_scope(&norm, 0);
    assert!(matches!(outer.kind, RouteLayerKind::Path));
    match outer.route.atoms.as_slice() {
        [RouteAtom::Static(atom)] => assert_eq!(atom.value(), "v1"),
        other => panic!("expected single path route atom, got {other:?}"),
    }
    assert!(outer.scope_name.as_ref().is_some());
    assert_eq!(outer.items.len(), 1);
    let endpoint = single_child_endpoint(outer);
    assert_eq!(endpoint.name, "Show");
}

#[test]
fn normalize_routes_creates_empty_path_layer_for_route_less_scope() {
    let norm = normalize_ok(
        r#"
        api! {
            client RouteEmptyApi {
                base "https://example.com"
            }

            scope tenant {
                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#,
    );

    let outer = top_scope(&norm, 0);
    assert!(matches!(outer.kind, RouteLayerKind::Path));
    assert!(outer.route.atoms.is_empty());
    assert_eq!(outer.items.len(), 1);
    let endpoint = single_child_endpoint(outer);
    assert_eq!(endpoint.name, "Show");
}

#[test]
fn normalize_route_layer_ownership_does_not_copy_outer_state_to_synthetic_path_layer() {
    let norm = normalize_ok(
        r#"
        api! {
            client RouteOwnedApi {
                base "https://example.com"
                secret token: String
                credential key = api_key(secret.token)
            }

            scope tenant(tenant_id: String) {
                host [fmt["tenant-", tenant_id]]
                path ["v1"]
                auth header "X-Token" = key
                query {
                    "trace" = tenant_id
                }
                headers {
                    "x-scope" = tenant_id
                }
                rate_limit api
                rate_limit key tenant_key = tenant_id
                profile audit

                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#,
    );

    let outer = top_scope(&norm, 0);
    assert_eq!(outer.params.len(), 1);
    assert_eq!(outer.auth_uses.len(), 1);
    assert!(outer.policy.headers.is_some());
    assert!(outer.policy.query.is_some());
    assert!(outer.rate_limit.is_some());
    assert_eq!(outer.rate_limit_keys.len(), 1);
    assert_eq!(outer.behavior_uses.len(), 1);

    let inner = single_child_scope(outer);
    assert!(inner.scope_name.is_none());
    assert!(inner.params.is_empty());
    assert!(inner.auth_uses.is_empty());
    assert!(inner.policy.headers.is_none());
    assert!(inner.policy.query.is_none());
    assert!(inner.rate_limit.is_none());
    assert!(inner.rate_limit_keys.is_empty());
    assert!(inner.behavior_uses.is_empty());
}
