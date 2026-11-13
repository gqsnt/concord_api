//! Tables 6.1 to 6.4 of docs/cases.md, "Another try": each outcome of a try, for each kind of
//! request, under each policy. Every cell is run; a cell that fails names its row and column.
//!
//! Each cell scripts the outcome of its row for the first try, then a success. "Another try"
//! is a value after two tries; "none" (and "final") is the outcome of the row after one try
//! (a Build error: after none).
//!
//! The columns are the endpoints of `api/tour` that have the kind of request:
//!
//! | Column of the table | Endpoint |
//! |---|---|
//! | A `GET` | `tasks.get` |
//! | A `POST` | `tasks.create` |
//! | A `POST` with `idempotent;` | `shapes.again`, in `c4_idempotent.rs` |
//! | A `PUT` with `idempotent false;` | `files.upload`, its body in memory |
//! | A body that is a stream | `files.upload`, its body a stream |
//!
//! `files.upload` has no arrow, so nothing of a 2xx body is read: for its columns, "a body
//! that broke" is a failure of class body that the transport reports, and "a Mismatch" does
//! not arise. Those two rows, with the head and with a body that is read, are in
//! `c4_idempotent.rs` on `shapes.replace` and `shapes.upload`. The two files hold the 210
//! cells of the four tables.

mod common;

use common::engine::*;
use common::stream;
use concord::error::{Class, Kind};
use concord::transport::TransportError;
use concord::Retry;
use concord_testkit::Script;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Column {
    Get,
    Post,
    PutNotIdempotent,
    Stream,
}

const COLUMNS: [Column; 4] = [
    Column::Get,
    Column::Post,
    Column::PutNotIdempotent,
    Column::Stream,
];

impl Column {
    fn name(self) -> &'static str {
        match self {
            Column::Get => "A GET",
            Column::Post => "A POST",
            Column::PutNotIdempotent => "A PUT with idempotent false;",
            Column::Stream => "A body that is a stream",
        }
    }

    fn reads_a_body(self) -> bool {
        matches!(self, Column::Get | Column::Post)
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
    /// The cell cannot be made with the endpoints of `api/tour`.
    Skip,
}

use Want::{Again, Once, Skip};

/// The success that follows the outcome of the row.
fn success_of(script: &Script, column: Column) {
    if column.reads_a_body() {
        json(script, 200, TASK);
    } else {
        script.answer(200);
    }
}

/// Script the outcome of the row for the first try.
fn script_row(script: &Script, row: Row, column: Column) {
    match row {
        Row::Value => success_of(script, column),
        Row::Status(status) => {
            script.answer(status).body("no");
        }
        Row::Connect => {
            script.fail(TransportError::connect());
        }
        Row::Timeout => {
            script.never();
        }
        Row::BodyBroke if column.reads_a_body() => {
            script
                .answer(200)
                .header("content-type", "application/json")
                .header("x-head", "kept")
                .body(r#"{"id":"#)
                .cut();
        }
        Row::BodyBroke => {
            script.fail(TransportError::new(Class::Body));
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
    policy: Option<Retry>,
    bad_base: bool,
) -> Result<(), concord::Error> {
    match column {
        Column::Get | Column::Post => {
            let mut client = tasks(script);
            if let Some(policy) = policy {
                client = client.with_retry(policy);
            }
            if bad_base {
                client = client.with_base("not a url");
            }
            if column == Column::Get {
                client.get(7).await.map(drop)
            } else {
                client.create(new_task()).await.map(drop)
            }
        }
        Column::PutNotIdempotent | Column::Stream => {
            let mut client = files(script);
            if let Some(policy) = policy {
                client = client.with_retry(policy);
            }
            if bad_base {
                client = client.with_base("not a url");
            }
            let body = if column == Column::Stream {
                stream("abc")
            } else {
                concord::Body::new("abc", "text/plain")
            };
            client.upload("a.txt", body).await
        }
    }
}

/// The error that the outcome of the row is.
fn is_the_row(error: &concord::Error, row: Row, column: Column) -> Result<(), String> {
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
            if column.reads_a_body() {
                // C5.4: it keeps the status and the headers.
                check(
                    error.is_status(200)
                        && error.headers().and_then(|h| h.get("x-head")) == Some("kept"),
                    || "the status and the headers are not kept".to_owned(),
                )?;
            }
            Ok(())
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
    policy: impl FnOnce(Count) -> Option<Retry>,
    rule_wait: Option<std::time::Duration>,
) -> Result<(), String> {
    let script = Script::new();
    script_row(&script, row, column);
    success_of(&script, column);
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
            outcome::<(), ()>(&Err(error))
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
                (_, Err(error)) => is_the_row(&error, row, column),
            }
        }
        (Skip, _) => Ok(()),
    }
}

