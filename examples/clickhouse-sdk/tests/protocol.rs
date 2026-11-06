//! Wire-level tests of the ClickHouse SDK against the loopback server: request method, target,
//! headers and exact body bytes; documented success and failure shapes; the late-exception
//! detection contract and its documented limits.
//!
//! The server is a fake: it proves agreement with the documented shapes, not conformance with a
//! real ClickHouse server (see PROVENANCE.md).

mod support;

use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use concord::stream::ReadErrorKind;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_clickhouse::{
    CallContext, ClickHouse, Credentials, Error, Identifier, LateSource, Outcome, OutputFormat,
    Query, QueryId, RequestError, ResponseProblem, Table, stream,
};
use serde::{Deserialize, Serialize};
use support::*;

fn football_query() -> Query {
    Query::new(
        "SELECT date, season, home_team, away_team, home_team_goals, away_team_goals FROM football WHERE season = {season:Int16} AND home_team = {team:String}",
    )
    .bind("season", 2021_i16)
    .unwrap()
    .bind("team", "Sutton United")
    .unwrap()
}

#[tokio::test]
async fn a_select_is_a_get_with_the_sql_the_parameters_and_the_format_header() {
    let server = TestServer::start(|request| {
        rows_reply(request, OutputFormat::JsonEachRow, vec![football_json()])
    })
    .await;
    let clickhouse = client(&server.url());
    let rows = clickhouse
        .select(football_query(), OutputFormat::JsonEachRow)
        .query_id(QueryId::new("report-2021-sutton").unwrap())
        .rows::<Match>()
        .await
        .unwrap();
    rows.collect().await.unwrap();

    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        "/?query=SELECT+date%2C+season%2C+home_team%2C+away_team%2C+home_team_goals%2C+away_team_goals+FROM+football+WHERE+season+%3D+%7Bseason%3AInt16%7D+AND+home_team+%3D+%7Bteam%3AString%7D&query_id=report-2021-sutton&param_season=2021&param_team=Sutton+United"
    );
    assert_eq!(request.header("x-clickhouse-format"), Some("JSONEachRow"));
    // HTTP Basic: base64("analyst:fixture-password").
    assert_eq!(
        request.header("authorization"),
        Some("Basic YW5hbHlzdDpmaXh0dXJlLXBhc3N3b3Jk")
    );
    assert_eq!(request.header("x-clickhouse-user"), None);
    assert_eq!(request.header("x-clickhouse-key"), None);
    assert_eq!(request.header("x-clickhouse-database"), None);
    assert!(request.body.is_empty() && request.body_len == 0);
}

#[tokio::test]
async fn header_authentication_and_the_call_context_use_the_documented_names() {
    let server = TestServer::start(|request| {
        rows_reply(request, OutputFormat::CsvWithNames, vec![football_csv()])
    })
    .await;
    let http = ClickHouse::recommended_http_client().unwrap();
    let clickhouse = ClickHouse::new(http, Credentials::headers(USER, PASSWORD).unwrap())
        .with_base_url(&server.url())
        .unwrap()
        .with_database(Identifier::new("default").unwrap());
    let context = CallContext::new()
        .database(Identifier::new("analytics").unwrap())
        .quota_key("team-reports")
        .unwrap();
    clickhouse
        .select(Query::new("SELECT 1"), OutputFormat::CsvWithNames)
        .context(&context)
        .rows::<Match>()
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(request.header("x-clickhouse-user"), Some(USER));
    assert_eq!(request.header("x-clickhouse-key"), Some(PASSWORD));
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("x-clickhouse-database"), Some("analytics"));
    assert_eq!(request.target, "/?quota_key=team-reports&query=SELECT+1");
}

#[tokio::test]
async fn no_credentials_send_no_authentication() {
    let server =
        TestServer::start(|request| rows_reply(request, OutputFormat::JsonEachRow, vec![])).await;
    let http = ClickHouse::recommended_http_client().unwrap();
    let clickhouse = ClickHouse::new(http, Credentials::none())
        .with_base_url(&server.url())
        .unwrap();
    let rows = clickhouse
        .select(Query::new("SELECT 1"), OutputFormat::JsonEachRow)
        .rows::<serde_json::Value>()
        .await
        .unwrap();
    assert!(rows.collect().await.unwrap().rows.is_empty());
    let request = server.single();
    for name in ["authorization", "x-clickhouse-user", "x-clickhouse-key"] {
        assert_eq!(request.header(name), None, "{name}");
    }
}

