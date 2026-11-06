//! Independent application consumer: only the SDK's README and public rustdoc supplied its API.
//! Fixtures are hand-authored; this is a local protocol fake, not provider conformance evidence.

use std::convert::Infallible;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::stream::ReadErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_clickhouse::{
    CallContext, ClickHouse, Credentials, Error, Identifier, LateSource, Outcome, OutputFormat,
    Query, QueryId, Table, stream,
};
use serde::{Deserialize, Serialize};

const JSON: &[u8] = include_bytes!("../fixtures/first-use/stock.jsonl");
const CSV: &[u8] = include_bytes!("../fixtures/first-use/stock.csv");
const EXCEPTION: &[u8] = include_bytes!("../fixtures/first-use/late-exception.bin");
const SERVER_ERROR: &[u8] = include_bytes!("../fixtures/first-use/server-error.txt");
const TAG: &str = "firstusefixturex";
const TENANT: &str = "demo\t'&?+\\\n";
const SELECT: &str = "SELECT seq, label, quantity FROM inventory WHERE tenant = {tenant:String} AND quantity >= {minimum:UInt16} ORDER BY seq";
const VERIFY: &str = "SELECT seq, label, quantity FROM {table:Identifier} ORDER BY seq";
const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StockRow {
    seq: u32,
    label: String,
    quantity: u16,
}

fn stock() -> Vec<StockRow> {
    vec![
        StockRow {
            seq: 1,
            label: "café, \"Paris\"\nwarehouse".into(),
            quantity: 7,
        },
        StockRow {
            seq: 2,
            label: "plain".into(),
            quantity: 11,
        },
    ]
}

fn format(csv: bool) -> OutputFormat {
    if csv {
        OutputFormat::CsvWithNames
    } else {
        OutputFormat::JsonEachRow
    }
}

fn wire_format(csv: bool) -> &'static str {
    if csv { "CSVWithNames" } else { "JSONEachRow" }
}

fn media_type(csv: bool) -> &'static str {
    if csv {
        "text/csv; charset=UTF-8; header=present"
    } else {
        "application/x-ndjson; charset=UTF-8"
    }
}

fn fixture(csv: bool) -> &'static [u8] {
    if csv { CSV } else { JSON }
}

fn query() -> Query {
    Query::new(SELECT)
        .bind("tenant", TENANT)
        .unwrap()
        .bind("minimum", 7_u16)
        .unwrap()
}

fn context() -> CallContext {
    CallContext::new()
        .database(Identifier::new("fixture_db").unwrap())
        .quota_key("synthetic-loader")
        .unwrap()
}

fn client(server: &TestServer) -> ClickHouse {
    // Explicitly synthetic header credentials; no proxy/redirect can send them off loopback.
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .timeout(DEADLINE)
        .build()
        .unwrap();
    ClickHouse::new(
        http,
        Credentials::headers("synthetic-first-use-user", "not-a-real-password").unwrap(),
    )
    .with_base_url(&server.url())
    .unwrap()
    .with_database(Identifier::new("default_fixture").unwrap())
}

fn head(csv: bool) -> Reply {
    Reply::new(200)
        .header("Content-Type", media_type(csv))
        .header("X-ClickHouse-Format", wire_format(csv))
        .header("X-ClickHouse-Exception-Tag", TAG)
        .header("X-ClickHouse-Query-Id", "synthetic-response-id")
}

fn chunks(bytes: &[u8]) -> Body {
    Body::Chunks(
        bytes
            .chunks(7)
            .map(|b| (Duration::from_millis(1), b.to_vec().into()))
            .collect(),
    )
}

fn assert_auth(request: &Recorded) {
    assert_eq!(request.path(), "/");
    assert_eq!(
        request.header("X-ClickHouse-User"),
        Some("synthetic-first-use-user")
    );
    assert_eq!(
        request.header("X-ClickHouse-Key"),
        Some("not-a-real-password")
    );
    assert!(request.query_all("user").is_empty());
    assert!(request.query_all("password").is_empty());
}

