//! Independent first-use consumer of `concord-sdk-snowflake`.
//!
//! An application-style suite written from the SDK's README/rustdoc and the Snowflake SQL API v2
//! documentation (submitting-requests, handling-responses, cancelling-requests, and the SQL API
//! reference for the cancel/failure bodies). The fake provider below is my own: every response
//! document is hand-authored from those pages with synthetic handles, IDs, SQL and data. Nothing
//! here is a captured Snowflake response, and no test talks to Snowflake.

use std::collections::HashMap;
use std::io::Write as _;
use std::num::NonZeroUsize;
use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, Script, TestServer};
use concord_sdk_snowflake::pagination::Outcome;
use concord_sdk_snowflake::{
    Client, ClientOptions, Error, LocalStop, PollEnd, PollOutcome, PollPolicy, ProtocolError,
    RequestContext, RequestId, Statement, StatementState, TokenType,
};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------------
// Synthetic identities. None of these is a real handle, request ID or credential.

const TOKEN: &str = "synthetic-first-use-token-not-a-credential";
const USER_AGENT: &str = "first-use-app/0.1";
const HANDLE_A: &str = "01f0a000-0000-4a00-8000-00000000000a";
const HANDLE_B: &str = "01f0b000-0000-4b00-8000-00000000000b";
const SUBMIT_A: &str = "00000000-0000-4000-8000-00000000a001";
const SUBMIT_B: &str = "00000000-0000-4000-8000-00000000b001";
const CANCEL_B: &str = "00000000-0000-4000-8000-00000000b002";
const SQL_A: &str = "select ID, LABEL from FIRST_USE_DEMO order by ID";
const SQL_B: &str = "select count(*) from FIRST_USE_LARGE_DEMO";

// ---------------------------------------------------------------------------------------------
// The fake provider: one scripted queue of replies per route.

/// `METHOD path` plus `?partition=N` when present; `requestId`/`async` are asserted separately.
fn route(request: &Recorded) -> String {
    match request.query_all("partition").as_slice() {
        [] => format!("{} {}", request.method, request.path()),
        [index] => format!("{} {}?partition={index}", request.method, request.path()),
        _ => format!("{} {}?partition=<repeated>", request.method, request.path()),
    }
}

#[derive(Default)]
struct Fake {
    routes: HashMap<String, Script>,
}

impl Fake {
    fn on(mut self, route: &str, replies: impl IntoIterator<Item = Reply>) -> Self {
        let script = self.routes.entry(route.to_owned()).or_default();
        for reply in replies {
            script.push(reply);
        }
        self
    }

    async fn start(self) -> TestServer {
        let routes = self.routes;
        TestServer::start(move |request| match routes.get(&route(request)) {
            Some(script) if script.remaining() > 0 => script.next(),
            // An unscripted request is visible to the test as an unexpected status.
            _ => Reply::text(418, "text/plain", "unscripted route in first_use fake"),
        })
        .await
    }
}

fn submit_route() -> String {
    "POST /api/v2/statements".to_owned()
}
fn status_route(handle: &str) -> String {
    format!("GET /api/v2/statements/{handle}")
}
fn partition_route(handle: &str, index: usize) -> String {
    format!("GET /api/v2/statements/{handle}?partition={index}")
}
fn cancel_route(handle: &str) -> String {
    format!("POST /api/v2/statements/{handle}/cancel")
}

// ---------------------------------------------------------------------------------------------
// Response documents, hand-authored from the SQL API v2 pages.

/// 202 QueryStatus (POST 202 / GET 202 in the reference).
fn pending(handle: &str) -> Reply {
    Reply::json(
        202,
        json!({
            "code": "333334",
            "message": "Asynchronous execution in progress. Use provided query id to perform query monitoring and management.",
            "statementHandle": handle,
            "statementStatusUrl": format!("/api/v2/statements/{handle}"),
        })
        .to_string(),
    )
}

/// Column metadata for the two-column demo table (rowType entries per handling-responses).
fn row_type() -> Value {
    json!([
        {"name": "ID", "database": "FIRST_USE_DB", "schema": "PUBLIC", "table": "FIRST_USE_DEMO",
         "type": "fixed", "nullable": false, "precision": 38, "scale": 0, "length": null, "byteLength": null},
        {"name": "LABEL", "database": "FIRST_USE_DB", "schema": "PUBLIC", "table": "FIRST_USE_DEMO",
         "type": "text", "nullable": true, "precision": null, "scale": null, "length": 16777216, "byteLength": 16777216}
    ])
}

