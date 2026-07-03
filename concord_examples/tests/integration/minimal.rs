use bytes::Bytes;
use concord_examples::minimal::{MinimalApi, User};
use concord_test_support::{MockReply, assert_request, mock};
use http::{HeaderValue, StatusCode};

#[tokio::test]
async fn minimal_api_generated_client_execute_and_direct_await_work() {
    let (transport, handle) = mock()
        .reply(user_reply(42, "Ada"))
        .reply(user_reply(7, "Grace"))
        .reply(user_reply(9, "Linus").with_header(
            "x-request-id".parse().unwrap(),
            HeaderValue::from_static("req-9"),
        ))
        .build();
    let api = MinimalApi::new_with_transport(transport);

    let via_execute = api.users().get_user(42).execute().await.unwrap();
    assert_eq!(
        via_execute,
        User {
            id: 42,
            name: "Ada".to_string()
        }
    );

    let via_direct_await = api.users().get_user(7).await.unwrap();
    assert_eq!(via_direct_await.name, "Grace");

    let decoded = api
        .users()
        .get_user(9)
        .execute_decoded_with::<concord_core::prelude::Json<User>>()
        .await
        .unwrap();
    assert_eq!(decoded.value().name, "Linus");
    assert_eq!(decoded.status(), StatusCode::OK);
    assert_eq!(decoded.meta().endpoint, "users::GetUser");
    assert_eq!(decoded.meta().method, http::Method::GET);
    assert_eq!(
        decoded
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("req-9")
    );

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 3);
    assert_request(&recorded[0])
        .host("api.example.com")
        .path("/users/42")
        .body_absent();
    assert_request(&recorded[1])
        .host("api.example.com")
        .path("/users/7")
        .body_absent();
    assert_request(&recorded[2])
        .host("api.example.com")
        .path("/users/9")
        .body_absent();
    handle.finish();
}

fn user_reply(id: u64, name: &str) -> MockReply {
    MockReply::ok_json(Bytes::from(format!(r#"{{"id":{id},"name":"{name}"}}"#)))
}
