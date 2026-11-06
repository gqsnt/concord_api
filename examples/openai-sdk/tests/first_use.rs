//! Independent application consumer. Sources and observed friction: ../FIRST_USE.md.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_openai::{
    BatchInput, BatchRequest, BatchStatus, Error, Event, OpenAi, ResponseMeta, ResponseRequest,
    batch_jsonl,
};
use serde_json::{Value, json};

const COMPLETED: &[u8] = include_bytes!("../fixtures/first-use/completed.sse");
const EARLY: &[u8] = include_bytes!("../fixtures/first-use/early-eof.sse");
const FAILED: &[u8] = include_bytes!("../fixtures/first-use/failed.sse");
const EVENT_ERROR: &[u8] = include_bytes!("../fixtures/first-use/error.sse");
const MALFORMED: &[u8] = include_bytes!("../fixtures/first-use/malformed.sse");
const INCOMPLETE: &[u8] = include_bytes!("../fixtures/first-use/incomplete.sse");
const BATCH_INPUT: &[u8] = include_bytes!("../fixtures/first-use/input.jsonl");
const BATCH_OUTPUT: &[u8] = include_bytes!("../fixtures/first-use/output.jsonl");
const BATCH_ERRORS: &[u8] = include_bytes!("../fixtures/first-use/errors.jsonl");
const BATCH_EXPIRED: &[u8] = include_bytes!("../fixtures/first-use/expired.jsonl");
const BAD_ROWS: &[u8] = include_bytes!("../fixtures/first-use/rows-with-problems.jsonl");

fn client(server: &TestServer) -> OpenAi {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    OpenAi::new(http, "synthetic-first-use-token")
        .unwrap()
        .with_base_url(&server.url())
        .unwrap()
}

fn input() -> ResponseRequest {
    ResponseRequest::new("fixture-model", "Say hello in French")
}

fn response_document(status: &str) -> Value {
    json!({
        "id": "resp_consumer", "object": "response", "created_at": 1,
        "model": "fixture-model", "status": status, "error": null,
        "output": [{"type": "message", "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": "Bonjour 🌍", "annotations": []}]}]
    })
}

fn provider_problem() -> Value {
    json!({"error": {"message": "Synthetic request rejected", "type": "invalid_request_error",
        "param": "input", "code": "synthetic_rejection"}})
}

fn json_reply(status: u16, document: Value) -> Reply {
    Reply::json(status, serde_json::to_vec(&document).unwrap())
        .header("x-request-id", "synthetic-request-id")
}

fn assert_wire(request: &Recorded, method: &str, target: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.target, target);
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-first-use-token")
    );
    assert!(request.host.starts_with("127.0.0.1:"));
    if method == "GET" {
        assert!(request.body.is_empty());
    }
}

