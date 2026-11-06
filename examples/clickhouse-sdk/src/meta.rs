//! Response metadata: what the ClickHouse HTTP interface reports in response headers.
//!
//! Every accessor returns an `Option`: the documented headers are **not** promised on every
//! response (a proxy may drop them, an older server may not send them, `X-ClickHouse-Progress`
//! needs `send_progress_in_http_headers=1`). Header values are the server's, unredacted.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::reqwest::header::HeaderMap;
use serde_json::Value;

/// `X-ClickHouse-Query-Id`.
pub const QUERY_ID: &str = "x-clickhouse-query-id";
/// `X-ClickHouse-Summary`.
pub const SUMMARY: &str = "x-clickhouse-summary";
/// `X-ClickHouse-Progress` (zero or more).
pub const PROGRESS: &str = "x-clickhouse-progress";
/// `X-ClickHouse-Format`.
pub const FORMAT: &str = "x-clickhouse-format";
/// `X-ClickHouse-Timezone`.
pub const TIMEZONE: &str = "x-clickhouse-timezone";
/// `X-ClickHouse-Server-Display-Name`.
pub const SERVER_DISPLAY_NAME: &str = "x-clickhouse-server-display-name";
/// `X-ClickHouse-Exception-Code`.
pub const EXCEPTION_CODE: &str = "x-clickhouse-exception-code";
/// `X-ClickHouse-Exception-Tag`.
pub const EXCEPTION_TAG: &str = "x-clickhouse-exception-tag";

/// The head of a ClickHouse response (status, version, headers, operation), with accessors for
/// the documented `X-ClickHouse-*` headers.
///
/// `Debug` prints the status, the operation and the header **names**, never a value.
#[derive(Clone)]
pub struct ResponseMeta(concord::ResponseMeta);

impl ResponseMeta {
    pub(crate) fn new(meta: concord::ResponseMeta) -> Self {
        Self(meta)
    }

    /// Concord's owned head.
    pub fn inner(&self) -> &concord::ResponseMeta {
        &self.0
    }

    /// The HTTP status.
    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    /// Every header, unredacted.
    pub fn headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    fn text(&self, name: &str) -> Option<&str> {
        self.0.headers().get(name).and_then(|v| v.to_str().ok())
    }

    /// The query ID the server reported (`X-ClickHouse-Query-Id`): the caller's `query_id`, or
    /// one the server assigned.
    pub fn query_id(&self) -> Option<&str> {
        self.text(QUERY_ID)
    }

    /// The response format the server reported (`X-ClickHouse-Format`).
    pub fn format(&self) -> Option<&str> {
        self.text(FORMAT)
    }

    /// `X-ClickHouse-Timezone`.
    pub fn timezone(&self) -> Option<&str> {
        self.text(TIMEZONE)
    }

    /// `X-ClickHouse-Server-Display-Name`.
    pub fn server_display_name(&self) -> Option<&str> {
        self.text(SERVER_DISPLAY_NAME)
    }

    /// The exception tag (`X-ClickHouse-Exception-Tag`): present when the server marks an
    /// exception written into a started body with this tag.
    pub fn exception_tag(&self) -> Option<&str> {
        self.text(EXCEPTION_TAG)
    }

    /// `X-ClickHouse-Exception-Code`, when present and an integer.
    pub fn exception_code(&self) -> Option<i32> {
        self.text(EXCEPTION_CODE)?.trim().parse().ok()
    }

    /// The parsed `X-ClickHouse-Summary`: `Ok(None)` when absent.
    ///
    /// # Errors
    ///
    /// [`HeaderError`] when present but not a JSON object of counters.
    pub fn summary(&self) -> Result<Option<Counters>, HeaderError> {
        match self.0.headers().get(SUMMARY) {
            None => Ok(None),
            Some(value) => Counters::parse(value.as_bytes(), SUMMARY).map(Some),
        }
    }

