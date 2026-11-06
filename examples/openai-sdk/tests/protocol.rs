//! Offline protocol and workflow checks.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_openai::{BatchInput, Error, OpenAi, ResponseRequest, batch_jsonl};

async fn setup(replies: Vec<Reply>) -> (TestServer, OpenAi) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_| script.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let client = OpenAi::new(http, "synthetic-token")
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    (server, client)
}
#[tokio::test]
async fn response_and_typed_http_error() {
    let (s, c) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/response.json")),
        Reply::json(400, include_bytes!("../fixtures/error.json"))
            .header("x-request-id", "synthetic-request"),
    ])
    .await;
    let request = ResponseRequest::new("model-fixture", "hello");
    assert_eq!(
        c.create_response(request.clone()).await.unwrap().value.id,
        "resp_fixture"
    );
    match c.create_response(request).await.unwrap_err() {
        Error::Provider {
            meta,
            error: Some(e),
        } => {
            assert_eq!(meta.status().as_u16(), 400);
            assert_eq!(meta.headers()["x-request-id"], "synthetic-request");
            assert!(!format!("{meta:?}").contains("synthetic-request"));
            assert_eq!(e.param.as_deref(), Some("model"));
        }
        e => panic!("{e:?}"),
    }
    let r = s.requests();
    assert_eq!(r[0].method, "POST");
    assert_eq!(r[0].target, "/v1/responses");
    assert_eq!(r[0].header("authorization"), Some("Bearer synthetic-token"));
    assert_eq!(r[0].header("content-type"), Some("application/json"));
    assert_eq!(
        r[0].body,
        br#"{"model":"model-fixture","input":"hello","stream":false}"#
    );
    s.shutdown().await;
}
#[tokio::test]
async fn multipart_exact_bytes_and_validation() {
    let (s, c) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/file.json")),
        Reply::json(200, include_bytes!("../fixtures/file.json")),
    ])
    .await;
    let bytes = b"{\"custom_id\":\"one\"}\n";
    c.upload_batch_file("batch.jsonl", bytes).await.unwrap();
    let r = s.single();
    assert_eq!(r.method, "POST");
    assert_eq!(r.target, "/v1/files");
    // The boundary is `concord::multipart`'s first candidate (the SDK no longer frames the body
    // with its own fixed `concord-batch-boundary`); the bytes are otherwise unchanged.
    assert_eq!(
        r.header("content-type"),
        Some("multipart/form-data; boundary=concord-boundary-0")
    );
    let mut expected = b"--concord-boundary-0\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--concord-boundary-0\r\nContent-Disposition: form-data; name=\"file\"; filename=\"batch.jsonl\"\r\nContent-Type: application/jsonl\r\n\r\n".to_vec();
    expected.extend_from_slice(bytes);
    expected.extend_from_slice(b"\r\n--concord-boundary-0--\r\n");
    assert_eq!(r.body, expected);
    assert!(c.upload_batch_file("bad\r\nname", bytes).await.is_err());
    // Bytes that contain a boundary candidate are no longer refused: the format chooses one that
    // occurs in no part (here the next candidate), so they are sent byte for byte.
    let colliding = b"concord-batch-boundary\n--concord-boundary-0\n";
    c.upload_batch_file("ok", colliding).await.unwrap();
    let r = &s.requests()[1];
    assert_eq!(
        r.header("content-type"),
        Some("multipart/form-data; boundary=concord-boundary-1")
    );
    let mut expected = b"--concord-boundary-1\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--concord-boundary-1\r\nContent-Disposition: form-data; name=\"file\"; filename=\"ok\"\r\nContent-Type: application/jsonl\r\n\r\n".to_vec();
    expected.extend_from_slice(colliding);
    expected.extend_from_slice(b"\r\n--concord-boundary-1--\r\n");
    assert_eq!(r.body, expected);
    let row = BatchInput::response("same", ResponseRequest::new("fixture", "text"));
    assert!(batch_jsonl(&[row.clone(), row]).is_err());
    s.shutdown().await;
}
#[tokio::test]
async fn malformed_error_and_file_limit() {
    let (s, c) = setup(vec![
        Reply::json(500, b"invalid"),
        Reply::new(200).body("oversized"),
    ])
    .await;
    assert!(matches!(
        c.batch("x").await,
        Err(Error::Provider { error: None, .. })
    ));
    assert!(matches!(c.content("x", 3).await, Err(Error::Decode { .. })));
    assert!(c.file("").await.is_err());
    s.shutdown().await;
}
/// Between waves 2-3 (coordinator integration): the public declaration's own `send()` goes
/// through `SendPolicy`, and the SSE endpoint is `-> stream` (its body is not read by `send()`).
#[tokio::test]
async fn generated_send_has_the_sdk_error_and_does_not_read_the_event_stream() {
    use concord::reqwest::header::HeaderValue;
    use concord_example_testserver::Body;
    use concord_sdk_openai::{OpenAiApi, OpenAiApiConfig};
    let script = Script::new(vec![
        Reply::json(400, include_bytes!("../fixtures/error.json")),
        Reply::new(200)
            .header("content-type", "text/event-stream")
            .with_body(Body::Stall),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let api = OpenAiApi::new(
        reqwest::Client::new(),
        OpenAiApiConfig {
            authorization: HeaderValue::from_static("Bearer synthetic-token"),
        },
    )
    .with_base_url(s.url())
    .unwrap();
    let request = ResponseRequest::new("model-fixture", "hello");
    match api.response(request.clone()).send().await.unwrap_err() {
        Error::Provider {
            meta,
            error: Some(e),
        } => {
            assert_eq!(meta.status().as_u16(), 400);
            assert_eq!(e.param.as_deref(), Some("model"));
        }
        e => panic!("{e:?}"),
    }
    // The body never ends: `send()` must return with the head only.
    let opened = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        api.response_stream(request).send(),
    )
    .await
    .expect("send() does not read the event stream")
    .unwrap();
    assert_eq!(opened.meta.status().as_u16(), 200);
    assert_eq!(
        opened.meta.operation().unwrap().name(),
        "OpenAiApi.response_stream"
    );
    drop(opened);
    let r = s.requests();
    assert_eq!(r.len(), 2);
    assert_eq!(r[1].header("accept"), Some("text/event-stream"));
    s.shutdown().await;
}
