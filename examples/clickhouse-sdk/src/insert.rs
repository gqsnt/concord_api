//! Streamed inserts: rows serialized while the request body is sent.

use std::error::Error as StdError;
use std::fmt;

use concord::csv::{CsvOptions, CsvWriter, HeaderMode, LineEnding, QuoteStyle, StreamRows};
use concord::jsonl::{JsonLinesOptions, JsonLinesWriter, StreamRecords};
use concord::stream::{ByteSource, Upload};
use concord::{Incoming, Received};
use futures_core::Stream;
use serde::Serialize;

use crate::client::{CallContext, endpoint_name, head_exception, server_exception};
use crate::error::{Error, LateException};
use crate::guard::{BodyException, Guarded};
use crate::meta::{Counters, HeaderError, ResponseMeta};
use crate::query::{Identifier, QueryId, RequestError, Table};
use crate::wire::ClickHouseHttp;

/// An insert being configured: target, columns, query ID, context; then the body.
///
/// The SDK writes the statement itself from validated identifiers:
/// `` INSERT INTO `db`.`t` (`a`, `b`) FORMAT JSONEachRow `` in the `query` URL parameter, the
/// rows as the `POST` body. Nothing in the statement comes from row data.
/// The declared `Encoded` request format preserves the writer's runtime `content-type`:
/// `application/x-ndjson` for JSONEachRow or `text/csv; header=present` for CSVWithNames.
/// The untyped response adds no `accept`; reqwest supplies its client default at execution
/// (`*/*` by default). Credentials and database headers follow [`crate::ClickHouse`].
///
/// An insert is **not** transactional: ClickHouse writes it in blocks. When the body fails after
/// it started (the source fails, a row cannot be serialized, the connection breaks), part of
/// the rows may already be stored; such errors report
/// [`is_outcome_unknown`](Error::is_outcome_unknown). Nothing is retried or rolled back.
#[must_use = "an insert does nothing until a body method is awaited"]
pub struct Insert {
    api: ClickHouseHttp,
    table: Table,
    columns: Vec<Identifier>,
    query_id: Option<QueryId>,
}

impl Insert {
    pub(crate) fn new(api: ClickHouseHttp, table: Table) -> Self {
        Self {
            api,
            table,
            columns: Vec::new(),
            query_id: None,
        }
    }

    /// The columns the rows fill, in order (written into the statement; for CSVWithNames also
    /// the header). Without them the server maps JSON members and CSV header names itself.
    pub fn columns(mut self, columns: impl IntoIterator<Item = Identifier>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    /// The query ID (`query_id`).
    pub fn query_id(mut self, id: QueryId) -> Self {
        self.query_id = Some(id);
        self
    }

    /// The caller's context (database, quota key) for this call.
    pub fn context(mut self, context: &CallContext) -> Self {
        self.api = context.apply(self.api);
        self
    }

    /// The statement the SDK sends.
    pub fn statement(&self, format: &str) -> String {
        let mut statement = format!("INSERT INTO {}", self.table.sql());
        if !self.columns.is_empty() {
            let columns: Vec<String> = self.columns.iter().map(|c| format!("`{c}`")).collect();
            statement.push_str(&format!(" ({})", columns.join(", ")));
        }
        statement.push_str(" FORMAT ");
        statement.push_str(format);
        statement
    }

    /// Streams `rows` as `JSONEachRow`: compact JSON, one object per line, a line feed after
    /// every row (`Content-Type: application/x-ndjson`, chunked). The source is polled only while
    /// the body is sent, once.
    ///
    /// # Errors
    ///
    /// As [`csv_with_names`](Self::csv_with_names).
    pub async fn json_each_row<S, T, E>(self, rows: S) -> Result<InsertReceipt, Error>
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + 'static,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        let upload =
            JsonLinesWriter::new(JsonLinesOptions::ndjson()).upload(StreamRecords::new(rows));
        let statement = self.statement("JSONEachRow");
        self.send(statement, upload).await
    }