/// 200 ResultSet: partition zero's rows plus `partitionInfo` for every partition.
fn first_result(handle: &str, partition_rows: &[u64], data: Value) -> Reply {
    let partition_info: Vec<Value> = partition_rows
        .iter()
        .enumerate()
        .map(|(i, rows)| {
            if i == 0 {
                json!({"rowCount": rows, "uncompressedSize": 40 * rows})
            } else {
                json!({"rowCount": rows, "uncompressedSize": 40 * rows, "compressedSize": 30 * rows})
            }
        })
        .collect();
    let total: u64 = partition_rows.iter().sum();
    Reply::json(
        200,
        json!({
            "resultSetMetaData": {
                "numRows": total,
                "format": "jsonv2",
                "rowType": row_type(),
                "partitionInfo": partition_info,
            },
            "data": data,
            "code": "090001",
            "statementStatusUrl": format!("/api/v2/statements/{handle}"),
            "sqlState": "00000",
            "statementHandle": handle,
            "message": "Statement executed successfully.",
            "createdOn": 1_762_400_000_000_u64,
        })
        .to_string(),
    )
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).expect("gzip into memory");
    encoder.finish().expect("gzip into memory")
}

/// A later partition: gzip JSON without metadata (handling-responses, "Retrieving additional
/// partitions"). The page shows no complete specimen; I use a `data` envelope.
fn later_partition(data: Value) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json")
        .header("content-encoding", "gzip")
        .body(gzip(json!({ "data": data }).to_string().as_bytes()))
}

