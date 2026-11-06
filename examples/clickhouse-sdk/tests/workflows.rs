//! The two workflows of the slice, end to end through the public API against a stateful fake
//! server, plus the perturbation (chunk splits, then a late exception) and cancellation.
//!
//! - **A**: a typed parameterized `SELECT` whose response format is chosen at run time
//!   (`CSVWithNames` or `JSONEachRow`).
//! - **B**: a streamed insert (`JSONEachRow`, then `CSVWithNames`) into a fixture table, queried
//!   back with a distinct query ID and the same caller context.
//!
//! The fake decodes `param_*` values from the escaped format and filters the documented rows
//! itself; it does not run SQL. Agreement with it is not ClickHouse conformance.

mod support;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord::bytes::Bytes;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_clickhouse::{
    CallContext, ClickHouse, Error, Identifier, LateSource, Outcome, OutputFormat, Query, QueryId,
    Table, stream,
};
use support::*;

/// Workflow A's query: two typed placeholders, no value in the SQL text.
fn matches_of(team: &str, season: i16) -> Query {
    Query::new(
        "SELECT date, season, home_team, away_team, home_team_goals, away_team_goals FROM football WHERE season = {season:Int16} AND home_team = {team:String} ORDER BY date",
    )
    .bind("season", season)
    .unwrap()
    .bind("team", team)
    .unwrap()
}

