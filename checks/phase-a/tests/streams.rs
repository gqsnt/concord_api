//! Truthful incremental outputs: `-> stream T` returns the reader, `-> stream` the unread body;
//! readers compose with standard streams without losing limits, error kinds, cancellation state
//! or completion.

use std::time::Duration;

use concord::ErrorKind;
use concord::csv::{CsvOptions, CsvReader, CsvWriter, HeaderMode, StreamRows};
use concord::jsonl::{JsonLinesOptions, JsonLinesReader, RecordErrors};
use concord::stream::{Completion, ReadErrorKind};
use concord_check_phase_a::{Event, Reports, ReportsConfig, Row};
use concord_example_testserver::{Body, Reply, TestServer};
use futures_util::{StreamExt, TryStreamExt};

fn api(server: &TestServer) -> Reports {
    Reports::new(
        concord::reqwest::Client::new(),
        ReportsConfig {
            token: "Bearer t".to_owned(),
        },
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
}

fn chunks(parts: &[(&'static str, u64)]) -> Body {
    Body::Chunks(
        parts
            .iter()
            .map(|(text, ms)| {
                (
                    Duration::from_millis(*ms),
                    concord::bytes::Bytes::from_static(text.as_bytes()),
                )
            })
            .collect(),
    )
}

#[tokio::test]
async fn a_stream_endpoint_returns_its_reader_before_any_row() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv; header=present")
            .with_body(chunks(&[
                ("id;amount\n1;1", 0),
                ("0\n2;20\n", 0),
                ("3;x\n", 0),
            ]))
    })
    .await;
    let mut rows = api(&server).rows("r-1").send().await.unwrap();
    assert_eq!(rows.position().records, 0, "opening reads no row");
    assert_eq!(
        server.single().header("accept"),
        Some("text/csv"),
        "the format's Accept"
    );
    assert_eq!(
        rows.next().await.unwrap().unwrap(),
        Row { id: 1, amount: 10 }
    );
    assert_eq!(
        rows.next().await.unwrap().unwrap(),
        Row { id: 2, amount: 20 }
    );
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(
        error.operation(),
        Some("Reports.rows"),
        "identity reaches the reader"
    );
    assert!(error.is_terminal());
    assert_eq!(error.position().records, 2);
    assert!(rows.next().await.is_none());
    assert_eq!(
        rows.completion(),
        Completion::Failed,
        "an error is not the end of input"
    );
}

#[tokio::test]
async fn a_refused_stream_head_keeps_the_response_unread() {
    let server = TestServer::start(|_| Reply::json(200, "[1,2]")).await;
    let error = api(&server).rows("r").send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.endpoint(), Some("Reports.rows"));
    assert_eq!(
        error.into_response().unwrap().text().await.unwrap(),
        "[1,2]"
    );

    let server = TestServer::start(|_| Reply::json(410, "{\"reason\":\"Expired\"}")).await;
    let error = api(&server).events().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(410));
}

#[tokio::test]
async fn watch_events_split_anywhere_and_end_cleanly() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(chunks(&[
                ("{\"type\":\"ADDED\",\"na", 0),
                ("me\":\"caf\u{e9}", 5),
                ("\"}\n{\"type\":\"DELETED\",\"name\":\"b\"}", 5),
            ]))
    })
    .await;
    let mut events = api(&server).events().send().await.unwrap();
    let mut seen = Vec::new();
    while let Some(event) = events.next().await {
        seen.push(event.unwrap());
    }
    assert_eq!(
        seen,
        [
            Event {
                kind: "ADDED".into(),
                name: "caf\u{e9}".into()
            },
            Event {
                kind: "DELETED".into(),
                name: "b".into()
            },
        ]
    );
    assert_eq!(events.completion(), Completion::EndOfInput);
}

#[tokio::test]
async fn an_incoming_body_opens_with_a_runtime_reader_and_keeps_identity() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .body("7|70\n8|eighty\n")
    })
    .await;
    let incoming = api(&server).export("job-1").send().await.unwrap();
    assert_eq!(incoming.operation().unwrap().name(), "Reports.export");
    let reader = CsvReader::new(
        CsvOptions::new(HeaderMode::Absent, b'|')
            .read_mapping(concord::csv::ReadMapping::columns(["id", "amount"])),
    )
    .unwrap();
    let mut rows = reader.open::<Row>(incoming).unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap(),
        Row { id: 7, amount: 70 }
    );
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(error.operation(), Some("Reports.export"));
}

#[tokio::test]
async fn a_dropped_pending_next_loses_nothing() {
    // Row 2 is split across chunks with a pause between them: the first `next()` after row 1 is
    // cancelled mid-row, the following one completes the same row.
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .with_body(chunks(&[("id;amount\n1;10\n2;", 0), ("20\n", 300)]))
    })
    .await;
    let mut stream = api(&server).rows("r").send().await.unwrap().into_stream();
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Row { id: 1, amount: 10 }
    );
    let cancelled = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
    assert!(
        cancelled.is_err(),
        "the read was pending and its future was dropped"
    );
    assert_eq!(stream.completion(), Completion::Reading);
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Row { id: 2, amount: 20 }
    );
    assert!(stream.next().await.is_none());
    assert_eq!(stream.completion(), Completion::EndOfInput);
    assert_eq!(stream.position().records, 2);
}