/// 422 QueryFailureStatus (reference: POST 422 example code/SQLSTATE).
fn compilation_failure(handle: &str) -> Reply {
    Reply::json(
        422,
        json!({
            "code": "000904",
            "message": "SQL compilation error: error line 1 at position 7\ninvalid identifier 'NOPE'",
            "sqlState": "42000",
            "statementHandle": handle,
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------------------------
// Application helpers.

fn client(server: &TestServer) -> Client {
    let options = ClientOptions {
        user_agent: USER_AGENT.to_owned(),
        ..ClientOptions::default()
    };
    Client::new(&server.url(), TOKEN, TokenType::OAuth, options).expect("loopback client")
}

fn context(id: &str) -> RequestContext {
    RequestContext {
        submission_id: RequestId::new(id).expect("UUID"),
    }
}

fn statement_a() -> Statement {
    let mut statement = Statement::new(SQL_A);
    statement.session.warehouse = Some("FIRST_USE_WH".into());
    statement.session.database = Some("FIRST_USE_DB".into());
    statement.timeout = Some(60);
    statement
}

fn quick_policy(max_attempts: usize) -> PollPolicy {
    PollPolicy {
        max_attempts: NonZeroUsize::new(max_attempts).expect("nonzero"),
        interval: Duration::from_millis(2),
        max_elapsed: Duration::from_secs(10),
    }
}

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).expect("nonzero")
}

fn cells(rows: &[Vec<Option<String>>]) -> Vec<Vec<Option<&str>>> {
    rows.iter()
        .map(|row| row.iter().map(Option::as_deref).collect())
        .collect()
}

fn routes_of(server: &TestServer) -> Vec<String> {
    server.requests().iter().map(route).collect()
}

/// The common authorization/negotiation headers every documented operation requires.
fn assert_common_headers(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some(&*format!("Bearer {TOKEN}"))
    );
    assert_eq!(
        request.header("x-snowflake-authorization-token-type"),
        Some("OAUTH")
    );
    assert_eq!(request.header("user-agent"), Some(USER_AGENT));
    let accept = request.header("accept").expect("accept header");
    assert!(accept.contains("application/json"), "accept: {accept}");
}

// ---------------------------------------------------------------------------------------------
// Outcome A: submit async, reuse the first result, discover and fan out the remaining partitions.

#[tokio::test]
async fn a_submit_poll_reuse_first_result_and_fan_out() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A)])
        .on(
            &status_route(HANDLE_A),
            [
                pending(HANDLE_A),
                first_result(HANDLE_A, &[2, 1, 2], json!([["1", "alpha"], ["2", null]])),
            ],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [later_partition(json!([["3", "gamma"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 2),
            [later_partition(json!([["4", "null"], ["5", "épsilon"]]))],
        )
        .start()
        .await;
    let client = client(&server);

    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    assert_eq!(submission.query.handle().as_str(), HANDLE_A);
    assert_eq!(submission.query.context(), &context(SUBMIT_A));
    match &submission.state {
        StatementState::Pending(reply) => {
            assert_eq!(reply.value.code, "333334");
            assert_eq!(reply.meta.status().as_u16(), 202);
        }
        StatementState::Complete(_) => panic!("the fake answered 202"),
    }

    let first = match submission
        .wait(quick_policy(5), &LocalStop::default())
        .await
        .expect("poll")
    {
        PollOutcome::Complete(first) => first,
        other => panic!("expected completion, got {other:?}"),
    };
    let metadata = first.plan().metadata().clone();
    assert_eq!(metadata.num_rows, 5);
    assert_eq!(metadata.format, "jsonv2");
    assert_eq!(
        metadata
            .partition_info
            .iter()
            .map(|p| p.row_count)
            .collect::<Vec<_>>(),
        [2, 1, 2]
    );
    assert_eq!(metadata.row_type.len(), 2);

    let mut partitions = first.bounded_partitions(nz(8), nz(2));
    let mut delivered = Vec::new();
    while let Some(partition) = partitions.next().await {
        let partition = partition.expect("every scripted partition succeeds");
        assert_eq!(partition.plan().query().handle().as_str(), HANDLE_A);
        assert_eq!(partition.plan().query().context(), &context(SUBMIT_A));
        delivered.push((partition.index, partition.rows));
    }
    let report = partitions.finish();
    assert!(report.is_complete(), "{:?}", report.remaining.outcome);
    assert_eq!(report.missing().count(), 0);
    assert_eq!(report.remaining.outcome, Outcome::Exhausted);
    assert!(report.unconsumed_first.is_none());

    let indexes: Vec<usize> = delivered.iter().map(|(i, _)| *i).collect();
    assert_eq!(indexes, [0, 1, 2]);
    let all_rows: Vec<Vec<Option<String>>> =
        delivered.into_iter().flat_map(|(_, rows)| rows).collect();
    assert_eq!(
        cells(&all_rows),
        [
            vec![Some("1"), Some("alpha")],
            vec![Some("2"), None],
            vec![Some("3"), Some("gamma")],
            vec![Some("4"), Some("null")], // the literal string, not SQL NULL
            vec![Some("5"), Some("épsilon")],
        ]
    );

    // Wire: one submit, two status checks, partitions 1 and 2 only (zero was reused).
    let requests = server.requests();
    let mut seen = routes_of(&server);
    seen[3..].sort(); // the two partitions may be admitted concurrently
    assert_eq!(
        seen,
        [
            submit_route(),
            status_route(HANDLE_A),
            status_route(HANDLE_A),
            partition_route(HANDLE_A, 1),
            partition_route(HANDLE_A, 2),
        ]
    );

    let submit = &requests[0];
    assert_common_headers(submit);
    assert_eq!(submit.header("content-type"), Some("application/json"));
    let mut pairs = submit.query_pairs();
    pairs.sort();
    assert_eq!(
        pairs,
        [
            ("async".to_owned(), "true".to_owned()),
            ("requestId".to_owned(), SUBMIT_A.to_owned()),
        ]
    );
    let body: Value = serde_json::from_slice(&submit.body).expect("JSON submit body");
    assert_eq!(
        body,
        json!({
            "statement": SQL_A,
            "timeout": 60,
            "database": "FIRST_USE_DB",
            "warehouse": "FIRST_USE_WH",
        })
    );

    for status in &requests[1..3] {
        assert_common_headers(status);
        assert_eq!(
            status.query(),
            None,
            "status GETs carry no requestId/partition"
        );
        assert!(status.body.is_empty());
    }
    for part in &requests[3..] {
        assert_common_headers(part);
        assert_eq!(part.query_all("requestId"), Vec::<String>::new());
        let encoding = part.header("accept-encoding").expect("accept-encoding");
        assert!(encoding.contains("gzip"), "accept-encoding: {encoding}");
    }

    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn a_immediate_completion_is_reused_without_any_status_request() {
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(
                HANDLE_A,
                &[2, 1],
                json!([["1", "alpha"], ["2", "beta"]]),
            )],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [later_partition(json!([["3", null]]))],
        )
        .start()
        .await;
    let client = client(&server);

    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    assert!(matches!(submission.state, StatementState::Complete(_)));
    let PollOutcome::Complete(first) = submission
        .wait(quick_policy(1), &LocalStop::default())
        .await
        .expect("wait")
    else {
        panic!("immediate completion must be reused");
    };
    // Partition zero is the submit response itself.
    assert_eq!(first.response().meta.status().as_u16(), 200);
    assert_eq!(first.response().value.data.len(), 2);

    let mut partitions = first.bounded_partitions(nz(4), nz(1));
    let zero = partitions.next().await.expect("zero").expect("zero ok");
    assert_eq!(zero.index, 0);
    assert_eq!(
        cells(&zero.rows),
        [
            vec![Some("1"), Some("alpha")],
            vec![Some("2"), Some("beta")]
        ]
    );
    let one = partitions.next().await.expect("one").expect("one ok");
    assert_eq!(
        (one.index, cells(&one.rows)),
        (1, vec![vec![Some("3"), None]])
    );
    assert!(partitions.next().await.is_none());
    assert!(partitions.finish().is_complete());

    assert_eq!(
        routes_of(&server),
        [submit_route(), partition_route(HANDLE_A, 1)]
    );
}

#[tokio::test]
async fn a_partitions_are_independently_addressable_out_of_order() {
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(HANDLE_A, &[1, 1, 1], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [later_partition(json!([["2", "beta"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 2),
            [later_partition(json!([["3", "gamma"]]))],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let StatementState::Complete(first) = submission.state else {
        panic!("complete on submit");
    };
    let plan = first.plan();

    let two = plan.fetch(2).await.expect("partition 2 first");
    assert_eq!(
        (two.index, cells(&two.rows)),
        (2, vec![vec![Some("3"), Some("gamma")]])
    );
    let one = plan.fetch(1).await.expect("then partition 1");
    assert_eq!(
        (one.index, cells(&one.rows)),
        (1, vec![vec![Some("2"), Some("beta")]])
    );

    // Zero is owned by the first result; three is outside partitionInfo. Neither is sent.
    for index in [0, 3] {
        match plan.fetch(index).await {
            Err(Error::Protocol { kind, .. }) => assert_eq!(kind, ProtocolError::InvalidPartition),
            other => panic!("fetch({index}) should be refused locally, got {other:?}"),
        }
    }
    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            partition_route(HANDLE_A, 2),
            partition_route(HANDLE_A, 1),
        ]
    );
}

#[tokio::test]
async fn a_failed_partition_stops_the_traversal_and_is_recoverable() {
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(HANDLE_A, &[1, 1, 1], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [
                Reply::json(
                    500,
                    json!({"code": "390000", "message": "synthetic internal error"}).to_string(),
                ),
                later_partition(json!([["2", "beta"]])),
            ],
        )
        .on(
            &partition_route(HANDLE_A, 2),
            [later_partition(json!([["3", "gamma"]]))],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let PollOutcome::Complete(first) = submission
        .wait(quick_policy(1), &LocalStop::default())
        .await
        .expect("wait")
    else {
        panic!("complete on submit");
    };

    // Window 1: ordered, one request at a time, stop on the first failure (the defaults).
    let mut partitions = first.bounded_partitions(nz(8), nz(1));
    let zero = partitions.next().await.expect("zero").expect("zero ok");
    assert_eq!(zero.index, 0);
    let failure = partitions
        .next()
        .await
        .expect("an emitted failure")
        .expect_err("partition 1 failed");
    assert_eq!(failure.index, 1);
    match &failure.error {
        Error::Server(server_error) => {
            assert_eq!(server_error.meta.status().as_u16(), 500);
            assert_eq!(
                server_error.document.as_ref().map(|d| d.code.as_str()),
                Some("390000")
            );
            assert!(!server_error.is_cancelled());
        }
        other => panic!("expected a server error, got {other:?}"),
    }
    assert!(
        partitions.next().await.is_none(),
        "stop-on-error ends the traversal"
    );
    let report = partitions.finish();
    assert!(!report.is_complete());
    assert_eq!(report.remaining.outcome, Outcome::Failed);
    assert_eq!(report.missing().collect::<Vec<_>>(), [1, 2]);

    // The report keeps the bound plan: deliberately retry what is missing.
    let mut recovered = Vec::new();
    for index in report.missing().collect::<Vec<_>>() {
        recovered.push(report.plan.fetch(index).await.expect("retry succeeds"));
    }
    assert_eq!(
        recovered
            .iter()
            .map(|p| (p.index, cells(&p.rows)))
            .collect::<Vec<_>>(),
        [
            (1, vec![vec![Some("2"), Some("beta")]]),
            (2, vec![vec![Some("3"), Some("gamma")]]),
        ]
    );
}

#[tokio::test]
async fn a_partition_stream_that_ends_early_is_a_correlated_failure() {
    let complete = gzip(json!({"data": [["2", "beta"]]}).to_string().as_bytes());
    let prefix = complete[..complete.len() / 2].to_vec();
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(HANDLE_A, &[1, 1, 1], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            // Chunked transfer cut after a prefix: the connection drops without the last chunk.
            [Reply::new(200)
                .header("content-type", "application/json")
                .header("content-encoding", "gzip")
                .with_body(Body::FailAfter(vec![prefix.clone().into()]))],
        )
        .on(
            &partition_route(HANDLE_A, 2),
            // Complete HTTP framing, but the gzip stream itself is truncated.
            [Reply::new(200)
                .header("content-type", "application/json")
                .header("content-encoding", "gzip")
                .body(prefix)],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let StatementState::Complete(first) = submission.state else {
        panic!("complete on submit");
    };
    let plan = first.plan();

    let transport = plan.fetch(1).await.expect_err("body ended early");
    match &transport {
        Error::Concord {
            meta: Some(meta),
            source,
        } => {
            assert_eq!(meta.status().as_u16(), 200, "the head is kept");
            // Naming the stage needs the `concord` crate itself: the SDK does not re-export it.
            assert_eq!(source.kind(), concord::ErrorKind::Transport);
        }
        other => panic!("a dropped body is a Concord error with the head kept: {other:?}"),
    }

    let truncated = plan.fetch(2).await.expect_err("gzip truncated");
    match truncated {
        Error::PartitionBody { meta, source } => {
            assert_eq!(meta.status().as_u16(), 200);
            assert!(
                matches!(source, concord_sdk_snowflake::PartitionBodyError::Gzip(_)),
                "{source:?}"
            );
        }
        other => panic!("expected a partition body error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_partition_that_disagrees_with_partition_info_is_refused() {
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(HANDLE_A, &[1, 2], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [
                // partitionInfo says two rows; one arrives.
                later_partition(json!([["2", "beta"]])),
                // Two rows, but one has three cells for a two-column result.
                later_partition(json!([["2", "beta"], ["3", "gamma", "extra"]])),
            ],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let StatementState::Complete(first) = submission.state else {
        panic!("complete on submit");
    };
    for attempt in ["row count", "row width"] {
        match first.plan().fetch(1).await {
            Err(Error::Protocol { kind, meta }) => {
                assert_eq!(kind, ProtocolError::InvalidRows, "{attempt}");
                assert_eq!(meta.map(|m| m.status().as_u16()), Some(200), "{attempt}");
            }
            other => panic!("{attempt}: expected InvalidRows, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn a_request_budget_bounds_fanout_and_names_the_unfetched_partition() {
    let server = Fake::default()
        .on(
            &submit_route(),
            [first_result(HANDLE_A, &[1, 1, 1], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [later_partition(json!([["2", "beta"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 2),
            [later_partition(json!([["3", "gamma"]]))],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let StatementState::Complete(first) = submission.state else {
        panic!("complete on submit");
    };

    // One later request allowed: zero (no I/O) and one are delivered, two is not admitted.
    let mut partitions = first.bounded_partitions(nz(1), nz(1));
    let mut indexes = Vec::new();
    while let Some(partition) = partitions.next().await {
        indexes.push(partition.expect("ok").index);
    }
    assert_eq!(indexes, [0, 1]);
    let report = partitions.finish();
    assert!(
        !report.is_complete(),
        "None after a budget is not completion"
    );
    assert_eq!(report.remaining.outcome, Outcome::Bounded);
    assert_eq!(report.missing().collect::<Vec<_>>(), [2]);
    assert_eq!(
        routes_of(&server),
        [submit_route(), partition_route(HANDLE_A, 1)]
    );

    let two = report
        .plan
        .fetch(2)
        .await
        .expect("fetch what the budget left");
    assert_eq!(two.index, 2);
}

#[tokio::test]
async fn a_provider_error_document_on_submit_is_a_typed_server_error() {
    let server = Fake::default()
        .on(&submit_route(), [compilation_failure(HANDLE_A)])
        .start()
        .await;
    let client = client(&server);
    let error = client
        .submit(
            context(SUBMIT_A),
            Statement::new("select NOPE from FIRST_USE_DEMO"),
        )
        .await
        .expect_err("422 on submit");
    match &error {
        Error::Server(server_error) => {
            assert_eq!(server_error.meta.status().as_u16(), 422);
            let document = server_error.document.as_ref().expect("typed document");
            assert_eq!(document.code, "000904");
            assert_eq!(document.sql_state.as_deref(), Some("42000"));
            assert_eq!(
                document.statement_handle.as_ref().map(|h| h.as_str()),
                Some(HANDLE_A)
            );
            assert!(!server_error.is_cancelled());
        }
        other => panic!("expected Error::Server, got {other:?}"),
    }
    // Display/Debug must not leak the provider message (which may echo SQL).
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("NOPE"), "{shown}");
    assert!(!shown.contains("compilation error"), "{shown}");
}

#[tokio::test]
async fn a_statement_failure_while_polling_is_terminal() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A)])
        .on(
            &status_route(HANDLE_A),
            [pending(HANDLE_A), compilation_failure(HANDLE_A)],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    match submission
        .wait(quick_policy(5), &LocalStop::default())
        .await
        .expect("a correlated 422 is an outcome, not an Err")
    {
        PollOutcome::Terminal(error) => {
            assert!(!error.is_cancelled());
            assert_eq!(error.meta.status().as_u16(), 422);
            assert_eq!(
                error.document.as_ref().map(|d| d.code.as_str()),
                Some("000904")
            );
        }
        other => panic!("expected Terminal, got {other:?}"),
    }
    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            status_route(HANDLE_A),
            status_route(HANDLE_A)
        ]
    );
}

#[tokio::test]
async fn a_malformed_result_document_is_a_decode_error_not_a_result() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A)])
        .on(
            &status_route(HANDLE_A),
            [
                Reply::json(200, "{\"resultSetMetaData\": {broken"),
                // Well-formed JSON, but no partitionInfo at all.
                Reply::json(
                    200,
                    json!({
                        "resultSetMetaData": {"numRows": 1, "format": "jsonv2", "rowType": row_type()},
                        "data": [["1", "alpha"]],
                        "code": "090001",
                        "sqlState": "00000",
                        "statementHandle": HANDLE_A,
                        "message": "Statement executed successfully.",
                    })
                    .to_string(),
                ),
                // partitionInfo present but empty: no descriptor for the rows that arrived.
                Reply::json(
                    200,
                    json!({
                        "resultSetMetaData": {
                            "numRows": 1, "format": "jsonv2", "rowType": row_type(),
                            "partitionInfo": [],
                        },
                        "data": [["1", "alpha"]],
                        "code": "090001",
                        "sqlState": "00000",
                        "statementHandle": HANDLE_A,
                        "message": "Statement executed successfully.",
                    })
                    .to_string(),
                ),
            ],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    // Invalid JSON and a missing partitionInfo field are both Concord decode errors, with the
    // 200 head kept. (I had predicted ProtocolError::InvalidManifest for the missing field from
    // its rustdoc, "Metadata is absent"; observed: serde rejects it first. See FIRST_USE.md.)
    for case in ["invalid JSON", "partitionInfo missing"] {
        match submission.query.status().await {
            Err(Error::Concord {
                meta: Some(meta),
                source,
            }) => {
                assert_eq!(meta.status().as_u16(), 200, "{case}");
                assert_eq!(source.kind(), concord::ErrorKind::Decode, "{case}");
            }
            other => panic!("{case}: expected a decode error, got {other:?}"),
        }
    }
    match submission.query.status().await {
        Err(Error::Protocol { kind, meta }) => {
            assert_eq!(kind, ProtocolError::InvalidManifest);
            assert_eq!(meta.map(|m| m.status().as_u16()), Some(200));
        }
        other => panic!("empty partitionInfo: expected InvalidManifest, got {other:?}"),
    }
}

#[tokio::test]
async fn a_rate_limited_status_check_is_an_error_without_retry() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A)])
        .on(
            &status_route(HANDLE_A),
            [
                Reply::json(
                    429,
                    json!({"code": "000610", "message": "synthetic too many requests"}).to_string(),
                ),
                pending(HANDLE_A),
            ],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let query = submission.query.clone();
    match submission
        .wait(quick_policy(5), &LocalStop::default())
        .await
    {
        Err(Error::Server(error)) => assert_eq!(error.meta.status().as_u16(), 429),
        other => panic!("README: 429 remains an error with no automatic retry; got {other:?}"),
    }
    assert_eq!(routes_of(&server), [submit_route(), status_route(HANDLE_A)]);

    // The application decides to look again: the retained query is still addressable.
    assert!(matches!(
        query.status().await.expect("status"),
        StatementState::Pending(_)
    ));
}

#[tokio::test]
async fn a_polling_bound_is_unresolved_and_the_query_resumes() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A)])
        .on(
            &status_route(HANDLE_A),
            [
                pending(HANDLE_A),
                pending(HANDLE_A),
                first_result(HANDLE_A, &[1], json!([["1", "alpha"]])),
            ],
        )
        .start()
        .await;
    let client = client(&server);
    let submission = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit");
    let query = submission.query.clone();
    match submission
        .wait(quick_policy(2), &LocalStop::default())
        .await
        .expect("bounded")
    {
        PollOutcome::Unresolved {
            reason,
            attempts,
            last_pending,
        } => {
            assert_eq!(reason, PollEnd::Attempts);
            assert_eq!(attempts, 2);
            assert_eq!(last_pending.expect("latest pending").value.code, "333334");
        }
        other => panic!("expected Unresolved, got {other:?}"),
    }

    // A local stop before polling sends nothing at all.
    let stop = LocalStop::default();
    stop.stop();
    match query.poll(quick_policy(5), &stop).await.expect("stopped") {
        PollOutcome::Unresolved {
            reason, attempts, ..
        } => {
            assert_eq!((reason, attempts), (PollEnd::LocalStop, 0));
        }
        other => panic!("expected LocalStop, got {other:?}"),
    }

    // Resuming the same remote query completes it; nothing is resubmitted.
    let PollOutcome::Complete(first) = query
        .poll(quick_policy(5), &LocalStop::default())
        .await
        .expect("resume")
    else {
        panic!("expected completion on resume");
    };
    assert_eq!(first.plan().metadata().num_rows, 1);
    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            status_route(HANDLE_A),
            status_route(HANDLE_A),
            status_route(HANDLE_A),
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// Outcome B: submit and explicitly cancel a separate long-running statement.

/// 200 CancelStatus (reference: cancel operation example code/SQLSTATE/message).
fn cancel_accepted(handle: &str) -> Reply {
    Reply::json(
        200,
        json!({
            "code": "000604",
            "sqlState": "57014",
            "message": "SQL execution canceled",
            "statementHandle": handle,
            "statementStatusUrl": format!("/api/v2/statements/{handle}"),
        })
        .to_string(),
    )
}

/// 422 QueryFailureStatus for a cancelled statement. Synthetic: the docs show no status body
/// observed after a cancel; I reuse the documented cancellation pair.
fn cancelled_failure(handle: &str) -> Reply {
    Reply::json(
        422,
        json!({
            "code": "000604",
            "sqlState": "57014",
            "message": "SQL execution canceled",
            "statementHandle": handle,
        })
        .to_string(),
    )
}

/// 422 on the cancel endpoint (reference: cancel 422 example code/SQLSTATE).
fn cancel_not_found(handle: &str) -> Reply {
    Reply::json(
        422,
        json!({
            "code": "000709",
            "message": format!("Statement {handle} not found"),
            "sqlState": "02000",
            "statementHandle": handle,
        })
        .to_string(),
    )
}

fn statement_b() -> Statement {
    let mut statement = Statement::new(SQL_B);
    statement.session.warehouse = Some("FIRST_USE_WH".into());
    statement
}

fn cancel_id() -> RequestId {
    RequestId::new(CANCEL_B).expect("UUID")
}

#[tokio::test]
async fn b_cancel_a_separate_long_running_statement_while_a_completes() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_A), pending(HANDLE_B)])
        .on(
            &status_route(HANDLE_A),
            [first_result(HANDLE_A, &[1, 1], json!([["1", "alpha"]]))],
        )
        .on(
            &partition_route(HANDLE_A, 1),
            [later_partition(json!([["2", "beta"]]))],
        )
        .on(&cancel_route(HANDLE_B), [cancel_accepted(HANDLE_B)])
        .on(
            &status_route(HANDLE_B),
            [pending(HANDLE_B), cancelled_failure(HANDLE_B)],
        )
        .start()
        .await;
    let client = client(&server);

    let a = client
        .submit(context(SUBMIT_A), statement_a())
        .await
        .expect("submit A");
    let b = client
        .submit(context(SUBMIT_B), statement_b())
        .await
        .expect("submit B");
    assert_eq!(b.query.handle().as_str(), HANDLE_B);
    assert_eq!(b.query.context(), &context(SUBMIT_B));

    let observation = b
        .query
        .cancel_and_wait(cancel_id(), quick_policy(5), &LocalStop::default())
        .await;
    let ack = observation.acknowledgement.expect("cancel acknowledged");
    assert_eq!(ack.meta.status().as_u16(), 200);
    assert_eq!(ack.value.code, "000604");
    assert_eq!(ack.value.sql_state.as_deref(), Some("57014"));
    assert_eq!(
        ack.value.statement_handle.as_ref().map(|h| h.as_str()),
        Some(HANDLE_B)
    );
    match observation.outcome.expect("observation") {
        PollOutcome::Terminal(error) => {
            assert!(error.is_cancelled(), "observed terminal cancellation");
            assert_eq!(error.meta.status().as_u16(), 422);
        }
        other => panic!("expected an observed cancellation, got {other:?}"),
    }

    // A is a different statement and completes normally after B was cancelled.
    let PollOutcome::Complete(first) = a
        .wait(quick_policy(5), &LocalStop::default())
        .await
        .expect("A")
    else {
        panic!("A completes");
    };
    let mut partitions = first.bounded_partitions(nz(4), nz(1));
    let mut rows = Vec::new();
    while let Some(partition) = partitions.next().await {
        rows.extend(partition.expect("A partition").rows);
    }
    assert!(partitions.finish().is_complete());
    assert_eq!(
        cells(&rows),
        [
            vec![Some("1"), Some("alpha")],
            vec![Some("2"), Some("beta")]
        ]
    );

    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            submit_route(),
            cancel_route(HANDLE_B),
            status_route(HANDLE_B),
            status_route(HANDLE_B),
            status_route(HANDLE_A),
            partition_route(HANDLE_A, 1),
        ]
    );
    let requests = server.requests();
    assert_eq!(requests[1].query_all("requestId"), [SUBMIT_B]);
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).expect("JSON"),
        json!({"statement": SQL_B, "warehouse": "FIRST_USE_WH"})
    );
    let cancel = &requests[2];
    assert_common_headers(cancel);
    assert_eq!(cancel.method, "POST");
    // The cancel carries its own request ID, not the submission's.
    assert_eq!(
        cancel.query_pairs(),
        [("requestId".to_owned(), CANCEL_B.to_owned())]
    );
    assert!(
        cancel.body.is_empty(),
        "cancel body: {:?}",
        cancel.body_text()
    );
    for status in &requests[3..5] {
        assert_eq!(status.query(), None);
    }
}