#[tokio::test]
async fn values_travel_in_the_escaped_format_never_in_the_sql() {
    let server =
        TestServer::start(|request| rows_reply(request, OutputFormat::JsonEachRow, vec![])).await;
    let hostile = "x' OR 1=1 --\tback\\slash\nline";
    let query = Query::new(
        "SELECT * FROM {table:Identifier} WHERE name = {name:String} AND note = {note:Nullable(String)} AND ok = {ok:Bool} AND n = {n:UInt64}",
    )
    .bind("table", Identifier::new("football").unwrap())
    .unwrap()
    .bind("name", hostile)
    .unwrap()
    .bind("note", None::<&str>)
    .unwrap()
    .bind("ok", true)
    .unwrap()
    .bind("n", u64::MAX)
    .unwrap();
    let sql = query.sql().to_owned();
    client(&server.url())
        .select(query, OutputFormat::JsonEachRow)
        .rows::<serde_json::Value>()
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(param(&request, "query").as_deref(), Some(sql.as_str()));
    assert_eq!(param(&request, "param_table").as_deref(), Some("football"));
    let name = param(&request, "param_name").unwrap();
    assert_eq!(name, "x' OR 1=1 --\\tback\\\\slash\\nline");
    assert_eq!(unescape(&name).as_deref(), Some(hostile));
    // A raw tab would be `%09`, which the HTTP interface page says does not parse.
    assert!(request.target.contains("%5Ctback%5C%5Cslash%5Cnline"));
    assert!(!request.target.contains("%09"));
    assert_eq!(param(&request, "param_note").as_deref(), Some("\\N"));
    assert_eq!(param(&request, "param_ok").as_deref(), Some("true"));
    assert_eq!(
        param(&request, "param_n").as_deref(),
        Some("18446744073709551615")
    );
}

#[tokio::test]
async fn an_unbound_placeholder_or_a_bad_name_sends_nothing() {
    let server =
        TestServer::start(|request| rows_reply(request, OutputFormat::JsonEachRow, vec![])).await;
    let clickhouse = client(&server.url());
    let error = clickhouse
        .select(
            Query::new("SELECT {a:UInt8}, {b:String}")
                .bind("a", 1_u8)
                .unwrap(),
            OutputFormat::JsonEachRow,
        )
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    assert!(matches!(
        &error,
        Error::Request(RequestError::Unbound { name }) if name == "b"
    ));
    assert_eq!(
        error.to_string(),
        "request refused before sending: the placeholder `b` has no bound value"
    );
    let error = clickhouse
        .select(Query::new("  "), OutputFormat::JsonEachRow)
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Request(RequestError::EmptyQuery)));
    assert!(Identifier::new("football; DROP TABLE x").is_err());
    assert!(QueryId::new("id\r\nx-injected: 1").is_err());
    assert!(QueryId::new("").is_err());
    assert!(Credentials::basic("user:colon", "p").is_err());
    assert!(Credentials::headers("user", "p\r\nx: y").is_err());
    assert!(CallContext::new().quota_key("k\n").is_err());
    assert_eq!(server.requests().len(), 0);
}