/// The fake's evaluation of workflow A's query over the documented rows.
fn evaluate(request: &Recorded) -> Reply {
    let format = requested_format(request).expect("a format header");
    let team = unescape(&param(request, "param_team").unwrap()).unwrap();
    let season: u16 = param(request, "param_season").unwrap().parse().unwrap();
    let keep = |row: &Match| row.home_team == team && row.season == season;
    let body = match format {
        OutputFormat::JsonEachRow => String::from_utf8(football_json())
            .unwrap()
            .lines()
            .filter(|line| keep(&serde_json::from_str(line).unwrap()))
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
        OutputFormat::CsvWithNames => {
            let csv = String::from_utf8(football_csv()).unwrap();
            let mut lines = csv.lines();
            let mut out = format!("{}\n", lines.next().unwrap());
            for line in lines {
                let fields: Vec<&str> = line.split(',').collect();
                if fields[2].trim_matches('"') == team && fields[1] == season.to_string() {
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out
        }
    };
    rows_reply(request, format, vec![body.into_bytes()])
}

/// The format a caller selects at run time (a report setting, say).
fn format_from_setting(setting: &str) -> OutputFormat {
    match setting {
        "csv" => OutputFormat::CsvWithNames,
        _ => OutputFormat::JsonEachRow,
    }
}

#[tokio::test]
async fn workflow_a_typed_parameterized_select_in_a_runtime_selected_format() {
    let server = TestServer::start(evaluate).await;
    let clickhouse = client(&server.url());
    let expected: Vec<Match> = football_rows()
        .into_iter()
        .filter(|row| row.home_team == "Bradford City")
        .collect();
    assert_eq!(expected.len(), 1);

    for setting in ["csv", "json"] {
        let format = format_from_setting(setting);
        let query_id = QueryId::new(format!("report-bradford-{setting}")).unwrap();
        let mut rows = clickhouse
            .select(matches_of("Bradford City", 2021), format)
            .query_id(query_id.clone())
            .rows::<Match>()
            .await
            .unwrap();
        let mut read = Vec::new();
        while let Some(row) = rows.next().await {
            read.push(row.unwrap());
        }
        assert_eq!(read, expected, "{setting}");
        assert_eq!(rows.outcome(), Outcome::Complete);
        assert_eq!(rows.meta().query_id(), Some(query_id.as_str()));
        assert_eq!(rows.meta().format(), Some(format.name()));
    }

    // A value with characters that matter to SQL and to the escaped format reaches the fake's
    // filter intact and matches nothing; the SQL text is unchanged.
    let rows = clickhouse
        .select(
            matches_of("Bradford City' OR '1'='1\t", 2021),
            OutputFormat::JsonEachRow,
        )
        .rows::<Match>()
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert!(rows.rows.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            param(request, "query").as_deref(),
            Some(matches_of("x", 1).sql())
        );
    }
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Fixture {
    date: String,
    season: u16,
    home_team: String,
    away_team: String,
    home_team_goals: u8,
    away_team_goals: u8,
}

async fn read_perturbed(
    format: OutputFormat,
    chunks: Vec<Vec<u8>>,
    clean_end: bool,
) -> (Vec<Fixture>, Error) {
    let tag = "pqtrsvwxyzabcdef";
    let server = TestServer::start(move |request| {
        let body = if clean_end {
            // Give each HTTP chunk time to arrive separately, including the split UTF-8 bytes.
            Body::Chunks(
                chunks
                    .iter()
                    .map(|c| (Duration::from_millis(2), Bytes::from(c.clone())))
                    .collect(),
            )
        } else {
            Body::FailAfter(chunks.iter().map(|c| Bytes::from(c.clone())).collect())
        };
        rows_reply_with(request, format, Some(tag), body)
    })
    .await;
    let mut rows = client(&server.url())
        .select(Query::new("SELECT * FROM football_live"), format)
        .rows::<Fixture>()
        .await
        .unwrap();
    let mut read = Vec::new();
    loop {
        match rows.next().await {
            Some(Ok(row)) => read.push(row),
            Some(Err(error)) => {
                assert_eq!(rows.outcome(), Outcome::Failed);
                assert!(rows.next().await.is_none());
                return (read, error);
            }
            None => panic!("{format}: the body ended without the late exception"),
        }
    }
}

fn assert_late_after_three(format: OutputFormat, read: &[Fixture], error: &Error) {
    assert_eq!(read.len(), 3, "{format}");
    assert_eq!(read[0].home_team, "Sutton United");
    assert_eq!(read[2].away_team, "Swindon Town");
    let Error::LateException(late) = error else {
        panic!("{format}: {error:?}")
    };
    assert_eq!(late.code(), Some(241));
    assert_eq!(late.name(), Some("MEMORY_LIMIT_EXCEEDED"));
    assert_eq!(late.rows_before(), 3);
    assert_eq!(late.source(), LateSource::Block { complete: true });
    assert_eq!(late.meta().status().as_u16(), 200);
}

#[tokio::test]
async fn workflow_a_survives_split_fields_and_rows_then_reports_the_late_exception() {
    // CSV: a quoted field split in its middle, a doubled quote split between its two quotes, a
    // quoted line break split after the LF, and the exception marker split inside `__exception__`
    // and inside the tag.
    let csv = fixture("perturbed.csvwithnames");
    let cuts = [
        offset_of(&csv, b"\"Sutton United\"", 7),
        offset_of(&csv, b"\"\"Town\"\"", 1),
        offset_of(&csv, b"Line\nBreak", 5),
        offset_of(&csv, b"__exception__", 6),
        offset_of(&csv, b"pqtrsvwx", 4),
    ];
    let (read, error) =
        read_perturbed(OutputFormat::CsvWithNames, split_at(&csv, &cuts), false).await;
    assert_late_after_three(OutputFormat::CsvWithNames, &read, &error);
    assert_eq!(read[1].home_team, "Harrogate \"Town\", A.F.C.");
    assert_eq!(read[1].away_team, "Line\nBreak United");

    // JSONEachRow: a two-byte and a four-byte UTF-8 character split between their bytes, then
    // the marker split right after its leading CR.
    let json = fixture("perturbed.jsoneachrow");
    let cuts = [
        offset_of(&json, "ö".as_bytes(), 1),
        offset_of(&json, "😀".as_bytes(), 2),
        offset_of(&json, b"\r\n__exception__", 1),
    ];
    let (read, error) =
        read_perturbed(OutputFormat::JsonEachRow, split_at(&json, &cuts), false).await;
    assert_late_after_three(OutputFormat::JsonEachRow, &read, &error);
    assert_eq!(read[1].home_team, "1. FC Köln");
    assert_eq!(read[1].away_team, "Málaga CF 😀");

    // Every byte its own chunk, both formats.
    for (format, body) in [
        (OutputFormat::CsvWithNames, csv),
        (OutputFormat::JsonEachRow, json),
    ] {
        let bytes: Vec<Vec<u8>> = body.iter().map(|b| vec![*b]).collect();
        let (read, error) = read_perturbed(format, bytes, false).await;
        assert_late_after_three(format, &read, &error);
    }
}

#[tokio::test]
async fn split_rows_and_tagged_exception_are_detected_even_with_clean_http_end() {
    for (format, name, needle, delta) in [
        (
            OutputFormat::CsvWithNames,
            "perturbed.csvwithnames",
            b"\"\"Town\"\"".as_slice(),
            1,
        ),
        (
            OutputFormat::JsonEachRow,
            "perturbed.jsoneachrow",
            "😀".as_bytes(),
            2,
        ),
    ] {
        let body = fixture(name);
        let cuts = [
            offset_of(&body, needle, delta),
            offset_of(&body, b"__exception__", 6),
        ];
        let (read, error) = read_perturbed(format, split_at(&body, &cuts), true).await;
        assert_late_after_three(format, &read, &error);
        if format == OutputFormat::CsvWithNames {
            assert_eq!(read[1].home_team, "Harrogate \"Town\", A.F.C.");
        } else {
            assert_eq!(read[1].away_team, "Málaga CF 😀");
        }
    }
}

/// A fake table store: rows inserted per `(database, table)`, served back filtered by season.
#[derive(Clone, Default)]
struct Store(Arc<Mutex<HashMap<TableKey, Vec<u8>>>>);

/// `(database, table)`.
type TableKey = (String, String);

impl Store {
    fn respond(&self, request: &Recorded) -> Reply {
        let database = request
            .header("x-clickhouse-database")
            .unwrap_or("default")
            .to_owned();
        let query_id = param(request, "query_id").unwrap_or_default();
        if request.method == "POST" {
            // `INSERT INTO `t` FORMAT <F>`: the statement is the SDK's; the body is the data.
            let statement = param(request, "query").unwrap();
            let table = statement.split('`').nth(1).unwrap().to_owned();
            let lines = request.body.iter().filter(|b| **b == b'\n').count();
            let rows = if statement.ends_with("CSVWithNames") {
                lines - 1
            } else {
                lines
            };
            self.0
                .lock()
                .unwrap()
                .insert((database, table), request.body.clone());
            return Reply::new(200)
                .header("X-ClickHouse-Query-Id", query_id)
                .header("X-ClickHouse-Exception-Tag", TAG)
                .header(
                    "X-ClickHouse-Summary",
                    format!(
                        r#"{{"written_rows":"{rows}","written_bytes":"{}"}}"#,
                        request.body.len()
                    ),
                );
        }
        let table = param(request, "param_table").unwrap();
        let season = param(request, "param_season").unwrap();
        let format = requested_format(request).unwrap();
        let stored = self.0.lock().unwrap()[&(database, table)].clone();
        let text = String::from_utf8(stored).unwrap();
        let mut lines = text.lines();
        let mut body = String::new();
        if format == OutputFormat::CsvWithNames {
            body.push_str(lines.next().unwrap());
            body.push('\n');
        }
        for line in lines {
            let row_season = match format {
                OutputFormat::JsonEachRow => serde_json::from_str::<Match>(line).unwrap().season,
                OutputFormat::CsvWithNames => line
                    .split(',')
                    .nth(1)
                    .unwrap()
                    .trim_matches('"')
                    .parse()
                    .unwrap(),
            };
            if row_season.to_string() == season {
                body.push_str(line);
                body.push('\n');
            }
        }
        rows_reply(request, format, vec![body.into_bytes()])
    }
}

fn loaded_rows() -> Vec<Match> {
    let mut rows = football_rows();
    rows.push(Match {
        date: "2026-01-17".into(),
        season: 2026,
        home_team: "Synthetic Rovers".into(),
        away_team: "Fixture Town".into(),
        home_team_goals: 1,
        away_team_goals: 1,
    });
    rows
}

/// The documented CSVWithNames output with every field quoted (the SDK's insert dialect).
fn documented_csv_all_quoted() -> Vec<u8> {
    let csv = String::from_utf8(football_csv()).unwrap();
    let mut out = String::new();
    for line in csv.lines() {
        let fields: Vec<String> = line
            .split(',')
            .map(|f| {
                if f.starts_with('"') {
                    f.to_owned()
                } else {
                    format!("\"{f}\"")
                }
            })
            .collect();
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str("\"2026-01-17\",\"2026\",\"Synthetic Rovers\",\"Fixture Town\",\"1\",\"1\"\n");
    out.into_bytes()
}

async fn load_and_verify(
    clickhouse: &ClickHouse,
    server: &TestServer,
    format: OutputFormat,
    run: &str,
) -> Vec<Match> {
    let table = Identifier::new("football_load").unwrap();
    let context = CallContext::new()
        .database(Identifier::new("fixtures").unwrap())
        .quota_key("loader-7")
        .unwrap();
    let insert_id = QueryId::new(format!("football-load/{run}/insert")).unwrap();
    let verify_id = QueryId::new(format!("football-load/{run}/verify")).unwrap();
    assert_ne!(insert_id, verify_id);

    let insert = clickhouse
        .insert(Table::new(table.clone()))
        .query_id(insert_id.clone())
        .context(&context);
    let source = stream::from_iter(loaded_rows().into_iter().map(Ok::<_, Infallible>));
    let receipt = match format {
        OutputFormat::JsonEachRow => insert.json_each_row(source).await,
        OutputFormat::CsvWithNames => insert.csv_with_names(source).await,
    }
    .unwrap();
    assert_eq!(receipt.query_id(), Some(insert_id.as_str()));
    assert_eq!(receipt.summary().unwrap().unwrap().written_rows, Some(18));

    let query = Query::new("SELECT * FROM {table:Identifier} WHERE season = {season:Int16}")
        .bind("table", &table)
        .unwrap()
        .bind("season", 2021_i16)
        .unwrap();
    let collected = clickhouse
        .select(query, format)
        .query_id(verify_id.clone())
        .context(&context)
        .rows::<Match>()
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(collected.outcome, Outcome::Complete);
    assert_eq!(collected.meta.query_id(), Some(verify_id.as_str()));
    let _ = server;
    collected.rows
}

#[tokio::test]
async fn workflow_b_streamed_insert_then_query_back_with_a_distinct_id_and_the_same_context() {
    let store = Store::default();
    let responder = store.clone();
    let server = TestServer::start(move |request| responder.respond(request)).await;
    let clickhouse = client(&server.url());

    for (run, format) in [
        ("json", OutputFormat::JsonEachRow),
        ("csv", OutputFormat::CsvWithNames),
    ] {
        let back = load_and_verify(&clickhouse, &server, format, run).await;
        assert_eq!(
            back,
            football_rows(),
            "{run}: the 2021 rows, filtered by the bound season"
        );
    }

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    let mut ids = Vec::new();
    for request in &requests {
        assert_eq!(request.header("x-clickhouse-database"), Some("fixtures"));
        assert_eq!(param(request, "quota_key").as_deref(), Some("loader-7"));
        ids.push(param(request, "query_id").unwrap());
    }
    assert_eq!(
        ids,
        [
            "football-load/json/insert",
            "football-load/json/verify",
            "football-load/csv/insert",
            "football-load/csv/verify"
        ]
    );
    // The JSONEachRow insert body of the documented rows is the documented output byte for byte
    // (plus the synthetic 2026 row); the CSV body is the documented output with every field quoted.
    let mut json = football_json();
    json.extend_from_slice(
        br#"{"date":"2026-01-17","season":2026,"home_team":"Synthetic Rovers","away_team":"Fixture Town","home_team_goals":1,"away_team_goals":1}"#,
    );
    json.push(b'\n');
    assert_eq!(requests[0].body, json);
    assert!(requests[0].chunked);
    assert_eq!(
        param(&requests[0], "query").as_deref(),
        Some("INSERT INTO `football_load` FORMAT JSONEachRow")
    );
    assert_eq!(requests[2].body, documented_csv_all_quoted());
    assert_eq!(
        param(&requests[2], "query").as_deref(),
        Some("INSERT INTO `football_load` FORMAT CSVWithNames")
    );
    assert_eq!(
        param(&requests[1], "param_table").as_deref(),
        Some("football_load")
    );
}

#[tokio::test]
async fn cancellation_stops_the_read_and_asks_the_server_to_kill_the_query() {
    let server = TestServer::start(|request| {
        if request.method == "POST" {
            let id = unescape(&param(request, "param_id").unwrap()).unwrap();
            let body = format!(
                "{{\"kill_status\":\"waiting\",\"query_id\":\"{id}\",\"user\":\"analyst\",\"query\":\"SELECT number FROM system.numbers\"}}\n"
            );
            return rows_reply(request, OutputFormat::JsonEachRow, vec![body.into_bytes()]);
        }
        // An endless result, one row every 20 ms, generated lazily.
        let mut n = 0u64;
        let rows = Box::new(move || {
            n += 1;
            Some(Bytes::from(format!("{{\"number\":\"{n}\"}}\n")))
        });
        rows_reply_with(
            request,
            OutputFormat::JsonEachRow,
            Some(TAG),
            Body::Generate(rows, Some(Duration::from_millis(20))),
        )
    })
    .await;
    let clickhouse = client(&server.url());
    let id = QueryId::new("endless-numbers-1").unwrap();

    #[derive(serde::Deserialize)]
    struct Number {
        // UInt64 is a JSON string in JSONEachRow by default.
        number: String,
    }
    let mut rows = clickhouse
        .select(
            Query::new("SELECT number FROM system.numbers"),
            OutputFormat::JsonEachRow,
        )
        .query_id(id.clone())
        .rows::<Number>()
        .await
        .unwrap();
    assert_eq!(rows.next().await.unwrap().unwrap().number, "1");
    assert_eq!(rows.next().await.unwrap().unwrap().number, "2");
    // A `next()` dropped while it waits loses nothing: the next call returns row 3.
    let pending = tokio::time::timeout(Duration::from_millis(1), rows.next()).await;
    if let Ok(row) = pending {
        assert_eq!(row.unwrap().unwrap().number, "3");
    } else {
        assert_eq!(rows.next().await.unwrap().unwrap().number, "3");
    }
    assert_eq!(rows.outcome(), Outcome::Reading);

    // Server side: KILL QUERY with the ID bound; client side: drop the rows.
    let statuses = clickhouse.cancel(&id).await.unwrap();
    assert_eq!(statuses[0].query_id, "endless-numbers-1");
    assert_eq!(statuses[0].kill_status, "waiting");
    drop(rows);
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0)
            .await,
        "the dropped response stops the server's writes"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        param(&requests[1], "param_id").as_deref(),
        Some("endless-numbers-1")
    );
}