#[tokio::test]
async fn b_rejected_cancel_is_kept_and_completion_can_win_the_race() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_B)])
        .on(&cancel_route(HANDLE_B), [cancel_not_found(HANDLE_B)])
        .on(
            &status_route(HANDLE_B),
            [first_result(HANDLE_B, &[1], json!([["42", null]]))],
        )
        .start()
        .await;
    let client = client(&server);
    let b = client
        .submit(context(SUBMIT_B), statement_b())
        .await
        .expect("submit B");
    let observation = b
        .query
        .cancel_and_wait(cancel_id(), quick_policy(5), &LocalStop::default())
        .await;
    match &observation.acknowledgement {
        Err(Error::Server(error)) => {
            assert_eq!(error.meta.status().as_u16(), 422);
            let document = error.document.as_ref().expect("document");
            assert_eq!(
                (document.code.as_str(), document.sql_state.as_deref()),
                ("000709", Some("02000"))
            );
            assert!(!error.is_cancelled());
        }
        other => panic!("expected the rejection to be kept, got {other:?}"),
    }
    match observation.outcome {
        Ok(PollOutcome::Complete(first)) => {
            assert_eq!(first.plan().query().handle().as_str(), HANDLE_B);
            assert_eq!(
                cells(&first.response().value.data),
                [vec![Some("42"), None]]
            );
        }
        other => panic!("completion won the race; got {other:?}"),
    }
    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            cancel_route(HANDLE_B),
            status_route(HANDLE_B)
        ]
    );
}

