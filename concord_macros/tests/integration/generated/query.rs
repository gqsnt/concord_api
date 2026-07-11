use bytes::Bytes;
use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::{MockReply, assert_request, mock};

use self::query_vector_api::QueryVectorApi;
use self::vector_auth_api::VectorAuthApi;

mod custom {
    pub struct Vec<T>(pub T);

    impl<T: std::fmt::Display> std::fmt::Display for Vec<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }
}

api! {
    client QueryVectorApi {
        base "https://example.com"
        var inherited_query: String
        var inherited_tags: Vec<String>

        query {
            "query" = vars.inherited_query
            "tags" = vars.inherited_tags
            "maybe" = vars.inherited_query
            "optional_tags" = vars.inherited_tags
            "client_marker" = vars.inherited_query
        }
    }

    scope grouped(scope_query: String, scope_tags: Vec<String>) {
        query {
            "query" = scope_query
            "tags" = scope_tags
            "maybe" = scope_query
            "optional_tags" = scope_tags
            "scope_marker" = scope_query
        }

        scope inner(inner_query: String, inner_tags: Vec<String>) {
            query {
                "tags" = inner_tags
                "inner_marker" = inner_query
            }

            GET NestedSearch(query: String, tags: Vec<String>)
                as nested_search
                path ["nested-search"]
                query {
                    "tags" = tags
                    "nested_marker" = query
                }
                -> Json<String>

            GET NestedInherited(query: String)
                as nested_inherited
                path ["nested-inherited"]
                query {
                    "nested_marker" = query
                }
                -> Json<String>
        }

        GET Search(
            query: String,
            tags: Vec<String>,
            maybe?: String,
            optional_tags?: Vec<String>
        )
            as search
            path ["search"]
            query {
                "query" = query
                tags
                "maybe" = maybe
                "optional_tags" = optional_tags
                "later" = query
            }
            -> Json<String>
    }

    GET Custom(custom: custom::Vec<String>)
        as custom
        path ["custom"]
        query { custom }
        -> Json<String>
}

api! {
    client VectorAuthApi {
        base "https://example.com"
        secret auth_key: String
        credential api_key = api_key(secret.auth_key)
    }

        GET Protected(tags: Vec<String>)
            as protected
            path ["protected"]
            auth query "auth_key" = api_key
            query {
                "auth_key" = tags
            }
            -> Json<String>
}

fn reply() -> MockReply {
    MockReply::ok_json(Bytes::from_static(br#""ok""#))
}

#[tokio::test]
async fn generated_query_replacement_supports_all_cardinalities_and_order() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string(), "client-b".to_string()],
        transport,
    );

    api.grouped(
        "scope".to_string(),
        vec!["scope-a".to_string(), "scope-b".to_string()],
    )
    .search(
        "scalar".to_string(),
        vec![
            "space value".to_string(),
            "+".to_string(),
            "&".to_string(),
            "=".to_string(),
            "/".to_string(),
            "é".to_string(),
        ],
    )
    .maybe("optional".to_string())
    .optional_tags(vec!["optional-a".to_string(), "optional-b".to_string()])
    .execute()
    .await
    .expect("generated query request succeeds");

    let requests = handle.recorded();
    handle.finish();
    assert_eq!(requests.len(), 1);
    let pairs: Vec<(String, String)> = requests[0]
        .url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("client_marker".into(), "client".into()),
            ("scope_marker".into(), "scope".into()),
            ("query".into(), "scalar".into()),
            ("tags".into(), "space value".into()),
            ("tags".into(), "+".into()),
            ("tags".into(), "&".into()),
            ("tags".into(), "=".into()),
            ("tags".into(), "/".into()),
            ("tags".into(), "é".into()),
            ("maybe".into(), "optional".into()),
            ("optional_tags".into(), "optional-a".into()),
            ("optional_tags".into(), "optional-b".into()),
            ("later".into(), "scalar".into()),
        ]
    );
    assert_request(&requests[0])
        .query_values("tags", &["space value", "+", "&", "=", "/", "é"])
        .query_values("optional_tags", &["optional-a", "optional-b"]);
}

