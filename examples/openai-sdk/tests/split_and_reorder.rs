//! Maintenance perturbation: SSE bytes cut at every possible chunk boundary (inside line endings,
//! the blank line that ends an event, and 2-, 3- and 4-byte UTF-8 characters), and batch result
//! rows in reordered layouts that contain failed items, correlated by `custom_id`. Every body is
//! served chunked by the loopback test server; see ../MAINTENANCE.md.
use concord::{bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_openai::{
    Batch, BatchInput, BatchItem, BatchStatus, Error, Event, OpenAi, ResponseRequest, batch_jsonl,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

const EVENTS: &[u8] = include_bytes!("../fixtures/events.sse");
const ERROR_EVENTS: &[u8] = include_bytes!("../fixtures/error-event.sse");
const OUTPUT: &[u8] = include_bytes!("../fixtures/reordered-output.jsonl");
const ERRORS: &[u8] = include_bytes!("../fixtures/reordered-error.jsonl");

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

async fn setup(replies: Vec<Reply>) -> (TestServer, OpenAi) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_| script.next()).await;
    let client = OpenAi::new(http(), "synthetic-token")
        .unwrap()
        .with_base_url(&server.url())
        .unwrap();
    (server, client)
}

/// A chunked body cut at each offset of `cuts` (strictly increasing, inside `data`).
fn cut(data: &[u8], cuts: &[usize]) -> Body {
    let mut chunks = Vec::new();
    let mut start = 0;
    for end in cuts.iter().copied().chain(std::iter::once(data.len())) {
        assert!(start < end && end <= data.len(), "bad cut {end}");
        chunks.push((Duration::ZERO, Bytes::copy_from_slice(&data[start..end])));
        start = end;
    }
    Body::Chunks(chunks)
}

/// One byte per HTTP chunk: every delimiter and every multi-byte character is split at every
/// interior position at once.
fn every_byte(data: &[u8]) -> Body {
    cut(data, &(1..data.len()).collect::<Vec<_>>())
}

/// The single-cut layouts (one per interior offset) followed by the one-byte-per-chunk layout.
fn layouts(data: &[u8]) -> Vec<Body> {
    let mut all: Vec<Body> = (1..data.len()).map(|k| cut(data, &[k])).collect();
    all.push(every_byte(data));
    all
}

fn sse(body: Body) -> Reply {
    Reply::new(200)
        .header("content-type", "text/event-stream")
        .with_body(body)
}