#[tokio::test]
async fn b_cancel_acknowledgement_without_the_documented_pair_is_refused() {
    // The reference's CancelStatus object section shows `"code": "0"`; its operation example
    // shows `000604`/`57014`. The SDK documents accepting only the pair.
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_B)])
        .on(
            &cancel_route(HANDLE_B),
            [Reply::json(
                200,
                json!({
                    "code": "0",
                    "sqlState": "",
                    "message": "successfully canceled",
                    "statementHandle": HANDLE_B,
                    "statementStatusUrl": format!("/api/v2/statements/{HANDLE_B}"),
                })
                .to_string(),
            )],
        )
        .start()
        .await;
    let client = client(&server);
    let b = client
        .submit(context(SUBMIT_B), statement_b())
        .await
        .expect("submit B");
    match b.query.cancel(cancel_id()).await {
        Err(Error::Protocol { kind, meta }) => {
            assert_eq!(kind, ProtocolError::UnexpectedCancellation);
            assert_eq!(meta.map(|m| m.status().as_u16()), Some(200));
        }
        other => panic!("expected UnexpectedCancellation, got {other:?}"),
    }
    // `Query::cancel` is one POST and never polls.
    assert_eq!(routes_of(&server), [submit_route(), cancel_route(HANDLE_B)]);
}