#[tokio::test]
async fn recoverable_line_errors_do_not_end_the_stream() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/jsonl")
            .body("{\"id\":1,\"amount\":1}\nnot json\n{\"id\":2,\"amount\":2}\n")
    })
    .await;
    let incoming = api(&server).export("j").send().await.unwrap();
    let reader =
        JsonLinesReader::new(JsonLinesOptions::json_lines().record_errors(RecordErrors::Yield));
    let items: Vec<_> = reader
        .open::<Row>(incoming)
        .unwrap()
        .into_stream()
        .collect()
        .await;
    assert_eq!(items.len(), 3);
    let error = items[1].as_ref().unwrap_err();
    assert!(!error.is_terminal(), "a skipped line");
    assert_eq!(error.operation(), Some("Reports.export"));
    assert_eq!(items[2].as_ref().unwrap(), &Row { id: 2, amount: 2 });
}

#[tokio::test]
async fn a_limit_ends_the_stream_as_failed_not_finished() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/jsonl")
            .body("{\"id\":1,\"amount\":1}\n{\"id\":2,\"amount\":2}\n{\"id\":3,\"amount\":3}\n")
    })
    .await;
    let incoming = api(&server).export("j").send().await.unwrap();
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines().max_records(2));
    let mut stream = reader.open::<Row>(incoming).unwrap().into_stream();
    // `try_collect` stops at the first error: caller policy, and the stream says why.
    let collected: Result<Vec<Row>, _> = (&mut stream).try_collect().await;
    let error = collected.unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Limit);
    assert!(error.is_terminal());
    assert_eq!(stream.completion(), Completion::Failed);
    assert_eq!(stream.position().records, 2);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn dropping_before_the_end_is_not_end_of_input() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .with_body(chunks(&[("id;amount\n1;10\n", 0), ("2;20\n", 200)]))
    })
    .await;
    let mut rows = api(&server).rows("r").send().await.unwrap();
    rows.next().await.unwrap().unwrap();
    assert_eq!(rows.completion(), Completion::Reading);
    drop(rows);
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.live_connections() == 0)
            .await
    );
}

#[tokio::test]
async fn download_transform_upload_round_trip() {
    let server = TestServer::start(|r| {
        if r.method == "GET" {
            Reply::new(200)
                .header("content-type", "text/csv; header=present")
                .body("id;amount\n1;10\n2;20\n")
        } else {
            Reply::json(200, "{\"id\":0,\"amount\":2}")
        }
    })
    .await;
    let api = api(&server);
    // Reader → standard stream transformation → runtime writer → declared upload body.
    let doubled = api
        .rows("src")
        .send()
        .await
        .unwrap()
        .into_stream()
        .map_ok(|row| Row {
            id: row.id,
            amount: row.amount * 2,
        })
        .map_err(Box::<dyn std::error::Error + Send + Sync>::from);
    let writer = CsvWriter::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let upload = writer.upload(StreamRows::new(doubled)).unwrap();
    let stored = api.upload("dst", upload).send().await.unwrap();
    assert_eq!(stored.amount, 2);
    let put = server
        .requests()
        .into_iter()
        .find(|r| r.method == "POST")
        .unwrap();
    assert_eq!(put.header("content-type"), Some("text/csv; header=present"));
    // The writer's default terminator is CRLF (RFC 4180).
    assert_eq!(put.body_text(), "id,amount\r\n1,20\r\n2,40\r\n");
}

#[tokio::test]
async fn a_watch_reports_the_download_outcome_after_the_upload() {
    let server = TestServer::start(|r| {
        if r.method == "POST" {
            return Reply::json(200, "{\"id\":0,\"amount\":0}");
        }
        let body = if r.target.contains("broken") {
            "id;amount\n1;10\n2;x\n"
        } else {
            "id;amount\n1;10\n2;20\n"
        };
        Reply::new(200)
            .header("content-type", "text/csv")
            .body(body)
    })
    .await;
    let api = api(&server);
    for (id, upload_ok, completion) in [
        ("whole", true, Completion::EndOfInput),
        ("broken", false, Completion::Failed),
    ] {
        let mut rows = api.rows(id).send().await.unwrap().into_stream();
        let watch = rows.watch();
        let writer = CsvWriter::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
        let upload = writer
            .upload(StreamRows::new(
                rows.map_err(Box::<dyn std::error::Error + Send + Sync>::from),
            ))
            .unwrap();
        let sent = api.upload("dst", upload).send().await;
        assert_eq!(sent.is_ok(), upload_ok, "{id}: {sent:?}");
        assert_eq!(watch.completion(), completion, "{id}");
        if !upload_ok {
            assert_eq!(watch.failure(), Some(ReadErrorKind::Decode));
            assert_eq!(watch.position().records, 1);
        }
    }
}

#[tokio::test]
async fn a_watch_made_after_the_failure_still_reports_it() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "text/csv")
            .body("id;amount\n1;x\n")
    })
    .await;
    let mut stream = api(&server).rows("r").send().await.unwrap().into_stream();
    assert!(stream.next().await.unwrap().is_err());
    assert_eq!(stream.failure(), Some(ReadErrorKind::Decode));
    let watch = stream.watch();
    assert_eq!(watch.completion(), Completion::Failed);
    assert_eq!(watch.failure(), Some(ReadErrorKind::Decode));
}