fn crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in data {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

fn find(data: &[u8], needle: &[u8]) -> usize {
    data.windows(needle.len())
        .position(|w| w == needle)
        .unwrap_or_else(|| panic!("fixture lacks {:?}", String::from_utf8_lossy(needle)))
}

fn describe(event: &Event) -> String {
    match event {
        Event::Created { response } => format!("created {}", response.id),
        Event::OutputTextDelta { delta } => format!("delta {delta}"),
        Event::Completed { response } => {
            format!("completed {} {:?}", response.id, response.status)
        }
        Event::Failed { .. } | Event::Error { .. } => panic!("terminal failures are errors"),
        Event::Other => "other".into(),
    }
}

/// Reads one stream to its end: the events seen and how it ended (`Ok` only after completion).
async fn read_stream(client: &OpenAi) -> (Vec<String>, Result<(), Error>) {
    let mut stream = client
        .stream_response(ResponseRequest::new("fixture-model", "hello"))
        .await
        .unwrap();
    let mut seen = Vec::new();
    loop {
        match stream.next().await {
            Ok(Some(event)) => seen.push(describe(&event)),
            Ok(None) => return (seen, Ok(())),
            Err(error) => return (seen, Err(error)),
        }
    }
}

#[tokio::test]
async fn the_test_server_delivers_every_cut_as_a_separate_client_chunk() {
    // Guards against a vacuous perturbation: the cuts must reach the client as chunk boundaries.
    let script = Script::new([sse(every_byte(EVENTS))]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut response = http().get(server.url()).send().await.unwrap();
    let mut pieces = Vec::new();
    while let Some(piece) = response.chunk().await.unwrap() {
        pieces.push(piece);
    }
    assert_eq!(pieces.len(), EVENTS.len());
    assert!(pieces.iter().all(|p| p.len() == 1));
    assert_eq!(pieces.concat(), EVENTS);
    server.shutdown().await;
}

#[tokio::test]
async fn sse_split_at_every_boundary_decodes_the_same_events() {
    let lf = EVENTS.to_vec();
    let crlf = crlf(EVENTS);
    // The cuts cover these sequences only if the fixture still contains them.
    for (data, delimiter) in [(&lf, &b"\n\n"[..]), (&crlf, &b"\r\n\r\n"[..])] {
        find(data, delimiter);
        find(data, "é".as_bytes());
        find(data, "🌍".as_bytes());
    }
    let expected = vec![
        "created resp_fixture".to_string(),
        "delta café 🌍".to_string(),
        "completed resp_fixture Some(\"completed\")".to_string(),
    ];
    for data in [lf, crlf] {
        let replies: Vec<Reply> = layouts(&data).into_iter().map(sse).collect();
        let count = replies.len();
        let (server, client) = setup(replies).await;
        for layout in 0..count {
            let (seen, end) = read_stream(&client).await;
            assert!(end.is_ok(), "layout {layout}: {end:?}");
            assert_eq!(seen, expected, "layout {layout}");
        }
        let requests = server.requests();
        assert_eq!(requests.len(), count);
        assert!(requests.iter().all(|r| r.target == "/v1/responses"
            && r.body == br#"{"model":"fixture-model","input":"hello","stream":true}"#));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn split_error_event_and_split_eof_stay_typed_failures() {
    find(ERROR_EVENTS, "—".as_bytes());
    for data in [ERROR_EVENTS.to_vec(), crlf(ERROR_EVENTS)] {
        let replies: Vec<Reply> = layouts(&data).into_iter().map(sse).collect();
        let count = replies.len();
        let (server, client) = setup(replies).await;
        for layout in 0..count {
            let (seen, end) = read_stream(&client).await;
            assert_eq!(
                seen,
                ["created resp_split_error", "delta partiel é"],
                "layout {layout}"
            );
            match end {
                Err(Error::StreamEventError { message, code }) => {
                    assert_eq!(message, "Synthetic failure — réessayez 🌍");
                    assert_eq!(code, Some(json!("server_error")));
                }
                other => panic!("layout {layout}: {other:?}"),
            }
        }
        server.shutdown().await;
    }
    // A clean end of the body in the middle of a 4-byte character is not completion.
    let inside = find(EVENTS, "🌍".as_bytes()) + 2;
    let (server, client) = setup(layouts(&EVENTS[..inside]).into_iter().map(sse).collect()).await;
    for layout in 0..inside {
        let (seen, end) = read_stream(&client).await;
        assert_eq!(seen, ["created resp_fixture"], "layout {layout}");
        assert!(
            matches!(end, Err(Error::Stream(_))),
            "layout {layout}: {end:?}"
        );
    }
    server.shutdown().await;
}

fn custom_id(line: &[u8]) -> String {
    serde_json::from_slice::<Value>(line).unwrap()["custom_id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Whole lines (with their `\n`) keyed by `custom_id`, in file order.
fn rows(file: &[u8]) -> Vec<(String, Vec<u8>)> {
    file.split_inclusive(|b| *b == b'\n')
        .map(|line| (custom_id(line), line.to_vec()))
        .collect()
}

fn permutations(items: &[&str]) -> Vec<Vec<String>> {
    if items.len() <= 1 {
        return vec![items.iter().map(|s| s.to_string()).collect()];
    }
    let mut all = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, first.to_string());
            all.push(tail);
        }
    }
    all
}

fn outcome(item: &BatchItem) -> String {
    match (&item.response, &item.error) {
        (Some(r), None) => {
            let text = r
                .body
                .pointer("/output/0/content/0/text")
                .or_else(|| r.body.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let request = r.request_id.as_deref().unwrap_or("-");
            format!("{} {} {request} {text}", item.id, r.status_code)
        }
        (None, Some(e)) => {
            let code = e.code.as_deref().unwrap_or("-");
            format!("{} error {code} {}", item.id, e.message)
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn batch_rows_correlate_by_custom_id_in_every_reordered_layout() {
    let inputs = ["request-1", "request-2", "request-3", "request-4"].map(|id| {
        BatchInput::response(
            id,
            ResponseRequest::new("fixture-model", format!("input {id}")),
        )
    });
    batch_jsonl(&inputs).unwrap();
    let mut by_id: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let output_order: Vec<String> = rows(OUTPUT)
        .into_iter()
        .map(|(id, line)| {
            by_id.insert(id.clone(), line);
            id
        })
        .collect();
    for (id, line) in rows(ERRORS) {
        by_id.insert(id, line);
    }
    // The fixture order is not the input order, and the failed item sits between successes:
    // a positional pairing of inputs and rows would attach outcomes to the wrong requests.
    assert_eq!(output_order, ["request-4", "request-2", "request-1"]);

    // Layouts: the failed request-2 among the output lines (as the fixture has it), and moved to
    // the error file (where the official guide says failed requests' error information goes).
    let mut files: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for output in permutations(&["request-1", "request-2", "request-4"]) {
        files.push((output, vec!["request-3".into()]));
    }
    for output in permutations(&["request-1", "request-4"]) {
        for errors in permutations(&["request-2", "request-3"]) {
            files.push((output.clone(), errors));
        }
    }
    assert!(files.iter().any(|(o, _)| *o == output_order));
    let join = |ids: &[String]| {
        ids.iter()
            .flat_map(|id| by_id[id].clone())
            .collect::<Vec<_>>()
    };
    let replies = files
        .iter()
        .flat_map(|(output, errors)| {
            [
                Reply::new(200).with_body(every_byte(&join(output))),
                Reply::new(200).with_body(every_byte(&join(errors))),
            ]
        })
        // Last: the output file alone, for a batch without an error file.
        .chain([Reply::new(200).with_body(every_byte(OUTPUT))])
        .collect();
    let (server, client) = setup(replies).await;
    let batch = Batch {
        id: "batch_fixture".into(),
        status: BatchStatus::Completed,
        output_file_id: Some("file_output".into()),
        error_file_id: Some("file_error".into()),
        errors: None,
        request_counts: None,
    };
    let expected: BTreeMap<String, String> = [
        ("request-1", "batch_req_1 200 req_1 Bonjour"),
        (
            "request-2",
            "batch_req_2 400 req_2 Synthetic refusal — modèle indisponible",
        ),
        (
            "request-3",
            "batch_req_3 error batch_expired Synthetic expiry — délai dépassé",
        ),
        ("request-4", "batch_req_4 200 req_4 Grüße 🌍"),
    ]
    .into_iter()
    .map(|(id, outcome)| (id.to_string(), outcome.to_string()))
    .collect();
    let submitted: Vec<&str> = inputs.iter().map(|i| i.custom_id.as_str()).collect();
    for (output, errors) in &files {
        let results = client.batch_results(&batch, 64 * 1024).await.unwrap();
        let layout = format!("output {output:?}, errors {errors:?}");
        assert!(
            results.problems.is_empty(),
            "{layout}: {:?}",
            results.problems
        );
        assert!(results.file_errors.is_empty(), "{layout}");
        // Every submitted ID has exactly one result, and no result lacks a submitted ID.
        assert_eq!(
            results.items.keys().map(String::as_str).collect::<Vec<_>>(),
            submitted,
            "{layout}"
        );
        let seen: BTreeMap<String, String> = results
            .items
            .iter()
            .map(|(id, item)| {
                assert_eq!(&item.custom_id, id);
                (id.clone(), outcome(item))
            })
            .collect();
        assert_eq!(seen, expected, "{layout}");
    }
    // A request with no row in either file is absent from `items`, not reported: the caller
    // finds it by comparing the keys with the submitted IDs (README, Batch).
    let without_errors = Batch {
        error_file_id: None,
        ..batch
    };
    let results = client
        .batch_results(&without_errors, 64 * 1024)
        .await
        .unwrap();
    assert!(results.problems.is_empty() && results.file_errors.is_empty());
    let missing: Vec<&str> = submitted
        .iter()
        .copied()
        .filter(|id| !results.items.contains_key(*id))
        .collect();
    assert_eq!(missing, ["request-3"]);
    assert_eq!(outcome(&results.items["request-2"]), expected["request-2"]);

    let requests = server.requests();
    assert_eq!(requests.len(), files.len() * 2 + 1);
    let (last, pairs) = requests.split_last().unwrap();
    for pair in pairs.chunks(2) {
        assert_eq!(pair[0].target, "/v1/files/file_output/content");
        assert_eq!(pair[1].target, "/v1/files/file_error/content");
    }
    assert_eq!(last.target, "/v1/files/file_output/content");
    server.shutdown().await;
}