    /// Streams `rows` as `CSVWithNames`: a header (the [`columns`](Self::columns), else the
    /// first row's field names), then one record per row, **every field double-quoted** and
    /// records ended by a line feed (`Content-Type: text/csv; header=present`, chunked).
    ///
    /// Quoting every field is deliberate: ClickHouse's CSV input trims spaces around unquoted
    /// fields and, with `format_csv_allow_single_quotes`, would read an unquoted field starting
    /// with `'` as a single-quoted string. A quoted field is read as written (quotes doubled).
    /// NULL has no quoted spelling (`\N` is the unquoted NULL); send nullable data as
    /// [`json_each_row`](Self::json_each_row).
    ///
    /// # Errors
    ///
    /// [`Error::Concord`]: `Build` for invalid columns (before sending), `Transport` when the
    /// transfer failed (a source or row failure after the body started included);
    /// [`Error::Server`], [`Error::LateException`] as for a query.
    pub async fn csv_with_names<S, T, E>(self, rows: S) -> Result<InsertReceipt, Error>
    where
        S: Stream<Item = Result<T, E>> + Send + 'static,
        T: Serialize + 'static,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        let writer = CsvWriter::new(
            CsvOptions::new(HeaderMode::Present, b',')
                .quote_style(QuoteStyle::Always)
                .write_terminator(LineEnding::Lf),
        )
        .expect("the CSVWithNames dialect is valid");
        let mut source = StreamRows::new(rows);
        if !self.columns.is_empty() {
            source = source.with_columns(self.columns.iter().map(Identifier::as_str));
        }
        let upload = writer
            .upload(source)
            .map_err(|e| Error::Request(RequestError::InvalidColumns { label: e.label() }))?;
        let statement = self.statement("CSVWithNames");
        self.send(statement, upload).await
    }

    async fn send(self, statement: String, upload: Upload) -> Result<InsertReceipt, Error> {
        let endpoint = self
            .api
            .insert(statement, upload)
            .maybe_query_id(self.query_id.clone());
        let operation = endpoint_name::<Incoming, _>(&endpoint);
        let received: Received<Incoming, String> = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            return Err(server_exception(received, operation).await);
        }
        let meta = ResponseMeta::new(received.meta());
        let incoming = received.into_incoming()?;
        if meta.exception_code().is_some() {
            return Err(head_exception(incoming, operation, meta).await);
        }
        // An insert answers an empty body; read it to its end through the guard, so an
        // exception block written after a 2xx head is not mistaken for success.
        let mut body = Guarded::new(incoming, meta.exception_tag());
        let mut bytes = 0u64;
        loop {
            match body.next_chunk().await {
                Ok(Some(chunk)) => bytes += chunk.len() as u64,
                Ok(None) => break,
                Err(error) => {
                    if let Some(block) = error
                        .source_unredacted()
                        .and_then(|source| source.downcast_ref::<BodyException>())
                    {
                        return Err(Error::LateException(Box::new(LateException::from_block(
                            operation, block, 0, meta,
                        ))));
                    }
                    // The guard fails only with an exception block (above) or with the body's
                    // own failure, passed through: an `Incoming`'s failure carries Concord's
                    // `Transport` error, taken back by value. The loop stops at the first one.
                    let transport = error
                        .into_source_unredacted()
                        .and_then(|source| source.downcast::<concord::Error>().ok())
                        .expect("a guard failure without a block is the body's failure");
                    return Err(Error::Concord(*transport));
                }
            }
        }
        Ok(InsertReceipt {
            requested_query_id: self.query_id,
            body_bytes: bytes,
            meta,
        })
    }
}

impl fmt::Debug for Insert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Insert")
            .field("table", &self.table)
            .field("columns", &self.columns)
            .field("query_id", &self.query_id)
            .finish()
    }
}

/// What the server reported for a successful insert.
#[derive(Debug)]
pub struct InsertReceipt {
    requested_query_id: Option<QueryId>,
    body_bytes: u64,
    meta: ResponseMeta,
}

impl InsertReceipt {
    /// The query ID the caller requested.
    pub fn requested_query_id(&self) -> Option<&QueryId> {
        self.requested_query_id.as_ref()
    }

    /// The query ID the server reported (`X-ClickHouse-Query-Id`), when sent.
    pub fn query_id(&self) -> Option<&str> {
        self.meta.query_id()
    }

    /// `X-ClickHouse-Summary`, when sent (`written_rows`, `written_bytes`).
    ///
    /// # Errors
    ///
    /// [`HeaderError`] when present but malformed.
    pub fn summary(&self) -> Result<Option<Counters>, HeaderError> {
        self.meta.summary()
    }

    /// Body bytes of the answer (ClickHouse answers an insert with an empty body).
    pub fn body_bytes(&self) -> u64 {
        self.body_bytes
    }

    /// The response head.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}