#[tokio::test]
async fn generated_query_optional_values_and_empty_vectors_remove_inherited_keys() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string()],
        transport,
    );

    api.grouped("scope".to_string(), vec!["scope-a".to_string()])
        .search("scalar".to_string(), Vec::new())
        .execute()
        .await
        .expect("generated query request succeeds");

    let requests = handle.recorded();
    handle.finish();
    assert_request(&requests[0])
        .query_values("query", &["scalar"])
        .query_absent("tags")
        .query_absent("maybe")
        .query_absent("optional_tags");
}

#[tokio::test]
async fn generated_query_optional_vector_some_empty_removes_key() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string()],
        transport,
    );

    api.grouped("scope".to_string(), vec!["scope-a".to_string()])
        .search("scalar".to_string(), vec!["tag".to_string()])
        .optional_tags(Vec::new())
        .execute()
        .await
        .expect("generated query request succeeds");

    let requests = handle.recorded();
    handle.finish();
    assert_request(&requests[0])
        .query_values("tags", &["tag"])
        .query_absent("optional_tags");
}

#[tokio::test]
async fn generated_nested_scopes_replace_vector_values_and_preserve_unrelated_order() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string(), "client-b".to_string()],
        transport,
    );

    api.grouped("outer".to_string(), vec!["outer-a".to_string()])
        .inner(
            "inner".to_string(),
            vec!["inner-a".to_string(), "inner-b".to_string()],
        )
        .nested_search("endpoint".to_string(), Vec::new())
        .execute()
        .await
        .expect("nested generated query request succeeds");

    let requests = handle.recorded();
    handle.finish();
    let pairs: Vec<(String, String)> = requests[0]
        .url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("client_marker".into(), "client".into()),
            ("query".into(), "outer".into()),
            ("maybe".into(), "outer".into()),
            ("optional_tags".into(), "outer-a".into()),
            ("scope_marker".into(), "outer".into()),
            ("inner_marker".into(), "inner".into()),
            ("nested_marker".into(), "endpoint".into()),
        ]
    );
}

#[tokio::test]
async fn generated_nested_scope_vector_replaces_outer_without_endpoint_override() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string(), "client-b".to_string()],
        transport,
    );

    api.grouped("outer".to_string(), vec!["outer-a".to_string()])
        .inner(
            "inner".to_string(),
            vec!["inner-a".to_string(), "inner-b".to_string()],
        )
        .nested_inherited("endpoint".to_string())
        .execute()
        .await
        .expect("nested inherited query request succeeds");

    let requests = handle.recorded();
    handle.finish();
    assert_request(&requests[0]).query_values("tags", &["inner-a", "inner-b"]);
}

#[tokio::test]
async fn generated_custom_vec_named_type_remains_scalar() {
    let (transport, handle) = mock().reply(reply()).build();
    let api = QueryVectorApi::new_with_transport(
        "client".to_string(),
        vec!["client-a".to_string()],
        transport,
    );

    api.custom(custom::Vec("scalar".to_string()))
        .execute()
        .await
        .expect("custom Vec-shaped scalar request succeeds");

    let requests = handle.recorded();
    handle.finish();
    assert_request(&requests[0]).query_values("custom", &["scalar"]);
}

#[tokio::test]
async fn generated_nonempty_vector_preserves_query_auth_collision_redaction() {
    let secret = "QUERY_VECTOR_AUTH_SECRET";
    let (transport, handle) = mock().build();
    let api = VectorAuthApi::new_with_transport(secret.to_string(), transport);

    let err = api
        .protected(vec!["public".to_string()])
        .execute()
        .await
        .expect_err("public query vector must collide with query auth");
    assert_eq!(handle.recorded_len(), 0);
    handle.finish();
    assert!(err.to_string().contains("auth_key"));
    assert!(!err.to_string().contains(secret));
    assert!(!format!("{err:?}").contains(secret));
}

#[tokio::test]
async fn generated_empty_vector_allows_query_auth_without_public_collision() {
    let secret = "QUERY_VECTOR_AUTH_SECRET";
    let (transport, handle) = mock().reply(reply()).build();
    let api = VectorAuthApi::new_with_transport(secret.to_string(), transport);

    api.protected(Vec::new())
        .execute()
        .await
        .expect("empty public vector permits query auth");

    let requests = handle.recorded();
    handle.finish();
    let pairs: Vec<(String, String)> = requests[0]
        .url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(pairs, vec![("auth_key".to_string(), secret.to_string())]);
}
