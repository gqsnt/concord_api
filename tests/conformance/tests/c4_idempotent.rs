//! C4.1: the mark `idempotent`, and the cells of tables 6.1 to 6.4 of docs/reference/cases.md that
//! needed an endpoint of `tests/tour` that the four first clients do not have. `Shapes` has them:
//!
//! | Column of the table | Endpoint |
//! |---|---|
//! | A `POST` with `idempotent;` | `shapes.again`: every row |
//! | A `PUT` with `idempotent false;` | `shapes.replace`, an arrow: the rows "A body that broke" (with its head) and "A Mismatch" |
//! | A body that is a stream | `shapes.upload`, an arrow: the same two rows |
//!
//! The other cells of the last two columns are in `c4_tables.rs`, on `files.upload`.
//! Each cell scripts the outcome of its row for the first try, then a success.

mod common;

use common::engine::*;
use common::stream;
use concord::error::{Class, Kind};
use concord::transport::TransportError;
use concord::Retry;
use concord_testkit::Script;
use std::time::Duration;
use tour::ShapesProblem;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Column {
    PostIdempotent,
    PutNotIdempotent,
    Stream,
}

impl Column {
    fn name(self) -> &'static str {
        match self {
            Column::PostIdempotent => "A POST with idempotent;",
            Column::PutNotIdempotent => "A PUT with idempotent false;",
            Column::Stream => "A body that is a stream",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Row {
    Value,
    Status(u16),
    Connect,
    Timeout,
    BodyBroke,
    Other,
    Blocked,
    Mismatch,
    Build,
}

/// What a cell says.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Want {
    /// Another try.
    Again,
    /// No other try: the outcome of the row (a value, final, or none).
    Once,
}

use Want::{Again, Once};

/// Script the outcome of the row for the first try.
fn script_row(script: &Script, row: Row) {
    match row {
        Row::Value => json(script, 200, ITEM),
        Row::Status(status) => {
            script.answer(status).body("no");
        }
        Row::Connect => {
            script.fail(TransportError::connect());
        }
        Row::Timeout => {
            script.never();
        }
        Row::BodyBroke => {
            script
                .answer(200)
                .header("content-type", "application/json")
                .header("x-head", "kept")
                .body(r#"{"id":"#)
                .cut();
        }
        Row::Other => {
            script.fail(TransportError::new(Class::Other));
        }
        Row::Blocked => {
            script.fail(TransportError::blocked());
        }
        Row::Mismatch => json(script, 200, "not json"),
        Row::Build => {}
    }
}

/// Send the request of the column, with the policy, and keep the outcome only.
async fn send(
    script: &Script,
    column: Column,
    policy: Option<Retry<ShapesProblem>>,
    bad_base: bool,
) -> Result<(), concord::Error<ShapesProblem>> {
    let mut client = shapes(script);
    if let Some(policy) = policy {
        client = client.with_retry(policy);
    }
    if bad_base {
        client = client.with_base("not a url");
    }
    match column {
        Column::PostIdempotent => client.again(1).await.map(drop),
        Column::PutNotIdempotent => client.replace(1, item()).await.map(drop),
        Column::Stream => client.upload(stream("abc")).await.map(drop),
    }
}

/// The error that the outcome of the row is.
fn is_the_row(error: &concord::Error<ShapesProblem>, row: Row) -> Result<(), String> {
    match row {
        Row::Value => Err("a value was expected".to_owned()),
        Row::Status(status) => {
            is(error, Kind::Refused, None)?;
            check(error.is_status(status), || {
                format!("status {:?}, not {status}", error.status())
            })
        }
        Row::Connect => is(error, Kind::Transport, Some(Class::Connect)),
        Row::Timeout => is(error, Kind::Transport, Some(Class::Timeout)),
        Row::BodyBroke => {
            is(error, Kind::Transport, Some(Class::Body))?;
            // C5.4: it keeps the status and the headers.
            check(
                error.is_status(200)
                    && error.headers().and_then(|h| h.get("x-head")) == Some("kept"),
                || "the status and the headers are not kept".to_owned(),
            )
        }
        Row::Other => is(error, Kind::Transport, Some(Class::Other)),
        Row::Blocked => is(error, Kind::Transport, Some(Class::Blocked)),
        Row::Mismatch => is(error, Kind::Mismatch, None),
        Row::Build => is(error, Kind::Build, None),
    }
}

/// One cell: the outcome of the row, then what the cell wants.
async fn one_cell(
    row: Row,
    column: Column,
    want: Want,
    policy: fn(Count) -> Option<Retry<ShapesProblem>>,
    rule_wait: Option<Duration>,
) -> Result<(), String> {
    let script = Script::new();
    script_row(&script, row);
    json(&script, 200, ITEM);
    let asks = Count::default();
    let bad_base = matches!(row, Row::Build);

    let result = send(&script, column, policy(asks.clone()), bad_base).await;
    let sent = script.requests().len();

    match (want, result) {
        (Again, Ok(())) => {
            check(sent == 2, || format!("a value after {sent} tries, not 2"))?;
            if let Some(wait) = rule_wait {
                let requests = script.requests();
                let gap = requests[1].at() - requests[0].at();
                check(gap >= wait, || {
                    format!("waited {gap:?}, less than {wait:?}")
                })?;
                check(asks.get() == 1, || {
                    format!("the rule was asked {} times", asks.get())
                })?;
            }
            Ok(())
        }
        (Again, Err(error)) => Err(format!(
            "another try was expected; {} tries, then {}",
            sent,
            outcome::<(), ShapesProblem>(&Err(error))
        )),
        (Once, result) => {
            let expected_sent = if bad_base { 0 } else { 1 };
            check(sent == expected_sent, || {
                format!("{sent} tries, not {expected_sent}: {}", outcome(&result))
            })?;
            if rule_wait.is_some() {
                check(asks.get() == 0, || {
                    format!("the rule was asked {} times", asks.get())
                })?;
            }
            match (row, result) {
                (Row::Value, Ok(())) => Ok(()),
                (_, Ok(())) => Err("a value, not the outcome of the row".to_owned()),
                (_, Err(error)) => is_the_row(&error, row),
            }
        }
    }
}

/// A row of a table: its name, its outcome, and the cell of each column that it has.
type TableRow = (&'static str, Row, Vec<(Column, Want)>);

/// Run a table.
async fn table(
    name: &'static str,
    rows: Vec<TableRow>,
    policy: fn(Count) -> Option<Retry<ShapesProblem>>,
    rule_wait: Option<Duration>,
) {
    let mut cells = Vec::new();
    for (row_name, row, wants) in rows {
        for (column, want) in wants {
            cells.push(cell(
                row_name,
                column.name(),
                one_cell(row, column, want, policy, rule_wait),
            ));
        }
    }
    run_cells(name, cells).await;
}

const VALUE: &str = "A value";
const STANDARD: &str = "A refusal with a status of the standard table";
const ANOTHER: &str = "Another refusal";
const CONNECT: &str = "A failed connection";
const TIMEOUT: &str = "A timeout";
const BODY: &str = "A body that broke";
const OTHER: &str = "Another failure of the transport";
const BLOCKED: &str = "blocked";
const MISMATCH: &str = "A Mismatch";
const BUILD: &str = "A Build error";

/// The cell of `shapes.again` only.
fn again(want: Want) -> Vec<(Column, Want)> {
    vec![(Column::PostIdempotent, want)]
}

/// The cell of `shapes.again`, and the two cells of the other columns, which have no other
/// try in every table: a `PUT` with `idempotent false;` may not be repeated (C4.1), and a
/// stream is sent once (C3.2).
fn three(want: Want) -> Vec<(Column, Want)> {
    vec![
        (Column::PostIdempotent, want),
        (Column::PutNotIdempotent, Once),
        (Column::Stream, Once),
    ]
}

#[tokio::test(start_paused = true)]
async fn c4_1_table_6_1_retry_is_off() {
    table(
        "table 6.1",
        vec![
            (VALUE, Row::Value, again(Once)),
            (STANDARD, Row::Status(503), again(Once)),
            (ANOTHER, Row::Status(404), again(Once)),
            (CONNECT, Row::Connect, again(Once)),
            (TIMEOUT, Row::Timeout, again(Once)),
            (BODY, Row::BodyBroke, three(Once)),
            (OTHER, Row::Other, again(Once)),
            (BLOCKED, Row::Blocked, again(Once)),
            (MISMATCH, Row::Mismatch, three(Once)),
            (BUILD, Row::Build, again(Once)),
        ],
        |_| None,
        None,
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn c4_2_table_6_2_with_retry_standard() {
    table(
        "table 6.2",
        vec![
            (VALUE, Row::Value, again(Once)),
            (STANDARD, Row::Status(503), again(Again)),
            (ANOTHER, Row::Status(404), again(Once)),
            (CONNECT, Row::Connect, again(Again)),
            (TIMEOUT, Row::Timeout, again(Again)),
            (BODY, Row::BodyBroke, three(Again)),
            (OTHER, Row::Other, again(Once)),
            (BLOCKED, Row::Blocked, again(Once)),
            (MISMATCH, Row::Mismatch, three(Once)),
            (BUILD, Row::Build, again(Once)),
        ],
        |_| Some(Retry::standard()),
        None,
    )
    .await;
}

/// The wait that the rule of table 6.3 gives.
const RULE_WAIT: Duration = Duration::from_secs(2);

#[tokio::test(start_paused = true)]
async fn c4_7_table_6_3_with_a_rule_of_when_that_gives_a_wait() {
    table(
        "table 6.3",
        vec![
            (VALUE, Row::Value, again(Once)),
            (STANDARD, Row::Status(503), again(Again)),
            (ANOTHER, Row::Status(404), again(Again)),
            (CONNECT, Row::Connect, again(Again)),
            (TIMEOUT, Row::Timeout, again(Again)),
            (BODY, Row::BodyBroke, three(Again)),
            (OTHER, Row::Other, again(Again)),
            (BLOCKED, Row::Blocked, again(Once)),
            (MISMATCH, Row::Mismatch, three(Once)),
            (BUILD, Row::Build, again(Once)),
        ],
        |asks| {
            Some(Retry::standard().when(move |_| {
                asks.add();
                Some(RULE_WAIT)
            }))
        },
        Some(RULE_WAIT),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn c4_1_table_6_4_with_a_status_listed_with_not_applied_on() {
    table(
        "table 6.4",
        vec![
            (VALUE, Row::Value, again(Once)),
            (
                "A refusal with a listed status that the standard table names (a 429)",
                Row::Status(429),
                again(Again),
            ),
            (
                "A refusal with a listed status that the standard table does not name (a 409)",
                Row::Status(409),
                again(Once),
            ),
            (
                "A refusal with a status of the standard table that the policy does not list (a 503)",
                Row::Status(503),
                again(Again),
            ),
            (ANOTHER, Row::Status(404), again(Once)),
            (CONNECT, Row::Connect, again(Again)),
            (TIMEOUT, Row::Timeout, again(Again)),
            (BODY, Row::BodyBroke, three(Again)),
            (OTHER, Row::Other, again(Once)),
            (BLOCKED, Row::Blocked, again(Once)),
            (MISMATCH, Row::Mismatch, three(Once)),
            (BUILD, Row::Build, again(Once)),
        ],
        |_| Some(Retry::standard().not_applied_on(429).not_applied_on(409)),
        None,
    )
    .await;
}

// The mark as an expression: `idempotent trace.idempotent();` on `shapes.order`, a `POST`.

#[tokio::test(start_paused = true)]
async fn c4_1_the_mark_runs_once_for_a_send_after_idle_timeout_and_before_the_request() {
    let script = Script::new();
    script.answer(503);
    json(&script, 200, ITEM);
    let trace = tour::Trace::new(true, Duration::from_secs(5));
    let value = success(
        shapes(&script)
            .with_retry(Retry::standard())
            .order(&trace)
            .await,
    );
    assert_eq!(value, item());
    assert_eq!(script.requests().len(), 2, "the mark allowed another try");
    // Once for the send, not once for each try; the request is built after the mark.
    assert_eq!(trace.seen(), ["idle_timeout", "idempotent", "query"]);
    assert_eq!(
        script.requests()[1].url(),
        "https://shapes.example.com/order?step=query"
    );
}

#[tokio::test(start_paused = true)]
async fn c4_1_a_mark_that_gives_false_takes_the_other_try_away() {
    let script = Script::new();
    script.answer(503);
    json(&script, 200, ITEM);
    let trace = tour::Trace::new(false, Duration::from_secs(5));
    let error = failure(
        shapes(&script)
            .with_retry(Retry::standard())
            .order(&trace)
            .await,
    );
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(503));
    assert_eq!(
        script.requests().len(),
        1,
        "a POST that the mark does not allow"
    );
    assert_eq!(trace.seen(), ["idle_timeout", "idempotent", "query"]);
}

#[tokio::test(start_paused = true)]
async fn c4_1_the_mark_runs_for_every_send_also_with_a_call_override_of_idle_timeout() {
    let script = Script::new();
    json(&script, 200, ITEM);
    json(&script, 200, ITEM);
    let trace = tour::Trace::new(true, Duration::from_secs(5));
    let call = shapes(&script)
        .order(&trace)
        .with_idle_timeout(Duration::from_secs(1));
    success(call.clone().await);
    success(call.await);
    assert_eq!(
        trace.seen(),
        [
            "idle_timeout",
            "idempotent",
            "query",
            "idle_timeout",
            "idempotent",
            "query"
        ]
    );
}

#[test]
fn c10_3_request_and_into_request_run_the_entries_and_neither_the_mark_nor_idle_timeout() {
    let trace = tour::Trace::new(true, Duration::from_secs(5));
    let call = tour::Shapes::new().order(&trace);
    assert!(call.request().is_ok());
    assert!(call.into_request().is_ok());
    assert_eq!(trace.seen(), ["query", "query"]);
}
