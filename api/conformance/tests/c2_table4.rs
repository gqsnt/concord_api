//! Table 4 of docs/cases.md, "A value and its place": each shape of value in each place.
//!
//! Every cell that is decided when the request is built is here; a cell that is decided at
//! compile time (message 13: an optional input in a hole of a path or of a host) is not, and
//! is marked `None`. A cell that fails names its row and its column; every cell is run, and
//! the test fails once with the list.
//!
//! Every text and value is compared as it is: the hex digits of `%XX` are upper case
//! (ANSWERS.md A3), and a Build error is compared word for word (A1, A6).

#[macro_use]
mod common;

use common::*;
use concord::__private::{AdaptValue as _, Draft, Piece, Value, ValueRef};
use concord::body::Entries;
use concord::Secret;
use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};

const ENDPOINT: &str = "tasks.get";
const HOST: &str = "build error: input filter is not one label of a host, in tasks.get";
const SEGMENT: &str = "build error: input filter gives an empty or dot path segment, in tasks.get";
const SHAPE: &str = "build error: input filter cannot be written in a query, in tasks.get";
const FINITE: &str = "build error: input filter is not a finite number, in tasks.get";
const FINITE_HEADER: &str = "build error: input x-note is not a finite number, in tasks.get";

#[derive(Serialize)]
struct Nested {
    a: u32,
}

/// What a cell of a request column says.
enum Want {
    /// The URL, as text.
    Url(&'static str),
    /// The query, as text: `""` when no pair is sent, and then there is no `?` (A4).
    Query(&'static str),
    /// Every value of the header `x-note`, in order (none when it is not sent), and whether
    /// it is marked sensitive.
    Header(&'static [&'static str], bool),
    /// The Build error of a shape that cannot be written at the place of the column:
    /// `input <name> cannot be written in a <place>` (ANSWERS.md A1).
    Shape,
    /// A Build error, word for word.
    Text(&'static str),
}

fn check(built: Built, want: &Want, column: &Column) -> Result<(), String> {
    let outcome = match built {
        Ok(request) => Ok(request),
        Err(error) => Err((error.kind(), error.to_string())),
    };
    match (want, outcome) {
        (Want::Url(expected), Ok(request)) => {
            let actual = url(&request);
            (actual == *expected)
                .then_some(())
                .ok_or(format!("the URL is {actual}, not {expected}"))
        }
        (Want::Query(expected), Ok(request)) => {
            let actual = request.url().query();
            let wanted = (!expected.is_empty()).then_some(*expected);
            (actual == wanted)
                .then_some(())
                .ok_or(format!("the query is {actual:?}, not {wanted:?}"))
        }
        (Want::Header(expected, marked), Ok(request)) => {
            let actual = header(&request, "x-note");
            if actual != *expected {
                return Err(format!("the header is {actual:?}, not {expected:?}"));
            }
            if !expected.is_empty() && sensitive(&request, "x-note") != *marked {
                return Err(format!("the header is not marked sensitive: {marked}"));
            }
            Ok(())
        }
        (Want::Shape, Err((kind, text))) => {
            if kind != concord::error::Kind::Build {
                return Err(format!("a {kind:?} error, not a Build error"));
            }
            let expected = cannot_be_written(column.name, column.place, ENDPOINT);
            (text == expected)
                .then_some(())
                .ok_or(format!("the text is {text:?}, not {expected:?}"))
        }
        (Want::Text(expected), Err((kind, text))) => {
            if kind != concord::error::Kind::Build {
                return Err(format!("a {kind:?} error, not a Build error"));
            }
            (text == *expected)
                .then_some(())
                .ok_or(format!("the text is {text:?}, not {expected:?}"))
        }
        (_, Ok(request)) => Err(format!("a request to {}, not a Build error", request.url())),
        (_, Err((kind, _))) => Err(format!("a {kind:?} error, not a request")),
    }
}

/// Runs one cell; a panic (such as a `todo!()`) is a failure of that cell.
fn cell(
    failures: &mut Vec<String>,
    row: &str,
    column: &str,
    run: impl FnOnce() -> Result<(), String>,
) {
    let outcome = catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|panic| {
        let text = panic
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(format!("panicked: {text}"))
    });
    if let Err(why) = outcome {
        failures.push(format!("[{row}] x [{column}]: {why}"));
    }
}

fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} cells of table 4 fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// The seven places of a request. The name of every value is `filter`; a whole entry of
// headers is named by its key, `x-note`.

