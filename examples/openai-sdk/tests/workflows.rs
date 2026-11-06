//! Offline protocol and workflow checks.
use concord::{bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_openai::{
    BatchInput, BatchRequest, BatchStatus, Error, Event, OpenAi, ResponseRequest, batch_jsonl,
};
use std::time::Duration;

async fn setup(replies: Vec<Reply>) -> (TestServer, OpenAi) {
    let script = Script::new(replies);
    let s = TestServer::start(move |_| script.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let c = OpenAi::new(http, "synthetic-token")
        .unwrap()
        .with_base_url(&s.url())
        .unwrap();
    (s, c)
}
#[tokio::test]
async fn ordinary_and_fragmented_stream_through_completion() {
    let data = include_bytes!("../fixtures/events.sse");
    let u = data.windows(2).position(|v| v == "é".as_bytes()).unwrap();
    let delim = data.windows(2).position(|v| v == b"\n\n").unwrap();
    let chunks = vec![
        (Duration::ZERO, Bytes::copy_from_slice(&data[..delim + 1])),
        (
            Duration::ZERO,
            Bytes::copy_from_slice(&data[delim + 1..u + 1]),
        ),
        (Duration::ZERO, Bytes::copy_from_slice(&data[u + 1..])),
    ];
    let stream = Reply::new(200)
        .header("content-type", "text/event-stream")
        .with_body(Body::Chunks(chunks));
    let (s, c) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/response.json")),
        stream,
    ])
    .await;
    let request = ResponseRequest::new("fixture-model", "hello");
    assert_eq!(
        c.create_response(request.clone())
            .await
            .unwrap()
            .value
            .status
            .as_deref(),
        Some("completed")
    );
    let mut stream = c.stream_response(request).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Event::Created { .. })
    ));
    assert!(
        matches!(stream.next().await.unwrap(),Some(Event::OutputTextDelta{delta}) if delta=="café 🌍")
    );
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Event::Completed { .. })
    ));
    assert!(stream.next().await.unwrap().is_none());
    let r = s.requests();
    assert_eq!(r[1].target, "/v1/responses");
    assert_eq!(r[1].header("accept"), Some("text/event-stream"));
    assert_eq!(
        r[1].body,
        br#"{"model":"fixture-model","input":"hello","stream":true}"#
    );
    s.shutdown().await;
}
#[tokio::test]
async fn batch_upload_poll_and_correlate_partial_outcomes() {
    let (s, c) = setup(vec![
        Reply::json(200, include_bytes!("../fixtures/file.json")),
        Reply::json(200, include_bytes!("../fixtures/file.json")),
        Reply::json(200, include_bytes!("../fixtures/batch-running.json")),
        Reply::json(200, include_bytes!("../fixtures/batch-running.json")),
        Reply::json(200, include_bytes!("../fixtures/batch-completed.json")),
        Reply::new(200).body(include_bytes!("../fixtures/output.jsonl")),
        Reply::new(200).body(include_bytes!("../fixtures/error.jsonl")),
    ])
    .await;
    let rows = [
        BatchInput::response("request-1", ResponseRequest::new("fixture-model", "one")),
        BatchInput::response("request-2", ResponseRequest::new("fixture-model", "two")),
        BatchInput::response("request-3", ResponseRequest::new("fixture-model", "three")),
    ];
    let data = batch_jsonl(&rows).unwrap();
    let file = c
        .upload_batch_file("batch.jsonl", &data)
        .await
        .unwrap()
        .value;
    assert_eq!(c.file(&file.id).await.unwrap().value.purpose, "batch");
    let batch = c
        .create_batch(BatchRequest::responses(file.id))
        .await
        .unwrap()
        .value;
    let batch = c
        .poll_batch(&batch.id, 2, Duration::ZERO)
        .await
        .unwrap()
        .value;
    assert_eq!(batch.status, BatchStatus::Completed);
    let results = c.batch_results(&batch, 10000).await.unwrap();
    assert!(results.problems.is_empty());
    assert_eq!(
        results.items["request-1"]
            .response
            .as_ref()
            .unwrap()
            .status_code,
        400
    );
    assert!(results.file_errors.is_empty());
    assert_eq!(
        results.items["request-2"]
            .response
            .as_ref()
            .unwrap()
            .status_code,
        200
    );
    assert_eq!(
        results.items["request-3"]
            .error
            .as_ref()
            .unwrap()
            .code
            .as_deref(),
        Some("batch_expired")
    );
    let r = s.requests();
    assert_eq!(
        r.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(),
        vec![
            "/v1/files",
            "/v1/files/file_input",
            "/v1/batches",
            "/v1/batches/batch_fixture",
            "/v1/batches/batch_fixture",
            "/v1/files/file_output/content",
            "/v1/files/file_error/content"
        ]
    );
    assert_eq!(
        r[2].body,
        br#"{"input_file_id":"file_input","endpoint":"/v1/responses","completion_window":"24h"}"#
    );
    s.shutdown().await;
}
#[tokio::test]
async fn incomplete_stream_and_bounded_poll_are_errors() {
    let (s, c) = setup(vec![
        Reply::new(200)
            .header("content-type", "text/event-stream")
            .body(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n"),
        Reply::json(200, include_bytes!("../fixtures/batch-running.json")),
    ])
    .await;
    let mut stream = c
        .stream_response(ResponseRequest::new("fixture", "hi"))
        .await
        .unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Event::OutputTextDelta { .. })
    ));
    assert!(matches!(stream.next().await, Err(Error::Stream(_))));
    assert!(matches!(
        c.poll_batch("batch_fixture", 1, Duration::ZERO).await,
        Err(Error::PollLimit(_))
    ));
    s.shutdown().await;
}
#[tokio::test]
async fn stream_failure_transport_failure_and_local_cancellation() {
    let failure=Reply::new(200).header("content-type","text/event-stream").body(b"data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_failed\",\"status\":\"failed\",\"output\":[],\"error\":{\"message\":\"failed\"}}}\n\n");
    let broken = Reply::new(200)
        .header("content-type", "text/event-stream")
        .with_body(Body::FailAfter(vec![Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
        )]));
    let stall = Reply::new(200)
        .header("content-type", "text/event-stream")
        .with_body(Body::Stall);
    let (s, c) = setup(vec![failure, broken, stall]).await;
    let request = ResponseRequest::new("fixture", "hi");
    let mut stream = c.stream_response(request.clone()).await.unwrap();
    assert!(
        matches!(stream.next().await,Err(Error::StreamFailed(response)) if response.id=="resp_failed")
    );
    let mut stream = c.stream_response(request.clone()).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap(),
        Some(Event::OutputTextDelta { .. })
    ));
    // A body that breaks off is a Concord transport failure naming the operation, not an end.
    match stream.next().await {
        Err(Error::Concord(e)) => {
            assert_eq!(e.kind(), concord::ErrorKind::Transport);
            assert_eq!(e.operation().unwrap().name(), "OpenAiApi.response_stream");
            assert!(!e.to_string().contains("127.0.0.1"), "{e}");
        }
        other => panic!("expected a transport failure, got {other:?}"),
    }
    let mut stream = c.stream_response(request).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), stream.next())
            .await
            .is_err()
    );
    drop(stream);
    s.shutdown().await;
}
#[tokio::test]
async fn failed_batch_preserves_rows_and_failed_file() {
    let failed=br#"{"id":"batch_fixture","status":"failed","output_file_id":"file_output","error_file_id":"file_error","errors":{"data":[{"message":"synthetic"}]}}"#;
    let (s, c) = setup(vec![
        Reply::json(200, failed),
        Reply::new(200).body(include_bytes!("../fixtures/output.jsonl")),
        Reply::json(404, include_bytes!("../fixtures/error.json")),
    ])
    .await;
    let batch = c
        .poll_batch("batch_fixture", 2, Duration::ZERO)
        .await
        .unwrap()
        .value;
    assert_eq!(batch.status, BatchStatus::Failed);
    assert!(batch.errors.is_some());
    let mut results = c.batch_results(&batch, 10000).await.unwrap();
    assert!(results.items.contains_key("request-2"));
    assert_eq!(results.file_errors.len(), 1);
    results.add_file(
        b"not json\n{\"id\":\"x\",\"custom_id\":\"request-2\",\"response\":null,\"error\":null}\n",
    );
    assert_eq!(results.problems.len(), 2);
    s.shutdown().await;
}