#[tokio::test]
async fn b_pre_stopped_local_stop_still_sends_the_requested_cancel_and_nothing_else() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_B)])
        .on(&cancel_route(HANDLE_B), [cancel_accepted(HANDLE_B)])
        .start()
        .await;
    let client = client(&server);
    let b = client
        .submit(context(SUBMIT_B), statement_b())
        .await
        .expect("submit B");
    let stop = LocalStop::default();
    stop.stop();
    let observation = b
        .query
        .cancel_and_wait(cancel_id(), quick_policy(5), &stop)
        .await;
    assert_eq!(
        observation
            .acknowledgement
            .expect("acknowledged")
            .value
            .code,
        "000604"
    );
    match observation.outcome.expect("observation") {
        PollOutcome::Unresolved {
            reason,
            attempts,
            last_pending,
        } => {
            assert_eq!((reason, attempts), (PollEnd::LocalStop, 0));
            assert!(last_pending.is_none());
        }
        other => panic!("expected a local stop, got {other:?}"),
    }
    assert_eq!(routes_of(&server), [submit_route(), cancel_route(HANDLE_B)]);
}

#[tokio::test]
async fn b_cancel_observation_is_bounded_and_resumable() {
    let server = Fake::default()
        .on(&submit_route(), [pending(HANDLE_B)])
        .on(&cancel_route(HANDLE_B), [cancel_accepted(HANDLE_B)])
        .on(
            &status_route(HANDLE_B),
            [
                pending(HANDLE_B),
                pending(HANDLE_B),
                cancelled_failure(HANDLE_B),
            ],
        )
        .start()
        .await;
    let client = client(&server);
    let b = client
        .submit(context(SUBMIT_B), statement_b())
        .await
        .expect("submit B");
    let query = b.query.clone();
    let observation = b
        .query
        .cancel_and_wait(cancel_id(), quick_policy(2), &LocalStop::default())
        .await;
    assert!(observation.acknowledgement.is_ok());
    match observation.outcome.expect("observation") {
        PollOutcome::Unresolved {
            reason,
            attempts,
            last_pending,
        } => {
            assert_eq!((reason, attempts), (PollEnd::Attempts, 2));
            assert_eq!(last_pending.expect("pending").value.code, "333334");
        }
        other => panic!("still running after two checks; got {other:?}"),
    }
    // Later, one deliberate status check. `status()` has no failure state, so the cancelled
    // statement's 422 arrives as an error carrying the cancellation pair.
    match query.status().await {
        Err(Error::Server(error)) => assert!(error.is_cancelled()),
        other => panic!("expected the cancelled failure as Error::Server, got {other:?}"),
    }
    assert_eq!(
        routes_of(&server),
        [
            submit_route(),
            cancel_route(HANDLE_B),
            status_route(HANDLE_B),
            status_route(HANDLE_B),
            status_route(HANDLE_B),
        ]
    );
}

#[tokio::test]
async fn b_server_side_timeout_on_submit_is_not_a_caller_cancel() {
    // Reference: POST 408 "exceeded the timeout period; the statement was cancelled".
    // The body combination is synthetic (QueryStatus fields with the cancellation pair).
    let server = Fake::default()
        .on(
            &submit_route(),
            [Reply::json(
                408,
                json!({
                    "code": "000604",
                    "sqlState": "57014",
                    "message": "Statement reached its statement or warehouse timeout of 1 second(s) and was canceled.",
                    "statementHandle": HANDLE_B,
                })
                .to_string(),
            )],
        )
        .start()
        .await;
    let client = client(&server);
    let mut statement = statement_b();
    statement.timeout = Some(1);
    match client.submit(context(SUBMIT_B), statement).await {
        Err(Error::Server(error)) => {
            assert_eq!(error.meta.status().as_u16(), 408);
            assert!(error.is_cancelled(), "the server cancelled it");
        }
        other => panic!("expected a 408 server error, got {other:?}"),
    }
    // No cancel POST was sent by the application.
    assert_eq!(routes_of(&server), [submit_route()]);
    let body: Value = serde_json::from_slice(&server.single().body).expect("JSON");
    assert_eq!(body["timeout"], json!(1));
}