#[tokio::test]
async fn documented_rows_decode_identically_in_both_formats_with_their_metadata() {
    let server = TestServer::start(|request| {
        let format = requested_format(request).unwrap();
        let body = match format {
            OutputFormat::CsvWithNames => football_csv(),
            OutputFormat::JsonEachRow => football_json(),
        };
        rows_reply(request, format, vec![body])
    })
    .await;
    let clickhouse = client(&server.url());
    for format in [OutputFormat::CsvWithNames, OutputFormat::JsonEachRow] {
        let mut rows = clickhouse
            .select(Query::new("SELECT * FROM football"), format)
            .rows::<Match>()
            .await
            .unwrap();
        assert_eq!(rows.format(), format);
        let first = rows.next().await.unwrap().unwrap();
        assert_eq!(first.home_team, "Sutton United");
        if format == OutputFormat::CsvWithNames {
            assert_eq!(
                rows.columns().unwrap(),
                [
                    "date",
                    "season",
                    "home_team",
                    "away_team",
                    "home_team_goals",
                    "away_team_goals"
                ]
            );
        }
        let mut all = vec![first];
        while let Some(row) = rows.next().await {
            all.push(row.unwrap());
        }
        assert_eq!(all, football_rows());
        assert_eq!(rows.outcome(), Outcome::Complete);
        let meta = rows.meta();
        assert_eq!(meta.query_id(), Some("server-assigned-0001"));
        assert_eq!(meta.format(), Some(format.name()));
        assert_eq!(meta.timezone(), Some("Europe/London"));
        assert_eq!(meta.server_display_name(), Some("fixture-server"));
        assert_eq!(meta.exception_tag(), Some(TAG));
        let summary = meta.summary().unwrap().unwrap();
        assert_eq!(summary.read_rows, Some(1));
        assert_eq!(summary.elapsed_ns, Some(4_505_959));
        assert!(meta.progress().unwrap().is_empty());
        assert_eq!(rows.position().records, 17);
    }
}

#[tokio::test]
async fn metadata_headers_are_optional() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("Content-Type", NDJSON_TYPE)
            .body(football_json())
    })
    .await;
    let rows = client(&server.url())
        .select(
            Query::new("SELECT * FROM football"),
            OutputFormat::JsonEachRow,
        )
        .rows::<Match>()
        .await
        .unwrap();
    let meta = rows.meta().clone();
    let collected = rows.collect().await.unwrap();
    assert_eq!(collected.rows.len(), 17);
    // No tag announced: a clean end is not reported as complete.
    assert_eq!(collected.outcome, Outcome::EndedWithoutMarker);
    assert_eq!(meta.query_id(), None);
    assert_eq!(meta.format(), None);
    assert_eq!(meta.summary(), Ok(None));
    assert_eq!(meta.exception_code(), None);
}

