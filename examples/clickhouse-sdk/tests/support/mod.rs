//! Shared test support: fixtures, a client pointed at the loopback server, ClickHouse-shaped
//! replies, and the inverse of the escaped format (what the fake server does with `param_*`).

#![allow(dead_code)]

use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply};
use concord_sdk_clickhouse::{ClickHouse, Credentials, OutputFormat};
use serde::{Deserialize, Serialize};

/// Synthetic credentials: not a real account.
pub const USER: &str = "analyst";
pub const PASSWORD: &str = "fixture-password";

/// A synthetic exception tag (16 lowercase ASCII letters, like the documented ones).
pub const TAG: &str = "kqzvmhwtyaxbnrce";

/// The `X-ClickHouse-Summary` of the documented `SELECT 1` example.
pub const SUMMARY: &str = r#"{"read_rows":"1","read_bytes":"1","written_rows":"0","written_bytes":"0","total_rows_to_read":"1","result_rows":"0","result_bytes":"0","elapsed_ns":"4505959","memory_usage":"1111711"}"#;

/// `Content-Type` of CSVWithNames and JSONEachRow answers (ClickHouse source, not the docs).
pub const CSV_TYPE: &str = "text/csv; charset=UTF-8; header=present";
pub const NDJSON_TYPE: &str = "application/x-ndjson; charset=UTF-8";

pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The documented football rows (the `CSVWithNames` and `JSONEachRow` format pages).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Match {
    pub date: String,
    pub season: u16,
    pub home_team: String,
    pub away_team: String,
    pub home_team_goals: u8,
    pub away_team_goals: u8,
}

pub fn football_csv() -> Vec<u8> {
    fixture("football.csvwithnames")
}

pub fn football_json() -> Vec<u8> {
    fixture("football.jsoneachrow")
}

/// The documented rows, decoded from the JSONEachRow fixture by `serde_json` (the reference).
pub fn football_rows() -> Vec<Match> {
    String::from_utf8(football_json())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub fn client(server_url: &str) -> ClickHouse {
    let http = ClickHouse::recommended_http_client().unwrap();
    ClickHouse::new(http, Credentials::basic(USER, PASSWORD).unwrap())
        .with_base_url(server_url)
        .unwrap()
}

/// The decoded value of a URL query parameter (form-decoded once), if present.
pub fn param(request: &Recorded, name: &str) -> Option<String> {
    let all = request.query_all(name);
    assert!(all.len() <= 1, "{name} occurs {} times", all.len());
    all.into_iter().next()
}

/// The inverse of ClickHouse's escaped format for the sequences the SDK writes.
pub fn unescape(text: &str) -> Option<String> {
    if text == "\\N" {
        return None;
    }
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('t') => '\t',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('0') => '\0',
            Some('b') => '\u{8}',
            Some('f') => '\u{c}',
            Some(other) => other,
            None => '\\',
        });
    }
    Some(out)
}

/// The requested format (`X-ClickHouse-Format` request header).
pub fn requested_format(request: &Recorded) -> Option<OutputFormat> {
    match request.header("x-clickhouse-format")? {
        "CSVWithNames" => Some(OutputFormat::CsvWithNames),
        "JSONEachRow" => Some(OutputFormat::JsonEachRow),
        _ => None,
    }
}

/// A 2xx ClickHouse answer: the documented metadata headers, the query ID echoed (or a server
/// one), the exception tag [`TAG`], and the body in `chunks` (chunked transfer).
pub fn rows_reply(request: &Recorded, format: OutputFormat, chunks: Vec<Vec<u8>>) -> Reply {
    rows_reply_with(request, format, Some(TAG), Body::Chunks(timed(chunks)))
}

/// [`rows_reply`] with another tag (`None`: a server that sends none) and body.
pub fn rows_reply_with(
    request: &Recorded,
    format: OutputFormat,
    tag: Option<&str>,
    body: Body,
) -> Reply {
    let content_type = match format {
        OutputFormat::CsvWithNames => CSV_TYPE,
        OutputFormat::JsonEachRow => NDJSON_TYPE,
    };
    let query_id = param(request, "query_id").unwrap_or_else(|| "server-assigned-0001".into());
    let mut reply = Reply::new(200)
        .header("Content-Type", content_type)
        .header("X-ClickHouse-Summary", SUMMARY)
        .header("X-ClickHouse-Server-Display-Name", "fixture-server")
        .header("X-ClickHouse-Query-Id", query_id)
        .header("X-ClickHouse-Format", format.name())
        .header("X-ClickHouse-Timezone", "Europe/London");
    if let Some(tag) = tag {
        reply = reply.header("X-ClickHouse-Exception-Tag", tag);
    }
    reply.with_body(body)
}

/// Chunks without delays.
pub fn timed(chunks: Vec<Vec<u8>>) -> Vec<(Duration, concord::bytes::Bytes)> {
    chunks
        .into_iter()
        .map(|c| (Duration::ZERO, c.into()))
        .collect()
}

/// A non-2xx ClickHouse answer: the exception text and its code header.
pub fn exception_reply(status: u16, code: i32, text: &[u8]) -> Reply {
    Reply::new(status)
        .header("Content-Type", "text/plain; charset=UTF-8")
        .header("X-ClickHouse-Exception-Code", code.to_string())
        .header("X-ClickHouse-Query-Id", "server-assigned-0002")
        .body(text)
}

/// Splits `bytes` at the given offsets.
pub fn split_at(bytes: &[u8], offsets: &[usize]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    for &at in offsets {
        out.push(bytes[start..at].to_vec());
        start = at;
    }
    out.push(bytes[start..].to_vec());
    out
}

/// The offset of `needle` in `haystack` plus `delta` (to split inside it).
pub fn offset_of(haystack: &[u8], needle: &[u8], delta: usize) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap_or_else(|| panic!("{:?} not found", String::from_utf8_lossy(needle)))
        + delta
}
