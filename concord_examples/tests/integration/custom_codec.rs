use bytes::Bytes;
use concord_examples::custom_codec::{CreateUser, CustomCodecApi, User};
use concord_test_support::{ScriptedReply, assert_execution, deterministic_mock};
use http::{HeaderValue, StatusCode};

#[tokio::test]
async fn custom_codec_controls_body_accept_and_decode() {
    let (transport, handle) = deterministic_mock()
        .reply(
            ScriptedReply::status(StatusCode::OK)
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/x-concord-compact"),
                )
                .with_body(Bytes::from_static(b"7:Ada"))
                .expect_body(Bytes::from_static(b"Ada")),
        )
        .build();
    let api =
        CustomCodecApi::new_with_safe_reqwest_builder(|builder| transport.configure_both(builder))
            .expect("mock client");

    let user = api
        .create_user(CreateUser {
            name: "Ada".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(
        user,
        User {
            id: 7,
            name: "Ada".to_string()
        }
    );
    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 1);
    assert_execution(&recorded[0])
        .path("/users")
        .header(http::header::ACCEPT, "application/x-concord-compact")
        .header(http::header::CONTENT_TYPE, "application/x-concord-compact")
        .body_present();
    assert_eq!(recorded[0].known_body_length, Some(3));
    handle.finish();
}