/// Run a table: each row with its four cells.
async fn table(
    name: &'static str,
    rows: Vec<(&'static str, Row, [Want; 4])>,
    policy: fn(Count) -> Option<Retry>,
    rule_wait: Option<std::time::Duration>,
) {
    let mut cells = Vec::new();
    for (row_name, row, wants) in rows {
        for (column, want) in COLUMNS.into_iter().zip(wants) {
            if want == Skip {
                continue;
            }
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

const ALL_ONCE: [Want; 4] = [Once; 4];
const GET_ONLY: [Want; 4] = [Again, Once, Once, Once];
const MISMATCH_CELLS: [Want; 4] = [Once, Once, Skip, Skip];

#[tokio::test(start_paused = true)]
async fn c4_1_table_6_1_retry_is_off() {
    table(
        "table 6.1",
        vec![
            (VALUE, Row::Value, ALL_ONCE),
            (STANDARD, Row::Status(503), ALL_ONCE),
            (ANOTHER, Row::Status(404), ALL_ONCE),
            (CONNECT, Row::Connect, ALL_ONCE),
            (TIMEOUT, Row::Timeout, ALL_ONCE),
            (BODY, Row::BodyBroke, ALL_ONCE),
            (OTHER, Row::Other, ALL_ONCE),
            (BLOCKED, Row::Blocked, ALL_ONCE),
            (MISMATCH, Row::Mismatch, MISMATCH_CELLS),
            (BUILD, Row::Build, ALL_ONCE),
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
            (VALUE, Row::Value, ALL_ONCE),
            (STANDARD, Row::Status(503), GET_ONLY),
            (ANOTHER, Row::Status(404), ALL_ONCE),
            (CONNECT, Row::Connect, GET_ONLY),
            (TIMEOUT, Row::Timeout, GET_ONLY),
            (BODY, Row::BodyBroke, GET_ONLY),
            (OTHER, Row::Other, ALL_ONCE),
            (BLOCKED, Row::Blocked, ALL_ONCE),
            (MISMATCH, Row::Mismatch, MISMATCH_CELLS),
            (BUILD, Row::Build, ALL_ONCE),
        ],
        |_| Some(Retry::standard()),
        None,
    )
    .await;
}

/// The wait that the rule of table 6.3 gives.
const RULE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

#[tokio::test(start_paused = true)]
async fn c4_7_table_6_3_with_a_rule_of_when_that_gives_a_wait() {
    table(
        "table 6.3",
        vec![
            (VALUE, Row::Value, ALL_ONCE),
            (STANDARD, Row::Status(503), GET_ONLY),
            (ANOTHER, Row::Status(404), GET_ONLY),
            (CONNECT, Row::Connect, GET_ONLY),
            (TIMEOUT, Row::Timeout, GET_ONLY),
            (BODY, Row::BodyBroke, GET_ONLY),
            (OTHER, Row::Other, GET_ONLY),
            (BLOCKED, Row::Blocked, ALL_ONCE),
            (MISMATCH, Row::Mismatch, MISMATCH_CELLS),
            (BUILD, Row::Build, ALL_ONCE),
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
            (VALUE, Row::Value, ALL_ONCE),
            (
                "A refusal with a listed status that the standard table names (a 429)",
                Row::Status(429),
                [Again, Again, Again, Once],
            ),
            (
                "A refusal with a listed status that the standard table does not name (a 409)",
                Row::Status(409),
                ALL_ONCE,
            ),
            (
                "A refusal with a status of the standard table that the policy does not list (a 503)",
                Row::Status(503),
                GET_ONLY,
            ),
            (ANOTHER, Row::Status(404), ALL_ONCE),
            (CONNECT, Row::Connect, GET_ONLY),
            (TIMEOUT, Row::Timeout, GET_ONLY),
            (BODY, Row::BodyBroke, GET_ONLY),
            (OTHER, Row::Other, ALL_ONCE),
            (BLOCKED, Row::Blocked, ALL_ONCE),
            (MISMATCH, Row::Mismatch, MISMATCH_CELLS),
            (BUILD, Row::Build, ALL_ONCE),
        ],
        |_| Some(Retry::standard().not_applied_on(429).not_applied_on(409)),
        None,
    )
    .await;
}