#[tokio::test]
async fn progress_headers_are_requested_and_parsed_in_order() {
    let server = TestServer::start(|request| {
        rows_reply(request, OutputFormat::JsonEachRow, vec![football_json()])
            // The three lines of the documented progress example.
            .header("X-ClickHouse-Progress", r#"{"read_rows":"261636","read_bytes":"2093088","total_rows_to_read":"1000000","elapsed_ns":"14050417","memory_usage":"22205975"}"#)
            .header("X-ClickHouse-Progress", r#"{"read_rows":"654090","read_bytes":"5232720","total_rows_to_read":"1000000","elapsed_ns":"27948667","memory_usage":"83400279"}"#)
            .header("X-ClickHouse-Progress", r#"{"read_rows":"1000000","read_bytes":"8000000","total_rows_to_read":"1000000","elapsed_ns":"38002417","memory_usage":"80715679"}"#)
    })
    .await;
    let rows = client(&server.url())
        .select(
            Query::new("SELECT * FROM football"),
            OutputFormat::JsonEachRow,
        )
        .progress_headers(true)
        .wait_end_of_query(true)
        .rows::<Match>()
        .await
        .unwrap();
    let progress = rows.meta().progress().unwrap();
    let read: Vec<_> = progress.iter().map(|p| p.read_rows).collect();
    assert_eq!(read, [Some(261_636), Some(654_090), Some(1_000_000)]);
    assert_eq!(progress[2].memory_usage, Some(80_715_679));
    let request = server.single();
    assert_eq!(
        request.target,
        "/?query=SELECT+*+FROM+football&wait_end_of_query=1&send_progress_in_http_headers=1"
    );
}

#[tokio::test]
async fn a_server_exception_keeps_its_status_code_and_message_out_of_display() {
    let server =
        TestServer::start(|_| exception_reply(400, 62, &fixture("error_multi_statements.txt")))
            .await;
    let error = client(&server.url())
        .select(
            Query::new("SET ROLE my_role;SELECT * FROM my_table;"),
            OutputFormat::JsonEachRow,
        )
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    let Error::Server(exception) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(exception.status().as_u16(), 400);
    assert_eq!(exception.code(), Some(62));
    assert_eq!(exception.name(), None);
    assert_eq!(
        exception.message(),
        Some("Code: 62. DB::Exception: Syntax error (Multi-statements are not allowed)")
    );
    assert_eq!(error.exception_code(), Some(62));
    assert!(!error.is_outcome_unknown());
    assert_eq!(
        error.to_string(),
        "ClickHouseHttp.select: the server refused the query with HTTP status 400, exception code 62"
    );
    assert!(!format!("{error:?}").contains("Multi-statements"));
}

#[tokio::test]
async fn the_documented_bad_parameter_exception_is_parsed_without_a_code_header() {
    let server = TestServer::start(|_| {
        Reply::text(
            500,
            "text/plain; charset=UTF-8",
            fixture("error_bad_query_parameter.txt"),
        )
    })
    .await;
    let error = client(&server.url())
        .select(
            Query::new("SELECT splitByChar('\\t', {arg1:String})")
                .bind("arg1", "abc\t123")
                .unwrap(),
            OutputFormat::JsonEachRow,
        )
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    let Error::Server(exception) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(exception.status().as_u16(), 500);
    assert_eq!(exception.code(), Some(457));
    assert_eq!(exception.name(), Some("BAD_QUERY_PARAMETER"));
    assert_eq!(
        error.to_string(),
        "ClickHouseHttp.select: the server refused the query with HTTP status 500, exception code 457 (BAD_QUERY_PARAMETER)"
    );
}

#[tokio::test]
async fn another_format_than_requested_is_refused_before_reading() {
    let server = TestServer::start(|request| {
        rows_reply(request, OutputFormat::JsonEachRow, vec![football_json()])
    })
    .await;
    let error = client(&server.url())
        // The server answers JSONEachRow (a `FORMAT` clause winning on a server before 26.8).
        .select(
            Query::new("SELECT * FROM football FORMAT JSONEachRow"),
            OutputFormat::CsvWithNames,
        )
        .rows::<Match>()
        .await
        .unwrap_err();
    let Error::Response(refused) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        refused.problem(),
        &ResponseProblem::FormatMismatch {
            requested: "CSVWithNames",
            served: "JSONEachRow".into()
        }
    );
    assert_eq!(
        error.to_string(),
        "ClickHouseHttp.select: the response is not in the requested format CSVWithNames"
    );
}

#[tokio::test]
async fn a_content_type_the_reader_does_not_accept_is_refused() {
    let server =
        TestServer::start(|_| Reply::text(200, "text/tab-separated-values; charset=UTF-8", "1\n"))
            .await;
    let clickhouse = client(&server.url());
    for format in [OutputFormat::JsonEachRow, OutputFormat::CsvWithNames] {
        let error = clickhouse
            .select(Query::new("SELECT 1"), format)
            .rows::<serde_json::Value>()
            .await
            .unwrap_err();
        assert!(
            matches!(&error, Error::Response(r) if matches!(r.problem(), ResponseProblem::ContentType { .. })),
            "{error:?}"
        );
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct ThrowIf {
    #[serde(rename = "sleepEachRow(0.001)")]
    sleep: u8,
    #[serde(rename = "throwIf(equals(number, 2))")]
    throw_if: u8,
}

#[tokio::test]
async fn the_documented_late_exception_ends_the_rows_in_both_formats() {
    let cases = [
        (
            OutputFormat::CsvWithNames,
            "late_exception.csvwithnames",
            "rumfyutuqkncbgau",
        ),
        (
            OutputFormat::JsonEachRow,
            "late_exception.jsoneachrow",
            "dmrdfnujjqvszhav",
        ),
    ];
    for (format, name, tag) in cases {
        let body = fixture(name);
        let server = TestServer::start(move |request| {
            // 24.11 and later: the block, then the connection is dropped without the last chunk.
            rows_reply_with(
                request,
                format,
                Some(tag),
                Body::FailAfter(vec![body.clone().into()]),
            )
        })
        .await;
        let mut rows = client(&server.url())
            .select(
                Query::new("select sleepEachRow(0.001),throwIf(number=2)from numbers(5)"),
                format,
            )
            .rows::<ThrowIf>()
            .await
            .unwrap();
        for _ in 0..2 {
            assert_eq!(
                rows.next().await.unwrap().unwrap(),
                ThrowIf {
                    sleep: 0,
                    throw_if: 0
                }
            );
        }
        let error = rows.next().await.unwrap().unwrap_err();
        let Error::LateException(late) = &error else {
            panic!("{format}: {error:?}")
        };
        assert_eq!(late.code(), Some(395));
        assert_eq!(late.name(), Some("FUNCTION_THROW_IF_VALUE_IS_NON_ZERO"));
        assert_eq!(late.rows_before(), 2);
        assert_eq!(late.source(), LateSource::Block { complete: true });
        assert!(
            late.message()
                .starts_with("Code: 395. DB::Exception: Value passed")
        );
        assert!(late.message().ends_with("(version 25.11.1.1)"));
        assert_eq!(late.meta().status().as_u16(), 200);
        assert!(error.is_outcome_unknown());
        assert_eq!(
            error.to_string(),
            "ClickHouseHttp.select: the server reported an exception after answering HTTP 200 (exception code 395); 2 rows were returned before it"
        );
        assert!(!format!("{error:?}").contains("throwIf"));
        assert!(rows.next().await.is_none());
        assert_eq!(rows.outcome(), Outcome::Failed);
    }
}

#[tokio::test]
async fn an_exception_code_on_a_2xx_head_is_a_late_exception_before_any_row() {
    let block = {
        let message = "Code: 159. DB::Exception: Timeout exceeded: synthetic fixture. (TIMEOUT_EXCEEDED) (version 26.10.1.1)";
        format!(
            "\r\n__exception__\r\n{TAG}\r\n{message}\n{} {TAG}\r\n__exception__\r\n",
            message.len() + 1
        )
    };
    let server = TestServer::start(move |request| {
        rows_reply(
            request,
            OutputFormat::JsonEachRow,
            vec![block.clone().into_bytes()],
        )
        .header(
            "X-ClickHouse-Progress",
            r#"{"read_rows":"10","read_bytes":"80"}"#,
        )
        .header("X-ClickHouse-Exception-Code", "159")
    })
    .await;
    let error = client(&server.url())
        .select(Query::new("SELECT 1"), OutputFormat::JsonEachRow)
        .progress_headers(true)
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    let Error::LateException(late) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(late.code(), Some(159));
    assert_eq!(late.name(), Some("TIMEOUT_EXCEEDED"));
    assert_eq!(late.source(), LateSource::Block { complete: true });
    assert_eq!(late.rows_before(), 0);

    // Without a tag (a server that writes plain text): the head is the signal.
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("Content-Type", NDJSON_TYPE)
            .header("X-ClickHouse-Exception-Code", "159")
            .body("Code: 159. DB::Exception: Timeout exceeded. (TIMEOUT_EXCEEDED) (version 24.3.1.1)\n")
    })
    .await;
    let error = client(&server.url())
        .select(Query::new("SELECT 1"), OutputFormat::JsonEachRow)
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    let Error::LateException(late) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(late.source(), LateSource::Head);
    assert_eq!(late.code(), Some(159));
    assert_eq!(late.name(), Some("TIMEOUT_EXCEEDED"));
}

#[tokio::test]
async fn a_body_broken_without_a_block_is_a_transport_failure_not_an_end() {
    let csv = football_csv();
    let cut = offset_of(&csv, b"\"Port Vale\"", 3);
    let prefix = csv[..cut].to_vec();
    let server = TestServer::start(move |request| {
        rows_reply_with(
            request,
            OutputFormat::CsvWithNames,
            Some(TAG),
            Body::FailAfter(vec![prefix.clone().into()]),
        )
    })
    .await;
    let mut rows = client(&server.url())
        .select(
            Query::new("SELECT * FROM football"),
            OutputFormat::CsvWithNames,
        )
        .rows::<Match>()
        .await
        .unwrap();
    let mut read = 0;
    let error = loop {
        match rows.next().await.unwrap() {
            Ok(_) => read += 1,
            Err(error) => break error,
        }
    };
    assert_eq!(read, 3);
    let Error::Read(failure) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(failure.kind(), ReadErrorKind::Transport);
    assert_eq!(failure.position().records, 3);
    assert!(error.is_outcome_unknown());
    assert_eq!(rows.outcome(), Outcome::Failed);
}

#[tokio::test]
async fn an_untagged_plain_text_exception_is_caught_only_when_it_does_not_parse() {
    // JSONEachRow: the text is not JSON, so the reader refuses it.
    let server = TestServer::start(|request| {
        rows_reply_with(
            request,
            OutputFormat::JsonEachRow,
            None,
            Body::Full(fixture("untagged_plain_exception.jsoneachrow").into()),
        )
    })
    .await;
    let mut rows = client(&server.url())
        .select(
            Query::new("SELECT a, b, c FROM t"),
            OutputFormat::JsonEachRow,
        )
        .rows::<serde_json::Value>()
        .await
        .unwrap();
    assert!(rows.next().await.unwrap().is_ok());
    let error = rows.next().await.unwrap().unwrap_err();
    assert!(
        matches!(&error, Error::Read(f) if f.kind() == ReadErrorKind::Format),
        "{error:?}"
    );

    // CSVWithNames with three String columns: the pre-24.11 text (two commas) is a valid row.
    // The read ends cleanly; only the outcome says that nothing marked exceptions.
    #[derive(Debug, Deserialize)]
    struct Three {
        a: String,
    }
    let server = TestServer::start(|request| {
        rows_reply_with(
            request,
            OutputFormat::CsvWithNames,
            None,
            Body::Full(fixture("untagged_plain_exception.csvwithnames").into()),
        )
    })
    .await;
    let rows = client(&server.url())
        .select(
            Query::new("SELECT a, b, c FROM t"),
            OutputFormat::CsvWithNames,
        )
        .rows::<Three>()
        .await
        .unwrap();
    let collected = rows.collect().await.unwrap();
    assert_eq!(collected.rows.len(), 2);
    assert!(collected.rows[1].a.starts_with("Code: 395. DB::Exception"));
    assert_eq!(collected.outcome, Outcome::EndedWithoutMarker);
}

#[tokio::test]
async fn wait_end_of_query_turns_an_execution_error_into_a_status_but_is_not_relied_on() {
    let calls = Arc::new(Mutex::new(0));
    let seen = Arc::clone(&calls);
    let server = TestServer::start(move |request| {
        let mut calls = seen.lock().unwrap();
        *calls += 1;
        if *calls == 1 {
            // Buffered: the exception arrives before the head.
            exception_reply(500, 395, b"")
        } else {
            // The result exceeded the buffer: rows, then the block, after a 200.
            rows_reply_with(
                request,
                OutputFormat::CsvWithNames,
                Some("rumfyutuqkncbgau"),
                Body::FailAfter(vec![fixture("late_exception.csvwithnames").into()]),
            )
        }
    })
    .await;
    let clickhouse = client(&server.url());
    let select = || {
        clickhouse
            .select(
                Query::new("select sleepEachRow(0.001),throwIf(number=2)from numbers(5)"),
                OutputFormat::CsvWithNames,
            )
            .wait_end_of_query(true)
    };
    let error = select().rows::<ThrowIf>().await.unwrap_err();
    let Error::Server(exception) = &error else {
        panic!("{error:?}")
    };
    assert_eq!((exception.code(), exception.message()), (Some(395), None));

    let rows = select().rows::<ThrowIf>().await.unwrap();
    let error = rows.collect().await.unwrap_err();
    assert!(matches!(error, Error::LateException(_)), "{error:?}");
    for request in server.requests() {
        assert_eq!(param(&request, "wait_end_of_query").as_deref(), Some("1"));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Note {
    id: u32,
    text: String,
    score: Option<f64>,
}

fn notes() -> Vec<Note> {
    vec![
        Note {
            id: 1,
            text: "plain".into(),
            score: Some(1.5),
        },
        Note {
            id: 2,
            text: "quote \" comma , 'apostrophe\nnewline ünïcödé".into(),
            score: None,
        },
    ]
}

fn empty_insert_reply(request: &concord_example_testserver::Recorded) -> Reply {
    Reply::new(200)
        .header(
            "X-ClickHouse-Summary",
            r#"{"read_rows":"2","read_bytes":"80","written_rows":"2","written_bytes":"80","total_rows_to_read":"0","result_rows":"2","result_bytes":"80","elapsed_ns":"1200000"}"#,
        )
        .header(
            "X-ClickHouse-Query-Id",
            param(request, "query_id").unwrap_or_default(),
        )
        .header("X-ClickHouse-Exception-Tag", TAG)
}

#[tokio::test]
async fn a_json_each_row_insert_streams_the_exact_bytes_chunked() {
    let server = TestServer::start(empty_insert_reply).await;
    let table = Table::new(Identifier::new("notes").unwrap())
        .in_database(Identifier::new("fixtures").unwrap());
    let receipt = client(&server.url())
        .insert(table)
        .query_id(QueryId::new("load-notes-1").unwrap())
        .json_each_row(stream::from_iter(
            notes().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap();

    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.target,
        "/?query=INSERT+INTO+%60fixtures%60.%60notes%60+FORMAT+JSONEachRow&query_id=load-notes-1"
    );
    assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
    assert!(request.chunked, "a lazy body is sent chunked");
    assert_eq!(
        request.body_text(),
        "{\"id\":1,\"text\":\"plain\",\"score\":1.5}\n{\"id\":2,\"text\":\"quote \\\" comma , 'apostrophe\\nnewline ünïcödé\",\"score\":null}\n"
    );
    assert_eq!(receipt.query_id(), Some("load-notes-1"));
    assert_eq!(
        receipt.requested_query_id().map(QueryId::as_str),
        Some("load-notes-1")
    );
    assert_eq!(receipt.summary().unwrap().unwrap().written_rows, Some(2));
    assert_eq!(receipt.body_bytes(), 0);
}

#[tokio::test]
async fn a_csv_with_names_insert_quotes_every_field_and_ends_records_with_lf() {
    #[derive(Serialize)]
    struct Row {
        id: u32,
        text: String,
    }
    let server = TestServer::start(empty_insert_reply).await;
    let rows = vec![
        Row {
            id: 1,
            text: "plain".into(),
        },
        Row {
            id: 2,
            text: "quote \" comma , 'apostrophe\nnewline".into(),
        },
    ];
    client(&server.url())
        .insert(Table::new(Identifier::new("notes").unwrap()))
        .columns([
            Identifier::new("id").unwrap(),
            Identifier::new("text").unwrap(),
        ])
        .csv_with_names(stream::from_iter(rows.into_iter().map(Ok::<_, Infallible>)))
        .await
        .unwrap();
    let request = server.single();
    assert_eq!(
        param(&request, "query").as_deref(),
        Some("INSERT INTO `notes` (`id`, `text`) FORMAT CSVWithNames")
    );
    assert_eq!(
        request.header("content-type"),
        Some("text/csv; header=present")
    );
    assert_eq!(
        request.body,
        b"\"id\",\"text\"\n\"1\",\"plain\"\n\"2\",\"quote \"\" comma , 'apostrophe\nnewline\"\n"
    );
}

#[tokio::test]
async fn a_failing_upload_source_is_a_transport_error_with_an_unknown_outcome() {
    let server = TestServer::start(empty_insert_reply).await;
    let rows: Vec<Result<Note, std::io::Error>> = vec![
        Ok(notes()[0].clone()),
        Err(std::io::Error::other("the producer failed")),
    ];
    let error = client(&server.url())
        .insert(Table::new(Identifier::new("notes").unwrap()))
        .json_each_row(stream::from_iter(rows))
        .await
        .unwrap_err();
    let Error::Concord(inner) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(inner.kind(), concord::ErrorKind::Transport);
    assert!(error.is_outcome_unknown());
}

#[tokio::test]
async fn insert_failures_are_server_or_late_exceptions() {
    let server = TestServer::start(|_| {
        exception_reply(
            404,
            60,
            b"Code: 60. DB::Exception: Table fixtures.missing does not exist. (UNKNOWN_TABLE) (version 26.10.1.1)\n",
        )
    })
    .await;
    let error = client(&server.url())
        .insert(Table::new(Identifier::new("missing").unwrap()))
        .json_each_row(stream::from_iter(
            notes().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap_err();
    let Error::Server(exception) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(
        (
            exception.status().as_u16(),
            exception.code(),
            exception.name()
        ),
        (404, Some(60), Some("UNKNOWN_TABLE"))
    );
    assert_eq!(
        error.to_string(),
        "ClickHouseHttp.insert: the server refused the query with HTTP status 404, exception code 60 (UNKNOWN_TABLE)"
    );

    let block = {
        let message = "Code: 27. DB::Exception: Cannot parse input: synthetic fixture. (CANNOT_PARSE_INPUT_ASSERTION_FAILED) (version 26.10.1.1)";
        format!(
            "\r\n__exception__\r\n{TAG}\r\n{message}\n{} {TAG}\r\n__exception__\r\n",
            message.len() + 1
        )
    };
    let server = TestServer::start(move |request| {
        empty_insert_reply(request).with_body(Body::FailAfter(vec![block.clone().into()]))
    })
    .await;
    let error = client(&server.url())
        .insert(Table::new(Identifier::new("notes").unwrap()))
        .json_each_row(stream::from_iter(
            notes().into_iter().map(Ok::<_, Infallible>),
        ))
        .await
        .unwrap_err();
    let Error::LateException(late) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(late.code(), Some(27));
    assert_eq!(late.source(), LateSource::Block { complete: true });
}

#[tokio::test]
async fn cancel_sends_kill_query_with_the_id_as_a_bound_value() {
    let server = TestServer::start(|request| {
        rows_reply(
            request,
            OutputFormat::JsonEachRow,
            vec![fixture("kill_query.jsoneachrow")],
        )
    })
    .await;
    let statuses = client(&server.url())
        .cancel(&QueryId::new("report-2021-sutton").unwrap())
        .await
        .unwrap();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].kill_status, "waiting");
    assert_eq!(statuses[0].query_id, "report-2021-sutton");
    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/?param_id=report-2021-sutton");
    assert_eq!(
        request.body_text(),
        "KILL QUERY WHERE query_id = {id:String} ASYNC"
    );
    assert_eq!(request.header("x-clickhouse-format"), Some("JSONEachRow"));
    assert_eq!(
        request.header("content-type"),
        Some("text/plain; charset=utf-8")
    );
}

#[tokio::test]
async fn ping_is_get_ping() {
    let server = TestServer::start(|request| {
        if request.path() == "/ping" {
            Reply::text(200, "text/plain; charset=UTF-8", "Ok.\n")
        } else {
            Reply::new(404)
        }
    })
    .await;
    client(&server.url()).ping().await.unwrap();
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("GET", "/ping")
    );

    let server = TestServer::start(|_| Reply::new(503).body("unavailable")).await;
    let error = client(&server.url()).ping().await.unwrap_err();
    assert!(matches!(&error, Error::Server(e) if e.status().as_u16() == 503));
}

#[tokio::test]
async fn debug_output_withholds_secrets_and_values() {
    let clickhouse = client("http://127.0.0.1:9");
    let shown = format!("{clickhouse:?}");
    assert!(!shown.contains(PASSWORD), "{shown}");
    assert!(
        !shown.contains("YW5hbHlzdDpmaXh0dXJlLXBhc3N3b3Jk"),
        "{shown}"
    );
    let headers = ClickHouse::new(
        reqwest_client(),
        Credentials::headers(USER, PASSWORD).unwrap(),
    );
    assert!(!format!("{headers:?}").contains(PASSWORD));
    assert!(!format!("{:?}", Credentials::basic(USER, PASSWORD).unwrap()).contains(PASSWORD));
    let select = clickhouse.select(
        Query::new("SELECT {v:String}")
            .bind("v", "private-value")
            .unwrap(),
        OutputFormat::JsonEachRow,
    );
    assert!(!format!("{select:?}").contains("private-value"));
    // A refused connection: the error names the operation, not the URL or the value.
    let error = select.rows::<serde_json::Value>().await.unwrap_err();
    assert!(matches!(&error, Error::Concord(e) if e.kind() == concord::ErrorKind::Transport));
    let shown = error.to_string();
    assert!(
        !shown.contains("private-value") && !shown.contains("127.0.0.1"),
        "{shown}"
    );
}

fn reqwest_client() -> concord::reqwest::Client {
    ClickHouse::recommended_http_client().unwrap()
}

#[tokio::test]
async fn the_recommended_client_does_not_follow_redirects() {
    let server =
        TestServer::start(|_| Reply::new(307).header("Location", "http://127.0.0.1:9/elsewhere"))
            .await;
    let error = client(&server.url())
        .select(Query::new("SELECT 1"), OutputFormat::JsonEachRow)
        .rows::<serde_json::Value>()
        .await
        .unwrap_err();
    assert!(matches!(&error, Error::Server(e) if e.status().as_u16() == 307));
    assert_eq!(server.requests().len(), 1);
}