fn path(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.segment(&[Piece::Val("filter", value)]);
    finish(draft, ENDPOINT)
}

fn rest(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.segments("filter", value);
    finish(draft, ENDPOINT)
}

fn host(value: &dyn Value) -> Built {
    let mut draft = Draft::new(http::Method::GET);
    draft.base(&[
        Piece::Lit("https://api-"),
        Piece::Val("filter", value),
        Piece::Lit(".example.com"),
    ]);
    draft.segment(&[Piece::Lit("v1")]);
    finish(draft, ENDPOINT)
}

fn whole_query(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.query("filter", &[Piece::Val("filter", value)]);
    finish(draft, ENDPOINT)
}

fn whole_header(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.header("x-note", &[Piece::Val("x-note", value)]);
    finish(draft, ENDPOINT)
}

fn template_query(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.query("q", &[Piece::Lit("by-"), Piece::Val("filter", value)]);
    finish(draft, ENDPOINT)
}

fn template_header(value: &dyn Value) -> Built {
    let mut draft = get();
    draft.header("x-note", &[Piece::Lit("by-"), Piece::Val("filter", value)]);
    finish(draft, ENDPOINT)
}

/// A column: how a value is placed, the column's title, the name that its Build error
/// gives, and the place that it names.
struct Column {
    build: fn(&dyn Value) -> Built,
    title: &'static str,
    name: &'static str,
    place: &'static str,
}

const COLUMNS: [Column; 7] = [
    Column {
        build: path,
        title: "a {x} hole in a path",
        name: "filter",
        place: "path",
    },
    Column {
        build: rest,
        title: "a {*x} hole",
        name: "filter",
        place: "path",
    },
    Column {
        build: host,
        title: "a hole in a host",
        name: "filter",
        place: "host",
    },
    Column {
        build: whole_query,
        title: "a whole entry of query",
        name: "filter",
        place: "query",
    },
    Column {
        build: whole_header,
        title: "a whole entry of headers",
        name: "x-note",
        place: "header",
    },
    Column {
        build: template_query,
        title: "a hole in a template of an entry of query",
        name: "filter",
        place: "query",
    },
    Column {
        build: template_header,
        title: "a hole in a template of an entry of headers",
        name: "filter",
        place: "header",
    },
];

/// One row: the value goes through the adapter, then to each column whose cell is decided
/// when the request is built.
fn row(failures: &mut Vec<String>, name: &str, value: &dyn Value, wants: [Option<Want>; 7]) {
    for (column, want) in COLUMNS.iter().zip(wants) {
        if let Some(want) = want {
            cell(failures, name, column.title, || {
                check((column.build)(value), &want, column)
            });
        }
    }
}

use Want::{Header, Query, Shape, Text, Url};

