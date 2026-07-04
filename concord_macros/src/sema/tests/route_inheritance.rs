use super::helpers::{analyze_ok, endpoint_by_name, scope_module_path};
use crate::sema::{PathPiece, PrefixPiece};

#[test]
fn route_inheritance_combines_scope_path_layers_in_order() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope users {
                path ["users"]

                scope posts {
                    path ["posts"]

                    GET Show(post_id: String)
                        path [post_id]
                        -> Json<()>
                }
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Show");

    assert_eq!(
        scope_module_path(endpoint),
        vec!["users".to_string(), "posts".to_string()]
    );
    assert!(endpoint.prefix_pieces.is_empty());
    assert!(matches!(
        endpoint.scope_path_pieces.as_slice(),
        [PathPiece::Static(users), PathPiece::Static(posts)]
            if users == "users" && posts == "posts"
    ));
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::EpVar { field }] if field.to_string() == "post_id"
    ));
}

#[test]
fn route_inheritance_combines_host_and_path_scopes() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope tenant {
                host ["tenant"]

                scope users {
                    path ["users"]

                    GET Show(post_id: String)
                        path [post_id]
                        -> Json<()>
                }
            }
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Show");

    assert_eq!(
        scope_module_path(endpoint),
        vec!["tenant".to_string(), "users".to_string()]
    );
    assert!(matches!(
        endpoint.prefix_pieces.as_slice(),
        [PrefixPiece::Static(prefix)] if prefix == "tenant"
    ));
    assert!(matches!(
        endpoint.scope_path_pieces.as_slice(),
        [PathPiece::Static(path)] if path == "users"
    ));
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::EpVar { field }] if field.to_string() == "post_id"
    ));
}
