use super::helpers::{analyze_ok, scope_module_path, single_endpoint};
use crate::model::Scheme;
use crate::sema::PathPiece;

#[test]
fn route_resolution_lowers_endpoint_method_path_and_identity() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Show as show
                path ["health"]
                -> Json<()>
        }
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert_eq!(endpoint.method.to_string(), "GET");
    assert_eq!(endpoint.name.to_string(), "Show");
    assert_eq!(
        endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
        Some("show")
    );
    assert!(scope_module_path(endpoint).is_empty());
    assert!(endpoint.prefix_pieces.is_empty());
    assert!(endpoint.scope_path_pieces.is_empty());
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::Static(route)] if route == "health"
    ));
}

#[test]
fn route_resolution_lowers_client_base() {
    let api = analyze_ok(
        r#"
        api! {
            client RouteApi {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                -> Json<()>
        }
        "#,
    );

    assert!(matches!(api.scheme, Scheme::Https));
    assert_eq!(api.domain.value(), "example.com");
}
