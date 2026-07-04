use super::helpers::{analyze_ok, scope_module_path, single_endpoint};
use crate::sema::{FmtResolvedPiece, FmtVarSource, PathPiece};

#[test]
fn route_params_lowers_path_params_from_endpoint_fields() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show(post_id: String)
                path ["posts", post_id]
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert!(
        endpoint
            .vars
            .iter()
            .any(|var| var.rust.to_string() == "post_id")
    );
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::Static(prefix), PathPiece::EpVar { field }]
            if prefix == "posts" && field.to_string() == "post_id"
    ));
}

#[test]
fn route_params_resolve_optional_fmt_route_variables_as_optional() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show(prefix?: String)
                path [fmt["user-", prefix]]
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    let PathPiece::Fmt(fmt) = &endpoint.route_pieces[0] else {
        panic!("expected fmt route piece");
    };
    assert!(fmt.pieces.iter().any(|piece| matches!(
        piece,
        FmtResolvedPiece::Var {
            source: FmtVarSource::Ep,
            optional: true,
            ..
        }
    )));
}

#[test]
fn route_params_lowers_scope_and_endpoint_path_params() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            scope users(tenant_id: String) {
                path ["tenants", tenant_id]

                GET Show(user_id: String)
                    path ["users", user_id]
                    -> Json<()>
            }
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert_eq!(scope_module_path(endpoint), vec!["users".to_string()]);
    assert!(
        endpoint
            .vars
            .iter()
            .any(|var| var.rust.to_string() == "tenant_id")
    );
    assert!(
        endpoint
            .vars
            .iter()
            .any(|var| var.rust.to_string() == "user_id")
    );
    assert!(matches!(
        endpoint.scope_path_pieces.as_slice(),
        [PathPiece::Static(prefix), PathPiece::EpVar { field }]
            if prefix == "tenants" && field.to_string() == "tenant_id"
    ));
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::Static(prefix), PathPiece::EpVar { field }]
            if prefix == "users" && field.to_string() == "user_id"
    ));
}
