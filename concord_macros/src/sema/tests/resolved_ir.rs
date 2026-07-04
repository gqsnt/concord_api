use super::helpers::{analyze_ok, endpoint_by_name, single_endpoint, ty_string};
use crate::sema::{PathPiece, PrefixPiece};

#[test]
fn resolved_endpoint_debug_includes_inherited_tree_state() {
    let api = analyze_ok(
        r#"
        client Api {
            base "https://example.com"
            secret token: String
            credential key = api_key(secret.token)
        }

        scope protected {
            host ["tenant"]
            path ["v1"]
            auth header "X-Token" = key

            GET Me
                path ["me"]
                -> Json<()>
        }
        "#,
    );
    let endpoint = endpoint_by_name(&api, "Me");

    assert_eq!(endpoint.method, "GET");
    assert_eq!(
        endpoint
            .scope_modules
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["protected"]
    );
    assert_eq!(endpoint.policy.auth.len(), 1);
    assert!(matches!(
        endpoint.prefix_pieces.as_slice(),
        [PrefixPiece::Static(prefix)] if prefix == "tenant"
    ));
    assert!(matches!(
        endpoint.scope_path_pieces.as_slice(),
        [PathPiece::Static(path)] if path == "v1"
    ));
    assert!(matches!(
        endpoint.route_pieces.as_slice(),
        [PathPiece::Static(route)] if route == "me"
    ));
}

#[test]
fn body_signature_response_resolve_into_endpoint_model() {
    let api = analyze_ok(
        r#"
        client BodyApi {
            base "https://example.com"
        }

        POST Create(body: Json<CreateBody>)
            as create
            path ["items"]
            -> Json<CreateResponse>
        "#,
    );
    let endpoint = single_endpoint(&api);

    assert_eq!(endpoint.method, "POST");
    assert_eq!(
        endpoint.alias.as_ref().map(ToString::to_string).as_deref(),
        Some("create")
    );
    assert_eq!(
        ty_string(&endpoint.io.request_entity.adapter_ty),
        "::concord_core::advanced::EncodedRequest<Json<CreateBody>>"
    );
    assert_eq!(
        ty_string(
            endpoint
                .io
                .request_entity
                .public_input_ty
                .as_ref()
                .expect("public input type"),
        ),
        "CreateBody"
    );
    assert_eq!(
        ty_string(&endpoint.io.response_entity.adapter_ty),
        "::concord_core::advanced::BufferedResponse<Json<CreateResponse>>"
    );
    assert_eq!(
        ty_string(&endpoint.io.response_entity.public_output_ty),
        "CreateResponse"
    );
}