async fn shutdown(server: TestServer) {
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn select_runtime(csv: bool) {
    let server = TestServer::start(move |_| {
        head(csv)
            .header(
                "X-ClickHouse-Summary",
                r#"{"read_rows":"2","result_rows":"2"}"#,
            )
            .header("X-ClickHouse-Progress", r#"{"read_rows":"1"}"#)
            .header("X-ClickHouse-Progress", r#"{"read_rows":"2"}"#)
            .with_body(chunks(fixture(csv)))
    })
    .await;
    let api = client(&server);
    let mut rows = api
        .select(query(), format(csv))
        .query_id(QueryId::new("synthetic-select/1").unwrap())
        .context(&context())
        .wait_end_of_query(true)
        .progress_headers(true)
        .rows::<StockRow>()
        .await
        .unwrap();
    assert_eq!(rows.outcome(), Outcome::Reading);
    assert_eq!(rows.meta().query_id(), Some("synthetic-response-id"));
    assert_eq!(rows.meta().summary().unwrap().unwrap().read_rows, Some(2));
    let progress = rows.meta().progress().unwrap();
    assert_eq!(
        progress.iter().map(|p| p.read_rows).collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert_eq!(rows.next().await.unwrap().unwrap(), stock()[0]);
    // A consumed prefix is not proof of completion, even with the success marker announced.
    assert_eq!(rows.outcome(), Outcome::Reading);
    assert_eq!(rows.position().records, 1);
    if csv {
        assert_eq!(rows.columns().unwrap(), ["seq", "label", "quantity"]);
    } else {
        assert!(rows.columns().is_none());
    }
    assert_eq!(rows.next().await.unwrap().unwrap(), stock()[1]);
    assert!(rows.next().await.is_none());
    assert_eq!(rows.outcome(), Outcome::Complete);
    assert!(rows.next().await.is_none());
    assert_eq!(rows.position().records, 2);

    let request = server.single();
    assert_auth(&request);
    assert_eq!(request.method, "GET");
    assert!(request.body.is_empty());
    assert_eq!(request.query_all("query"), [SELECT]);
    assert_eq!(request.query_all("param_tenant"), ["demo\\t'&?+\\\\\\n"]);
    assert_eq!(request.query_all("param_minimum"), ["7"]);
    assert_eq!(request.query_all("query_id"), ["synthetic-select/1"]);
    assert_eq!(request.query_all("quota_key"), ["synthetic-loader"]);
    assert_eq!(request.query_all("wait_end_of_query"), ["1"]);
    assert_eq!(request.query_all("send_progress_in_http_headers"), ["1"]);
    assert_eq!(request.header("X-ClickHouse-Database"), Some("fixture_db"));
    assert_eq!(
        request.header("X-ClickHouse-Format"),
        Some(wire_format(csv))
    );
    shutdown(server).await;
}

#[tokio::test]
async fn parameterized_select_runtime_csv() {
    select_runtime(true).await;
}

#[tokio::test]
async fn parameterized_select_runtime_json() {
    select_runtime(false).await;
}

#[tokio::test]
async fn eof_without_marker_is_distinct_from_complete_including_empty_results() {
    for csv in [false, true] {
        for empty in [false, true] {
            let server = TestServer::start(move |_| {
                let body = if empty {
                    if csv {
                        b"seq,label,quantity\n".as_slice()
                    } else {
                        b"".as_slice()
                    }
                } else {
                    fixture(csv)
                };
                Reply::text(200, media_type(csv), body)
                    .header("X-ClickHouse-Format", wire_format(csv))
            })
            .await;
            let result = client(&server)
                .select(query(), format(csv))
                .rows::<StockRow>()
                .await
                .unwrap()
                .collect()
                .await
                .unwrap();
            assert_eq!(result.rows, if empty { vec![] } else { stock() });
            assert_eq!(result.outcome, Outcome::EndedWithoutMarker);
            assert!(result.meta.query_id().is_none());
            assert!(result.meta.summary().unwrap().is_none());
            assert_eq!(server.requests().len(), 1);
            shutdown(server).await;
        }
    }
}

#[tokio::test]
async fn missing_binding_is_refused_before_any_request() {
    let server = TestServer::start(|_| panic!("invalid query must not be sent")).await;
    let error = client(&server)
        .select(Query::new(SELECT), format(false))
        .rows::<StockRow>()
        .await
        .unwrap_err();
    assert!(matches!(&error, Error::Request(_)));
    assert!(!error.is_outcome_unknown());
    assert!(error.meta().is_none());
    assert!(server.requests().is_empty());
    shutdown(server).await;
}

#[tokio::test]
async fn provider_error_keeps_status_exception_and_metadata() {
    for code_header in [false, true] {
        let server = TestServer::start(move |_| {
            let reply = Reply::text(404, "text/plain; charset=UTF-8", SERVER_ERROR)
                .header("X-ClickHouse-Query-Id", "synthetic-rejected-select");
            if code_header {
                reply.header("X-ClickHouse-Exception-Code", "60")
            } else {
                reply
            }
        })
        .await;
        let error = client(&server)
            .select(query(), format(false))
            .rows::<StockRow>()
            .await
            .unwrap_err();
        assert!(!error.is_outcome_unknown());
        let Error::Server(problem) = &error else {
            panic!("expected server exception: {error:?}")
        };
        assert_eq!(problem.status().as_u16(), 404);
        assert_eq!(problem.code(), Some(60));
        assert_eq!(problem.name(), Some("UNKNOWN_TABLE"));
        assert_eq!(
            problem.message().unwrap(),
            std::str::from_utf8(SERVER_ERROR).unwrap().trim_end()
        );
        assert_eq!(problem.meta().query_id(), Some("synthetic-rejected-select"));
        assert!(!error.to_string().contains("synthetic inventory"));
        assert_eq!(server.single().query_all("query"), [SELECT]);
        shutdown(server).await;
    }
}

#[tokio::test]
async fn wrong_format_or_media_type_is_refused_without_waiting_for_body() {
    for wrong_format in [false, true] {
        let server = TestServer::start(move |_| {
            Reply::new(200)
                .header(
                    "X-ClickHouse-Format",
                    if wrong_format {
                        "CSVWithNames"
                    } else {
                        "JSONEachRow"
                    },
                )
                .header(
                    "Content-Type",
                    if wrong_format {
                        media_type(false)
                    } else {
                        "text/html"
                    },
                )
                .header("X-ClickHouse-Query-Id", "synthetic-refused-head")
                .with_body(Body::Stall)
        })
        .await;
        let api = client(&server);
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            api.select(query(), format(false)).rows::<StockRow>(),
        )
        .await
        .expect("head refusal must not read stalled body")
        .unwrap_err();
        assert!(matches!(&error, Error::Response(_)));
        assert_eq!(
            error.meta().unwrap().query_id(),
            Some("synthetic-refused-head")
        );
        assert_eq!(server.body_bytes_sent(), 0);
        shutdown(server).await;
    }
}

async fn late_select(csv: bool) {
    // Include all rows before the tagged block, then break chunk framing as documented for 24.11+.
    let server = TestServer::start(move |_| {
        let bytes = [fixture(csv), EXCEPTION].concat();
        head(csv).with_body(Body::FailAfter(
            bytes.chunks(1).map(|b| b.to_vec().into()).collect(),
        ))
    })
    .await;
    let mut rows = client(&server)
        .select(query(), format(csv))
        .rows::<StockRow>()
        .await
        .unwrap();
    let mut kept = Vec::new();
    let error = loop {
        match rows.next().await {
            Some(Ok(row)) => kept.push(row),
            Some(Err(error)) => break error,
            None => panic!("late error was reported as successful EOF"),
        }
    };
    assert_eq!(kept, stock());
    assert!(error.is_outcome_unknown());
    let Error::LateException(problem) = &error else {
        panic!("expected tagged exception: {error:?}")
    };
    assert_eq!(problem.code(), Some(395));
    assert_eq!(problem.name(), Some("FUNCTION_THROW_IF_VALUE_IS_NON_ZERO"));
    assert_eq!(problem.rows_before(), 2);
    assert!(matches!(
        problem.source(),
        LateSource::Block { complete: true }
    ));
    assert_eq!(problem.meta().query_id(), Some("synthetic-response-id"));
    assert!(problem.message().contains("synthetic first-use failure"));
    assert_eq!(rows.outcome(), Outcome::Failed);
    assert!(rows.next().await.is_none());
    assert_eq!(server.requests().len(), 1);
    shutdown(server).await;
}

#[tokio::test]
async fn csv_late_exception_preserves_delivered_rows() {
    late_select(true).await;
}

#[tokio::test]
async fn json_late_exception_preserves_delivered_rows() {
    late_select(false).await;
}

#[tokio::test]
async fn broken_body_after_rows_is_transport_failure_not_complete() {
    for csv in [false, true] {
        let server = TestServer::start(move |_| {
            head(csv).with_body(Body::FailAfter(vec![fixture(csv).to_vec().into()]))
        })
        .await;
        let mut rows = client(&server)
            .select(query(), format(csv))
            .rows::<StockRow>()
            .await
            .unwrap();
        for expected in stock() {
            assert_eq!(rows.next().await.unwrap().unwrap(), expected);
        }
        let error = rows.next().await.unwrap().unwrap_err();
        assert!(error.is_outcome_unknown());
        let Error::Read(problem) = &error else {
            panic!("expected read failure: {error:?}")
        };
        assert_eq!(problem.kind(), ReadErrorKind::Transport);
        assert_eq!(problem.position().records, 2);
        assert_eq!(problem.meta().query_id(), Some("synthetic-response-id"));
        assert_eq!(rows.outcome(), Outcome::Failed);
        assert!(rows.next().await.is_none());
        assert_eq!(server.requests().len(), 1);
        shutdown(server).await;
    }
}

#[tokio::test]
async fn undecodable_row_is_terminal_and_preserves_prefix() {
    for csv in [false, true] {
        let server = TestServer::start(move |_| {
            let bad = if csv {
                b"\"3\",\"bad\",\"not-an-integer\"\n".as_slice()
            } else {
                b"{\"seq\":3,\"label\":\"bad\",\"quantity\":\"not-an-integer\"}\n".as_slice()
            };
            head(csv).body([fixture(csv), bad].concat())
        })
        .await;
        let mut rows = client(&server)
            .select(query(), format(csv))
            .rows::<StockRow>()
            .await
            .unwrap();
        for expected in stock() {
            assert_eq!(rows.next().await.unwrap().unwrap(), expected);
        }
        let error = rows.next().await.unwrap().unwrap_err();
        let Error::Read(problem) = &error else {
            panic!("expected decode failure: {error:?}")
        };
        assert_eq!(problem.kind(), ReadErrorKind::Decode);
        assert_eq!(problem.position().records, 2);
        assert_eq!(rows.outcome(), Outcome::Failed);
        assert!(rows.next().await.is_none());
        shutdown(server).await;
    }
}

async fn insert_and_query_back(csv: bool) {
    // A bounded, stateful fake: stores only the documented body after validating exact wire bytes.
    // A SELECT before a valid insert cannot manufacture a successful query-back result.
    let stored = Arc::new(Mutex::new(None::<Vec<u8>>));
    let provider_store = Arc::clone(&stored);
    let server = TestServer::start(move |request| {
        assert_auth(request);
        assert_eq!(request.header("X-ClickHouse-Database"), Some("fixture_db"));
        assert_eq!(request.query_all("quota_key"), ["synthetic-loader"]);
        match request.method.as_str() {
            "POST" => {
                assert_eq!(
                    request.query_all("query"),
                    [format!(
                        "INSERT INTO `inventory_load` FORMAT {}",
                        wire_format(csv)
                    )]
                );
                assert_eq!(request.query_all("query_id"), ["synthetic-load/insert"]);
                assert_eq!(request.body, fixture(csv));
                assert!(request.chunked);
                assert_eq!(
                    request.header("Content-Type"),
                    Some(if csv {
                        "text/csv; header=present"
                    } else {
                        "application/x-ndjson"
                    })
                );
                *provider_store.lock().unwrap() = Some(request.body.clone());
                Reply::new(200)
                    .header("X-ClickHouse-Query-Id", "synthetic-load/insert")
                    .header("X-ClickHouse-Exception-Tag", TAG)
                    .header(
                        "X-ClickHouse-Summary",
                        r#"{"written_rows":"2","written_bytes":"123"}"#,
                    )
            }
            "GET" => {
                assert_eq!(request.query_all("query"), [VERIFY]);
                assert_eq!(request.query_all("param_table"), ["inventory_load"]);
                assert_eq!(request.query_all("query_id"), ["synthetic-load/verify"]);
                assert_eq!(
                    request.header("X-ClickHouse-Format"),
                    Some(wire_format(csv))
                );
                assert!(request.body.is_empty());
                let body = provider_store
                    .lock()
                    .unwrap()
                    .clone()
                    .expect("query back requires a completed insert");
                head(csv).with_body(chunks(&body))
            }
            _ => panic!("unexpected request: {}", request.method),
        }
    })
    .await;
    let api = client(&server);
    let table = Identifier::new("inventory_load").unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let source_polls = Arc::clone(&polls);
    let source = stream::from_iter(stock().into_iter().map(move |row| {
        source_polls.fetch_add(1, Ordering::SeqCst);
        Ok::<_, Infallible>(row)
    }));
    let insert = api
        .insert(Table::new(table.clone()))
        .query_id(QueryId::new("synthetic-load/insert").unwrap())
        .context(&context());
    let upload = async {
        if csv {
            insert.csv_with_names(source).await
        } else {
            insert.json_each_row(source).await
        }
    };
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let receipt = upload.await.unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert_eq!(receipt.query_id(), Some("synthetic-load/insert"));
    assert_eq!(receipt.summary().unwrap().unwrap().written_rows, Some(2));
    assert_eq!(receipt.body_bytes(), 0);
    assert_eq!(stored.lock().unwrap().as_deref(), Some(fixture(csv)));
    let result = api
        .select(
            Query::new(VERIFY).bind("table", &table).unwrap(),
            format(csv),
        )
        .query_id(QueryId::new("synthetic-load/verify").unwrap())
        .context(&context())
        .rows::<StockRow>()
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(result.rows, stock());
    assert_eq!(result.outcome, Outcome::Complete);
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "GET"]
    );
    shutdown(server).await;
}

#[tokio::test]
async fn stream_json_insert_and_query_back() {
    insert_and_query_back(false).await;
}

#[tokio::test]
async fn stream_csv_insert_and_query_back() {
    insert_and_query_back(true).await;
}

#[tokio::test]
async fn rejected_insert_returns_provider_exception_without_retry() {
    let server = TestServer::start(|_| {
        Reply::text(404, "text/plain", SERVER_ERROR)
            .header("X-ClickHouse-Query-Id", "synthetic-rejected-insert")
    })
    .await;
    let error = client(&server)
        .insert(Table::new(Identifier::new("inventory_load").unwrap()))
        .json_each_row(stream::from_iter(
            stock().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap_err();
    let Error::Server(problem) = &error else {
        panic!("expected server rejection: {error:?}")
    };
    assert_eq!(problem.code(), Some(60));
    assert_eq!(problem.status().as_u16(), 404);
    assert_eq!(problem.meta().query_id(), Some("synthetic-rejected-insert"));
    assert!(!error.is_outcome_unknown());
    assert_eq!(server.single().method, "POST");
    shutdown(server).await;
}

#[tokio::test]
async fn failed_upload_source_reports_unknown_outcome() {
    let server =
        TestServer::start(|_| panic!("incomplete body must not become an accepted insert")).await;
    let api = client(&server);
    let polls = Arc::new(AtomicUsize::new(0));
    let source_polls = Arc::clone(&polls);
    let source = stream::from_iter(
        [
            Ok(stock().remove(0)),
            Err(io::Error::other("synthetic source stopped after one row")),
        ]
        .into_iter()
        .inspect(move |_| {
            source_polls.fetch_add(1, Ordering::SeqCst);
        }),
    );
    let upload = api
        .insert(Table::new(Identifier::new("inventory_load").unwrap()))
        .json_each_row(source);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let error = upload.await.unwrap_err();
    assert!(matches!(&error, Error::Concord(_)));
    assert!(error.is_outcome_unknown());
    assert_eq!(polls.load(Ordering::SeqCst), 2);
    assert!(server.requests().is_empty());
    shutdown(server).await;
}

#[tokio::test]
async fn insert_broken_receipt_body_reports_unknown_outcome() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("X-ClickHouse-Query-Id", "synthetic-broken-receipt")
            .with_body(Body::FailAfter(vec![]))
    })
    .await;
    // Retain the outbound ID independently: the failure below does not carry response metadata.
    let requested_id = "synthetic-broken-receipt";
    let error = client(&server)
        .insert(Table::new(Identifier::new("inventory_load").unwrap()))
        .query_id(QueryId::new(requested_id).unwrap())
        .json_each_row(stream::from_iter(
            stock().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap_err();
    assert!(error.is_outcome_unknown());
    // Observed public API: this broken receipt returns a transport error without metadata.
    let Error::Concord(transport) = &error else {
        panic!("expected transport error: {error:?}")
    };
    assert_eq!(transport.kind(), concord::ErrorKind::Transport);
    assert!(error.meta().is_none());
    let request = server.single();
    assert_eq!(request.body, JSON);
    assert_eq!(request.query_all("query_id"), [requested_id]);
    shutdown(server).await;
}

#[tokio::test]
async fn insert_late_exception_is_not_a_successful_receipt() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("X-ClickHouse-Exception-Tag", TAG)
            .header("X-ClickHouse-Query-Id", "synthetic-late-insert")
            .with_body(Body::FailAfter(
                EXCEPTION.chunks(3).map(|b| b.to_vec().into()).collect(),
            ))
    })
    .await;
    let error = client(&server)
        .insert(Table::new(Identifier::new("inventory_load").unwrap()))
        .csv_with_names(stream::from_iter(
            stock().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap_err();
    let Error::LateException(problem) = &error else {
        panic!("expected late insert exception: {error:?}")
    };
    assert_eq!(problem.code(), Some(395));
    assert_eq!(problem.rows_before(), 0);
    assert!(error.is_outcome_unknown());
    assert_eq!(problem.meta().query_id(), Some("synthetic-late-insert"));
    assert_eq!(server.single().body, CSV);
    shutdown(server).await;
}

#[tokio::test]
async fn missing_insert_metadata_stays_optional() {
    let server = TestServer::start(|_| Reply::new(200)).await;
    let receipt = client(&server)
        .insert(Table::new(Identifier::new("inventory_load").unwrap()))
        .json_each_row(stream::from_iter(
            stock().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap();
    assert!(receipt.query_id().is_none());
    assert!(receipt.summary().unwrap().is_none());
    assert_eq!(receipt.body_bytes(), 0);
    assert_eq!(server.single().body, JSON);
    shutdown(server).await;
}