fn assert_response_wire(request: &Recorded, streaming: bool) {
    assert_wire(request, "POST", "/v1/responses");
    assert!(
        request
            .header("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], "fixture-model");
    assert_eq!(body["input"], "Say hello in French");
    // Both omission and explicit false mean an ordinary provider request.
    assert_eq!(
        body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        streaming
    );
    assert_eq!(
        request.header("accept"),
        Some(if streaming {
            "text/event-stream"
        } else {
            "application/json"
        })
    );
}

fn assert_meta(meta: &ResponseMeta, status: u16, method: &str) {
    assert_eq!(meta.status.as_u16(), status);
    assert_eq!(meta.headers["x-request-id"], "synthetic-request-id");
    let operation = meta
        .operation
        .as_ref()
        .expect("declared operation identity");
    assert!(operation.is_endpoint());
    assert!(!operation.name().is_empty());
    assert_eq!(operation.method(), Some(method));
}

fn sse_reply(bytes: &'static [u8]) -> Reply {
    // One byte per HTTP chunk splits SSE framing and the four-byte emoji.
    Reply::new(200)
        .header("content-type", "text/event-stream; charset=utf-8")
        .header("x-request-id", "synthetic-request-id")
        .with_body(Body::Chunks(
            bytes
                .iter()
                .map(|byte| (Duration::ZERO, vec![*byte].into()))
                .collect(),
        ))
}

#[tokio::test]
async fn ordinary_response_exposes_text_and_http_metadata() {
    let server = TestServer::start(|_| json_reply(200, response_document("completed"))).await;
    let response = client(&server).create_response(input()).await.unwrap();
    assert_eq!(response.value.id, "resp_consumer");
    assert_eq!(response.value.status.as_deref(), Some("completed"));
    assert_eq!(response.value.output[0]["content"][0]["text"], "Bonjour 🌍");
    assert!(response.value.error.is_none());
    assert_meta(&response.meta, 200, "POST");
    assert_response_wire(&server.single(), false);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn ordinary_failed_response_is_data_even_with_http_success() {
    let server = TestServer::start(|_| {
        let mut document = response_document("failed");
        document["output"] = json!([]);
        document["error"] = json!({"code": "server_error", "message": "Synthetic model failure"});
        json_reply(200, document)
    })
    .await;
    let response = client(&server).create_response(input()).await.unwrap();
    assert_eq!(response.value.status.as_deref(), Some("failed"));
    assert_eq!(response.value.error.unwrap()["code"], "server_error");
    assert_meta(&response.meta, 200, "POST");
    assert_response_wire(&server.single(), false);
    server.shutdown().await;
}

#[tokio::test]
async fn http_errors_preserve_document_and_headers_for_both_response_modes() {
    for streaming in [false, true] {
        for readable in [false, true] {
            let server = TestServer::start(move |_| {
                if readable {
                    json_reply(429, provider_problem()).header("retry-after", "7")
                } else {
                    Reply::text(502, "text/html", "synthetic upstream unavailable")
                        .header("x-request-id", "synthetic-request-id")
                }
            })
            .await;
            let api = client(&server);
            let error = if streaming {
                match api.stream_response(input()).await {
                    Err(error) => error,
                    Ok(_) => panic!("HTTP error must not open an event stream"),
                }
            } else {
                api.create_response(input()).await.unwrap_err()
            };
            match error {
                Error::Provider { meta, error } => {
                    assert_meta(&meta, if readable { 429 } else { 502 }, "POST");
                    if readable {
                        assert_eq!(meta.headers["retry-after"], "7");
                        let error = error.unwrap();
                        assert_eq!(error.message, "Synthetic request rejected");
                        assert_eq!(error.kind.as_deref(), Some("invalid_request_error"));
                        assert_eq!(error.param.as_deref(), Some("input"));
                        assert_eq!(error.code, Some(json!("synthetic_rejection")));
                    } else {
                        assert!(error.is_none());
                    }
                }
                other => panic!("expected provider error, got {other:?}"),
            }
            assert_response_wire(&server.single(), streaming);
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn malformed_ordinary_response_keeps_response_metadata() {
    let server = TestServer::start(|_| {
        Reply::json(200, "{bad-json}").header("x-request-id", "synthetic-request-id")
    })
    .await;
    match client(&server).create_response(input()).await.unwrap_err() {
        Error::Decode { meta, .. } => assert_meta(&meta, 200, "POST"),
        other => panic!("expected decode error, got {other:?}"),
    }
    assert_response_wire(&server.single(), false);
    server.shutdown().await;
}

#[tokio::test]
async fn streaming_response_completes_across_utf8_and_sse_chunk_boundaries() {
    let server = TestServer::start(|_| sse_reply(COMPLETED)).await;
    let mut stream = client(&server).stream_response(input()).await.unwrap();
    assert_meta(&stream.meta, 200, "POST");
    let mut text = String::new();
    let mut kinds = Vec::new();
    while let Some(event) = stream.next().await.unwrap() {
        match event {
            Event::Created { response } => {
                assert_eq!(response.status.as_deref(), Some("in_progress"));
                kinds.push("created");
            }
            Event::Other => kinds.push("other"),
            Event::OutputTextDelta { delta } => {
                text.push_str(&delta);
                kinds.push("delta");
            }
            Event::Completed { response } => {
                assert_eq!(response.status.as_deref(), Some("completed"));
                assert_eq!(response.output[0]["content"][0]["text"], text);
                kinds.push("completed");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert_eq!(text, "Bonjour 🌍");
    assert_eq!(kinds, ["created", "other", "delta", "delta", "completed"]);
    assert!(stream.next().await.unwrap().is_none());
    assert_response_wire(&server.single(), true);
    server.shutdown().await;
}

#[tokio::test]
async fn clean_http_eof_without_completed_is_a_stream_failure() {
    let server = TestServer::start(|_| sse_reply(EARLY)).await;
    let mut stream = client(&server).stream_response(input()).await.unwrap();
    let partial = match stream.next().await.unwrap().unwrap() {
        Event::OutputTextDelta { delta } => delta,
        other => panic!("expected partial text, got {other:?}"),
    };
    assert!(matches!(stream.next().await, Err(Error::Stream(_))));
    assert_eq!(partial, "Partial");
    // Stream errors do not carry metadata; the still-owned reader does.
    assert_meta(&stream.meta, 200, "POST");
    assert_response_wire(&server.single(), true);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_response_and_error_events_preserve_partial_text_and_typed_failure() {
    for bytes in [FAILED, EVENT_ERROR] {
        let server = TestServer::start(move |_| sse_reply(bytes)).await;
        let mut stream = client(&server).stream_response(input()).await.unwrap();
        assert!(
            matches!(stream.next().await.unwrap(), Some(Event::OutputTextDelta { delta }) if delta == "Partial")
        );
        let error = stream.next().await.unwrap_err();
        if bytes == FAILED {
            match error {
                Error::StreamFailed(response) => {
                    assert_eq!(response.id, "resp_failed");
                    assert_eq!(response.status.as_deref(), Some("failed"));
                    assert_eq!(response.error.unwrap()["code"], "server_error");
                }
                other => panic!("expected failed response, got {other:?}"),
            }
        } else {
            match error {
                Error::StreamEventError { message, code } => {
                    assert_eq!(message, "Synthetic event failure");
                    assert_eq!(code, Some(json!("server_error")));
                }
                other => panic!("expected provider event error, got {other:?}"),
            }
        }
        assert_response_wire(&server.single(), true);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_sse_is_not_completion_and_keeps_preceding_delta() {
    let server = TestServer::start(|_| sse_reply(MALFORMED)).await;
    let mut stream = client(&server).stream_response(input()).await.unwrap();
    assert!(
        matches!(stream.next().await.unwrap(), Some(Event::OutputTextDelta { delta }) if delta == "Partial")
    );
    assert!(matches!(stream.next().await, Err(Error::Json(_))));
    assert_response_wire(&server.single(), true);
    server.shutdown().await;
}

#[tokio::test]
async fn incomplete_event_is_not_success_but_its_reason_is_not_exposed() {
    let server = TestServer::start(|_| sse_reply(INCOMPLETE)).await;
    let mut stream = client(&server).stream_response(input()).await.unwrap();
    assert!(
        matches!(stream.next().await.unwrap(), Some(Event::OutputTextDelta { delta }) if delta == "Partial")
    );
    assert!(matches!(stream.next().await.unwrap(), Some(Event::Other)));
    assert!(matches!(stream.next().await, Err(Error::Stream(_))));
    assert_response_wire(&server.single(), true);
    server.shutdown().await;
}

#[tokio::test]
async fn dropping_a_pending_stream_read_stops_local_consumption() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/event-stream")
            .with_body(Body::Stall)
    })
    .await;
    let mut stream = client(&server).stream_response(input()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), stream.next())
            .await
            .is_err()
    );
    drop(stream);
    assert_response_wire(&server.single(), true);
    assert_eq!(server.shutdown().await.open_after, 0);
}

fn batch_document(status: &str, files: bool) -> Value {
    let counts = match status {
        "validating" | "failed" => json!({"total": 3, "completed": 0, "failed": 0}),
        "in_progress" => json!({"total": 3, "completed": 1, "failed": 0}),
        _ => json!({"total": 3, "completed": 2, "failed": 1}),
    };
    json!({
        "id": "batch_consumer", "object": "batch", "endpoint": "/v1/responses",
        "input_file_id": "file_input", "completion_window": "24h", "created_at": 1,
        "status": status, "output_file_id": if files { Some("file_output") } else { None },
        "error_file_id": if files { Some("file_errors") } else { None },
        "errors": null, "request_counts": counts
    })
}

fn file_document() -> Value {
    json!({"id": "file_input", "object": "file", "filename": "input.jsonl",
        "purpose": "batch", "bytes": BATCH_INPUT.len(), "created_at": 1, "status": "processed"})
}

fn file_reply(bytes: &[u8]) -> Reply {
    Reply::text(200, "application/jsonl", bytes).header("x-request-id", "synthetic-request-id")
}

fn batch_inputs() -> Vec<BatchInput> {
    [
        ("one", "First: café"),
        ("two", "Second"),
        ("three", "Third"),
    ]
    .into_iter()
    .map(|(id, prompt)| BatchInput::response(id, ResponseRequest::new("fixture-model", prompt)))
    .collect()
}

fn assert_jsonl_input(bytes: &[u8]) {
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert!(!bytes.contains(&b'\r'));
    let rows = |bytes: &[u8]| -> Vec<Value> {
        std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    };
    assert_eq!(rows(bytes), rows(BATCH_INPUT));
}

fn assert_upload_wire(request: &Recorded, file_bytes: &[u8]) {
    assert_wire(request, "POST", "/v1/files");
    let content_type = request.header("content-type").unwrap();
    assert!(content_type.starts_with("multipart/form-data;"));
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .unwrap()
        .trim_matches('"');
    assert!(!boundary.is_empty());
    let delimiter = format!("--{boundary}");
    let body = std::str::from_utf8(&request.body).unwrap();
    assert!(body.starts_with(&format!("{delimiter}\r\n")));
    assert!(body.ends_with(&format!("{delimiter}--\r\n")));
    let mut names = Vec::new();
    for part in body.split(&delimiter).skip(1) {
        if part == "--\r\n" {
            break;
        }
        let (headers, payload) = part
            .strip_prefix("\r\n")
            .unwrap()
            .split_once("\r\n\r\n")
            .unwrap();
        let payload = payload.strip_suffix("\r\n").unwrap();
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-disposition: form-data;")
        );
        if headers.contains("name=\"purpose\"") {
            names.push("purpose");
            assert_eq!(payload, "batch");
        } else {
            names.push("file");
            assert!(headers.contains("name=\"file\""));
            assert!(headers.contains("filename=\"input.jsonl\""));
            assert_eq!(payload.as_bytes(), file_bytes);
        }
    }
    names.sort_unstable();
    assert_eq!(names, ["file", "purpose"]);
}

#[tokio::test]
async fn upload_poll_and_reconcile_both_result_files_by_application_id() {
    let polls = AtomicUsize::new(0);
    let server = TestServer::start(move |request| {
        match (request.method.as_str(), request.target.as_str()) {
            ("POST", "/v1/files") | ("GET", "/v1/files/file_input") => {
                json_reply(200, file_document())
            }
            ("POST", "/v1/batches") => json_reply(200, batch_document("validating", false)),
            ("GET", "/v1/batches/batch_consumer") => {
                let step = polls.fetch_add(1, Ordering::SeqCst);
                let status = ["in_progress", "finalizing", "completed"]
                    .get(step)
                    .copied()
                    .unwrap_or("unexpected_extra_poll");
                json_reply(200, batch_document(status, step == 2))
            }
            ("GET", "/v1/files/file_output/content") => file_reply(BATCH_OUTPUT),
            ("GET", "/v1/files/file_errors/content") => file_reply(BATCH_ERRORS),
            _ => json_reply(404, provider_problem()),
        }
    })
    .await;
    let api = client(&server);
    let input_bytes = batch_jsonl(&batch_inputs()).unwrap();
    assert_jsonl_input(&input_bytes);
    let uploaded = api
        .upload_batch_file("input.jsonl", &input_bytes)
        .await
        .unwrap();
    assert_meta(&uploaded.meta, 200, "POST");
    assert_eq!(uploaded.value.id, "file_input");
    assert_eq!(uploaded.value.filename, "input.jsonl");
    assert_eq!(uploaded.value.purpose, "batch");
    assert_eq!(uploaded.value.bytes, input_bytes.len() as u64);
    let metadata = api.file(&uploaded.value.id).await.unwrap();
    assert_meta(&metadata.meta, 200, "GET");
    assert_eq!(metadata.value.id, uploaded.value.id);
    assert_eq!(metadata.value.status.as_deref(), Some("processed"));
    let created = api
        .create_batch(BatchRequest::responses(uploaded.value.id))
        .await
        .unwrap();
    assert_eq!(created.value.status, BatchStatus::Validating);
    assert_meta(&created.meta, 200, "POST");
    let polled = api
        .poll_batch(&created.value.id, 5, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(polled.value.status, BatchStatus::Completed);
    assert!(polled.value.status.terminal());
    assert_eq!(polled.value.request_counts.as_ref().unwrap()["failed"], 1);
    assert_meta(&polled.meta, 200, "GET");
    let results = api.batch_results(&polled.value, 4096).await.unwrap();
    assert!(results.problems.is_empty());
    assert!(results.file_errors.is_empty());
    // Completion of the job does not imply all requests succeeded.
    assert_eq!(
        results.items.keys().map(String::as_str).collect::<Vec<_>>(),
        ["one", "three", "two"]
    );
    let one = results.items["one"].response.as_ref().unwrap();
    assert_eq!(one.status_code, 200);
    assert_eq!(one.request_id.as_deref(), Some("synthetic-row-one"));
    assert_eq!(
        one.body["output"][0]["content"][0]["text"],
        "Première réponse"
    );
    assert_eq!(
        results.items["two"].response.as_ref().unwrap().body["id"],
        "resp_two"
    );
    assert!(results.items["three"].response.is_none());
    let rejected = results.items["three"].error.as_ref().unwrap();
    assert_eq!(rejected.code.as_deref(), Some("invalid_prompt"));
    assert_eq!(rejected.message, "Synthetic prompt rejected");

    let requests = server.requests();
    let targets = [
        ("POST", "/v1/files"),
        ("GET", "/v1/files/file_input"),
        ("POST", "/v1/batches"),
        ("GET", "/v1/batches/batch_consumer"),
        ("GET", "/v1/batches/batch_consumer"),
        ("GET", "/v1/batches/batch_consumer"),
        ("GET", "/v1/files/file_output/content"),
        ("GET", "/v1/files/file_errors/content"),
    ];
    assert_eq!(requests.len(), targets.len());
    for (request, (method, target)) in requests.iter().zip(targets) {
        assert_wire(request, method, target);
    }
    assert_upload_wire(&requests[0], &input_bytes);
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[2].body).unwrap(),
        json!({
            "input_file_id": "file_input", "endpoint": "/v1/responses", "completion_window": "24h"
        })
    );
    assert!(
        requests[2]
            .header("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn polling_stops_on_failed_expired_and_cancelled_without_claiming_item_success() {
    for (status, expected) in [
        ("failed", BatchStatus::Failed),
        ("expired", BatchStatus::Expired),
        ("cancelled", BatchStatus::Cancelled),
    ] {
        let server = TestServer::start(move |request| match request.target.as_str() {
            "/v1/batches/batch_consumer" => {
                let mut batch = batch_document(status, status != "failed");
                if status == "failed" {
                    batch["errors"] = json!({"object": "list", "data": [{"code": "invalid_json_line", "line": 2, "message": "Synthetic validation failure"}]});
                }
                json_reply(200, batch)
            }
            "/v1/files/file_output/content" => file_reply(BATCH_OUTPUT),
            "/v1/files/file_errors/content" => file_reply(if status == "expired" { BATCH_EXPIRED } else { BATCH_ERRORS }),
            _ => json_reply(404, provider_problem()),
        }).await;
        let api = client(&server);
        let terminal = api
            .poll_batch("batch_consumer", 5, Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(terminal.value.status, expected);
        assert!(terminal.value.status.terminal());
        assert_ne!(terminal.value.status, BatchStatus::Completed);
        assert_meta(&terminal.meta, 200, "GET");
        assert_wire(&server.single(), "GET", "/v1/batches/batch_consumer");
        let results = api.batch_results(&terminal.value, 4096).await.unwrap();
        if status == "failed" {
            assert_eq!(terminal.value.errors.unwrap()["data"][0]["line"], 2);
            assert!(results.items.is_empty());
            assert_eq!(server.requests().len(), 1);
        } else {
            assert!(results.items["one"].response.is_some());
            assert_eq!(
                results.items["three"]
                    .error
                    .as_ref()
                    .unwrap()
                    .code
                    .as_deref(),
                Some(if status == "expired" {
                    "batch_expired"
                } else {
                    "invalid_prompt"
                })
            );
            assert_eq!(server.requests().len(), 3);
        }
        assert!(results.problems.is_empty());
        assert!(results.file_errors.is_empty());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn polling_limit_returns_the_last_observation_including_unknown_status() {
    for final_status in ["finalizing", "synthetic_future_status"] {
        let polls = AtomicUsize::new(0);
        let server = TestServer::start(move |_| {
            let status = if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                "in_progress"
            } else {
                final_status
            };
            json_reply(200, batch_document(status, false))
        })
        .await;
        let error = client(&server)
            .poll_batch("batch_consumer", 2, Duration::ZERO)
            .await
            .unwrap_err();
        match error {
            Error::PollLimit(last) => {
                assert_eq!(last.id, "batch_consumer");
                assert_eq!(
                    last.status,
                    if final_status == "finalizing" {
                        BatchStatus::Finalizing
                    } else {
                        BatchStatus::Unknown
                    }
                );
                assert!(!last.status.terminal());
                assert_eq!(last.request_counts.unwrap()["total"], 3);
            }
            other => panic!("expected last observed batch, got {other:?}"),
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        for request in &requests {
            assert_wire(request, "GET", "/v1/batches/batch_consumer");
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn failed_poll_request_returns_provider_error_without_retrying_to_completion() {
    let polls = AtomicUsize::new(0);
    let server = TestServer::start(move |_| {
        if polls.fetch_add(1, Ordering::SeqCst) == 0 {
            json_reply(200, batch_document("in_progress", false))
        } else {
            json_reply(503, provider_problem()).header("retry-after", "9")
        }
    })
    .await;
    match client(&server)
        .poll_batch("batch_consumer", 5, Duration::ZERO)
        .await
        .unwrap_err()
    {
        Error::Provider { meta, error } => {
            assert_meta(&meta, 503, "GET");
            assert_eq!(meta.headers["retry-after"], "9");
            assert_eq!(error.unwrap().message, "Synthetic request rejected");
        }
        other => panic!("expected provider error, got {other:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_wire(request, "GET", "/v1/batches/batch_consumer");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn either_result_file_can_fail_without_discarding_the_other_file() {
    for failed_file in ["file_output", "file_errors"] {
        let server = TestServer::start(move |request| match request.target.as_str() {
            "/v1/batches/batch_consumer" => json_reply(200, batch_document("completed", true)),
            target if target == format!("/v1/files/{failed_file}/content") => {
                json_reply(404, provider_problem())
            }
            "/v1/files/file_output/content" => file_reply(BATCH_OUTPUT),
            "/v1/files/file_errors/content" => file_reply(BATCH_ERRORS),
            _ => json_reply(404, provider_problem()),
        })
        .await;
        let api = client(&server);
        let batch = api.batch("batch_consumer").await.unwrap().value;
        let results = api.batch_results(&batch, 4096).await.unwrap();
        assert!(results.problems.is_empty());
        assert_eq!(results.file_errors.len(), 1);
        assert_eq!(results.file_errors[0].0, failed_file);
        match &results.file_errors[0].1 {
            Error::Provider { meta, error } => {
                assert_meta(meta, 404, "GET");
                assert_eq!(
                    error.as_ref().unwrap().code,
                    Some(json!("synthetic_rejection"))
                );
            }
            other => panic!("expected typed file error, got {other:?}"),
        }
        if failed_file == "file_output" {
            assert_eq!(results.items.len(), 1);
            assert!(results.items["three"].error.is_some());
        } else {
            assert_eq!(results.items.len(), 2);
            assert!(results.items["one"].response.is_some());
            assert!(results.items["two"].response.is_some());
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 3);
        for (request, target) in requests.iter().zip([
            "/v1/batches/batch_consumer",
            "/v1/files/file_output/content",
            "/v1/files/file_errors/content",
        ]) {
            assert_wire(request, "GET", target);
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_and_duplicate_rows_are_reported_while_later_rows_survive() {
    let server = TestServer::start(|request| match request.target.as_str() {
        "/v1/batches/batch_consumer" => json_reply(200, batch_document("completed", true)),
        "/v1/files/file_output/content" => file_reply(BAD_ROWS),
        "/v1/files/file_errors/content" => file_reply(BATCH_ERRORS),
        _ => json_reply(404, provider_problem()),
    })
    .await;
    let api = client(&server);
    let batch = api.batch("batch_consumer").await.unwrap().value;
    let results = api.batch_results(&batch, 4096).await.unwrap();
    assert_eq!(results.items.len(), 3);
    assert!(results.items.contains_key("kept"));
    assert_eq!(
        results.items["tail"].response.as_ref().unwrap().body["text"],
        "valid after malformed row"
    );
    assert!(results.items["three"].error.is_some());
    assert_eq!(results.problems.len(), 2);
    assert!(results.problems.iter().all(|problem| !problem.is_empty()));
    assert!(results.file_errors.is_empty());
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn per_file_download_limit_preserves_the_small_error_file() {
    let server = TestServer::start(|request| match request.target.as_str() {
        "/v1/batches/batch_consumer" => json_reply(200, batch_document("completed", true)),
        "/v1/files/file_output/content" => file_reply(BATCH_OUTPUT),
        "/v1/files/file_errors/content" => file_reply(BATCH_ERRORS),
        _ => json_reply(404, provider_problem()),
    })
    .await;
    let api = client(&server);
    let batch = api.batch("batch_consumer").await.unwrap().value;
    let results = api.batch_results(&batch, BATCH_ERRORS.len()).await.unwrap();
    assert_eq!(results.items.len(), 1);
    assert!(results.items["three"].error.is_some());
    assert!(results.problems.is_empty());
    assert_eq!(results.file_errors.len(), 1);
    assert_eq!(results.file_errors[0].0, "file_output");
    let content = api
        .content("file_errors", BATCH_ERRORS.len())
        .await
        .unwrap();
    assert_eq!(content.value, BATCH_ERRORS);
    assert_meta(&content.meta, 200, "GET");
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_batch_input_and_unsafe_filename_are_rejected_before_http() {
    let server = TestServer::start(|_| json_reply(500, provider_problem())).await;
    let api = client(&server);
    let mut duplicate = batch_inputs();
    duplicate[1].custom_id = duplicate[0].custom_id.clone();
    assert!(matches!(batch_jsonl(&duplicate), Err(Error::Invalid(_))));
    let mut empty_id = batch_inputs();
    empty_id[0].custom_id.clear();
    assert!(matches!(batch_jsonl(&empty_id), Err(Error::Invalid(_))));
    let mut streaming = batch_inputs();
    streaming[0].body.stream = Some(true);
    assert!(matches!(batch_jsonl(&streaming), Err(Error::Invalid(_))));
    assert!(matches!(
        api.upload_batch_file("bad\r\nname.jsonl", BATCH_INPUT)
            .await,
        Err(Error::Invalid(_))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}
