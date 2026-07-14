use bytes::Bytes;
use concord_core::advanced::MultipartBody;
use concord_core::prelude::Text;
use concord_macros::api;
use concord_test_support::{ScriptedReply, deterministic_mock};

api! {
    client MultipartRequestApi { base "https://example.com" }
    POST Upload(body: Multipart<()>)
        path ["upload"]
        -> Text<String>
}

#[tokio::test]
async fn generated_multipart_form_data_request_reaches_transport() {
    let (mock, handle) = deterministic_mock()
        .reply(ScriptedReply::ok_text(Bytes::from_static(b"ok")))
        .build();
    let api =
        multipart_request_api::MultipartRequestApi::new_with_safe_reqwest_builder(|builder| {
            mock.configure_application(builder)
        })
        .expect("deterministic generated multipart client");

    let response = api
        .upload(
            MultipartBody::new()
                .text("title", "hello")
                .bytes("file", Bytes::from_static(b"abc")),
        )
        .execute()
        .await
        .expect("multipart request succeeds");
    assert_eq!(response, "ok");

    let recorded = handle.recorded();
    assert_eq!(recorded.len(), 1);
    let content_type = recorded[0]
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("multipart content type");
    assert!(content_type.starts_with("multipart/form-data; boundary="));
    assert_eq!(
        recorded[0].body_category,
        concord_core::__development::CapturedBodyCategory::Multipart
    );
    assert_eq!(recorded[0].known_body_length, None);
    handle.finish();
}