#[test]
fn c2_5_table_4_each_shape_of_value_in_each_place_of_a_request() {
    let mut failures = Vec::new();
    let f = &mut failures;

    row(
        f,
        "an optional input (count?: u32), not set",
        &(&ValueRef(&None::<u32>)).adapt(),
        [
            None,
            None,
            None,
            Some(Query("")),
            Some(Header(&[], false)),
            Some(Query("")),
            Some(Header(&[], false)),
        ],
    );
    for (name, value) in [
        (
            "a None of a required input (x: Option<T>)",
            &(&ValueRef(&None::<u32>)).adapt() as &dyn Value,
        ),
        ("the unit", &(&ValueRef(&())).adapt()),
    ] {
        row(
            f,
            name,
            value,
            [
                Some(Shape),
                Some(Shape),
                Some(Shape),
                Some(Query("")),
                Some(Header(&[], false)),
                Some(Query("")),
                Some(Header(&[], false)),
            ],
        );
    }
    row(
        f,
        "milestone?: Option<u64>, set to nothing",
        &(&ValueRef(&Some(None::<u64>))).adapt(),
        [
            None,
            None,
            None,
            Some(Query("")),
            Some(Header(&[], false)),
            Some(Query("")),
            Some(Header(&[], false)),
        ],
    );
    row(
        f,
        "one text",
        &(&ValueRef(&"eu-1")).adapt(),
        [
            Some(Url("https://api.example.com/v1/eu-1")),
            Some(Url("https://api.example.com/v1/eu-1")),
            Some(Url("https://api-eu-1.example.com/v1")),
            Some(Query("filter=eu-1")),
            Some(Header(&["eu-1"], false)),
            Some(Query("q=by-eu-1")),
            Some(Header(&["by-eu-1"], false)),
        ],
    );
    row(
        f,
        "one text, with bytes to encode",
        &(&ValueRef(&"a~b/c%")).adapt(),
        [
            Some(Url("https://api.example.com/v1/a~b%2Fc%25")),
            Some(Url("https://api.example.com/v1/a~b/c%25")),
            Some(Text(HOST)),
            Some(Query("filter=a~b%2Fc%25")),
            Some(Header(&["a~b/c%"], false)),
            Some(Query("q=by-a~b%2Fc%25")),
            Some(Header(&["by-a~b/c%"], false)),
        ],
    );
    row(
        f,
        "one text, empty",
        &(&ValueRef(&"")).adapt(),
        [
            Some(Text(SEGMENT)),
            Some(Text(SEGMENT)),
            Some(Text(HOST)),
            Some(Query("filter=")),
            Some(Header(&[""], false)),
            Some(Query("q=by-")),
            Some(Header(&["by-"], false)),
        ],
    );
    row(
        f,
        "one text, ..",
        &(&ValueRef(&"..")).adapt(),
        [
            Some(Text(SEGMENT)),
            Some(Text(SEGMENT)),
            Some(Text(HOST)),
            Some(Query("filter=..")),
            Some(Header(&[".."], false)),
            Some(Query("q=by-..")),
            Some(Header(&["by-.."], false)),
        ],
    );
    row(
        f,
        "a number",
        &(&ValueRef(&42u32)).adapt(),
        [
            Some(Url("https://api.example.com/v1/42")),
            Some(Url("https://api.example.com/v1/42")),
            Some(Url("https://api-42.example.com/v1")),
            Some(Query("filter=42")),
            Some(Header(&["42"], false)),
            Some(Query("q=by-42")),
            Some(Header(&["by-42"], false)),
        ],
    );
    row(
        f,
        "a number, a whole float",
        &(&ValueRef(&1.0f64)).adapt(),
        [
            Some(Url("https://api.example.com/v1/1.0")),
            Some(Url("https://api.example.com/v1/1.0")),
            Some(Text(HOST)),
            Some(Query("filter=1.0")),
            Some(Header(&["1.0"], false)),
            Some(Query("q=by-1.0")),
            Some(Header(&["by-1.0"], false)),
        ],
    );
    row(
        f,
        "a number, a float that is not finite",
        &(&ValueRef(&f64::NAN)).adapt(),
        [
            Some(Text(FINITE)),
            Some(Text(FINITE)),
            Some(Text(FINITE)),
            Some(Text(FINITE)),
            Some(Text(FINITE_HEADER)),
            Some(Text(FINITE)),
            Some(Text(FINITE)),
        ],
    );
    row(
        f,
        "a bool",
        &(&ValueRef(&true)).adapt(),
        [
            Some(Url("https://api.example.com/v1/true")),
            Some(Url("https://api.example.com/v1/true")),
            Some(Url("https://api-true.example.com/v1")),
            Some(Query("filter=true")),
            Some(Header(&["true"], false)),
            Some(Query("q=by-true")),
            Some(Header(&["by-true"], false)),
        ],
    );
    row(
        f,
        "a list",
        &(&ValueRef(&vec!["a", "b"])).adapt(),
        [
            Some(Shape),
            Some(Shape),
            Some(Shape),
            Some(Query("filter=a&filter=b")),
            Some(Header(&["a", "b"], false)),
            Some(Text(SHAPE)),
            Some(Shape),
        ],
    );
    row(
        f,
        "an empty list",
        &(&ValueRef(&Vec::<String>::new())).adapt(),
        [
            Some(Shape),
            Some(Shape),
            Some(Shape),
            Some(Query("")),
            Some(Header(&[], false)),
            Some(Query("")),
            Some(Header(&[], false)),
        ],
    );
    row(
        f,
        "a struct",
        &(&ValueRef(&Nested { a: 1 })).adapt(),
        [
            Some(Shape),
            Some(Shape),
            Some(Shape),
            Some(Text(SHAPE)),
            Some(Shape),
            Some(Text(SHAPE)),
            Some(Shape),
        ],
    );
    let secret = Secret::new("s-k");
    row(
        f,
        "a Secret",
        &(&ValueRef(&secret)).adapt(),
        [
            Some(Url("https://api.example.com/v1/s-k")),
            Some(Url("https://api.example.com/v1/s-k")),
            Some(Url("https://api-s-k.example.com/v1")),
            Some(Query("filter=s-k")),
            Some(Header(&["s-k"], true)),
            Some(Query("q=by-s-k")),
            Some(Header(&["by-s-k"], true)),
        ],
    );
    row(
        f,
        "an optional Secret (key?: Secret), set",
        &(&ValueRef(&Some(Secret::new("s-k")))).adapt(),
        [
            None,
            None,
            None,
            Some(Query("filter=s-k")),
            Some(Header(&["s-k"], true)),
            Some(Query("q=by-s-k")),
            Some(Header(&["by-s-k"], true)),
        ],
    );
    row(
        f,
        "an optional Secret (key?: Secret), not set",
        &(&ValueRef(&None::<Secret>)).adapt(),
        [
            None,
            None,
            None,
            Some(Query("")),
            Some(Header(&[], false)),
            Some(Query("")),
            Some(Header(&[], false)),
        ],
    );
    report(failures);
}