    /// Every parsed `X-ClickHouse-Progress`, in the order the server sent them (empty when
    /// absent; the server sends them only with `send_progress_in_http_headers=1`, before the
    /// body).
    ///
    /// # Errors
    ///
    /// [`HeaderError`] for the first one that is not a JSON object of counters.
    pub fn progress(&self) -> Result<Vec<Counters>, HeaderError> {
        self.0
            .headers()
            .get_all(PROGRESS)
            .iter()
            .map(|value| Counters::parse(value.as_bytes(), PROGRESS))
            .collect()
    }
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

/// The counters of `X-ClickHouse-Summary` and `X-ClickHouse-Progress`. ClickHouse sends them as
/// JSON strings (`{"read_rows":"1",...}`); a number is accepted too. A counter the server did
/// not send is `None`; unknown members are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Counters {
    /// `read_rows`.
    pub read_rows: Option<u64>,
    /// `read_bytes`.
    pub read_bytes: Option<u64>,
    /// `written_rows`.
    pub written_rows: Option<u64>,
    /// `written_bytes`.
    pub written_bytes: Option<u64>,
    /// `total_rows_to_read`.
    pub total_rows_to_read: Option<u64>,
    /// `result_rows` (summary).
    pub result_rows: Option<u64>,
    /// `result_bytes` (summary).
    pub result_bytes: Option<u64>,
    /// `elapsed_ns`.
    pub elapsed_ns: Option<u64>,
    /// `memory_usage` (documented from v25.11).
    pub memory_usage: Option<u64>,
}

impl Counters {
    fn parse(bytes: &[u8], header: &'static str) -> Result<Self, HeaderError> {
        let invalid = || HeaderError { header };
        let Value::Object(map) = serde_json::from_slice::<Value>(bytes).map_err(|_| invalid())?
        else {
            return Err(invalid());
        };
        let field = |name: &str| -> Result<Option<u64>, HeaderError> {
            match map.get(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) => s.parse().map(Some).map_err(|_| invalid()),
                Some(Value::Number(n)) => n.as_u64().map(Some).ok_or_else(invalid),
                Some(_) => Err(invalid()),
            }
        };
        Ok(Self {
            read_rows: field("read_rows")?,
            read_bytes: field("read_bytes")?,
            written_rows: field("written_rows")?,
            written_bytes: field("written_bytes")?,
            total_rows_to_read: field("total_rows_to_read")?,
            result_rows: field("result_rows")?,
            result_bytes: field("result_bytes")?,
            elapsed_ns: field("elapsed_ns")?,
            memory_usage: field("memory_usage")?,
        })
    }
}

/// A metadata header is present but not in the documented shape. The header **name** only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderError {
    /// The header name.
    pub header: &'static str,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the {} header is not a JSON object of counters",
            self.header
        )
    }
}

impl std::error::Error for HeaderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_summary_parses() {
        // The `SELECT 1` example of the HTTP interface page.
        let summary = br#"{"read_rows":"1","read_bytes":"1","written_rows":"0","written_bytes":"0","total_rows_to_read":"1","result_rows":"0","result_bytes":"0","elapsed_ns":"4505959","memory_usage":"1111711"}"#;
        let counters = Counters::parse(summary, SUMMARY).unwrap();
        assert_eq!(counters.read_rows, Some(1));
        assert_eq!(counters.elapsed_ns, Some(4_505_959));
        assert_eq!(counters.memory_usage, Some(1_111_711));
    }

    #[test]
    fn malformed_counters_are_errors_not_absence() {
        for bad in [
            &b"not json"[..],
            b"[1]",
            br#"{"read_rows":"x"}"#,
            br#"{"read_rows":true}"#,
        ] {
            assert_eq!(
                Counters::parse(bad, PROGRESS),
                Err(HeaderError { header: PROGRESS })
            );
        }
        let partial = Counters::parse(br#"{"read_rows":5,"future":"1"}"#, PROGRESS).unwrap();
        assert_eq!(partial.read_rows, Some(5));
        assert_eq!(partial.written_rows, None);
    }
}
