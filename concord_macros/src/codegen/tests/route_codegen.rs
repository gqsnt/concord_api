use super::helpers::*;
use quote::quote;

#[test]
fn generated_route_builds_resolved_route_from_semantic_pieces() {
    let out = expanded(quote! {
        client RoutePlanApi {
            base "https://example.com"
            var region: String
        }

        scope regional {
            host [vars.region, "api"]
            path ["v1"]

            GET Show(id: String)
                path ["items", id]
                -> Json<String>
        }
    });

    assert_contains_all(
        &out,
        &[
            "let mut route = < super :: RoutePlanApiCx as :: concord_core :: prelude :: ClientContext > :: base_route (vars , __concord_auth_vars)",
            "route.host_mut().push",
            "route.path_mut().push_raw(\"v1\")",
            "route.path_mut().push_raw(\"items\")",
            "route.path_mut().push_segment_encoded(&__segment)",
            "route.host().validate(ctx_err.clone())",
            "scheme : < super :: RoutePlanApiCx as :: concord_core :: prelude :: ClientContext > :: SCHEME",
            "host : route.host().join",
            "path : route.path().as_str().to_string()",
        ],
    );
}

#[test]
fn static_path_slash_behavior_characterized() {
    let out = expanded(quote! {
        client StaticPathSlashApi {
            base "https://example.com"
        }

        GET Show
            path ["a/b"]
            -> Json<String>
    });

    assert_contains_all(&out, &["route.path_mut().push_raw(\"a/b\")"]);
}

#[test]
fn generated_route_rejects_dynamic_slash_segments() {
    let out = expanded(quote! {
        client SnapshotRouteGuard {
            base "https://example.com"
        }

        GET Show(id: String, prefix?: String)
            as show
            path ["users", id, fmt["p-", prefix]]
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "__segment.is_empty()",
            "__segment.contains('/')",
            "__segment.contains('\\\\')",
            "ApiClientError::invalid_param(ctx.clone()",
            "route.path_mut().push_segment_encoded(&__segment)",
        ],
    );
}

#[test]
fn generated_query_contains_optional_and_empty_string_semantics() {
    let out = expanded(quote! {
        client SnapshotQueryPolicy {
            base "https://example.com"
        }

        GET Search(maybe?: String)
            as search
            path ["search"]
            query {
                "maybe" = maybe,
                "empty" = ""
            }
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "if let ::core::option::Option::Some(__v) = ep.maybe.as_ref()",
            "policy.remove_query(\"maybe\")",
            "policy.set_query(\"empty\",(\"\").to_string())",
        ],
    );
}

#[test]
fn generated_query_vectors_use_resolved_replacement_operations() {
    let out = expanded(quote! {
        client VectorQueryCodegen {
            base "https://example.com"
        }

        GET Search(tags: Vec<String>, optional_tags?: Vec<String>)
            as search
            path ["search"]
            query {
                tags
                optional_tags
            }
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "policy.replace_query_values (\"tags\"",
            ".iter () . map (| __v | __v.to_string ())",
            "if let ::core::option::Option::Some(__v) = ep.optional_tags.as_ref()",
            "__v.iter () . map (| __item | __item.to_string ())",
            "policy.remove_query(\"optional_tags\")",
        ],
    );
}

#[test]
fn generated_query_uses_exact_standard_vec_shapes_only() {
    let out = expanded(quote! {
        client ExactVectorShapes {
            base "https://example.com"
        }

        GET Search(
            plain: Vec<String>,
            qualified: std::vec::Vec<String>,
            absolute: ::std::vec::Vec<String>,
            custom: custom::Vec<String>
        )
            as search
            path ["search"]
            query {
                plain
                qualified
                absolute
                custom
            }
            -> Json<String>
    });

    assert_eq!(out.matches("replace_query_values").count(), 3);
}