/// What a cell of a column of a body says about the form.
enum FormWant {
    /// These bytes.
    Pairs(&'static str),
    /// The writer refuses: a Build error, `the body could not be written`.
    Refused,
}

fn body_cell(
    failures: &mut Vec<String>,
    name: &str,
    entries: &Entries<'_>,
    json_want: &str,
    form_want: FormWant,
) {
    cell(failures, name, "an entry of a body, through json", || {
        let body = concord::json(entries).map_err(|error| format!("json refused: {error}"))?;
        let actual = String::from_utf8_lossy(body.as_bytes().unwrap_or_default()).into_owned();
        (actual == json_want)
            .then_some(())
            .ok_or(format!("json wrote {actual}, not {json_want}"))
    });
    cell(failures, name, "an entry of a body, through form", || {
        let written = concord::form(entries);
        match (form_want, written) {
            (FormWant::Pairs(expected), Ok(body)) => {
                let actual =
                    String::from_utf8_lossy(body.as_bytes().unwrap_or_default()).into_owned();
                (actual == expected)
                    .then_some(())
                    .ok_or(format!("form wrote {actual:?}, not {expected:?}"))
            }
            (FormWant::Refused, Err(_)) => Ok(()),
            (FormWant::Pairs(_), Err(error)) => Err(format!("form refused: {error}")),
            (FormWant::Refused, Ok(_)) => Err("form wrote it, and should refuse".into()),
        }
    });
}

/// One entry `filter`, given to `Entries` through the adapter as the generated code does.
macro_rules! body_row {
    ($failures:expr, $name:expr, $value:expr, $json:expr, $form:expr) => {{
        let value = $value;
        let adapted = ValueRef(&value);
        let mut entries = Entries::new();
        entries.__push("filter", (&adapted).adapt());
        body_cell($failures, $name, &entries, $json, $form);
    }};
}

#[test]
fn c2_14_table_4_each_shape_of_value_in_an_entry_of_a_body() {
    use FormWant::{Pairs, Refused};
    let mut failures = Vec::new();
    let f = &mut failures;
    let cursor: Option<u32> = None;

    body_row!(
        f,
        "an optional input, not set: cursor;",
        cursor,
        "{}",
        Pairs("")
    );
    body_row!(
        f,
        "an optional input, not set: an expression over it",
        cursor.map(|c| c + 1),
        "{}",
        Pairs("")
    );
    body_row!(
        f,
        "a None of a required input",
        None::<u32>,
        "{}",
        Pairs("")
    );
    body_row!(f, "the unit", (), r#"{"filter":null}"#, Refused);
    body_row!(
        f,
        "a None of a required input, as Some(x)",
        Some(None::<u32>),
        r#"{"filter":null}"#,
        Pairs("")
    );
    body_row!(
        f,
        "milestone?: Option<u64>, set to nothing",
        Some(None::<u64>),
        r#"{"filter":null}"#,
        Pairs("")
    );
    body_row!(
        f,
        "one text",
        "eu-1",
        r#"{"filter":"eu-1"}"#,
        Pairs("filter=eu-1")
    );
    body_row!(f, "a number", 42u32, r#"{"filter":42}"#, Pairs("filter=42"));
    // As `serde_urlencoded` writes it: a float as `ryu` writes it, `.0` for a whole one.
    body_row!(
        f,
        "a number, a float",
        1.5f64,
        r#"{"filter":1.5}"#,
        Pairs("filter=1.5")
    );
    body_row!(
        f,
        "a number, a whole float",
        1.0f64,
        r#"{"filter":1.0}"#,
        Pairs("filter=1.0")
    );
    // As `serde_urlencoded` writes a text: a space as `+`; `*-._` and letters and digits as
    // they are; every other byte as `%XX`.
    body_row!(
        f,
        "one text, with bytes to encode",
        "a b+c&d=é*~",
        r#"{"filter":"a b+c&d=é*~"}"#,
        Pairs("filter=a+b%2Bc%26d%3D%C3%A9*%7E")
    );
    body_row!(
        f,
        "a bool",
        true,
        r#"{"filter":true}"#,
        Pairs("filter=true")
    );
    body_row!(
        f,
        "a list",
        vec!["a", "b"],
        r#"{"filter":["a","b"]}"#,
        Refused
    );
    body_row!(
        f,
        "an empty list",
        Vec::<String>::new(),
        r#"{"filter":[]}"#,
        Refused
    );
    body_row!(
        f,
        "a struct",
        Nested { a: 1 },
        r#"{"filter":{"a":1}}"#,
        Refused
    );
    body_row!(
        f,
        "a Secret",
        Secret::new("s-k"),
        r#"{"filter":"s-k"}"#,
        Pairs("filter=s-k")
    );
    body_row!(
        f,
        "an optional Secret, set",
        Some(Secret::new("s-k")),
        r#"{"filter":"s-k"}"#,
        Pairs("filter=s-k")
    );
    body_row!(
        f,
        "an optional Secret, not set",
        None::<Secret>,
        "{}",
        Pairs("")
    );
    report(failures);
}
